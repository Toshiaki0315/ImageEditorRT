//! EXIF・GPS・MakerNote を表示用に読む（Python 版の core/exif_info.py を移したもの）。
//!
//! 標準のタグは kamadak-exif で読み、kamadak-exif が読まない MakerNote は makernote で読む。

use std::io::Cursor;

use exif::{Context, Exif, Field, In, Tag, Value};
use serde::Serialize;

use crate::makernote::read_maker_note;

/// 値の文字列の長さの上限（それより長ければ省略する）。
pub const MAX_VALUE_LENGTH: usize = 300;

/// 表示するときのグループ（並びもこの順）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum Group {
    #[serde(rename = "画像")]
    Image,
    #[serde(rename = "撮影")]
    Exif,
    #[serde(rename = "位置情報 (GPS)")]
    Gps,
    MakerNote,
    #[serde(rename = "互換性")]
    Interop,
    #[serde(rename = "サムネイル")]
    Thumbnail,
}

/// 表示する 1 項目。tag は元のタグ名、label は日本語訳を添えた項目名。
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Entry {
    pub group: Group,
    pub tag: String,
    pub label: String,
    pub value: String,
}

/// 撮影した場所（10 進の度。南緯・西経は負）。
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct GpsPosition {
    pub latitude: f64,
    pub longitude: f64,
}

impl GpsPosition {
    /// macOS のマップアプリでこの場所を開く URL。
    pub fn map_url(&self) -> String {
        let point = format!("{:.6},{:.6}", self.latitude, self.longitude);
        format!("maps://?ll={point}&q={point}")
    }
}

/// 読めた EXIF の情報。maker_note は MakerNote の形式（なければ None）。
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExifInfo {
    pub entries: Vec<Entry>,
    pub maker_note: Option<String>,
    pub gps: Option<GpsPosition>,
}

/// ファイルの中身（JPEG・HEIC・PNG・TIFF・WebP）から EXIF を読む。EXIF がなければ空。
pub fn read_exif_info(file: &[u8]) -> ExifInfo {
    match exif::Reader::new().read_from_container(&mut Cursor::new(file)) {
        Ok(exif) => build(&exif),
        Err(_) => ExifInfo::default(),
    }
}

/// ファイルの中身から、EXIF の TIFF の部分（"II*\0" / "MM\0*" で始まる）を取り出す。
pub fn raw_exif(file: &[u8]) -> Option<Vec<u8>> {
    exif::Reader::new().read_from_container(&mut Cursor::new(file)).ok().map(|e| e.buf().to_vec())
}

fn build(exif: &Exif) -> ExifInfo {
    let mut entries: Vec<Entry> = exif
        .fields()
        .filter(|f| !is_pointer(f.tag) && f.tag != Tag::MakerNote)
        .map(|f| entry(group_of(f), &f.tag.to_string(), value_text(exif, f)))
        .collect();

    let mut maker_note = None;
    if let Some(note) = read_maker_note(exif.buf()) {
        if note.is_decoded() {
            entries
                .extend(note.tags.iter().map(|(name, value)| entry(Group::MakerNote, name, value.clone())));
            maker_note = Some(note.format);
        } else {
            let summary = format!("解読できない形式（{} バイト）", note.size);
            entries.push(entry(Group::MakerNote, "MakerNote", summary));
            maker_note = Some(format!("{}（解読できない形式）", note.format));
        }
    }
    let gps = gps_position(exif);
    friendly_gps(&mut entries, exif);
    entries.sort_by_key(|e| e.group); // 安定な並べ替えなので、グループの中の順はそのまま
    ExifInfo { entries, maker_note, gps }
}

fn entry(group: Group, tag: &str, value: String) -> Entry {
    let label = match JAPANESE_NAMES.binary_search_by_key(&tag, |&(t, _)| t) {
        Ok(i) => format!("{tag}（{}）", JAPANESE_NAMES[i].1),
        Err(_) => tag.to_string(),
    };
    Entry { group, tag: tag.to_string(), label, value }
}

