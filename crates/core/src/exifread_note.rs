//! 主なメーカー（Canon・Nikon・Sony・Apple・Fujifilm・Olympus・Casio・DJI）の MakerNote を、
//! 旧版が使っていた exifread（3.5.1）と同じ読み方・同じ表示で読む。
//!
//! exifread の ExifHeader.decode_maker_note・dump_ifd・_process_tag をそのまま移したもの。
//! 値の位置がデータの外にあれば 0 や空として読む、値の表示は Python の str() の形にする、
//! といった細かいところも合わせる（壊れた MakerNote でも旧版と同じものを出すため）。
//! タグの名前・値の表は exifread_tables.rs（exifread の表から自動で作ったもの）。

use crate::exifread_tables as tables;

/// タグの表の 1 行: (タグ番号, 名前, 値の表示のしかた)。
pub type TagDef = (u16, &'static str, Format);

/// 値の表示のしかた（exifread のタグの表の 2 つ目の要素）。
#[derive(Clone, Copy, Debug)]
pub enum Format {
    /// 値をそのまま出す
    Plain,
    /// 値ごとに名前を引く（見つからなければ値そのもの）
    Map(&'static [(i64, &'static str)]),
    /// 決まった関数で文字列にする
    Func(Func),
}

/// exifread のタグの表で使われている、値を文字列にする関数。
#[derive(Clone, Copy, Debug)]
pub enum Func {
    EvBias,
    MakeString,
    SpecialMode,
    ConvertTemp,
}

/// exifread で読んだ結果。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// MakerNote の項目を読めた。make は Image Make の値（表示用に整える前）。
    Decoded { make: String, tags: Vec<(String, String)> },
    /// exifread が読まない（メーカーが違う・MakerNote がない・項目が 1 つもない）
    NotDecoded,
    /// exifread が例外で止まる（旧版は MakerNote・UserComment などを読まずに読み直していた）
    Failed,
}

/// exifread が例外で止まる場面。
#[derive(Debug)]
struct Failed;

type Result<T> = std::result::Result<T, Failed>;

/// (Image Make の値, MakerNote の項目 (名前, 値))。
type Decoded = (String, Vec<(String, String)>);

/// EXIF の TIFF ブロック（"II*\0" / "MM\0*" で始まる）の MakerNote を exifread と同じように読む。
pub fn read(tiff: &[u8]) -> Outcome {
    match decode(tiff) {
        Ok(Some((make, tags))) if !tags.is_empty() => Outcome::Decoded { make, tags },
        Ok(_) => Outcome::NotDecoded,
        Err(Failed) => Outcome::Failed,
    }
}

// --- 値 ------------------------------------------------------------------------------

/// 数の値 1 つ（Python での型に合わせる）。
#[derive(Clone, Copy, Debug, PartialEq)]
enum Val {
    Int(i64),
    /// 分数（exifread の Ratio。Python の Fraction と同じく約分して表示する）
    Ratio(i64, i64),
    /// 浮動小数点数（exifread は (値,) のタプルにする）
    Float(f64),
    /// 文字列の 1 文字（文字列の値を 1 つずつ見るとき）
    Char(char),
}

/// タグの値（exifread の IfdTag.values）。
#[derive(Clone, Debug, PartialEq)]
enum Values {
    /// ASCII（NUL より前を UTF-8 として読めたもの）
    Ascii(String),
    /// ASCII だが UTF-8 として読めなかったもの（Python では bytes のまま残る）
    Bytes(Vec<u8>),
    List(Vec<Val>),
}

impl Values {
    /// 1 つずつの値（Python で for で回したときの要素）。
    fn items(&self) -> Vec<Val> {
        match self {
            Values::Ascii(s) => s.chars().map(Val::Char).collect(),
            Values::Bytes(b) => b.iter().map(|&v| Val::Int(i64::from(v))).collect(),
            Values::List(v) => v.clone(),
        }
    }

    /// Python の str(values)。
    fn str(&self) -> String {
        match self {
            Values::Ascii(s) => s.clone(),
            Values::Bytes(b) => bytes_repr(b),
            Values::List(v) => list_repr(v),
        }
    }
}

