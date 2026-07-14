use image::{DynamicImage, RgbaImage};
use image::imageops::FilterType;

use crate::error::AppError;

/// 将 PNG 字节解码为 RGBA8 图像（等价 Python `.convert("RGBA")`）
pub fn decode_rgba(bytes: &[u8]) -> Result<RgbaImage, AppError> {
    let img = image::load_from_memory(bytes)?;
    Ok(img.to_rgba8())
}

/// 将 RGBA8 图像编码为 PNG 字节
pub fn encode_png(img: &RgbaImage) -> Result<Vec<u8>, AppError> {
    let mut buf = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut buf);
    img.write_to(&mut cursor, image::ImageFormat::Png)?;
    Ok(buf)
}

/// 裁剪图片
/// - `x`, `y`: 裁剪起点
/// - `w`, `h`: 裁剪宽高
pub fn crop(image_bytes: &[u8], x: u32, y: u32, w: u32, h: u32) -> Result<Vec<u8>, AppError> {
    let img = decode_rgba(image_bytes)?;
    let cropped = image::imageops::crop_imm(&img, x, y, w, h);
    let result = DynamicImage::ImageRgba8(cropped.to_image());
    let mut buf = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut buf);
    result.write_to(&mut cursor, image::ImageFormat::Png)?;
    Ok(buf)
}

/// 缩放图片到指定尺寸（Lanczos3 = Pillow Image.LANCZOS）
pub fn resize(image_bytes: &[u8], target_w: u32, target_h: u32) -> Result<Vec<u8>, AppError> {
    let img = decode_rgba(image_bytes)?;
    let resized = image::imageops::resize(&img, target_w, target_h, FilterType::Lanczos3);
    encode_png(&resized)
}

/// 居中裁剪为正方形 + 缩放到 target×target
/// 等价 Python `make_square_sync`
pub fn make_square(image_bytes: &[u8], target: u32) -> Result<Vec<u8>, AppError> {
    let img = decode_rgba(image_bytes)?;
    let (w, h) = (img.width(), img.height());

    // 居中裁剪为正方形
    let side = w.min(h);
    let left = (w - side) / 2;
    let top = (h - side) / 2;
    let cropped = image::imageops::crop_imm(&img, left, top, side, side);

    // 缩放到目标尺寸
    let resized = image::imageops::resize(&cropped.to_image(), target, target, FilterType::Lanczos3);
    encode_png(&resized)
}

/// 合成到白色背景后编码为指定格式（用于非 PNG 格式导出）
/// 等价 Python `_to_bytes` 的非 PNG 分支
pub fn composite_on_white_and_encode(img: &RgbaImage, format: image::ImageFormat) -> Result<Vec<u8>, AppError> {
    let (w, h) = (img.width(), img.height());
    let mut bg = RgbaImage::from_pixel(w, h, image::Rgba([255, 255, 255, 255]));

    // 用 alpha 通道将前景混合到白色背景上
    for (x, y, pixel) in img.enumerate_pixels() {
        let bg_pixel = bg.get_pixel_mut(x, y);
        let alpha = pixel[3] as f32 / 255.0;
        let inv_alpha = 1.0 - alpha;
        for c in 0..3 {
            bg_pixel[c] = (pixel[c] as f32 * alpha + 255.0 * inv_alpha) as u8;
        }
        bg_pixel[3] = 255;
    }

    let mut buf = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut buf);
    bg.write_to(&mut cursor, format)?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 创建一个 100x200 的测试 PNG（红色）
    fn make_test_png(w: u32, h: u32) -> Vec<u8> {
        let img = RgbaImage::from_pixel(w, h, image::Rgba([255, 0, 0, 255]));
        encode_png(&img).unwrap()
    }

    #[test]
    fn test_crop() {
        let bytes = make_test_png(100, 200);
        let result = crop(&bytes, 10, 20, 50, 60).unwrap();
        let img = decode_rgba(&result).unwrap();
        assert_eq!(img.width(), 50);
        assert_eq!(img.height(), 60);
    }

    #[test]
    fn test_make_square_landscape() {
        let bytes = make_test_png(200, 100);
        let result = make_square(&bytes, 64).unwrap();
        let img = decode_rgba(&result).unwrap();
        assert_eq!(img.width(), 64);
        assert_eq!(img.height(), 64);
    }

    #[test]
    fn test_make_square_portrait() {
        let bytes = make_test_png(100, 200);
        let result = make_square(&bytes, 128).unwrap();
        let img = decode_rgba(&result).unwrap();
        assert_eq!(img.width(), 128);
        assert_eq!(img.height(), 128);
    }
}
