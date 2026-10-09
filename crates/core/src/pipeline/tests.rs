//! 処理の流れ（pipeline）のテスト。

use super::*;
use crate::local::LocalAdjust;
use crate::transform::Orientation;
use image::Rgba;

fn sample() -> RgbaImage {
    RgbaImage::from_fn(120, 80, |x, y| Rgba([(x * 2) as u8, (y * 3) as u8, ((x + y) % 256) as u8, 200]))
}

#[test]
fn default_settings_change_nothing() {
    let image = sample();
    assert_eq!(apply_edits(&image, &EditSettings::default()).unwrap(), image);
    assert_eq!(render_preview(&image, &EditSettings::default(), 1.0, false), image);
}

#[test]
fn heavy_settings_keep_size_and_alpha() {
    let image = sample();
    let out = apply_edits(&image, &EditSettings::heavy()).unwrap();
    assert_eq!(out.dimensions(), image.dimensions());
    assert_ne!(out, image);
    // 透過（アルファ）はすべての処理の後も残る（FR-PRC-03）。文字を描いた画素だけは別
    let same_alpha = out.pixels().zip(image.pixels()).filter(|(a, b)| a[3] == b[3]).count();
    assert!(same_alpha as f64 > 0.95 * f64::from(image.width() * image.height()));
}

#[test]
fn settings_from_json_use_defaults() {
    let s: EditSettings = serde_json::from_str(
        r#"{"exposure": 1.5, "dioramaBlur": 40, "orientation": {"rotation": 90},
            "crop": {"x": 1, "y": 2, "width": 3, "height": 4}, "keepAspect": false, "filter": "hdr"}"#,
    )
    .unwrap();
    assert_eq!(s.exposure, 1.5);
    assert_eq!(s.diorama_blur, 40);
    assert_eq!(s.temperature, 6500);
    assert_eq!(s.orientation, Orientation::new(90, false));
    assert_eq!(s.crop, Some(CropRect::new(1, 2, 3, 4)));
    assert!(!s.keep_aspect);
    assert_eq!(s.filter, FilterType::Hdr);
    assert_eq!(s.width, None);
}

#[test]
fn preview_vignette_stays_inside_crop() {
    let image = RgbaImage::from_pixel(100, 80, Rgba([200, 200, 200, 255]));
    let settings =
        EditSettings { vignette: 100, crop: Some(CropRect::new(20, 20, 40, 30)), ..EditSettings::default() };
    let out = render_preview(&image, &settings, 1.0, false);
    assert_eq!(out.get_pixel(5, 5), image.get_pixel(5, 5)); // 範囲の外は変えない
    assert!(out.get_pixel(20, 20)[0] < 200); // 範囲の角は暗くなる
    assert_eq!(out.get_pixel(40, 35), image.get_pixel(40, 35)); // 範囲の中心はそのまま
}

#[test]
fn trimmed_preview_is_the_crop() {
    let image = sample();
    let settings = EditSettings { crop: Some(CropRect::new(10, 20, 30, 40)), ..EditSettings::default() };
    let out = render_preview(&image, &settings, 0.5, true);
    assert_eq!(out.dimensions(), (15, 20));
    assert_eq!(out.get_pixel(0, 0), image.get_pixel(5, 10));
}

#[test]
fn make_preview_keeps_small_images() {
    let image = sample();
    let (preview, factor) = make_preview(&image, 1600);
    assert_eq!((preview.dimensions(), factor), (image.dimensions(), 1.0));
    let (preview, factor) = make_preview(&image, 60);
    assert_eq!(preview.dimensions(), (60, 40));
    assert_eq!(factor, 0.5);
}

