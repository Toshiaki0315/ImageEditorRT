//! EXIF の TIFF ブロック（IFD の並び）の読み書き。Python 版の core/tiff.py を移したもの。
//!
//! 値は 4 バイト以下なら IFD の中に、それより大きければ「TIFF の先頭からの位置」で別の場所に
//! 書く。MakerNote の中にもこの位置で値を指すものがあるので、保存するときは MakerNote を
//! 元と同じ位置に置き直す（[`ExifBlock::to_bytes`]）。

use std::collections::BTreeMap;

pub const EXIF_HEADER: &[u8] = b"Exif\0\0";
pub const MAX_IFD_ENTRIES: u16 = 1000;
pub const TAG_ORIENTATION: u16 = 0x0112;
pub const TAG_EXIF_IFD: u16 = 0x8769;
pub const TAG_GPS_IFD: u16 = 0x8825;
pub const TAG_INTEROP_IFD: u16 = 0xA005;
pub const TAG_MAKERNOTE: u16 = 0x927C;
pub const TAG_PIXEL_X: u16 = 0xA002;
pub const TAG_PIXEL_Y: u16 = 0xA003;
/// 画像データなどの位置を指すタグ。保存する画像のデータとは合わないので書き写さない
const LOCATION_TAGS: [u16; 7] = [0x0111, 0x0117, 0x0144, 0x0145, 0x014A, 0x0201, 0x0202];
const SHORT: u16 = 3;
const LONG: u16 = 4;
const UNDEFINED: u16 = 7;

/// バイト順。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Order {
    Little,
    Big,
}

impl Order {
    pub fn from_mark(mark: &[u8]) -> Option<Self> {
        match mark {
            b"II" => Some(Order::Little),
            b"MM" => Some(Order::Big),
            _ => None,
        }
    }

    pub fn mark(self) -> &'static [u8; 2] {
        match self {
            Order::Little => b"II",
            Order::Big => b"MM",
        }
    }

    pub fn u16(self, b: &[u8]) -> u16 {
        let b = [b[0], b[1]];
        match self {
            Order::Little => u16::from_le_bytes(b),
            Order::Big => u16::from_be_bytes(b),
        }
    }

    pub fn u32(self, b: &[u8]) -> u32 {
        let b = [b[0], b[1], b[2], b[3]];
        match self {
            Order::Little => u32::from_le_bytes(b),
            Order::Big => u32::from_be_bytes(b),
        }
    }

    pub fn put_u16(self, v: u16) -> [u8; 2] {
        match self {
            Order::Little => v.to_le_bytes(),
            Order::Big => v.to_be_bytes(),
        }
    }

    pub fn put_u32(self, v: u32) -> [u8; 4] {
        match self {
            Order::Little => v.to_le_bytes(),
            Order::Big => v.to_be_bytes(),
        }
    }
}

/// TIFF の型ごとの 1 個の大きさ（バイト）。知らない型なら None。
pub fn type_size(kind: u16) -> Option<usize> {
    match kind {
        1 | 2 | 6 | 7 => Some(1),
        3 | 8 => Some(2),
        4 | 9 | 11 | 13 => Some(4),
        5 | 10 | 12 => Some(8),
        _ => None,
    }
}

/// IFD の 1 項目。raw は値そのもの（4 バイト以下）か、値の位置（4 バイト）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IfdEntry {
    pub tag: u16,
    pub kind: u16,
    pub count: u32,
    pub raw: [u8; 4],
}

impl IfdEntry {
    /// 値のバイト列（base は位置の基準）。データの外なら None。
    pub fn value<'a>(&'a self, data: &'a [u8], order: Order, base: usize) -> Option<&'a [u8]> {
        let size = type_size(self.kind)?.checked_mul(self.count as usize)?;
        if size <= 4 {
            return Some(&self.raw[..size]);
        }
        let start = base.checked_add(order.u32(&self.raw) as usize)?;
        data.get(start..start.checked_add(size)?)
    }

    pub fn pointer(&self, order: Order) -> usize {
        order.u32(&self.raw) as usize
    }
}

/// EXIF のバイト列から TIFF の部分を返す（先頭の "Exif\0\0" は取り除く）。
pub fn tiff_block(raw: &[u8]) -> Option<&[u8]> {
    let data = raw.strip_prefix(EXIF_HEADER).unwrap_or(raw);
    (data.len() >= 8 && (data.starts_with(b"II*\0") || data.starts_with(b"MM\0*"))).then_some(data)
}

/// offset の IFD の項目を返す。IFD として正しくなさそうなら None。
pub fn read_ifd(data: &[u8], offset: usize, order: Order) -> Option<Vec<IfdEntry>> {
    let count = order.u16(data.get(offset..offset + 2)?);
    if count == 0 || count > MAX_IFD_ENTRIES {
        return None;
    }
    let end = offset + 2 + 12 * count as usize;
    if end > data.len() {
        return None;
    }
    let mut entries = Vec::with_capacity(count as usize);
    for i in 0..count as usize {
        let e = &data[offset + 2 + 12 * i..offset + 14 + 12 * i];
        let kind = order.u16(&e[2..4]);
        if type_size(kind).is_none() {
            if i == 0 {
                return None; // 最初の項目から型が変なら IFD ではない
            }
            continue;
        }
        entries.push(IfdEntry {
            tag: order.u16(&e[0..2]),
            kind,
            count: order.u32(&e[4..8]),
            raw: [e[8], e[9], e[10], e[11]],
        });
    }
    (!entries.is_empty()).then_some(entries)
}