fn group_of(field: &Field) -> Group {
    if field.ifd_num == In::THUMBNAIL {
        return Group::Thumbnail;
    }
    match field.tag.context() {
        Context::Exif => Group::Exif,
        Context::Gps => Group::Gps,
        Context::Interop => Group::Interop,
        _ => Group::Image,
    }
}

/// ほかの IFD の位置を指すだけのタグ（表示しない）。
fn is_pointer(tag: Tag) -> bool {
    matches!(tag, Tag::ExifIFDPointer | Tag::GPSInfoIFDPointer | Tag::InteropIFDPointer)
        || (tag.context() == Context::Tiff && tag.number() == 0x014A) // SubIFDs
}

fn value_text(exif: &Exif, field: &Field) -> String {
    let mut text = field.display_value().with_unit(exif).to_string().replace('\0', "");
    // 文字列は "Canon" のように引用符つきで返るので外す
    if text.len() >= 2
        && text.starts_with('"')
        && text.ends_with('"')
        && matches!(field.value, Value::Ascii(_))
    {
        text = text[1..text.len() - 1].to_string();
    }
    let text = text.trim();
    match text.char_indices().nth(MAX_VALUE_LENGTH) {
        Some((i, _)) => format!("{}…", &text[..i]),
        None => text.to_string(),
    }
}

// --- 位置情報 ------------------------------------------------------------------------

fn ascii_of(exif: &Exif, tag: Tag) -> String {
    match exif.get_field(tag, In::PRIMARY).map(|f| &f.value) {
        Some(Value::Ascii(v)) => v.iter().map(|s| String::from_utf8_lossy(s).into_owned()).collect(),
        _ => String::new(),
    }
}

/// 緯度・経度のタグ（度・分・秒の 3 つ）を 10 進の度にする。
fn degrees(exif: &Exif, tag: Tag) -> Option<f64> {
    match &exif.get_field(tag, In::PRIMARY)?.value {
        Value::Rational(v) if v.len() == 3 && v.iter().all(|r| r.denom != 0) => {
            Some(v[0].to_f64() + v[1].to_f64() / 60.0 + v[2].to_f64() / 3600.0)
        }
        _ => None,
    }
}

/// 南緯・西経を負にした緯度・経度。
fn signed(exif: &Exif, tag: Tag, reference: Tag, negative: &str) -> Option<f64> {
    let value = degrees(exif, tag)?;
    Some(if ascii_of(exif, reference).trim().to_uppercase().starts_with(negative) { -value } else { value })
}

fn gps_position(exif: &Exif) -> Option<GpsPosition> {
    let latitude = signed(exif, Tag::GPSLatitude, Tag::GPSLatitudeRef, "S")?;
    let longitude = signed(exif, Tag::GPSLongitude, Tag::GPSLongitudeRef, "W")?;
    ((-90.0..=90.0).contains(&latitude) && (-180.0..=180.0).contains(&longitude))
        .then_some(GpsPosition { latitude, longitude })
}

/// 10 進の度を「35° 39′ 21.87″ N」のような度分秒にする（符号は方角の文字で表す）。
pub fn format_dms(degrees: f64, positive: &str, negative: &str) -> String {
    let sign = if degrees >= 0.0 { positive } else { negative };
    let value = degrees.abs();
    let mut whole = value.trunc() as u32;
    let minutes_float = (value - value.trunc()) * 60.0;
    let mut minutes = minutes_float.trunc() as u32;
    let mut seconds = (minutes_float - minutes_float.trunc()) * 60.0;
    if seconds >= 59.995 {
        // 四捨五入で 60.00″ にならないよう繰り上げる
        seconds = 0.0;
        minutes += 1;
    }
    if minutes == 60 {
        minutes = 0;
        whole += 1;
    }
    format!("{whole}° {minutes}′ {seconds:.2}″ {sign}")
}