#[test]
fn diorama_guide_follows_crop() {
    // 原寸 400×200 を 0.5 倍にしたプレビュー（200×100）。範囲は原寸で y = 40〜120
    let settings = EditSettings {
        diorama_position: 50,
        diorama_width: 20,
        crop: Some(CropRect::new(0, 40, 400, 80)),
        ..EditSettings::default()
    };
    let guide = diorama_guide((200, 100), &settings, 0.5);
    assert!(guide.horizontal);
    let solid: Vec<f64> = guide.lines.iter().filter(|l| l.1).map(|l| l.0).collect();
    // 範囲（プレビューで y = 20〜60）の 40%〜60% → y = 36〜44 → 高さ 100 に対して 0.36〜0.44
    assert!((solid[0] - 0.36).abs() < 1e-9 && (solid[1] - 0.44).abs() < 1e-9, "{solid:?}");
    // 縦の帯・90° 回転: 幅は回転後のもの
    let vertical = EditSettings {
        diorama_direction: DioramaDirection::Vertical,
        orientation: Orientation::new(90, false),
        ..EditSettings::default()
    };
    let guide = diorama_guide((200, 100), &vertical, 1.0);
    assert!(!guide.horizontal);
    assert!((guide.lines[1].0 - 0.4).abs() < 1e-9);
}

#[test]
fn local_contrast_temperature_and_saturation_work_inside_only() {
    // 左半分は赤みのある灰色、右半分は暗い青。円は画像の真ん中（どちらにもかかる）
    let original = RgbaImage::from_fn(200, 100, |x, _| {
        if x < 100 {
            image::Rgba([170, 120, 110, 255])
        } else {
            image::Rgba([40, 60, 120, 255])
        }
    });
    let circle = |a: LocalAdjust| EditSettings {
        local_adjustments: vec![LocalAdjust {
            shape: local::LocalShape::Ellipse { rect: CropRect::new(50, 0, 100, 100) },
            feather: 0,
            ..a
        }],
        ..EditSettings::default()
    };
    let out = |a: LocalAdjust| apply_edits(&original, &circle(a)).unwrap();
    let (inside_left, inside_right, outside) = ((90, 50), (110, 50), (5, 50));
    let px = |image: &RgbaImage, (x, y): (u32, u32)| image.get_pixel(x, y).0;
    // 色温度（全体と同じ向き）: 下げると円の中だけ暖かく（赤が増え青が減る）、上げると青く
    let warm = out(LocalAdjust { temperature: 3000, ..LocalAdjust::default() });
    let (before, after) = (px(&original, inside_left), px(&warm, inside_left));
    assert!(after[0] > before[0] && after[2] < before[2], "{before:?} → {after:?}");
    assert_eq!(px(&warm, outside), px(&original, outside));
    let cool = px(&out(LocalAdjust { temperature: 9000, ..LocalAdjust::default() }), inside_left);
    assert!(cool[0] < before[0] && cool[2] > before[2], "{before:?} → {cool:?}");
    // 彩度 -100 なら円の中は灰色に
    let gray = out(LocalAdjust { saturation: -100, ..LocalAdjust::default() });
    let p = px(&gray, inside_right);
    assert!(p[0].abs_diff(p[2]) <= 2, "{p:?}");
    assert_eq!(px(&gray, outside), px(&original, outside));
    // コントラストを上げると、明るい左と暗い右の差が円の中で広がる
    let contrast = out(LocalAdjust { contrast: 60, ..LocalAdjust::default() });
    let spread =
        |image: &RgbaImage| i32::from(px(image, inside_left)[1]) - i32::from(px(image, inside_right)[1]);
    assert!(spread(&contrast) > spread(&original), "{} → {}", spread(&original), spread(&contrast));
}

