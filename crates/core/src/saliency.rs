//! 目立つ部分の認識（おまかせ切り抜き。旧版にはない）。macOS の Vision の注目度
//! （VNGenerateAttentionBasedSaliencyImageRequest。人がまず目を向けそうなところ）で、目立つ部分の枠を見つける
//! （端末の中だけで処理する）。

use image::RgbaImage;
use objc2_vision::VNGenerateAttentionBasedSaliencyImageRequest;

use crate::transform::CropRect;
use crate::vision::{perform, to_image_rect};

/// 画像の目立つ部分をすべて囲む枠（画像の座標、px）。見つからなければ None。
pub fn salient_rect(image: &RgbaImage) -> Result<Option<CropRect>, String> {
    // SAFETY: 引数のない初期化
    let done =
        perform(image, "目立つ部分", || unsafe { VNGenerateAttentionBasedSaliencyImageRequest::new() })?;
    let size = image.dimensions();
    // SAFETY: 認識が終わった後に、見つけた部分の枠を読むだけ
    let rects: Vec<CropRect> = unsafe { done.request.results() }
        .and_then(|results| results.firstObject())
        .and_then(|observation| unsafe { observation.salientObjects() })
        .map_or_else(Vec::new, |objects| {
            (0..objects.count())
                .map(|i| to_image_rect(unsafe { objects.objectAtIndex(i).boundingBox() }, size))
                .collect()
        });
    Ok(union(&rects))
}

/// おまかせ切り抜きの範囲: 原寸を factor 倍に縮めたプレビュー preview（今の向き）で目立つ部分を探し、原寸
/// （大きさ size、回転・反転した後）の座標で、aspect の比でその部分が中央寄りに入る範囲を返す。見つからなければ None。
pub fn auto_crop(
    preview: &RgbaImage,
    factor: f64,
    size: (u32, u32),
    aspect: crate::crop::AspectChoice,
) -> Result<Option<CropRect>, String> {
    let Some(found) = salient_rect(preview)? else { return Ok(None) };
    Ok(crate::crop::subject_crop(size, found.to_original(factor), aspect))
}

/// 枠をすべて囲む枠（なければ None）。
fn union(rects: &[CropRect]) -> Option<CropRect> {
    let left = rects.iter().map(|r| r.x).min()?;
    let top = rects.iter().map(|r| r.y).min()?;
    let right = rects.iter().map(CropRect::right).max()?;
    let bottom = rects.iter().map(CropRect::bottom).max()?;
    Some(CropRect::new(left, top, right - left, bottom - top))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn union_covers_all_rects() {
        assert_eq!(union(&[]), None);
        let rects = [CropRect::new(10, 20, 30, 40), CropRect::new(50, 5, 10, 10)];
        assert_eq!(union(&rects), Some(CropRect::new(10, 5, 50, 55)));
    }

    /// 原寸の半分のプレビューの右下に赤い丸があれば、原寸で 1:1 の範囲を右に寄せる（Vision が使えない環境では確かめない）。
    #[test]
    fn auto_crop_maps_the_preview_spot_to_the_original() {
        use crate::crop::AspectChoice;
        use crate::frames::FrameType;
        use crate::shapes::ShapeType;
        use crate::transform::AspectRatio;
        let preview = spot_image();
        let square = AspectChoice {
            ratio: AspectRatio::Square,
            portrait: false,
            frame: FrameType::None,
            shape: ShapeType::Rectangle,
        };
        match auto_crop(&preview, 0.5, (800, 600), square) {
            // 目立つ部分は原寸の右下 → 600×600 の正方形は、中央（x = 100）より右へ寄る（枠の大きさは Vision による）
            Ok(Some(rect)) => {
                assert_eq!((rect.y, rect.width, rect.height), (0, 600, 600));
                assert!(rect.x > 150 && rect.x <= 200, "{rect:?}");
            }
            Ok(None) => panic!("目立つ部分が見つからない"),
            Err(e) if e.contains("inference context") => eprintln!("Vision が使えない環境: {e}"),
            Err(e) => panic!("{e}"),
        }
    }

    /// 灰色の地の右下（中心 (300, 220)、400 × 300）に赤い丸を置いた画像。
    fn spot_image() -> RgbaImage {
        RgbaImage::from_fn(400, 300, |x, y| {
            let (dx, dy) = (f64::from(x) - 300.0, f64::from(y) - 220.0);
            if dx * dx + dy * dy < 40.0 * 40.0 {
                Rgba([230, 30, 30, 255])
            } else {
                Rgba([128, 128, 128, 255])
            }
        })
    }

    /// 灰色の地の右下に赤い丸を置くと、目立つ部分は右下に見つかる（GPU のない CI の仮想マシンでは
    /// Vision が使えないことがあるので、そのときは確かめない）。
    #[test]
    fn red_spot_is_salient() {
        match salient_rect(&spot_image()) {
            Ok(Some(rect)) => {
                let (cx, cy) = (rect.x + rect.width / 2, rect.y + rect.height / 2);
                assert!(cx > 200 && cy > 150, "右下に見つかるはず: {rect:?}");
            }
            Ok(None) => panic!("目立つ部分が見つからない"),
            Err(e) if e.contains("inference context") => eprintln!("Vision が使えない環境: {e}"),
            Err(e) => panic!("{e}"),
        }
    }
}
