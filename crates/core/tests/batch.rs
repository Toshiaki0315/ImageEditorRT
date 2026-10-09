//! 一括処理（core/batch.rs）: 保存先の名前の決め方・画像の集め方・失敗と中止の扱い。

use std::path::{Path, PathBuf};

use image::{Rgba, RgbaImage};
use imageeditorrt_core::batch::{
    batch_settings, collect_images, output_path, run_batch, summary, BatchOptions, BatchPrivacy,
    OutputFormat, OutputNaming,
};
use imageeditorrt_core::frames::FrameType;
use imageeditorrt_core::pipeline::EditSettings;
use imageeditorrt_core::presets::Preset;
use imageeditorrt_core::save::SaveOptions;
use imageeditorrt_core::shapes::ShapeType;

/// テストごとの空のフォルダ。
fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("imageeditorrt-batch-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn touch(path: &Path) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, b"x").unwrap();
}

fn options(look: Preset, long_side: Option<u32>) -> BatchOptions {
    BatchOptions {
        look,
        long_side,
        save: SaveOptions::default(),
        privacy: BatchPrivacy::default(),
        naming: OutputNaming::default(),
        format: OutputFormat::default(),
    }
}

fn plain() -> Preset {
    Preset::from_settings("今の加工", &EditSettings::default())
}