#[test]
fn local_adjustments_land_on_the_same_place_in_preview_and_save() {
    let original = RgbaImage::from_pixel(400, 200, image::Rgba([100, 100, 100, 255]));
    // 右半分を切り抜き、その中の円（原寸 240〜360 × 40〜160）だけ明るくする
    let settings = EditSettings {
        crop: Some(CropRect::new(200, 0, 200, 200)),
        local_adjustments: vec![LocalAdjust {
            shape: local::LocalShape::Ellipse { rect: CropRect::new(240, 40, 120, 120) },
            exposure: 1.0,
            feather: 0,
            ..LocalAdjust::default()
        }],
        ..EditSettings::default()
    };
    // 保存: 切り抜いた画像（200×200）の (100, 100) が円の中心
    let saved = apply_edits(&original, &settings).unwrap();
    assert!(saved.get_pixel(100, 100)[0] > 120 && saved.get_pixel(5, 5)[0] == 100);
    // プレビュー（半分の大きさ・全体表示）: (150, 50) が円の中心、切り抜く範囲の外は変えない
    let small = RgbaImage::from_pixel(200, 100, image::Rgba([100, 100, 100, 255]));
    let preview = render_preview(&small, &settings, 0.5, false);
    assert!(preview.get_pixel(150, 50)[0] > 120 && preview.get_pixel(105, 5)[0] == 100);
    // 加工前の表示には部分補正を入れない
    assert!(before_settings((400, 200), &settings).local_adjustments.is_empty());
}

#[test]
fn long_side_follows_the_cropped_and_rotated_photo() {
    // 横長の原寸を 90° 回すと縦長 → 高さを決める
    let rotated =
        EditSettings { orientation: Orientation::new(90, false), width: Some(10), ..EditSettings::default() };
    let tall = long_side_settings((4000, 3000), &rotated, 1080);
    assert_eq!((tall.width, tall.height, tall.keep_aspect), (None, Some(1080), true));
    assert_eq!(output_size((4000, 3000), &tall).unwrap(), (810, 1080));
    // 横長に切り抜いていれば幅（ほかの設定はそのまま）
    let cropped = EditSettings {
        crop: Some(CropRect::new(0, 0, 3000, 1000)),
        exposure: 0.5,
        ..EditSettings::default()
    };
    let wide = long_side_settings((3000, 4000), &cropped, 1600);
    assert_eq!((wide.width, wide.height, wide.exposure), (Some(1600), None, 0.5));
    assert_eq!(output_size((3000, 4000), &wide).unwrap(), (1600, 533));
}

#[test]
fn before_settings_keep_only_orientation_and_crop() {
    let settings = EditSettings {
        orientation: Orientation::new(90, true),
        crop: Some(CropRect::new(10, 20, 100, 60)),
        frame: FrameType::Polaroid,
        shape: ShapeType::Circle,
        corner_radius: 30,
        filter: FilterType::Sepia,
        exposure: 1.5,
        vignette: 40,
        sharpen: 50,
        diorama_blur: 60,
        width: Some(50),
        text: TextSettings { text: "ABC".into(), ..TextSettings::default() },
        ..EditSettings::default()
    };
    let before = before_settings((300, 200), &settings);
    // フレームの写真部分の比に合わせた範囲を、そのまま切り抜く範囲として残す
    let crop = effective_crop((200, 300), settings.crop, FrameType::Polaroid, ShapeType::Circle);
    assert!(crop.is_some() && crop != settings.crop);
    assert_eq!(before, EditSettings { orientation: settings.orientation, crop, ..EditSettings::default() });
    // 範囲がなくフレーム・形もなければ、向きだけ
    let plain = EditSettings { orientation: Orientation::new(180, false), ..EditSettings::default() };
    assert_eq!(before_settings((300, 200), &plain), plain);
}

#[test]
fn actual_size_guide_is_relative_to_the_saved_photo() {
    let settings = EditSettings {
        diorama_position: 50,
        diorama_width: 20,
        crop: Some(CropRect::new(0, 40, 400, 80)),
        width: Some(200),
        keep_aspect: true,
        ..EditSettings::default()
    };
    // 保存結果は切り抜いた写真そのもの（リサイズしても割合は同じ）
    let guide = actual_size_diorama_guide((400, 200), &settings).unwrap();
    let solid: Vec<f64> = guide.lines.iter().filter(|l| l.1).map(|l| l.0).collect();
    assert!((solid[0] - 0.4).abs() < 1e-9 && (solid[1] - 0.6).abs() < 1e-9, "{solid:?}");
    // フレームがあれば、余白を除いた写真の部分に対する位置
    let framed = EditSettings { frame: FrameType::Polaroid, crop: None, ..settings };
    let photo = {
        let rect = effective_crop((400, 200), None, FrameType::Polaroid, ShapeType::Rectangle).unwrap();
        transform::fit_size((rect.width as u32, rect.height as u32), Some(200), None, true).unwrap()
    };
    let (_, top, _, bottom) = frames::frame_margins(FrameType::Polaroid, photo);
    let total = f64::from(top + photo.1 + bottom);
    let guide = actual_size_diorama_guide((400, 200), &framed).unwrap();
    let expected = (f64::from(top) + 0.4 * f64::from(photo.1)) / total;
    assert!((guide.lines[1].0 - expected).abs() < 1e-9, "{:?} {expected}", guide.lines);
    assert_eq!(output_size((400, 200), &framed).unwrap().1, top + photo.1 + bottom);
}

