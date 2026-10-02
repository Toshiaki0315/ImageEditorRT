//! kamadak-exif が読めない MakerNote（ペンタックス・リコー・Samsung など）を読む
//! （Python 版の core/makernote.py を移したもの）。
//!
//! MakerNote は EXIF の中にメーカーが独自の形式で書く情報。多くは TIFF と同じ IFD
//! （タグ番号・型・個数・値の並び）で書かれているが、先頭の目印（ヘッダー）、バイト順、
//! 値の位置の基準（MakerNote の先頭か、TIFF の先頭か）がメーカー・機種ごとに違う。
//! タグ番号と名前は ExifTool のタグ一覧に合わせ、名前の分からないタグは番号で表す。

use serde::Serialize;

use crate::tiff::{read_ifd, type_size, IfdEntry, Order, TAG_EXIF_IFD, TAG_MAKERNOTE};

/// 値の表示で並べる数の上限（それより多ければ省略する）。
pub const MAX_VALUES_SHOWN: usize = 16;
const TAG_MAKE: u16 = 0x010F;
const ASCII: u16 = 2;

/// MakerNote を読んだ結果。format は形式の名前（"Pentax" など）。
/// 読めなかったときは tags が空で、size（バイト数）だけを持つ。
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MakerNote {
    pub format: String,
    /// (名前, 値)。名前が分からなければ "Tag 0x0012" のような番号。
    pub tags: Vec<(String, String)>,
    pub size: usize,
}

impl MakerNote {
    /// 項目を読めたか。
    pub fn is_decoded(&self) -> bool {
        !self.tags.is_empty()
    }
}

/// EXIF の TIFF ブロック（"II*\0" / "MM\0*" で始まる）から MakerNote を探して読む。
///
/// MakerNote がなければ None。形式が分からない・壊れている MakerNote は、項目なしで返す。
pub fn read_maker_note(tiff: &[u8]) -> Option<MakerNote> {
    let reader = TiffReader::new(tiff)?;
    let (offset, size) = reader.maker_note()?;
    let make = reader.make();
    Some(decode(&reader, &tiff[offset..offset + size], offset, &make))
}

// --- 形式ごとの読み方 ---------------------------------------------------------------

/// 読み方の候補: 形式の名前・IFD の位置（MakerNote の先頭から）・試すバイト順・試す位置の基準・タグの名前。
struct Candidate {
    name: &'static str,
    start: usize,
    orders: Vec<Order>,
    bases: Vec<usize>,
    table: &'static [(u16, &'static str)],
}

fn decode(reader: &TiffReader, note: &[u8], offset: usize, make: &str) -> MakerNote {
    let size = note.len();
    let upper = make.to_uppercase();
    if note.starts_with(b"HDRP") {
        return MakerNote { format: "Google HDR+".into(), tags: Vec::new(), size };
    }
    let tiff_order = reader.order;
    let order_at = |range: std::ops::Range<usize>| note.get(range).and_then(Order::from_mark);
    let candidate = |name, start, orders, bases, table| Candidate { name, start, orders, bases, table };
    let mut candidates = Vec::new();
    if note.starts_with(b"PENTAX \0") {
        let order = order_at(8..10).unwrap_or(tiff_order);
        candidates.push(candidate("Pentax", 10, vec![order], vec![offset], PENTAX_TAGS));
    } else if note.starts_with(b"AOC\0") {
        let orders = both(order_at(4..6).unwrap_or(tiff_order));
        candidates.push(candidate("Pentax", 6, orders, vec![offset, 0], PENTAX_TAGS));
    } else if note.starts_with(b"S1\0\0\0\0\0\0\x0c\0\0\0") {
        candidates.push(candidate("Pentax", 12, both(tiff_order), vec![offset], PENTAX_TAGS));
    } else if note.starts_with(b"RICOH\0II") || note.starts_with(b"RICOH\0MM") {
        let order = order_at(6..8).unwrap_or(tiff_order);
        candidates.push(candidate("Ricoh（Pentax 形式）", 8, vec![order], vec![offset], PENTAX_TAGS));
    } else if upper.starts_with("RICOH") || upper.starts_with("PENTAX RICOH") {
        let header = &note[..note.len().min(8)];
        if header == b"MM\0\x2a\0\0\0\x08" || header == b"II\x2a\0\x08\0\0\0" {
            // WG-M1 などの、TIFF と同じヘッダーの形式（位置は MakerNote の先頭が基準）
            let order = Order::from_mark(&header[..2]).unwrap_or(tiff_order);
            candidates.push(candidate("Ricoh", 8, vec![order], vec![offset], RICOH_TAGS));
        } else {
            candidates.push(candidate("Ricoh", 8, both(tiff_order), vec![0, offset], RICOH_TAGS));
        }
    } else if upper.starts_with("SAMSUNG") {
        candidates.push(candidate("Samsung", 0, both(tiff_order), vec![0, offset], SAMSUNG_TAGS));
    } else if upper.starts_with("PENTAX") || upper.starts_with("ASAHI") {
        candidates.push(candidate("Pentax", 0, both(tiff_order), vec![0, offset], PENTAX_TAGS));
    }
    // どれにも当てはまらなければ、ヘッダーのない IFD として読めるか試す
    candidates.push(candidate("不明な形式", 0, both(tiff_order), vec![0, offset], &[]));

    for c in &candidates {
        let tags = read_best(reader.data, offset + c.start, &c.orders, &c.bases, c.table);
        if !tags.is_empty() {
            return MakerNote { format: c.name.into(), tags, size };
        }
    }
    // 形式は分かったが読めなかった（壊れている・知らない版）ときも、形式の名前は出す
    MakerNote { format: candidates[0].name.into(), tags: Vec::new(), size }
}