/// 書き出す値（data はその EXIF のバイト順で並べたもの）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Value {
    pub kind: u16,
    pub count: u32,
    pub data: Vec<u8>,
}

type Ifd = BTreeMap<u16, Value>;

/// EXIF の IFD0・Exif・GPS・互換性の IFD の値と、MakerNote（元の位置ごと）。
#[derive(Clone, Debug)]
pub struct ExifBlock {
    pub order: Order,
    pub ifd0: Ifd,
    pub exif: Ifd,
    pub gps: Option<Ifd>,
    pub interop: Option<Ifd>,
    pub maker_note: Option<(usize, Vec<u8>)>,
}

impl ExifBlock {
    /// EXIF のバイト列（"Exif\0\0" 付きでもよい）を読む。読めなければ None。
    pub fn parse(raw: &[u8]) -> Option<Self> {
        let data = tiff_block(raw)?;
        let order = Order::from_mark(&data[..2])?;
        let ifd0 = read_ifd(data, order.u32(&data[4..8]) as usize, order)?;
        let mut block = ExifBlock {
            order,
            ifd0: values(data, &ifd0, order),
            exif: Ifd::new(),
            gps: None,
            interop: None,
            maker_note: None,
        };
        let find = |entries: &[IfdEntry], tag: u16| entries.iter().find(|e| e.tag == tag).cloned();
        if let Some(pointer) = find(&ifd0, TAG_EXIF_IFD) {
            let exif = read_ifd(data, pointer.pointer(order), order).unwrap_or_default();
            block.exif = values(data, &exif, order);
            if let Some(note) = find(&exif, TAG_MAKERNOTE).filter(|n| n.count > 4) {
                let at = note.pointer(order);
                let end = at + note.count as usize;
                if at >= 8 && end <= data.len() {
                    block.maker_note = Some((at, data[at..end].to_vec()));
                }
            }
            if let Some(pointer) = find(&exif, TAG_INTEROP_IFD) {
                block.interop =
                    read_ifd(data, pointer.pointer(order), order).map(|e| values(data, &e, order));
            }
        }
        if let Some(pointer) = find(&ifd0, TAG_GPS_IFD) {
            block.gps = read_ifd(data, pointer.pointer(order), order).map(|e| values(data, &e, order));
        }
        Some(block)
    }

    pub fn set_short(&mut self, in_exif: bool, tag: u16, value: u16) {
        let data = self.order.put_u16(value).to_vec();
        let ifd = if in_exif { &mut self.exif } else { &mut self.ifd0 };
        ifd.insert(tag, Value { kind: SHORT, count: 1, data });
    }

    pub fn set_long(&mut self, in_exif: bool, tag: u16, value: u32) {
        let data = self.order.put_u32(value).to_vec();
        let ifd = if in_exif { &mut self.exif } else { &mut self.ifd0 };
        ifd.insert(tag, Value { kind: LONG, count: 1, data });
    }

    /// "Exif\0\0" 付きの EXIF にする。MakerNote は元と同じ位置に置き、ほかの IFD はその後ろに並べる。
    pub fn to_bytes(&self, keep_maker_note: bool) -> Vec<u8> {
        let order = self.order;
        let maker_note = self.maker_note.as_ref().filter(|_| keep_maker_note);
        let start = maker_note.map_or(8, |(at, note)| even(at + note.len()));
        let placeholder = Value { kind: LONG, count: 1, data: vec![0; 4] };

        let mut ifd0 = self.ifd0.clone();
        let mut exif = self.exif.clone();
        if !exif.is_empty() || maker_note.is_some() {
            ifd0.insert(TAG_EXIF_IFD, placeholder.clone());
        }
        if self.gps.is_some() {
            ifd0.insert(TAG_GPS_IFD, placeholder.clone());
        }
        if self.interop.is_some() {
            exif.insert(TAG_INTEROP_IFD, placeholder.clone());
        }
        if maker_note.is_some() {
            exif.insert(TAG_MAKERNOTE, placeholder.clone());
        }
        let has_exif = ifd0.contains_key(&TAG_EXIF_IFD);
        // 先に IFD の位置を決める（ほかの IFD を指す項目は 4 バイトなので大きさは変わらない）
        let mut position = start;
        let ifd0_at = position;
        position = even(position + ifd_size(&ifd0));
        let exif_at = position;
        if has_exif {
            position = even(position + ifd_size(&exif));
        }
        let interop_at = position;
        if let Some(interop) = &self.interop {
            position = even(position + ifd_size(interop));
        }
        let gps_at = position;
        if let Some(gps) = &self.gps {
            position = even(position + ifd_size(gps));
        }
        let at = |p: usize| Value { kind: LONG, count: 1, data: order.put_u32(p as u32).to_vec() };
        if has_exif {
            ifd0.insert(TAG_EXIF_IFD, at(exif_at));
        }
        if self.gps.is_some() {
            ifd0.insert(TAG_GPS_IFD, at(gps_at));
        }
        if self.interop.is_some() {
            exif.insert(TAG_INTEROP_IFD, at(interop_at));
        }
        if let Some((note_at, note)) = maker_note {
            exif.insert(
                TAG_MAKERNOTE,
                Value {
                    kind: UNDEFINED,
                    count: note.len() as u32,
                    data: order.put_u32(*note_at as u32).to_vec(),
                },
            );
        }

        let mut out = vec![0u8; position];
        out[..2].copy_from_slice(order.mark());
        out[2..4].copy_from_slice(&order.put_u16(42));
        out[4..8].copy_from_slice(&order.put_u32(start as u32));
        if let Some((note_at, note)) = maker_note {
            out[*note_at..note_at + note.len()].copy_from_slice(note);
        }
        let mut write = |ifd: &Ifd, at: usize| {
            let bytes = write_ifd(ifd, at, order);
            out[at..at + bytes.len()].copy_from_slice(&bytes);
        };
        write(&ifd0, ifd0_at);
        if has_exif {
            write(&exif, exif_at);
        }
        if let Some(interop) = &self.interop {
            write(interop, interop_at);
        }
        if let Some(gps) = &self.gps {
            write(gps, gps_at);
        }
        [EXIF_HEADER, &out].concat()
    }
}