#[test]
#[cfg(target_os = "macos")]
fn text_goes_into_the_frame_margin() {
    let black = RgbaImage::from_pixel(200, 200, Rgba([0, 0, 0, 255]));
    let text = TextSettings {
        text: "写真".into(),
        position: TextPosition::FrameMargin,
        color: [255, 0, 0],
        opacity: 100,
        size: 10.0,
        ..TextSettings::default()
    };
    let settings = EditSettings { frame: FrameType::Polaroid, text: text.clone(), ..EditSettings::default() };
    let out = apply_edits(&black, &settings).unwrap();
    // ポラロイドの写真の下の余白（白）に赤い文字が入る。写真（黒）の中には入らない
    let (l, t, _, _) = frames::frame_margins(FrameType::Polaroid, (200, 200));
    let red = |p: &Rgba<u8>| p[0] > 200 && p[1] < 80;
    let in_margin = (t + 200..out.height()).flat_map(|y| (0..out.width()).map(move |x| (x, y)));
    assert!(in_margin.filter(|&(x, y)| red(out.get_pixel(x, y))).count() > 50);
    let in_photo = (t..t + 200).flat_map(|y| (l..l + 200).map(move |x| (x, y)));
    assert_eq!(in_photo.filter(|&(x, y)| red(out.get_pixel(x, y))).count(), 0);
    // フレームがなければ写真の下中央に描く
    let plain = apply_edits(&black, &EditSettings { text, ..EditSettings::default() }).unwrap();
    let bottom = (150..200).flat_map(|y| (50..150).map(move |x| (x, y)));
    assert!(bottom.filter(|&(x, y)| red(plain.get_pixel(x, y))).count() > 50);
}

#[test]
fn invalid_size_is_an_error() {
    let settings = EditSettings { width: Some(0), ..EditSettings::default() };
    assert!(apply_edits(&sample(), &settings).is_err());
    assert!(output_size((10, 10), &settings).is_err());
}

#[test]
fn straighten_keeps_the_size_and_is_kept_for_before() {
    let image = sample();
    let settings = EditSettings { straighten: 5.0, ..EditSettings::default() };
    let out = apply_edits(&image, &settings).unwrap();
    assert_eq!(out.dimensions(), image.dimensions());
    assert_ne!(out, image);
    assert_eq!(output_size(image.dimensions(), &settings).unwrap(), image.dimensions());
    // プレビュー（縮小なし）は保存と同じ。加工前の表示でも補正は残す
    assert_eq!(render_preview(&image, &settings, 1.0, false), out);
    assert_eq!(before_settings(image.dimensions(), &settings), settings);
    // 回転・反転の後にかける（90° 回した画像を回す）
    let rotated = EditSettings { orientation: Orientation::new(90, false), ..settings.clone() };
    let expected = transform::straighten(&Orientation::new(90, false).transpose(&image), 5.0);
    assert_eq!(apply_edits(&image, &rotated).unwrap(), expected);
}