/// 緯度・経度・高度を読みやすい値にする。
fn friendly_gps(entries: &mut [Entry], exif: &Exif) {
    for entry in entries.iter_mut().filter(|e| e.group == Group::Gps) {
        let text = match entry.tag.as_str() {
            "GPSLatitude" => signed(exif, Tag::GPSLatitude, Tag::GPSLatitudeRef, "S")
                .map(|v| format!("{}（{v:.6}）", format_dms(v, "N", "S"))),
            "GPSLongitude" => signed(exif, Tag::GPSLongitude, Tag::GPSLongitudeRef, "W")
                .map(|v| format!("{}（{v:.6}）", format_dms(v, "E", "W"))),
            "GPSAltitude" => match exif.get_field(Tag::GPSAltitude, In::PRIMARY).map(|f| &f.value) {
                Some(Value::Rational(v)) if !v.is_empty() && v[0].denom != 0 => {
                    let below = exif
                        .get_field(Tag::GPSAltitudeRef, In::PRIMARY)
                        .and_then(|f| f.value.get_uint(0))
                        .is_some_and(|r| r == 1);
                    Some(format!("{} {:.1} m", if below { "海面下" } else { "海抜" }, v[0].to_f64()))
                }
                _ => None,
            },
            _ => None,
        };
        if let Some(text) = text {
            entry.value = text;
        }
    }
}

// --- 日本語訳 ------------------------------------------------------------------------