/// バイト順と位置の基準の組み合わせを試し、値の位置がいちばん多く収まる読み方で返す。
fn read_best(
    data: &[u8],
    ifd_offset: usize,
    orders: &[Order],
    bases: &[usize],
    table: &[(u16, &str)],
) -> Vec<(String, String)> {
    let mut best: (usize, Vec<(String, String)>) = (0, Vec::new());
    for &order in orders {
        let Some(entries) = read_ifd(data, ifd_offset, order) else { continue };
        for &base in bases {
            let tags = decode_entries(data, &entries, order, base, table);
            if tags.len() > best.0 {
                best = (tags.len(), tags);
            }
        }
    }
    best.1
}

/// 項目を名前と値の文字列にする（値の位置がデータの外にある項目は読み飛ばす）。
fn decode_entries(
    data: &[u8],
    entries: &[IfdEntry],
    order: Order,
    base: usize,
    table: &[(u16, &str)],
) -> Vec<(String, String)> {
    entries
        .iter()
        .filter_map(|entry| {
            let value = entry.value(data, order, base)?;
            Some((tag_name(table, entry.tag), format_value(entry.kind, value, order)))
        })
        .collect()
}

/// タグの名前。表になければ "Tag 0x0012" のような番号。
pub fn tag_name(table: &[(u16, &str)], tag: u16) -> String {
    match table.binary_search_by_key(&tag, |&(t, _)| t) {
        Ok(i) => table[i].1.to_string(),
        Err(_) => format!("Tag 0x{tag:04x}"),
    }
}

fn both(first: Order) -> Vec<Order> {
    let second = if first == Order::Little { Order::Big } else { Order::Little };
    vec![first, second]
}

// --- TIFF の読み取り ----------------------------------------------------------------

/// EXIF の TIFF ブロックから、MakerNote の位置とメーカー名を探す。
struct TiffReader<'a> {
    data: &'a [u8],
    order: Order,
    ifd0: usize,
}

impl<'a> TiffReader<'a> {
    fn new(data: &'a [u8]) -> Option<Self> {
        let order = Order::from_mark(data.get(..2)?)?;
        if order.u16(data.get(2..4)?) != 42 {
            return None;
        }
        Some(Self { data, order, ifd0: order.u32(data.get(4..8)?) as usize })
    }

    fn find(&self, ifd_offset: usize, tag: u16) -> Option<IfdEntry> {
        read_ifd(self.data, ifd_offset, self.order)?.into_iter().find(|e| e.tag == tag)
    }

    fn make(&self) -> String {
        match self.find(self.ifd0, TAG_MAKE) {
            Some(entry) if entry.kind == ASCII => {
                entry.value(self.data, self.order, 0).map(ascii).unwrap_or_default()
            }
            _ => String::new(),
        }
    }

    /// MakerNote の位置と大きさ（TIFF の先頭から）。
    fn maker_note(&self) -> Option<(usize, usize)> {
        let exif = self.find(self.ifd0, TAG_EXIF_IFD)?;
        let note = self.find(exif.pointer(self.order), TAG_MAKERNOTE)?;
        if note.count <= 4 {
            return None;
        }
        let offset = note.pointer(self.order);
        let size = note.count as usize;
        (offset.checked_add(size)? <= self.data.len()).then_some((offset, size))
    }
}