#[test]
fn filter_thumbnails_cover_every_taste() {
    let image = sample();
    let settings = EditSettings {
        filter: FilterType::Sepia,
        filter_strength: 30,
        crop: Some(CropRect::new(0, 0, 60, 40)),
        exposure: 0.5,
        ..EditSettings::default()
    };
    // 原寸 240×160 を 0.5 倍にしたプレビュー。切り抜いた写真（原寸 60×40 → プレビュー 30×20）の長辺は大きくしない
    let thumbnails = filter_thumbnails(&image, &settings, 0.5, 240);
    assert_eq!(thumbnails.len(), FilterType::ALL.len());
    assert!(thumbnails.iter().all(|t| t.dimensions() == (30, 20)));
    // 「なし」の見本はテイストなし（強さに関係なく）、セピアは強さ 100% の完成形
    let none = EditSettings { filter: FilterType::None, ..settings.clone() };
    assert_eq!(thumbnails[0], render_preview(&image, &none, 0.5, true));
    let sepia = FilterType::ALL.iter().position(|&f| f == FilterType::Sepia).unwrap();
    let full = EditSettings { filter_strength: 100, ..settings.clone() };
    assert_eq!(thumbnails[sepia], render_preview(&image, &full, 0.5, true));
    // 大きい写真は、切り抜いた写真の長辺が max_side になるまで縮める
    let big = filter_thumbnails(&image, &EditSettings::default(), 1.0, 60);
    assert!(big.iter().all(|t| t.dimensions() == (60, 40)));
}

#[test]
fn regions_are_covered_after_rotation_and_dropped_for_before() {
    use crate::privacy::RegionKind;
    let image = RgbaImage::from_fn(80, 40, |x, y| Rgba([(x * 3) as u8, (y * 5) as u8, 0, 255]));
    // 90° 回した後（40×80）の座標の範囲
    let region = crate::privacy::Region {
        kind: RegionKind::Mosaic,
        rect: CropRect::new(0, 0, 20, 20),
        strength: 100,
        ..crate::privacy::Region::default()
    };
    let settings = EditSettings {
        orientation: Orientation::new(90, false),
        regions: vec![region.clone()],
        ..EditSettings::default()
    };
    let out = apply_edits(&image, &settings).unwrap();
    let mut expected = Orientation::new(90, false).transpose(&image);
    crate::privacy::cover(&mut expected, &[region], 1.0);
    assert_eq!(out, expected);
    assert_ne!(out, Orientation::new(90, false).transpose(&image));
    // プレビューも同じ。加工前の表示では外す
    assert_eq!(render_preview(&image, &settings, 1.0, false), out);
    assert!(before_settings(image.dimensions(), &settings).regions.is_empty());
}

#[test]
#[cfg(target_os = "macos")]
fn stamps_follow_crop_and_resize_and_keep_their_colors() {
    use crate::privacy::{Region, RegionKind};
    let image = RgbaImage::from_pixel(200, 100, Rgba([0, 0, 0, 255]));
    let stamp = Region { kind: RegionKind::Stamp, rect: CropRect::new(100, 20, 40, 40), ..Region::default() };
    // (80, 0) から 100 × 100 を切り抜き、50 × 50 にする → スタンプは (10, 10)〜(30, 30)
    let settings = EditSettings {
        crop: Some(CropRect::new(80, 0, 100, 100)),
        width: Some(50),
        filter: FilterType::Sepia,
        regions: vec![stamp],
        ..EditSettings::default()
    };
    let out = apply_edits(&image, &settings).unwrap();
    let plain = apply_edits(&image, &EditSettings { regions: vec![], ..settings.clone() }).unwrap();
    let changed: Vec<(u32, u32)> = out
        .enumerate_pixels()
        .filter(|(x, y, p)| *p != plain.get_pixel(*x, *y))
        .map(|(x, y, _)| (x, y))
        .collect();
    assert!(!changed.is_empty());
    assert!(changed.iter().all(|&(x, y)| (10..30).contains(&x) && (10..30).contains(&y)), "{changed:?}");
    // テイスト（セピア）の後に描くので、絵文字の色（黄色の顔）が残る
    assert!(out.pixels().any(|p| p[0] > 200 && p[1] > 150 && p[2] < 80));
    // 切り抜いたプレビュー（0.5 倍）でも同じ位置
    let preview = make_preview(&image, 100).0;
    let shown = render_preview(&preview, &settings, 0.5, true);
    let blank = render_preview(&preview, &EditSettings { regions: vec![], ..settings.clone() }, 0.5, true);
    assert_eq!(shown.dimensions(), (50, 50));
    assert!(shown
        .enumerate_pixels()
        .filter(|(x, y, p)| *p != blank.get_pixel(*x, *y))
        .all(|(x, y, _)| (10..30).contains(&x) && (10..30).contains(&y)));
}