impl Val {
    /// Python の str()。
    fn str(self) -> String {
        match self {
            Val::Char(c) => c.to_string(),
            _ => self.repr(),
        }
    }

    /// Python の repr()。
    fn repr(self) -> String {
        match self {
            Val::Int(v) => v.to_string(),
            Val::Ratio(n, d) => ratio_str(n, d),
            Val::Float(v) => format!("({},)", float_repr(v)),
            Val::Char(c) => char_repr(c),
        }
    }

    /// 整数として等しいか（Python の辞書を引くときと同じ。分母が 1 の分数も整数と等しい）。
    fn as_key(self) -> Option<i64> {
        match self {
            Val::Int(v) => Some(v),
            Val::Ratio(n, d) if d != 0 => {
                let (n, d) = reduce(n, d);
                (d == 1).then_some(n)
            }
            _ => None,
        }
    }
}

fn gcd(a: i64, b: i64) -> i64 {
    let (mut a, mut b) = (a.unsigned_abs(), b.unsigned_abs());
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a as i64
}

/// Python の Fraction(n, d) と同じに約分する（分母は正）。
fn reduce(n: i64, d: i64) -> (i64, i64) {
    let g = gcd(n, d).max(1) * d.signum();
    (n / g, d / g)
}

/// exifread の Ratio の str（分母が 0 なら約分しない）。
fn ratio_str(n: i64, d: i64) -> String {
    let (n, d) = if d == 0 { (n, d) } else { reduce(n, d) };
    if d == 1 {
        n.to_string()
    } else {
        format!("{n}/{d}")
    }
}

fn list_repr(values: &[Val]) -> String {
    let items: Vec<String> = values.iter().map(|v| v.repr()).collect();
    format!("[{}]", items.join(", "))
}

/// Python の repr(float)（最短で元に戻る桁。指数が -4 未満か 16 以上なら指数表記）。
fn float_repr(x: f64) -> String {
    if x.is_nan() {
        return "nan".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "inf" } else { "-inf" }.into();
    }
    // "1.2345e-7" のような最短の表記から、数字の並びと指数を取り出す
    let sci = format!("{x:e}");
    let (mantissa, exponent) = sci.split_once('e').expect("指数表記");
    let exponent: i32 = exponent.parse().expect("指数");
    let negative = mantissa.starts_with('-');
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let sign = if negative { "-" } else { "" };
    if (-4..16).contains(&exponent) {
        let point = exponent + 1; // 小数点の前の桁の数
        let text = if point <= 0 {
            format!("0.{}{digits}", "0".repeat((-point) as usize))
        } else if point as usize >= digits.len() {
            format!("{digits}{}.0", "0".repeat(point as usize - digits.len()))
        } else {
            format!("{}.{}", &digits[..point as usize], &digits[point as usize..])
        };
        format!("{sign}{text}")
    } else {
        let rest = if digits.len() > 1 { format!(".{}", &digits[1..]) } else { String::new() };
        let exp_sign = if exponent < 0 { '-' } else { '+' };
        format!("{sign}{}{rest}e{exp_sign}{:02}", &digits[..1], exponent.abs())
    }
}

/// Python の repr(bytes)。
fn bytes_repr(bytes: &[u8]) -> String {
    let quote = if bytes.contains(&b'\'') && !bytes.contains(&b'"') { '"' } else { '\'' };
    let mut out = format!("b{quote}");
    for &b in bytes {
        match b {
            b'\\' => out.push_str("\\\\"),
            b'\t' => out.push_str("\\t"),
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            _ if char::from(b) == quote => {
                out.push('\\');
                out.push(quote);
            }
            0x20..=0x7E => out.push(char::from(b)),
            _ => out.push_str(&format!("\\x{b:02x}")),
        }
    }
    out.push(quote);
    out
}

/// Python の repr(1 文字の str)。
fn char_repr(c: char) -> String {
    let quote = if c == '\'' { '"' } else { '\'' };
    let body = match c {
        '\\' => "\\\\".to_string(),
        '\t' => "\\t".to_string(),
        '\n' => "\\n".to_string(),
        '\r' => "\\r".to_string(),
        '\0'..='\x1f' | '\x7f'..='\u{a0}' | '\u{ad}' => format!("\\x{:02x}", c as u32),
        _ => c.to_string(),
    };
    format!("{quote}{body}{quote}")
}