// --- 値の表示 ---------------------------------------------------------------------

/// TIFF の型と値のバイト列を、表示用の文字列にする。
pub fn format_value(kind: u16, value: &[u8], order: Order) -> String {
    if kind == ASCII {
        return ascii(value);
    }
    if kind == 1 || kind == 7 {
        let end = value.iter().rposition(|&b| b != 0).map_or(0, |i| i + 1);
        let text = &value[..end];
        if !text.is_empty() && text.len() <= 256 && text.iter().all(|&b| (32..127).contains(&b)) {
            return String::from_utf8_lossy(text).into_owned();
        }
        if kind == 7 {
            return hex(value);
        }
    }
    let Some(size) = type_size(kind) else { return hex(value) };
    let texts: Vec<String> = value
        .chunks_exact(size)
        .map(|b| match kind {
            1 => b[0].to_string(),
            6 => (b[0] as i8).to_string(),
            3 => order.u16(b).to_string(),
            8 => (order.u16(b) as i16).to_string(),
            4 | 13 => order.u32(b).to_string(),
            9 => (order.u32(b) as i32).to_string(),
            5 => format!("{}/{}", order.u32(&b[..4]), order.u32(&b[4..])),
            10 => format!("{}/{}", order.u32(&b[..4]) as i32, order.u32(&b[4..]) as i32),
            11 => general(f64::from(f32::from_bits(order.u32(b)))),
            _ => {
                let (hi, lo) = (u64::from(order.u32(&b[..4])), u64::from(order.u32(&b[4..])));
                let bits = if order == Order::Little { lo << 32 | hi } else { hi << 32 | lo };
                general(f64::from_bits(bits))
            }
        })
        .collect();
    join(&texts)
}

/// Python の "{:g}" と同じく、有効数字 6 桁で余分な 0 を付けない。
fn general(x: f64) -> String {
    if !x.is_finite() {
        return x.to_string();
    }
    if x == 0.0 {
        return "0".into();
    }
    let exponent = x.abs().log10().floor() as i32;
    if !(-4..6).contains(&exponent) {
        let text = format!("{:.5e}", x);
        let (mantissa, exp) = text.split_once('e').unwrap_or((&text, "0"));
        let mantissa = mantissa.trim_end_matches('0').trim_end_matches('.');
        let exp: i32 = exp.parse().unwrap_or(0);
        return format!("{mantissa}e{}{:02}", if exp < 0 { '-' } else { '+' }, exp.abs());
    }
    let decimals = (5 - exponent).max(0) as usize;
    let text = format!("{:.*}", decimals, x);
    if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        text
    }
}

fn join(texts: &[String]) -> String {
    if texts.len() == 1 {
        return texts[0].clone();
    }
    let shown = texts.iter().take(MAX_VALUES_SHOWN).cloned().collect::<Vec<_>>().join(", ");
    if texts.len() > MAX_VALUES_SHOWN {
        format!("[{shown}, …]（全 {} 個）", texts.len())
    } else {
        format!("[{shown}]")
    }
}

fn hex(value: &[u8]) -> String {
    let shown = value.iter().take(MAX_VALUES_SHOWN).map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ");
    if value.len() > MAX_VALUES_SHOWN {
        format!("{shown} …（{} バイト）", value.len())
    } else {
        shown
    }
}

fn ascii(value: &[u8]) -> String {
    let end = value.iter().position(|&b| b == 0).unwrap_or(value.len());
    String::from_utf8_lossy(&value[..end]).trim().to_string()
}

// --- タグの名前（ExifTool のタグ一覧に合わせる。番号の順に並べる） ------------------------