#[test]
fn photo_for_analysis_is_the_cropped_photo() {
    let image = sample();
    let settings =
        EditSettings { crop: Some(CropRect::new(20, 10, 60, 40)), exposure: 2.0, ..EditSettings::default() };
    // プレビューは原寸の半分。範囲も半分にして切り抜き、色の調整はかけない
    let photo = photo_for_analysis(&image, &settings, 0.5);
    assert_eq!(photo.dimensions(), (30, 20));
    assert_eq!(photo.get_pixel(0, 0), image.get_pixel(10, 5));
}

#[test]
fn tone_curve_and_hsl_are_applied() {
    let image = RgbaImage::from_fn(64, 8, |x, y| Rgba([(x * 4) as u8, (y * 30) as u8, 200, 255]));
    let curved = EditSettings { tone_curve: vec![[0, 0], [128, 190], [255, 255]], ..EditSettings::default() };
    let out = apply_edits(&image, &curved).unwrap();
    let row = curve::curve_row(&curved.tone_curve);
    assert_eq!(out.get_pixel(32, 0)[0], row[128]);
    let mut hsl = [HslAdjust::default(); HSL_BANDS];
    hsl[5].saturation = -100;
    let gray = apply_edits(&image, &EditSettings { hsl, ..EditSettings::default() }).unwrap();
    assert_ne!(gray, image);
    // 既定値なら何も変えない（JSON の既定値も同じ）
    let s: EditSettings = serde_json::from_str("{}").unwrap();
    assert_eq!((s.tone_curve, s.hsl), (curve::identity_curve(), [HslAdjust::default(); HSL_BANDS]));
}

#[test]
fn logo_is_drawn_on_the_photo_and_in_the_frame_margin() {
    let dir = std::env::temp_dir().join(format!("imageeditorrt-pipeline-logo-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("logo.png");
    RgbaImage::from_pixel(20, 20, Rgba([255, 0, 0, 255])).save(&path).unwrap();
    let logo = LogoSettings {
        path: path.to_string_lossy().into_owned(),
        size: 20.0,
        opacity: 100,
        ..LogoSettings::default()
    };
    let black = RgbaImage::from_pixel(200, 200, Rgba([0, 0, 0, 255]));
    let red = |image: &RgbaImage| image.pixels().filter(|p| p[0] > 200 && p[1] < 50).count();
    // 写真の右下に 40 × 40
    let on_photo =
        apply_edits(&black, &EditSettings { logo: logo.clone(), ..EditSettings::default() }).unwrap();
    assert_eq!(red(&on_photo), 1600);
    assert_eq!(
        render_preview(&black, &EditSettings { logo: logo.clone(), ..EditSettings::default() }, 1.0, false),
        on_photo
    );
    // フレームの余白: 写真（黒）には入らない
    let margin = EditSettings {
        frame: FrameType::Polaroid,
        logo: LogoSettings { position: TextPosition::FrameMargin, ..logo },
        ..EditSettings::default()
    };
    let framed = apply_edits(&black, &margin).unwrap();
    let (l, t, _, _) = frames::frame_margins(FrameType::Polaroid, (200, 200));
    let in_photo = (t..t + 200).flat_map(|y| (l..l + 200).map(move |x| (x, y)));
    assert_eq!(in_photo.filter(|&(x, y)| framed.get_pixel(x, y)[0] > 200).count(), 0);
    assert!(red(&framed) > 100);
    std::fs::remove_dir_all(&dir).unwrap();
}