/// タグ名の日本語訳（Python 版の exif_info.JAPANESE_NAMES と同じ。名前の順に並べる）。
const JAPANESE_NAMES: &[(&str, &str)] = &[
    ("AFAreaMode", "AF エリアモード"),
    ("ApertureValue", "絞り値 (APEX)"),
    ("Artist", "撮影者"),
    ("AspectRatio", "縦横比"),
    ("AutoBracketing", "オートブラケット"),
    ("BitsPerSample", "画素のビット数"),
    ("BodySerialNumber", "カメラのシリアル番号"),
    ("BrightnessValue", "輝度値 (APEX)"),
    ("CFAPattern", "CFA パターン"),
    ("CVAPattern", "CFA パターン"),
    ("CameraOwnerName", "カメラの所有者"),
    ("CameraTemperature", "カメラの温度"),
    ("ColorSpace", "色空間"),
    ("ColorTemperature", "色温度"),
    ("ComponentsConfiguration", "色の成分の並び"),
    ("CompressedBitsPerPixel", "画像の圧縮率"),
    ("Compression", "圧縮方式"),
    ("Contrast", "コントラスト"),
    ("Copyright", "著作権"),
    ("CustomRendered", "画像処理"),
    ("Date", "日付"),
    ("DateTime", "更新日時"),
    ("DateTimeDigitized", "デジタル化した日時"),
    ("DateTimeOriginal", "撮影日時"),
    ("DeviceSettingDescription", "撮影条件の記述"),
    ("DeviceType", "機器の種類"),
    ("DigitalZoom", "デジタルズーム"),
    ("DigitalZoomRatio", "デジタルズームの倍率"),
    ("DocumentName", "文書名"),
    ("DriveMode", "ドライブモード"),
    ("DynamicRangeExpansion", "ダイナミックレンジ拡大"),
    ("ExifImageLength", "画像の高さ"),
    ("ExifImageWidth", "画像の幅"),
    ("ExifVersion", "Exif のバージョン"),
    ("ExposureBiasValue", "露出補正"),
    ("ExposureCompensation", "露出補正"),
    ("ExposureIndex", "露出インデックス"),
    ("ExposureMode", "露出モード"),
    ("ExposureProgram", "露出プログラム"),
    ("ExposureTime", "露出時間"),
    ("FNumber", "F 値"),
    ("FaceDetect", "顔検出"),
    ("FileSource", "ファイルの出どころ"),
    ("FirmwareName", "ファームウェア名"),
    ("FirmwareVersion", "ファームウェアのバージョン"),
    ("Flash", "フラッシュ"),
    ("FlashEnergy", "フラッシュの強さ"),
    ("FlashMode", "フラッシュモード"),
    ("FlashPixVersion", "FlashPix のバージョン"),
    ("FocalLength", "焦点距離"),
    ("FocalLengthIn35mmFilm", "35mm 換算の焦点距離"),
    ("FocalPlaneResolutionUnit", "焦点面の解像度の単位"),
    ("FocalPlaneXResolution", "焦点面の水平解像度"),
    ("FocalPlaneYResolution", "焦点面の垂直解像度"),
    ("FocusMode", "フォーカスモード"),
    ("FrameNumber", "コマ番号"),
    ("GPSAltitude", "高度"),
    ("GPSAltitudeRef", "高度の基準"),
    ("GPSAreaInformation", "測位した地点の名前"),
    ("GPSDOP", "測位の精度"),
    ("GPSDate", "GPS の日付"),
    ("GPSDestBearing", "目的地の方角"),
    ("GPSDestBearingRef", "目的地の方角の基準"),
    ("GPSDestDistance", "目的地までの距離"),
    ("GPSDestDistanceRef", "目的地までの距離の単位"),
    ("GPSDestLatitude", "目的地の緯度"),
    ("GPSDestLatitudeRef", "目的地の北緯・南緯"),
    ("GPSDestLongitude", "目的地の経度"),
    ("GPSDestLongitudeRef", "目的地の東経・西経"),
    ("GPSDifferential", "差分補正"),
    ("GPSHPositioningError", "水平方向の測位誤差"),
    ("GPSImgDirection", "撮影方向"),
    ("GPSImgDirectionRef", "撮影方向の基準"),
    ("GPSLatitude", "緯度"),
    ("GPSLatitudeRef", "北緯・南緯"),
    ("GPSLongitude", "経度"),
    ("GPSLongitudeRef", "東経・西経"),
    ("GPSMapDatum", "測地系"),
    ("GPSMeasureMode", "測位の方式"),
    ("GPSProcessingMethod", "測位方式の名前"),
    ("GPSSatellites", "測位に使った衛星"),
    ("GPSSpeed", "速度"),
    ("GPSSpeedRef", "速度の単位"),
    ("GPSStatus", "受信機の状態"),
    ("GPSTimeStamp", "GPS の時刻 (UTC)"),
    ("GPSTrack", "進行方向"),
    ("GPSTrackRef", "進行方向の基準"),
    ("GPSVersionID", "GPS タグのバージョン"),
    ("GainControl", "ゲイン制御"),
    ("Gamma", "ガンマ"),
    ("HighISONoiseReduction", "高感度ノイズリダクション"),
    ("HostComputer", "ホストコンピューター"),
    ("ISO", "ISO 感度"),
    ("ISOSpeed", "ISO スピード"),
    ("ISOSpeedRatings", "撮影感度"),
    ("ImageDescription", "画像の説明"),
    ("ImageLength", "画像の高さ"),
    ("ImageNumber", "画像番号"),
    ("ImageTone", "仕上がり"),
    ("ImageType", "画像の種類"),
    ("ImageUniqueID", "画像の固有 ID"),
    ("ImageWidth", "画像の幅"),
    ("InterColorProfile", "ICC プロファイル"),
    ("InternalSerialNumber", "内部シリアル番号"),
    ("InteroperabilityIndex", "互換性の識別子"),
    ("InteroperabilityVersion", "互換性のバージョン"),
    ("JPEGInterchangeFormat", "JPEG データの位置"),
    ("JPEGInterchangeFormatLength", "JPEG データのバイト数"),
    ("LensFirmware", "レンズのファームウェア"),
    ("LensInfo", "レンズの情報"),
    ("LensMake", "レンズのメーカー"),
    ("LensModel", "レンズの機種"),
    ("LensSerialNumber", "レンズのシリアル番号"),
    ("LensSpecification", "レンズの仕様"),
    ("LensType", "レンズの種類"),
    ("LightSource", "光源"),
    ("MacroMode", "マクロモード"),
    ("Make", "メーカー"),
    ("MakerNote", "メーカーノート"),
    ("MakerNoteVersion", "メーカーノートのバージョン"),
    ("MaxApertureValue", "レンズの開放 F 値 (APEX)"),
    ("MeteringMode", "測光方式"),
    ("Model", "機種"),
    ("NoiseReduction", "ノイズリダクション"),
    ("OECF", "光電変換関数"),
    ("OffsetTime", "更新日時の時差"),
    ("OffsetTimeDigitized", "デジタル化した日時の時差"),
    ("OffsetTimeOriginal", "撮影日時の時差"),
    ("Orientation", "画像の向き"),
    ("OwnerName", "所有者"),
    ("PageName", "ページ名"),
    ("PhotometricInterpretation", "画素の構成"),
    ("PictureMode", "ピクチャーモード"),
    ("PlanarConfiguration", "画像データの並び"),
    ("PrimaryChromaticities", "原色の色度"),
    ("PrintIM", "プリントの設定 (PrintIM)"),
    ("Quality", "画質"),
    ("Rating", "評価"),
    ("RecommendedExposureIndex", "推奨露光指数"),
    ("ReferenceBlackWhite", "黒と白の基準値"),
    ("RelatedImageFileFormat", "関連画像のファイル形式"),
    ("RelatedImageLength", "関連画像の高さ"),
    ("RelatedImageWidth", "関連画像の幅"),
    ("RelatedSoundFile", "関連する音声ファイル"),
    ("ResolutionUnit", "解像度の単位"),
    ("RowsPerStrip", "ストリップの行数"),
    ("SamplesPerPixel", "色の成分の数"),
    ("Saturation", "彩度"),
    ("SceneCaptureType", "撮影シーン"),
    ("SceneType", "シーンの種類"),
    ("SelfTimerMode", "セルフタイマー"),
    ("SensingMethod", "センサーの方式"),
    ("SensitivityType", "感度の種類"),
    ("SerialNumber", "シリアル番号"),
    ("ShakeReductionInfo", "手ぶれ補正の情報"),
    ("Sharpness", "シャープネス"),
    ("ShutterCount", "シャッター回数"),
    ("ShutterSpeedValue", "シャッタースピード (APEX)"),
    ("Software", "ソフトウェア"),
    ("SpatialFrequencyResponse", "空間周波数応答"),
    ("SpectralSensitivity", "分光感度"),
    ("StripByteCounts", "ストリップのバイト数"),
    ("StripOffsets", "画像データの位置"),
    ("SubSecTime", "更新日時の秒未満"),
    ("SubSecTimeDigitized", "デジタル化した日時の秒未満"),
    ("SubSecTimeOriginal", "撮影日時の秒未満"),
    ("SubjectArea", "被写体の領域"),
    ("SubjectDistance", "被写体までの距離"),
    ("SubjectDistanceRange", "被写体までの距離の範囲"),
    ("SubjectLocation", "被写体の位置"),
    ("Time", "時刻"),
    ("TimeZoneOffset", "時差"),
    ("TransferFunction", "再生階調カーブ"),
    ("UserComment", "ユーザーコメント"),
    ("WhiteBalance", "ホワイトバランス"),
    ("WhiteBalanceMode", "ホワイトバランスのモード"),
    ("WhitePoint", "白色点"),
    ("XPAuthor", "作成者"),
    ("XPComment", "コメント"),
    ("XPKeywords", "キーワード"),
    ("XPSubject", "件名"),
    ("XPTitle", "タイトル"),
    ("XResolution", "水平解像度"),
    ("YCbCrCoefficients", "色変換の係数"),
    ("YCbCrPositioning", "色差の画素の位置"),
    ("YCbCrSubSampling", "色差のサンプリング比"),
    ("YResolution", "垂直解像度"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn japanese_names_are_sorted() {
        assert!(JAPANESE_NAMES.windows(2).all(|w| w[0].0 < w[1].0));
        assert_eq!(entry(Group::Image, "Make", "X".into()).label, "Make（メーカー）");
        assert_eq!(entry(Group::Image, "NoSuchTag", "X".into()).label, "NoSuchTag");
    }

    #[test]
    fn dms() {
        assert_eq!(format_dms(35.656075, "N", "S"), "35° 39′ 21.87″ N");
        assert_eq!(format_dms(-0.5, "E", "W"), "0° 30′ 0.00″ W");
        assert_eq!(format_dms(10.999999, "N", "S"), "11° 0′ 0.00″ N");
    }

    #[test]
    fn map_url() {
        let p = GpsPosition { latitude: 35.5, longitude: -139.25 };
        assert_eq!(p.map_url(), "maps://?ll=35.500000,-139.250000&q=35.500000,-139.250000");
    }

    #[test]
    fn not_an_image() {
        assert_eq!(read_exif_info(b"garbage"), ExifInfo::default());
    }
}