// --- 値を文字列にする関数（exifread の str_utils・makernote の関数） ------------------

fn call(func: Func, values: &Values) -> Result<String> {
    match func {
        Func::MakeString => Ok(make_string(values)),
        Func::EvBias => ev_bias(values),
        Func::SpecialMode => special_mode(values),
        // 値の並び全体には使われない（Canon の位置ごとのタグの 1 つの値にだけ使う）
        Func::ConvertTemp => Err(Failed),
    }
}

/// exifread の make_string: 表示できる文字（32〜255）だけを残し、両端の空白・NUL を取る。
fn make_string(values: &Values) -> String {
    let mut text = String::new();
    if !matches!(values, Values::Ascii(_)) {
        for v in values.items() {
            if let Val::Int(c @ 32..=255) = v {
                text.push(char::from_u32(c as u32).expect("255 以下"));
            }
        }
    }
    if text.is_empty() {
        text = match values {
            Values::List(v) => {
                let joined: String = v.iter().map(|x| x.str()).collect();
                if !joined.is_empty() && joined.chars().all(|c| c == '0') {
                    String::new()
                } else {
                    joined
                }
            }
            other => other.str(),
        };
    }
    text.trim_matches([' ', '\0']).to_string()
}

/// Python の a % b（余りの符号は b と同じ）。
fn py_mod(a: i64, b: i64) -> i64 {
    let r = a % b;
    if r != 0 && (r < 0) != (b < 0) {
        r + b
    } else {
        r
    }
}

/// Nikon の露出補正（exifread の nikon.ev_bias）。
fn ev_bias(values: &Values) -> Result<String> {
    let seq: Vec<i64> = match values {
        Values::Ascii(s) if s.chars().count() < 4 => return Ok(String::new()),
        Values::Ascii(_) => return Err(Failed), // 文字と数を比べて止まる
        other => {
            let items = other.items();
            if items.len() < 4 {
                return Ok(String::new());
            }
            items
                .iter()
                .map(|v| if let Val::Int(i) = v { Ok(*i) } else { Err(Failed) })
                .collect::<Result<_>>()?
        }
    };
    let known = [
        ([252, 1, 6, 0], "-2/3 EV"),
        ([253, 1, 6, 0], "-1/2 EV"),
        ([254, 1, 6, 0], "-1/3 EV"),
        ([0, 1, 6, 0], "0 EV"),
        ([2, 1, 6, 0], "+1/3 EV"),
        ([3, 1, 6, 0], "+1/2 EV"),
        ([4, 1, 6, 0], "+2/3 EV"),
    ];
    if seq.len() == 4 {
        if let Some((_, text)) = known.iter().find(|(k, _)| k[..] == seq[..]) {
            return Ok((*text).into());
        }
    }
    let mut i = seq[0];
    if i == 0 {
        return Ok("0 EV".into());
    }
    let mut text = if i > 127 {
        i = 256 - i;
        "-".to_string()
    } else {
        "+".to_string()
    };
    let step = seq[2];
    if step == 0 {
        return Err(Failed);
    }
    let whole = i as f64 / step as f64;
    i = py_mod(i, step);
    if whole != 0.0 {
        text = format!("{text}{} ", float_repr(whole));
    }
    if i == 0 {
        text.push_str("EV");
    } else {
        text = format!("{text}{} EV", ratio_str(i, step));
    }
    Ok(text)
}

/// Olympus の撮影モード（exifread の olympus.special_mode）。
fn special_mode(values: &Values) -> Result<String> {
    let items = values.items();
    if items.is_empty() {
        return Ok(String::new());
    }
    let mode1 = match items[0].as_key() {
        Some(0) => "Normal",
        Some(2) => "Fast",
        Some(3) => "Panorama",
        _ => "Unknown",
    };
    let mode2 = match items.get(2).ok_or(Failed)?.as_key() {
        Some(0) => "Non-panoramic",
        Some(1) => "Left to right",
        Some(2) => "Right to left",
        Some(3) => "Bottom to top",
        Some(4) => "Top to bottom",
        _ => "Unknown",
    };
    Ok(format!("{mode1} - Sequence {} - {mode2}", percent_d(items[1])?))
}