pub const PENTAX_TAGS: &[(u16, &str)] = &[
    (0x0000, "PentaxVersion"),
    (0x0001, "PentaxModelType"),
    (0x0002, "PreviewImageSize"),
    (0x0003, "PreviewImageLength"),
    (0x0004, "PreviewImageStart"),
    (0x0005, "PentaxModelID"),
    (0x0006, "Date"),
    (0x0007, "Time"),
    (0x0008, "Quality"),
    (0x0009, "PentaxImageSize"),
    (0x000B, "PictureMode"),
    (0x000C, "FlashMode"),
    (0x000D, "FocusMode"),
    (0x000E, "AFPointSelected"),
    (0x000F, "AFPointsInFocus"),
    (0x0010, "FocusPosition"),
    (0x0012, "ExposureTime"),
    (0x0013, "FNumber"),
    (0x0014, "ISO"),
    (0x0015, "LightReading"),
    (0x0016, "ExposureCompensation"),
    (0x0017, "MeteringMode"),
    (0x0018, "AutoBracketing"),
    (0x0019, "WhiteBalance"),
    (0x001A, "WhiteBalanceMode"),
    (0x001B, "BlueBalance"),
    (0x001C, "RedBalance"),
    (0x001D, "FocalLength"),
    (0x001E, "DigitalZoom"),
    (0x001F, "Saturation"),
    (0x0020, "Contrast"),
    (0x0021, "Sharpness"),
    (0x0022, "WorldTimeLocation"),
    (0x0023, "HometownCity"),
    (0x0024, "DestinationCity"),
    (0x0025, "HometownDST"),
    (0x0026, "DestinationDST"),
    (0x0027, "DSPFirmwareVersion"),
    (0x0028, "CPUFirmwareVersion"),
    (0x0029, "FrameNumber"),
    (0x002D, "EffectiveLV"),
    (0x0032, "ImageEditing"),
    (0x0033, "PictureMode"),
    (0x0034, "DriveMode"),
    (0x0035, "SensorSize"),
    (0x0037, "ColorSpace"),
    (0x0038, "ImageAreaOffset"),
    (0x0039, "RawImageSize"),
    (0x003C, "AFPointsInFocus"),
    (0x003D, "DataScaling"),
    (0x003E, "PreviewImageBorders"),
    (0x003F, "LensRec"),
    (0x0040, "SensitivityAdjust"),
    (0x0041, "ImageEditCount"),
    (0x0047, "CameraTemperature"),
    (0x0048, "AELock"),
    (0x0049, "NoiseReduction"),
    (0x004D, "FlashExposureComp"),
    (0x004F, "ImageTone"),
    (0x0050, "ColorTemperature"),
    (0x0053, "ColorTempDaylight"),
    (0x0054, "ColorTempShade"),
    (0x0055, "ColorTempCloudy"),
    (0x0056, "ColorTempTungsten"),
    (0x0057, "ColorTempFluorescentD"),
    (0x0058, "ColorTempFluorescentN"),
    (0x0059, "ColorTempFluorescentW"),
    (0x005A, "ColorTempFlash"),
    (0x005C, "ShakeReductionInfo"),
    (0x005D, "ShutterCount"),
    (0x0060, "FaceInfo"),
    (0x0062, "RawDevelopmentProcess"),
    (0x0067, "Hue"),
    (0x0068, "AWBInfo"),
    (0x0069, "DynamicRangeExpansion"),
    (0x006B, "TimeInfo"),
    (0x006C, "HighLowKeyAdj"),
    (0x006D, "ContrastHighlight"),
    (0x006E, "ContrastShadow"),
    (0x006F, "ContrastHighlightShadowAdj"),
    (0x0070, "FineSharpness"),
    (0x0071, "HighISONoiseReduction"),
    (0x0072, "AFAdjustment"),
    (0x0073, "MonochromeFilterEffect"),
    (0x0074, "MonochromeToning"),
    (0x0076, "FaceDetect"),
    (0x0077, "FaceDetectFrameSize"),
    (0x0079, "ShadowCorrection"),
    (0x007A, "ISOAutoMinSpeed"),
    (0x007B, "CrossProcess"),
    (0x007D, "LensCorr"),
    (0x007E, "WhiteLevel"),
    (0x007F, "BleachBypassToning"),
    (0x0080, "AspectRatio"),
    (0x0082, "BlurControl"),
    (0x0085, "HDR"),
    (0x0087, "ShutterType"),
    (0x0088, "NeutralDensityFilter"),
    (0x008B, "ISO"),
    (0x0092, "IntervalShooting"),
    (0x0095, "SkinToneCorrection"),
    (0x0096, "ClarityControl"),
    (0x009E, "HDF"),
    (0x0200, "BlackPoint"),
    (0x0201, "WhitePoint"),
    (0x0203, "ColorMatrixA"),
    (0x0204, "ColorMatrixB"),
    (0x0205, "CameraSettings"),
    (0x0206, "AEInfo"),
    (0x0207, "LensInfo"),
    (0x0208, "FlashInfo"),
    (0x0209, "AEMeteringSegments"),
    (0x020A, "FlashMeteringSegments"),
    (0x020B, "SlaveFlashMeteringSegments"),
    (0x020D, "WB_RGGBLevelsDaylight"),
    (0x020E, "WB_RGGBLevelsShade"),
    (0x020F, "WB_RGGBLevelsCloudy"),
    (0x0210, "WB_RGGBLevelsTungsten"),
    (0x0211, "WB_RGGBLevelsFluorescentD"),
    (0x0212, "WB_RGGBLevelsFluorescentN"),
    (0x0213, "WB_RGGBLevelsFluorescentW"),
    (0x0214, "WB_RGGBLevelsFlash"),
    (0x0215, "CameraInfo"),
    (0x0216, "BatteryInfo"),
    (0x021C, "ColorMatrixA2"),
    (0x021D, "ColorMatrixB2"),
    (0x021F, "AFInfo"),
    (0x0221, "KelvinWB"),
    (0x0222, "ColorInfo"),
    (0x0224, "EVStepInfo"),
    (0x0226, "ShotInfo"),
    (0x0227, "FacePos"),
    (0x0228, "FaceSize"),
    (0x0229, "SerialNumber"),
    (0x022A, "FilterInfo"),
    (0x022B, "LevelInfoK3III"),
    (0x022D, "WBLevels"),
    (0x022E, "Artist"),
    (0x022F, "Copyright"),
    (0x0230, "FirmwareVersion"),
    (0x0231, "ContrastDetectAFArea"),
    (0x0235, "CrossProcessParams"),
    (0x0238, "CAFPointInfo"),
    (0x0239, "LensInfoQ"),
    (0x023F, "Model"),
    (0x0243, "PixelShiftInfo"),
    (0x0245, "AFPointInfo"),
    (0x03FE, "DataDump"),
    (0x03FF, "TempInfo"),
    (0x0402, "ToneCurve"),
    (0x0403, "ToneCurves"),
    (0x040B, "FaceInfoK3III"),
    (0x040C, "AFInfoK3III"),
    (0x0E00, "PrintIM"),
];