fn values(data: &[u8], entries: &[IfdEntry], order: Order) -> Ifd {
    let skipped = [TAG_EXIF_IFD, TAG_GPS_IFD, TAG_INTEROP_IFD, TAG_MAKERNOTE];
    entries
        .iter()
        .filter(|e| !skipped.contains(&e.tag) && !LOCATION_TAGS.contains(&e.tag))
        .filter_map(|e| {
            let value = e.value(data, order, 0)?;
            Some((e.tag, Value { kind: e.kind, count: e.count, data: value.to_vec() }))
        })
        .collect()
}

fn ifd_size(ifd: &Ifd) -> usize {
    let data: usize = ifd.values().filter(|v| v.data.len() > 4).map(|v| even(v.data.len())).sum();
    2 + 12 * ifd.len() + 4 + data
}

fn write_ifd(ifd: &Ifd, position: usize, order: Order) -> Vec<u8> {
    let data_at = position + 2 + 12 * ifd.len() + 4;
    let mut head = order.put_u16(ifd.len() as u16).to_vec();
    let mut data = Vec::new();
    for (&tag, value) in ifd {
        // BTreeMap なのでタグ番号の小さい順（TIFF の決まり）
        head.extend_from_slice(&order.put_u16(tag));
        head.extend_from_slice(&order.put_u16(value.kind));
        head.extend_from_slice(&order.put_u32(value.count));
        if value.data.len() <= 4 {
            let mut field = [0u8; 4];
            field[..value.data.len()].copy_from_slice(&value.data);
            head.extend_from_slice(&field);
        } else {
            head.extend_from_slice(&order.put_u32((data_at + data.len()) as u32));
            data.extend_from_slice(&value.data);
            data.resize(even(data.len()), 0);
        }
    }
    head.extend_from_slice(&order.put_u32(0)); // 次の IFD（サムネイル）は書かない
    head.extend_from_slice(&data);
    head
}

fn even(value: usize) -> usize {
    value + (value % 2)
}

/// JPEG のバイト列に EXIF（"Exif\0\0" 付き）を APP1 として差し込む（SOI のすぐ後ろ）。
/// 元の APP1 (Exif) があれば取り除く。JPEG でなければ None。
pub fn insert_exif_into_jpeg(jpeg: &[u8], exif: &[u8]) -> Option<Vec<u8>> {
    if !jpeg.starts_with(&[0xFF, 0xD8]) || exif.len() + 2 > u16::MAX as usize {
        return None;
    }
    let mut out = Vec::with_capacity(jpeg.len() + exif.len() + 4);
    out.extend_from_slice(&[0xFF, 0xD8, 0xFF, 0xE1]);
    out.extend_from_slice(&((exif.len() + 2) as u16).to_be_bytes());
    out.extend_from_slice(exif);
    // 元の APP1 (Exif) を飛ばして残りを写す
    let mut i = 2;
    while i + 4 <= jpeg.len() && jpeg[i] == 0xFF && (0xE0..=0xEF).contains(&jpeg[i + 1]) {
        let length = u16::from_be_bytes([jpeg[i + 2], jpeg[i + 3]]) as usize;
        let segment = &jpeg[i..(i + 2 + length).min(jpeg.len())];
        if !(jpeg[i + 1] == 0xE1 && segment.get(4..10) == Some(EXIF_HEADER)) {
            out.extend_from_slice(segment);
        }
        i += 2 + length;
    }
    out.extend_from_slice(jpeg.get(i..)?);
    Some(out)
}