/// Python の "%d" % value（分数は 0 の方向に切り捨てる）。
fn percent_d(value: Val) -> Result<i64> {
    match value {
        Val::Int(v) => Ok(v),
        Val::Ratio(_, 0) => Err(Failed),
        Val::Ratio(n, d) => Ok(n / d),
        _ => Err(Failed),
    }
}

/// Canon の温度（exifread の canon.convert_temp）。
fn convert_temp(value: Val) -> Result<String> {
    let degrees = match value {
        Val::Int(v) => v - 128,
        Val::Ratio(n, d) if d != 0 => {
            // Fraction(n, d) - 128 を 0 の方向に切り捨てる
            let (n, d) = reduce(n, d);
            (n - 128 * d) / d
        }
        _ => return Err(Failed),
    };
    Ok(format!("{degrees} C"))
}

// --- IFD の読み取り（exifread の ExifHeader） ----------------------------------------

/// 型ごとの値 1 つのバイト数（exifread の FIELD_DEFINITIONS。0 は扱わない型）。
const TYPE_LENGTHS: [usize; 14] = [0, 1, 1, 2, 4, 8, 1, 1, 2, 4, 8, 4, 8, 4];
const BYTE: u16 = 1;
const ASCII: u16 = 2;
const UNDEFINED: u16 = 7;

/// 読んだタグ 1 つ（exifread の IfdTag のうち使うところ）。
#[derive(Clone, Debug)]
struct Tag {
    printable: String,
    field_type: u16,
    values: Values,
}

impl Tag {
    /// exifread が値を読み解いて作る項目（文字列だけを持つ）。
    fn text(printable: String) -> Self {
        Self { printable, field_type: 0, values: Values::List(Vec::new()) }
    }
}

struct Header<'a> {
    data: &'a [u8],
    little: bool,
    /// 位置の基準（exifread の self.offset）
    base: usize,
    /// 読んだタグ（Python の辞書と同じく、同じ名前は最初の位置のまま上書きする）
    tags: Vec<(String, Tag)>,
}