#[test]
fn output_names() {
    let dir = temp_dir("names");
    let source = dir.join("in/photo.JPG");
    touch(&source);
    let out = dir.join("out");
    // 元と同じ名前・拡張子（つづりもそのまま）
    assert_eq!(output_path(&source, &out), out.join("photo.JPG"));
    // 同じ名前があれば _edited、_edited_2 …
    touch(&out.join("photo.JPG"));
    assert_eq!(output_path(&source, &out), out.join("photo_edited.JPG"));
    touch(&out.join("photo_edited.JPG"));
    assert_eq!(output_path(&source, &out), out.join("photo_edited_2.JPG"));
    // 保存できない形式（HEIC）は .jpg
    // HEIC は HEIC のまま、RAW は JPEG
    assert_eq!(output_path(&dir.join("in/live.heic"), &out), out.join("live.heic"));
    assert_eq!(output_path(&dir.join("in/raw.CR3"), &out), out.join("raw.jpg"));
    // 元と同じフォルダに保存しても、元のファイルには書かない
    assert_eq!(output_path(&source, &dir.join("in")), dir.join("in/photo_edited.JPG"));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn collects_images_once_in_name_order() {
    let dir = temp_dir("collect");
    let folder = dir.join("folder");
    for name in ["b.png", "A.jpg", "c.txt", ".hidden.png", "sub/d.png"] {
        touch(&folder.join(name));
    }
    let single = dir.join("single.heic");
    touch(&single);
    let images =
        collect_images(&[folder.clone(), single.clone(), folder.join("b.png"), dir.join("none.png")], &[]);
    // フォルダは直下の対応形式の画像だけを名前順（大文字・小文字を区別しない）。隠しファイル・サブフォルダは除く
    assert_eq!(images, vec![folder.join("A.jpg"), folder.join("b.png"), single.clone()]);
    // すでに一覧にあるものは加えない（大文字・小文字の違いも同じファイル）
    let more = collect_images(&[folder.join("A.JPG"), single.clone()], &images);
    assert!(more.is_empty(), "{more:?}");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn long_side_follows_the_photo_orientation() {
    // 横長は幅、縦長は高さを長辺にする
    let landscape = batch_settings(&options(plain(), Some(800)), (4000, 3000)).unwrap();
    assert_eq!((landscape.width, landscape.height), (Some(800), None));
    let portrait = batch_settings(&options(plain(), Some(800)), (3000, 4000)).unwrap();
    assert_eq!((portrait.width, portrait.height), (None, Some(800)));
    // 円は中央の正方形で決める（幅）。リサイズしなければ大きさはそのまま
    let circle = Preset { shape: ShapeType::Circle, ..plain() };
    assert_eq!(batch_settings(&options(circle, Some(500)), (3000, 4000)).unwrap().width, Some(500));
    let framed = Preset { frame: FrameType::Polaroid, exposure: 1.0, ..plain() };
    let settings = batch_settings(&options(framed, None), (4000, 3000)).unwrap();
    assert_eq!(
        (settings.width, settings.height, settings.frame, settings.exposure),
        (None, None, FrameType::Polaroid, 1.0)
    );
    // 範囲の外はエラー
    assert!(batch_settings(&options(plain(), Some(0)), (10, 10)).is_err());
    assert!(batch_settings(&options(plain(), Some(20001)), (10, 10)).is_err());
}

#[test]
fn failures_do_not_stop_and_cancel_stops_before_the_next() {
    let sources: Vec<PathBuf> = ["a.png", "bad.png", "c.png", "d.png"].iter().map(PathBuf::from).collect();
    let mut seen = Vec::new();
    let results = run_batch(
        &sources,
        |p| {
            if p.ends_with("bad.png") {
                Err("壊れています".into())
            } else {
                Ok(PathBuf::from("out").join(p))
            }
        },
        |index, p| seen.push((index, p.to_path_buf())),
        || false,
    );
    assert_eq!(results.len(), 4);
    assert_eq!(results[1].error.as_deref(), Some("壊れています"));
    assert!(results[2].output.is_some());
    assert_eq!(seen.iter().map(|s| s.0).collect::<Vec<_>>(), vec![0, 1, 2, 3]);
    let text = summary(&results, false, Path::new("/out"));
    assert_eq!(
        text,
        "3 枚を保存しました。\n保存先: /out\n\n1 枚は処理できませんでした:\n・bad.png（壊れています）"
    );

    // 中止: 2 枚目を処理し終えたところで中止されると、3 枚目からは処理しない
    let done = std::cell::Cell::new(0);
    let results = run_batch(
        &sources,
        |p| {
            done.set(done.get() + 1);
            Ok(p.to_path_buf())
        },
        |_, _| {},
        || done.get() >= 2,
    );
    assert_eq!(results.len(), 2);
    assert!(summary(&results, true, Path::new("/out")).starts_with("中止しました。2 枚を保存しました。"));
}

#[test]
fn summary_lists_at_most_ten_failures() {
    let results: Vec<_> = (0..12)
        .map(|i| imageeditorrt_core::batch::BatchResult {
            source: PathBuf::from(format!("{i}.png")),
            output: None,
            error: Some("x".into()),
        })
        .collect();
    let text = summary(&results, false, Path::new("/o"));
    assert_eq!(text.matches("・").count(), 10);
    assert!(text.ends_with("…ほか 2 枚"), "{text}");
}

#[test]
#[cfg(target_os = "macos")]
fn processes_files_and_keeps_going_after_a_broken_one() {
    use imageeditorrt_core::batch::process_image;
    let dir = temp_dir("process");
    let input = dir.join("in");
    std::fs::create_dir_all(&input).unwrap();
    RgbaImage::from_pixel(40, 20, Rgba([200, 100, 50, 255])).save(input.join("wide.png")).unwrap();
    RgbaImage::from_pixel(20, 40, Rgba([10, 20, 30, 255])).save(input.join("tall.png")).unwrap();
    std::fs::write(input.join("broken.png"), b"not a png").unwrap();
    let out = dir.join("out");
    let opts = options(Preset { frame: FrameType::None, ..plain() }, Some(10));
    let sources = collect_images(std::slice::from_ref(&input), &[]);
    let results = run_batch(&sources, |p| process_image(p, &out, &opts), |_, _| {}, || false);
    assert_eq!(results.len(), 3);
    assert!(results[0].error.is_some(), "{results:?}"); // broken.png（名前順で先頭）
    let wide = image::open(out.join("wide.png")).unwrap();
    let tall = image::open(out.join("tall.png")).unwrap();
    // 長辺がそろう
    assert_eq!((wide.width(), wide.height()), (10, 5));
    assert_eq!((tall.width(), tall.height()), (5, 10));
    // もう一度処理すると _edited を付けて上書きしない。元の画像は変えない
    let again = process_image(&input.join("wide.png"), &out, &opts).unwrap();
    assert_eq!(again, out.join("wide_edited.png"));
    assert_eq!(image::open(input.join("wide.png")).unwrap().width(), 40);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn remove_gps_overrides_the_save_options() {
    let mut keep = options(plain(), None);
    keep.save.keep_gps = true;
    assert!(keep.save_options().keep_gps);
    let removed =
        BatchOptions { privacy: BatchPrivacy { remove_gps: true, ..BatchPrivacy::default() }, ..keep };
    assert!(!removed.save_options().keep_gps);
    assert_eq!(removed.save_options().quality, removed.save.quality);
}

#[test]
#[cfg(target_os = "macos")]
fn faces_are_covered_in_batch() {
    use imageeditorrt_core::privacy::RegionKind;
    let dir = temp_dir("privacy");
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/face.jpg");
    let plain_out =
        imageeditorrt_core::batch::process_image(&source, &dir.join("plain"), &options(plain(), None))
            .unwrap();
    let privacy =
        BatchPrivacy { faces: true, kind: RegionKind::Mosaic, strength: 100, ..BatchPrivacy::default() };
    let covered = BatchOptions { privacy, ..options(plain(), None) };
    let regions = match imageeditorrt_core::batch::privacy_regions(
        &image::open(&source).unwrap().to_rgba8(),
        &covered.privacy,
    ) {
        Err(e) if e.contains("inference context") => {
            eprintln!("この環境では顔を認識できないので飛ばす: {e}");
            return;
        }
        result => result.unwrap(),
    };
    assert_eq!(regions.len(), 1);
    let out = imageeditorrt_core::batch::process_image(&source, &dir.join("covered"), &covered).unwrap();
    let (a, b) = (image::open(plain_out).unwrap().to_rgba8(), image::open(out).unwrap().to_rgba8());
    // 顔のあたり（256 × 320 の (160, 85)）は変わり、左下の角は変わらない（JPEG の誤差はゆるす）
    let diff = |x: u32, y: u32| {
        (0..3)
            .map(|c| i32::from(a.get_pixel(x, y)[c]).abs_diff(i32::from(b.get_pixel(x, y)[c])))
            .max()
            .unwrap()
    };
    let face_changed =
        (140..180).flat_map(|x| (70..100).map(move |y| (x, y))).filter(|&(x, y)| diff(x, y) > 20).count();
    assert!(face_changed > 100, "{face_changed}");
    assert!(diff(5, 315) <= 6);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn naming_with_date_and_sequence() {
    use imageeditorrt_core::batch::named_output_path;
    use imageeditorrt_core::exif_info::CaptureDate;
    let dir = temp_dir("naming");
    let source = dir.join("in/IMG_0001.JPG");
    touch(&source);
    let out = dir.join("out");
    let date = CaptureDate { year: 2026, month: 10, day: 9, hour: 8, minute: 0 };
    let with_date = OutputNaming::WithDate;
    assert_eq!(
        named_output_path(&source, &out, &with_date, OutputFormat::Original, 0, Some(&date)),
        out.join("IMG_0001_20261009.JPG")
    );
    // 撮影日がなければ元の名前。重なれば _2
    assert_eq!(
        named_output_path(&source, &out, &with_date, OutputFormat::Original, 0, None),
        out.join("IMG_0001.JPG")
    );
    touch(&out.join("IMG_0001_20261009.JPG"));
    assert_eq!(
        named_output_path(&source, &out, &with_date, OutputFormat::Original, 0, Some(&date)),
        out.join("IMG_0001_20261009_2.JPG")
    );
    // 連番（一覧の順に 001 から。空なら「写真」、/ と : は _）
    let trip = OutputNaming::Sequence { prefix: " 旅行/京都 ".into() };
    assert_eq!(
        named_output_path(&source, &out, &trip, OutputFormat::Original, 0, None),
        out.join("旅行_京都_001.JPG")
    );
    assert_eq!(
        named_output_path(&source, &out, &trip, OutputFormat::Original, 11, None),
        out.join("旅行_京都_012.JPG")
    );
    let empty = OutputNaming::Sequence { prefix: "".into() };
    assert_eq!(
        named_output_path(&dir.join("in/a.heic"), &out, &empty, OutputFormat::Original, 2, None),
        out.join("写真_003.heic")
    );
    // 元の名前は今までどおり
    assert_eq!(
        named_output_path(&source, &out, &OutputNaming::Original, OutputFormat::Original, 5, Some(&date)),
        output_path(&source, &out)
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn output_format_changes_the_suffix() {
    use imageeditorrt_core::batch::named_output_path;
    let dir = temp_dir("format");
    let source = dir.join("in/IMG_0001.JPG");
    touch(&source);
    let out = dir.join("out");
    let original = OutputNaming::Original;
    let path = |format| named_output_path(&source, &out, &original, format, 0, None);
    assert_eq!(path(OutputFormat::Original), out.join("IMG_0001.JPG"));
    assert_eq!(path(OutputFormat::Heic), out.join("IMG_0001.heic"));
    // 同じ名前があれば _edited（形式を変えても上書きしない）
    touch(&out.join("IMG_0001.heic"));
    assert_eq!(path(OutputFormat::Heic), out.join("IMG_0001_edited.heic"));
    let trip = OutputNaming::Sequence { prefix: "旅行".into() };
    assert_eq!(
        named_output_path(&dir.join("in/b.png"), &out, &trip, OutputFormat::Jpeg, 0, None),
        out.join("旅行_001.jpg")
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
#[cfg(target_os = "macos")]
fn batch_can_save_as_heic() {
    use imageeditorrt_core::decode::decode_file;
    use imageeditorrt_core::exif_info::read_exif_info;
    use imageeditorrt_core::formats::Format;
    let dir = temp_dir("heic");
    // EXIF のある JPEG を、形式を HEIC にして保存する
    let source = dir.join("in/canon.jpg");
    std::fs::create_dir_all(source.parent().unwrap()).unwrap();
    std::fs::copy(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/canon.jpg"), &source).unwrap();
    let heic = BatchOptions { format: OutputFormat::Heic, ..options(plain(), None) };
    let out = imageeditorrt_core::batch::process_image(&source, &dir.join("out"), &heic).unwrap();
    assert_eq!(out, dir.join("out/canon.heic"));
    let bytes = std::fs::read(&out).unwrap();
    let decoded = decode_file(&bytes).unwrap();
    assert_eq!(decoded.format, Format::Heif);
    assert_eq!(decoded.image.dimensions(), image::open(&source).unwrap().to_rgba8().dimensions());
    // EXIF（機種）は残る
    let info = read_exif_info(&bytes);
    assert!(info.entries.iter().any(|e| e.tag == "Make" && e.value == "Canon"), "{:?}", info.entries);
    std::fs::remove_dir_all(&dir).unwrap();
}