pub const RICOH_TAGS: &[(u16, &str)] = &[
    (0x0001, "MakerNoteType"),
    (0x0002, "FirmwareVersion"),
    (0x0005, "SerialNumber"),
    (0x0E00, "PrintIM"),
    (0x1000, "RecordingFormat"),
    (0x1001, "ImageInfo"),
    (0x1002, "DriveMode"),
    (0x1003, "Sharpness"),
    (0x1004, "WhiteBalanceFineTune"),
    (0x1006, "FocusMode"),
    (0x1007, "AutoBracketing"),
    (0x1009, "MacroMode"),
    (0x100A, "FlashMode"),
    (0x100B, "FlashExposureComp"),
    (0x100C, "ManualFlashOutput"),
    (0x100D, "FullPressSnap"),
    (0x100E, "DynamicRangeExpansion"),
    (0x100F, "NoiseReduction"),
    (0x1010, "ImageEffects"),
    (0x1011, "Vignetting"),
    (0x1012, "Contrast"),
    (0x1013, "Saturation"),
    (0x1014, "Sharpness"),
    (0x1015, "ToningEffect"),
    (0x1016, "HueAdjust"),
    (0x1017, "WideAdapter"),
    (0x1018, "CropMode"),
    (0x1019, "NDFilter"),
    (0x101A, "WBBracketShotNumber"),
    (0x1200, "AFStatus"),
    (0x1201, "AFAreaXPosition1"),
    (0x1202, "AFAreaYPosition1"),
    (0x1203, "AFAreaXPosition"),
    (0x1204, "AFAreaYPosition"),
    (0x1205, "AFAreaMode"),
    (0x1307, "ColorTempKelvin"),
    (0x1308, "ColorTemperature"),
    (0x1500, "FocalLength"),
    (0x1601, "SensorWidth"),
    (0x1602, "SensorHeight"),
    (0x1603, "CroppedImageWidth"),
    (0x1604, "CroppedImageHeight"),
    (0x2001, "RicohSubdir"),
    (0x4001, "ThetaSubdir"),
];