impl Header<'_> {
    /// 基準から offset の場所の length バイト（データの外の分は短くなる）。
    fn bytes(&self, offset: usize, length: usize) -> &[u8] {
        let start = self.base.saturating_add(offset).min(self.data.len());
        let end = start.saturating_add(length).min(self.data.len());
        &self.data[start..end]
    }

    /// exifread の s2n: 数を読む（読み切れなければ 0）。
    fn s2n(&self, offset: usize, length: usize, signed: bool) -> i64 {
        let b = self.bytes(offset, length);
        if b.len() != length {
            return 0;
        }
        let mut v: u64 = 0;
        for i in 0..length {
            let byte = if self.little { b[length - 1 - i] } else { b[i] };
            v = (v << 8) | u64::from(byte);
        }
        if signed && length < 8 && v >> (length * 8 - 1) & 1 == 1 {
            (v as i64) - (1i64 << (length * 8))
        } else {
            v as i64
        }
    }

    fn u(&self, offset: usize, length: usize) -> usize {
        self.s2n(offset, length, false) as usize
    }

    fn get(&self, name: &str) -> Option<&Tag> {
        self.tags.iter().find(|(n, _)| n == name).map(|(_, t)| t)
    }

    fn set(&mut self, name: String, tag: Tag) {
        match self.tags.iter_mut().find(|(n, _)| *n == name) {
            Some(slot) => slot.1 = tag,
            None => self.tags.push((name, tag)),
        }
    }

    fn remove(&mut self, name: &str) {
        self.tags.retain(|(n, _)| n != name);
    }

    /// exifread の dump_ifd。
    fn dump_ifd(&mut self, ifd: usize, ifd_name: &str, table: &[TagDef], relative: bool) -> Result<()> {
        let entries = self.u(ifd, 2);
        for i in 0..entries {
            let entry = ifd + 2 + 12 * i;
            let tag = self.u(entry, 2) as u16;
            let def = table.binary_search_by_key(&tag, |d| d.0).ok().map(|i| table[i]);
            let name = def.map_or_else(|| format!("Tag 0x{tag:04X}"), |d| d.1.to_string());
            self.process_tag(ifd, ifd_name, def.map(|d| d.2), entry, name, relative)?;
        }
        Ok(())
    }

    /// exifread の _process_tag。
    fn process_tag(
        &mut self,
        ifd: usize,
        ifd_name: &str,
        format: Option<Format>,
        entry: usize,
        name: String,
        relative: bool,
    ) -> Result<()> {
        let field_type = self.u(entry + 2, 2) as u16;
        if field_type == 0 || usize::from(field_type) >= TYPE_LENGTHS.len() {
            return Ok(()); // 知らない型は読み飛ばす
        }
        let type_length = TYPE_LENGTHS[usize::from(field_type)];
        let count = self.u(entry + 4, 4);
        let mut offset = entry + 8;
        if count * type_length > 4 {
            offset = self.u(offset, 4);
            if relative {
                // Nikon の新しい形式: MakerNote の中の TIFF ヘッダーが位置の基準
                offset = offset + ifd - 8;
            }
        }
        let values = if field_type == ASCII {
            self.ascii(count, offset)
        } else {
            self.field(count, field_type, offset)
        };
        let printable = printable(count, &values, field_type, format)?;
        let tag = Tag { printable, field_type, values };
        self.set(format!("{ifd_name} {name}"), tag);
        Ok(())
    }

    fn ascii(&self, count: usize, offset: usize) -> Values {
        if count == 0 {
            return Values::Ascii(String::new());
        }
        let raw = self.bytes(offset, count);
        let raw = raw.split(|&b| b == 0).next().unwrap_or_default();
        match std::str::from_utf8(raw) {
            Ok(s) => Values::Ascii(s.to_string()),
            Err(_) => Values::Bytes(raw.to_vec()),
        }
    }

    /// exifread の _process_field（数の並び。1000 個以上なら読まない）。
    fn field(&self, count: usize, field_type: u16, mut offset: usize) -> Values {
        let signed = matches!(field_type, 6 | 8 | 9 | 10);
        let length = TYPE_LENGTHS[usize::from(field_type)];
        let mut values = Vec::new();
        if count < 1000 {
            for _ in 0..count {
                match field_type {
                    5 | 10 => {
                        values.push(Val::Ratio(self.s2n(offset, 4, signed), self.s2n(offset + 4, 4, signed)));
                    }
                    11 | 12 => {
                        let b = self.bytes(offset, length);
                        if b.len() == length {
                            let mut raw = [0u8; 8];
                            raw[..length].copy_from_slice(b);
                            if self.little == cfg!(target_endian = "big") {
                                raw[..length].reverse();
                            }
                            let v = if length == 4 {
                                f64::from(f32::from_ne_bytes(raw[..4].try_into().expect("4 バイト")))
                            } else {
                                f64::from_ne_bytes(raw)
                            };
                            values.push(Val::Float(v));
                        } // 読み切れない値は飛ばす（exifread も警告だけで飛ばす）
                    }
                    _ => values.push(Val::Int(self.s2n(offset, length, signed))),
                }
                offset += length;
            }
        }
        Values::List(values)
    }
}

/// exifread の _get_printable_for_field。
fn printable(count: usize, values: &Values, field_type: u16, format: Option<Format>) -> Result<String> {
    let mut text = if count == 1 && field_type != ASCII {
        match values {
            Values::List(v) => v.first().ok_or(Failed)?.str(),
            other => other.str(),
        }
    } else {
        match values {
            Values::List(v) if count > 50 && v.len() > 20 => {
                let head = list_repr(&v[..20]);
                format!("{}, ... ]", &head[..head.len() - 1])
            }
            Values::Bytes(b) if count > 50 && b.len() > 20 => {
                let head = bytes_repr(&b[..20]);
                format!("{}, ... ]", &head[..head.len() - 1])
            }
            other => other.str(),
        }
    };
    match format {
        None | Some(Format::Plain) => {}
        Some(Format::Func(func)) => text = call(func, values)?,
        Some(Format::Map(map)) => {
            text = values
                .items()
                .into_iter()
                .map(|v| lookup(map, v).map_or_else(|| v.repr(), str::to_string))
                .collect();
        }
    }
    Ok(text)
}

fn lookup(map: &[(i64, &'static str)], value: Val) -> Option<&'static str> {
    let key = value.as_key()?;
    map.binary_search_by_key(&key, |m| m.0).ok().map(|i| map[i].1)
}

// --- MakerNote の形式ごとの読み方（exifread の decode_maker_note） --------------------

/// (Image Make の値, MakerNote の項目) を返す。exifread が MakerNote を読まなければ None。
fn decode(tiff: &[u8]) -> Result<Option<Decoded>> {
    let mut h = Header { data: tiff, little: tiff.first() == Some(&b'I'), base: 0, tags: Vec::new() };
    // 先頭の IFD（"Image"）から、メーカー・機種・EXIF の IFD の位置を読む
    let ifd0 = h.u(4, 4);
    h.dump_ifd(ifd0, "Image", &[(0x010F, "Make", Format::Plain), (0x0110, "Model", Format::Plain)], false)?;
    h.tags.retain(|(n, _)| n == "Image Make" || n == "Image Model" || n == "Image Tag 0x8769");
    let Some(exif_ifd) = h.get("Image Tag 0x8769").map(|t| t.values.items()) else { return Ok(None) };
    let exif_ifd = match exif_ifd.first() {
        Some(Val::Int(v)) => *v as usize,
        _ => return Ok(None),
    };
    // EXIF の IFD から MakerNote（0x927C）を探す（値は読まず、位置と型だけを見る）
    let mut note = None;
    for i in 0..h.u(exif_ifd, 2) {
        let entry = exif_ifd + 2 + 12 * i;
        if h.u(entry, 2) != 0x927C {
            continue;
        }
        let field_type = h.u(entry + 2, 2) as u16;
        if field_type == 0 || usize::from(field_type) >= TYPE_LENGTHS.len() {
            continue;
        }
        let length = TYPE_LENGTHS[usize::from(field_type)];
        let count = h.u(entry + 4, 4);
        let at = if count * length > 4 { h.u(entry + 8, 4) } else { entry + 8 };
        note = Some((at, field_type, count));
    }
    let (Some((note_at, note_type, note_count)), Some(make)) =
        (note, h.get("Image Make").map(|t| t.printable.clone()))
    else {
        return Ok(None);
    };
    // MakerNote の先頭の値（目印を見るのに使う）
    let head: Vec<i64> = if matches!(note_type, BYTE | UNDEFINED) {
        h.bytes(note_at, note_count.min(14)).iter().map(|&b| i64::from(b)).collect()
    } else {
        Vec::new()
    };
    let starts =
        |mark: &[u8]| head.len() >= mark.len() && head.iter().zip(mark).all(|(&a, &b)| a == i64::from(b));

    let mut maker = Header { data: tiff, little: h.little, base: 0, tags: Vec::new() };
    if make.contains("NIKON") {
        if starts(b"Nikon\0\x01") {
            maker.dump_ifd(note_at + 8, "MakerNote", tables::NIKON_OLD, false)?;
        } else if starts(b"Nikon\0\x02") {
            if head.get(12..14) != Some(&[0, 42]) && head.get(12..14) != Some(&[42, 0]) {
                return Ok(None); // exifread は ValueError で MakerNote を読むのをやめる
            }
            maker.dump_ifd(note_at + 18, "MakerNote", tables::NIKON_NEW, true)?;
        } else {
            maker.dump_ifd(note_at, "MakerNote", tables::NIKON_NEW, false)?;
        }
    } else if make.starts_with("OLYMPUS") {
        maker.dump_ifd(note_at + 8, "MakerNote", tables::OLYMPUS, false)?;
    } else if make.contains("CASIO") || make.contains("Casio") {
        maker.dump_ifd(note_at, "MakerNote", tables::CASIO, false)?;
    } else if make.contains("SONY") {
        maker.dump_ifd(note_at, "MakerNote", tables::SONY, false)?;
    } else if make == "FUJIFILM" {
        // バイト順はいつもリトルエンディアン、位置は MakerNote の先頭が基準（IFD は 12 バイト目から）
        maker.little = true;
        maker.base = note_at;
        maker.dump_ifd(12, "MakerNote", tables::FUJIFILM, false)?;
    } else if make == "Apple" && starts(b"Apple iOS\0") {
        maker.base = note_at + 14;
        maker.dump_ifd(0, "MakerNote", tables::APPLE, false)?;
    } else if make == "DJI" {
        maker.little = true;
        maker.base = note_at;
        maker.dump_ifd(0, "MakerNote", tables::DJI, false)?;
    } else if make == "Canon" {
        maker.dump_ifd(note_at, "MakerNote", tables::CANON, false)?;
        for &(tag, defs) in tables::CANON_OFFSET_TAGS {
            let name = format!("MakerNote Tag 0x{tag:04X}");
            if let Some(found) = maker.get(&name).cloned() {
                canon_decode_tag(&mut maker, &found.values, defs)?;
                maker.remove(&name);
            }
        }
        let name = "MakerNote Tag 0x000D";
        if let Some(found) = maker.get(name).cloned() {
            let model = h.get("Image Model").map(|t| t.printable.clone());
            canon_camera_info(&mut maker, &found, model.as_deref());
            maker.remove(name);
        }
    } else {
        return Ok(None);
    }
    let tags = maker
        .tags
        .into_iter()
        .map(|(name, tag)| (name.trim_start_matches("MakerNote ").to_string(), tag.printable))
        .collect();
    Ok(Some((make, tags)))
}

/// Canon の、値の位置ごとに意味のあるタグを項目に分ける（exifread の _canon_decode_tag）。
fn canon_decode_tag(h: &mut Header, values: &Values, defs: &[TagDef]) -> Result<()> {
    let items = values.items();
    for (index, &value) in items.iter().enumerate().skip(1) {
        let def = u16::try_from(index)
            .ok()
            .and_then(|i| defs.binary_search_by_key(&i, |d| d.0).ok())
            .map(|i| defs[i]);
        let (name, format) = def.map_or(("Unknown", Format::Plain), |d| (d.1, d.2));
        let text = match format {
            Format::Plain => value.str(),
            Format::Map(map) => lookup(map, value).unwrap_or("Unknown").to_string(),
            Format::Func(Func::ConvertTemp) => convert_temp(value)?,
            Format::Func(_) => return Err(Failed),
        };
        h.set(format!("MakerNote {name}"), Tag::text(text));
    }
    Ok(())
}

/// Canon の CameraInfo（機種ごとに決まった位置の値）を読む（exifread の _canon_decode_camera_info）。
fn canon_camera_info(h: &mut Header, info: &Tag, model: Option<&str>) {
    /// (位置, 名前, バイト数, 値を文字列にする関数)
    type Layout = &'static [(usize, &'static str, usize, fn(i64) -> String)];
    fn temp(v: i64) -> String {
        format!("{} C", v - 128)
    }
    fn plus(v: i64) -> String {
        (v + 1).to_string()
    }
    fn minus(v: i64) -> String {
        (v - 1).to_string()
    }
    let Some(model) = model else { return };
    let layout: Layout = if ends_with_line(model, "EOS 5D") {
        &[(23, "CameraTemperature", 1, temp), (204, "DirectoryIndex", 4, minus), (208, "FileIndex", 2, plus)]
    } else if ends_with_line(model, "EOS 5D Mark II") {
        &[(25, "CameraTemperature", 1, temp), (443, "FileIndex", 4, plus), (455, "DirectoryIndex", 4, minus)]
    } else if ends_with_line(model, "EOS 5D Mark III") {
        &[
            (27, "CameraTemperature", 1, temp),
            (652, "FileIndex", 4, plus),
            (656, "FileIndex2", 4, plus),
            (664, "DirectoryIndex", 4, minus),
            (668, "DirectoryIndex2", 4, minus),
        ]
    } else if ["600D", "REBEL T3i", "Kiss X5"].iter().any(|w| has_word(model, w)) {
        &[(25, "CameraTemperature", 1, temp), (475, "FileIndex", 4, plus), (487, "DirectoryIndex", 4, minus)]
    } else {
        return;
    };
    if !matches!(info.field_type, BYTE | UNDEFINED) {
        return;
    }
    let bytes: Vec<u8> =
        info.values.items().iter().map(|v| if let Val::Int(b) = v { *b as u8 } else { 0 }).collect();
    for &(offset, name, size, func) in layout {
        let Some(raw) = bytes.get(offset..offset + size) else { continue };
        let value = raw.iter().rev().fold(0i64, |acc, &b| (acc << 8) | i64::from(b)); // リトルエンディアン
        h.set(format!("MakerNote {name}"), Tag::text(func(value)));
    }
}

/// 正規表現の "word$"（末尾か、末尾の改行の直前で終わる）。
fn ends_with_line(text: &str, word: &str) -> bool {
    text.ends_with(word) || text.strip_suffix('\n').is_some_and(|t| t.ends_with(word))
}

/// 正規表現の "\bword\b"（前後が英数字・_ でない場所にある）。
fn has_word(text: &str, word: &str) -> bool {
    let is_word = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_');
    text.match_indices(word).any(|(i, _)| {
        !is_word(text[..i].chars().next_back()) && !is_word(text[i + word.len()..].chars().next())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn python_reprs() {
        assert_eq!(float_repr(1.5), "1.5");
        assert_eq!(float_repr(2.0), "2.0");
        assert_eq!(float_repr(-0.0), "-0.0");
        assert_eq!(float_repr(1e16), "1e+16");
        assert_eq!(float_repr(1.5e-5), "1.5e-05");
        assert_eq!(float_repr(0.0001), "0.0001");
        assert_eq!(float_repr(123456.75), "123456.75");
        assert_eq!(float_repr(f64::from(0.1f32)), "0.10000000149011612");
        assert_eq!(float_repr(4.0 / 3.0), "1.3333333333333333");
        assert_eq!(ratio_str(4, 8), "1/2");
        assert_eq!(ratio_str(3, -6), "-1/2");
        assert_eq!(ratio_str(6, 3), "2");
        assert_eq!(ratio_str(5, 0), "5/0");
        assert_eq!(ratio_str(0, 7), "0");
        assert_eq!(bytes_repr(b"a'b\x80\\"), "b\"a'b\\x80\\\\\"");
        assert_eq!(char_repr('a'), "'a'");
        assert_eq!(char_repr('\''), "\"'\"");
    }

    #[test]
    fn helpers() {
        let ints = |v: &[i64]| Values::List(v.iter().map(|&i| Val::Int(i)).collect());
        assert_eq!(ev_bias(&ints(&[252, 1, 6, 0])).unwrap(), "-2/3 EV");
        assert_eq!(ev_bias(&ints(&[6, 1, 6, 0])).unwrap(), "+1.0 EV");
        assert_eq!(ev_bias(&ints(&[8, 1, 6, 0])).unwrap(), "+1.3333333333333333 1/3 EV");
        assert_eq!(ev_bias(&ints(&[250, 1, 6, 0])).unwrap(), "-1.0 EV");
        assert_eq!(ev_bias(&ints(&[1, 1])).unwrap(), "");
        assert_eq!(make_string(&ints(&[0x30, 0x31, 0x30, 0x30])), "0100");
        assert_eq!(make_string(&ints(&[0, 0])), "");
        assert_eq!(make_string(&ints(&[1, 2])), "12");
        assert_eq!(special_mode(&ints(&[3, 2, 1])).unwrap(), "Panorama - Sequence 2 - Left to right");
        assert!(special_mode(&ints(&[3, 2])).is_err());
        assert!(has_word("Canon EOS 600D", "600D") && !has_word("Canon EOS 1600D", "600D"));
        assert!(
            ends_with_line("Canon EOS 5D", "EOS 5D") && !ends_with_line("Canon EOS 5D Mark II", "EOS 5D")
        );
    }
}