pub const SAMSUNG_TAGS: &[(u16, &str)] = &[
    (0x0001, "MakerNoteVersion"),
    (0x0002, "DeviceType"),
    (0x0003, "SamsungModelID"),
    (0x0011, "OrientationInfo"),
    (0x0020, "SmartAlbumColor"),
    (0x0021, "PictureWizard"),
    (0x0030, "LocalLocationName"),
    (0x0031, "LocationName"),
    (0x0035, "PreviewIFD"),
    (0x0040, "RawDataByteOrder"),
    (0x0041, "WhiteBalanceSetup"),
    (0x0043, "CameraTemperature"),
    (0x0050, "RawDataCFAPattern"),
    (0x0100, "FaceDetect"),
    (0x0120, "FaceRecognition"),
    (0x0123, "FaceName"),
    (0xA001, "FirmwareName"),
    (0xA002, "SerialNumber"),
    (0xA003, "LensType"),
    (0xA004, "LensFirmware"),
    (0xA005, "InternalLensSerialNumber"),
    (0xA010, "SensorAreas"),
    (0xA011, "ColorSpace"),
    (0xA012, "SmartRange"),
    (0xA013, "ExposureCompensation"),
    (0xA014, "ISO"),
    (0xA018, "ExposureTime"),
    (0xA019, "FNumber"),
    (0xA01A, "FocalLengthIn35mmFormat"),
    (0xA020, "EncryptionKey"),
    (0xA021, "WB_RGGBLevelsUncorrected"),
    (0xA022, "WB_RGGBLevelsAuto"),
    (0xA023, "WB_RGGBLevelsIlluminator1"),
    (0xA024, "WB_RGGBLevelsIlluminator2"),
    (0xA025, "HighlightLinearityLimit"),
    (0xA028, "WB_RGGBLevelsBlack"),
    (0xA030, "ColorMatrix"),
    (0xA031, "ColorMatrixSRGB"),
    (0xA032, "ColorMatrixAdobeRGB"),
    (0xA033, "CbCrMatrixDefault"),
    (0xA034, "CbCrMatrix"),
    (0xA035, "CbCrGainDefault"),
    (0xA036, "CbCrGain"),
    (0xA040, "ToneCurveSRGBDefault"),
    (0xA041, "ToneCurveAdobeRGBDefault"),
    (0xA042, "ToneCurveSRGB"),
    (0xA043, "ToneCurveAdobeRGB"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_are_sorted() {
        for table in [PENTAX_TAGS, RICOH_TAGS, SAMSUNG_TAGS] {
            assert!(table.windows(2).all(|w| w[0].0 < w[1].0));
        }
        assert_eq!(tag_name(PENTAX_TAGS, 0x0229), "SerialNumber");
        assert_eq!(tag_name(PENTAX_TAGS, 0x7777), "Tag 0x7777");
    }

    #[test]
    fn formats_values_like_python() {
        let le = Order::Little;
        assert_eq!(format_value(2, b"abc\0\0", le), "abc");
        assert_eq!(format_value(7, b"0230", le), "0230");
        assert_eq!(format_value(7, b"\x00\x01\xff", le), "00 01 ff");
        assert_eq!(format_value(3, &300u16.to_le_bytes(), le), "300");
        assert_eq!(format_value(3, &300u16.to_be_bytes(), Order::Big), "300");
        let rational = [1u32.to_le_bytes(), 125u32.to_le_bytes()].concat();
        assert_eq!(format_value(5, &rational, le), "1/125");
        assert_eq!(format_value(9, &(-5i32).to_le_bytes(), le), "-5");
        assert_eq!(format_value(11, &1.5f32.to_le_bytes(), le), "1.5");
        assert_eq!(format_value(12, &0.1f64.to_be_bytes(), Order::Big), "0.1");
        let many: Vec<u8> = (0..MAX_VALUES_SHOWN as u16 + 4).flat_map(|i| i.to_le_bytes()).collect();
        let text = format_value(3, &many, le);
        assert!(
            text.starts_with("[0, 1, 2") && text.ends_with(&format!("（全 {} 個）", MAX_VALUES_SHOWN + 4))
        );
    }

    #[test]
    fn general_matches_python_g() {
        for (x, s) in
            [(1.0, "1"), (2.8, "2.8"), (1234567.0, "1.23457e+06"), (0.0001, "0.0001"), (1e-5, "1e-05")]
        {
            assert_eq!(general(x), s);
        }
    }

    #[test]
    fn not_tiff() {
        assert_eq!(read_maker_note(b"not a tiff at all"), None);
    }
}
