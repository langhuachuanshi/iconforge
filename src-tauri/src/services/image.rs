use image::{DynamicImage, GrayImage, Rgba, RgbaImage};
use image::imageops::FilterType;
use imageproc::distance_transform::Norm;
use imageproc::filter::gaussian_blur_f32;
use imageproc::morphology;

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

/// 按颜色去底（色键/魔棒）：与目标色距离 ≤ tolerance 的像素设为透明，其余保留
///
/// 适合 logo/图标去白底场景，比抠图模型更可控。
/// 距离用 RGB 欧氏距离，tolerance 范围 0~442（sqrt(3*255²)）。
pub fn remove_color(image_bytes: &[u8], target: [u8; 3], tolerance: f32) -> Result<Vec<u8>, AppError> {
    let mut img = decode_rgba(image_bytes)?;

    // 距离平方阈值（避免每个像素开方）
    let tol_sq = tolerance * tolerance;

    for pixel in img.pixels_mut() {
        let dr = pixel[0] as f32 - target[0] as f32;
        let dg = pixel[1] as f32 - target[1] as f32;
        let db = pixel[2] as f32 - target[2] as f32;
        let dist_sq = dr * dr + dg * dg + db * db;
        if dist_sq <= tol_sq {
            pixel[3] = 0; // 命中背景色 → 透明
        } else if pixel[3] > 0 {
            // 不透明像素保留原 alpha（已经是 255 或已抠图的半透明）
        }
    }

    encode_png(&img)
}

// ────────────────────────────────────────────────────────────────
// 边缘净化：收缩 / 羽化 / 去色晕 / 内描边
// ────────────────────────────────────────────────────────────────

/// 提取 alpha 通道为灰度图（用于形态学/模糊操作）
fn alpha_to_gray(img: &RgbaImage) -> GrayImage {
    GrayImage::from_fn(img.width(), img.height(), |x, y| {
        image::Luma([img.get_pixel(x, y)[3]])
    })
}

/// 通用原语：把灰度蒙版乘到图像 alpha 通道
/// new_alpha = round(orig_alpha * mask / 255)
pub fn apply_alpha_mask(img: &RgbaImage, mask: &GrayImage) -> RgbaImage {
    let mut out = img.clone();
    for (x, y, pixel) in out.enumerate_pixels_mut() {
        let m = mask.get_pixel(x, y)[0] as u32;
        pixel[3] = ((pixel[3] as u32 * m) / 255) as u8;
    }
    out
}

/// 收缩边缘：alpha 蒙版做形态学腐蚀，主体向内收缩 amount px
pub fn erode_alpha(image_bytes: &[u8], amount: u32) -> Result<Vec<u8>, AppError> {
    let img = decode_rgba(image_bytes)?;
    // amount=0 直接返回原图
    if amount == 0 {
        return encode_png(&img);
    }
    let mask = alpha_to_gray(&img);
    // k 是腐蚀半径，imageproc 要求 u8；amount 不会很大（1~10），clamp 一下防溢出
    let k = amount.min(255) as u8;
    let eroded = morphology::erode(&mask, Norm::LInf, k);
    let out = apply_alpha_mask(&img, &eroded);
    encode_png(&out)
}

/// 羽化边缘：alpha 蒙版做高斯模糊，边缘变半透明柔和过渡
/// sigma 越大羽化越宽（建议 0.5~3.0）
pub fn feather_alpha(image_bytes: &[u8], sigma: f32) -> Result<Vec<u8>, AppError> {
    let img = decode_rgba(image_bytes)?;
    if sigma <= 0.0 {
        return encode_png(&img);
    }
    let mask = alpha_to_gray(&img);
    let blurred = gaussian_blur_f32(&mask, sigma);
    let out = apply_alpha_mask(&img, &blurred);
    encode_png(&out)
}

/// 去色晕：把半透明边缘像素的 RGB 收敛到邻域内不透明像素的平均色，保留 alpha 不变
/// 解决抠图后边缘残留原背景色（蓝晕/白晕）的问题。
/// radius 为搜索邻域半径（像素），建议 2。
pub fn decontaminate(image_bytes: &[u8], radius: u32) -> Result<Vec<u8>, AppError> {
    let img = decode_rgba(image_bytes)?;
    let (w, h) = (img.width(), img.height());
    let r = radius.max(1) as i32;
    let mut out = img.clone();

    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let p = img.get_pixel(x as u32, y as u32);
            let alpha = p[3];
            // 只处理「半透明」边缘像素（alpha 在 1~254 之间）；完全不透明和完全透明的不动
            if alpha == 0 || alpha == 255 {
                continue;
            }
            // 在邻域内找不透明像素（alpha > 200）的平均色
            let mut sum_r = 0u32;
            let mut sum_g = 0u32;
            let mut sum_b = 0u32;
            let mut count = 0u32;
            for dy in -r..=r {
                for dx in -r..=r {
                    let nx = x + dx;
                    let ny = y + dy;
                    if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                        continue;
                    }
                    let n = img.get_pixel(nx as u32, ny as u32);
                    if n[3] > 200 {
                        sum_r += n[0] as u32;
                        sum_g += n[1] as u32;
                        sum_b += n[2] as u32;
                        count += 1;
                    }
                }
            }
            if count > 0 {
                let pix = out.get_pixel_mut(x as u32, y as u32);
                pix[0] = (sum_r / count) as u8;
                pix[1] = (sum_g / count) as u8;
                pix[2] = (sum_b / count) as u8;
                // alpha 保持不变
            }
        }
    }
    encode_png(&out)
}

/// 内描边：主体边界向内描 amount px 的指定颜色
/// 算法：腐蚀 alpha 蒙版（收缩 amount），原蒙版与腐蚀后的差集就是「边缘带」，
/// 把边缘带内像素的颜色替换为描边色，alpha 设满。
pub fn add_inner_stroke(image_bytes: &[u8], amount: u32, color: [u8; 3]) -> Result<Vec<u8>, AppError> {
    let img = decode_rgba(image_bytes)?;
    if amount == 0 {
        return encode_png(&img);
    }
    let mask = alpha_to_gray(&img);
    let k = amount.min(255) as u8;
    let eroded = morphology::erode(&mask, Norm::LInf, k);

    let mut out = img.clone();
    for (x, y, pixel) in out.enumerate_pixels_mut() {
        let orig_alpha = mask.get_pixel(x, y)[0];
        let eroded_alpha = eroded.get_pixel(x, y)[0];
        // 边缘带：原图有 alpha，但腐蚀后没了（或变弱）
        if orig_alpha > 128 && eroded_alpha <= 128 {
            pixel[0] = color[0];
            pixel[1] = color[1];
            pixel[2] = color[2];
            pixel[3] = orig_alpha;
        }
    }
    encode_png(&out)
}

// ────────────────────────────────────────────────────────────────
// 内容感知裁剪：Trim / 按宽高比裁剪
// ────────────────────────────────────────────────────────────────

/// 去除透明边距：扫描 alpha > 阈值 的边界框，裁剪到主体区域
/// 若图像全透明（无主体），返回原图不动。
pub fn trim_transparent(image_bytes: &[u8], threshold: u8) -> Result<Vec<u8>, AppError> {
    let img = decode_rgba(image_bytes)?;
    let (w, h) = (img.width(), img.height());
    let mut min_x = w;
    let mut min_y = h;
    let mut max_x = 0u32;
    let mut max_y = 0u32;
    let mut found = false;
    for y in 0..h {
        for x in 0..w {
            if img.get_pixel(x, y)[3] > threshold {
                found = true;
                if x < min_x { min_x = x; }
                if x > max_x { max_x = x; }
                if y < min_y { min_y = y; }
                if y > max_y { max_y = y; }
            }
        }
    }
    if !found {
        return encode_png(&img);
    }
    let cw = max_x - min_x + 1;
    let ch = max_y - min_y + 1;
    let cropped = image::imageops::crop_imm(&img, min_x, min_y, cw, ch);
    encode_png(&cropped.to_image())
}

/// 按宽高比智能裁剪：以 alpha 加权质心为中心，按目标比例裁剪，越界则贴边
/// ratio_w:ratio_h 为目标宽高比（如 1:1 传 1,1；3:4 传 3,4）
pub fn crop_to_aspect(image_bytes: &[u8], ratio_w: u32, ratio_h: u32) -> Result<Vec<u8>, AppError> {
    let img = decode_rgba(image_bytes)?;
    let (w, h) = (img.width(), img.height());
    let rw = ratio_w.max(1) as f64;
    let rh = ratio_h.max(1) as f64;
    let target_ratio = rw / rh;
    let cur_ratio = w as f64 / h as f64;

    // 算 alpha 加权质心
    let mut sum_x = 0.0f64;
    let mut sum_y = 0.0f64;
    let mut sum_w = 0.0f64;
    for y in 0..h {
        for x in 0..w {
            let a = img.get_pixel(x, y)[3] as f64;
            sum_x += x as f64 * a;
            sum_y += y as f64 * a;
            sum_w += a;
        }
    }
    let (cx, cy) = if sum_w > 0.0 {
        (sum_x / sum_w, sum_y / sum_w)
    } else {
        ((w as f64) / 2.0, (h as f64) / 2.0)
    };

    // 决定裁剪框尺寸：保持目标比例，在原图内取最大
    let (cw, ch) = if cur_ratio > target_ratio {
        // 原图偏宽 → 以高度为基准，宽度按比例缩
        (h as f64 * target_ratio, h as f64)
    } else {
        (w as f64, w as f64 / target_ratio)
    };
    let cw = cw.min(w as f64);
    let ch = ch.min(h as f64);

    // 以质心为中心放置裁剪框，越界贴边
    let mut left = (cx - cw / 2.0).round() as i64;
    let mut top = (cy - ch / 2.0).round() as i64;
    if left < 0 { left = 0; }
    if top < 0 { top = 0; }
    if left + cw as i64 > w as i64 { left = w as i64 - cw as i64; }
    if top + ch as i64 > h as i64 { top = h as i64 - ch as i64; }
    let left = left.max(0) as u32;
    let top = top.max(0) as u32;

    let cropped = image::imageops::crop_imm(&img, left, top, cw as u32, ch as u32);
    encode_png(&cropped.to_image())
}

// ────────────────────────────────────────────────────────────────
// 形状遮罩：圆角矩形 / 圆形
// ────────────────────────────────────────────────────────────────

/// 圆角矩形 alpha 遮罩（SDF 算法，自带 1px 抗锯齿过渡）
/// radius=0 退化为直角矩形
fn rounded_rect_mask(w: u32, h: u32, radius: f64) -> GrayImage {
    let mut mask = GrayImage::new(w, h);
    let r = radius.min(w.min(h) as f64 / 2.0);
    for y in 0..h {
        for x in 0..w {
            let px = x as f64;
            let py = y as f64;
            // 四个圆角的圆心位置
            let cx = if px < w as f64 / 2.0 { r } else { w as f64 - 1.0 - r };
            let cy = if py < h as f64 / 2.0 { r } else { h as f64 - 1.0 - r };
            // 像素若落在四角区域内，用「到圆角的距离 - r」；否则用「到最近直边的距离」
            let in_corner_x = (px < r) || (px > w as f64 - 1.0 - r);
            let in_corner_y = (py < r) || (py > h as f64 - 1.0 - r);
            // 统一 SDF 约定：dist>0 在矩形外（透明），dist<0 在矩形内（不透明）
            let dist = if in_corner_x && in_corner_y {
                // 到圆角的距离 - r：在圆角外为正
                ((px - cx).powi(2) + (py - cy).powi(2)).sqrt() - r
            } else {
                // 到最近直边的距离取负：在矩形内部为负
                -(px.min(w as f64 - 1.0 - px).min(py).min(h as f64 - 1.0 - py))
            };
            // [-0.5,0.5] 抗锯齿过渡
            let alpha = ((0.5 - dist) * 255.0).clamp(0.0, 255.0) as u8;
            mask.put_pixel(x, y, image::Luma([alpha]));
        }
    }
    mask
}

/// 应用圆角矩形遮罩：radius 为圆角半径（像素），0 = 直角
pub fn apply_rounded_mask(image_bytes: &[u8], radius: u32) -> Result<Vec<u8>, AppError> {
    let img = decode_rgba(image_bytes)?;
    let (w, h) = (img.width(), img.height());
    let mask = rounded_rect_mask(w, h, radius as f64);
    let out = apply_alpha_mask(&img, &mask);
    encode_png(&out)
}

/// 应用圆形遮罩（内切圆）
pub fn apply_circle_mask(image_bytes: &[u8]) -> Result<Vec<u8>, AppError> {
    let img = decode_rgba(image_bytes)?;
    let (w, h) = (img.width(), img.height());
    let mut mask = GrayImage::new(w, h);
    let cx = (w as f64 - 1.0) / 2.0;
    let cy = (h as f64 - 1.0) / 2.0;
    let r = w.min(h) as f64 / 2.0;
    for y in 0..h {
        for x in 0..w {
            // dist>0 在圆外 → 透明；dist<0 在圆内 → 不透明
            let dist = ((x as f64 - cx).powi(2) + (y as f64 - cy).powi(2)).sqrt() - r;
            let alpha = ((0.5 - dist) * 255.0).clamp(0.0, 255.0) as u8;
            mask.put_pixel(x, y, image::Luma([alpha]));
        }
    }
    let out = apply_alpha_mask(&img, &mask);
    encode_png(&out)
}

// ────────────────────────────────────────────────────────────────
// 调色：亮度 / 对比度 / 饱和度
// ────────────────────────────────────────────────────────────────

/// 调节亮度/对比度/饱和度，三者范围 -100~100（0 为不变），仅影响 RGB，不动 alpha
/// brightness: 正变亮负变暗（线性偏移）
/// contrast: 正增强负减弱（以 128 为中心做斜率）
/// saturation: 正增艳负减淡（绕灰度点缩放）
pub fn adjust_brightness_contrast(
    image_bytes: &[u8],
    brightness: f32,
    contrast: f32,
    saturation: f32,
) -> Result<Vec<u8>, AppError> {
    let mut img = decode_rgba(image_bytes)?;
    let b = brightness.clamp(-100.0, 100.0) * 2.55; // -100~100 → -255~255
    // contrast 斜率：0→1，+100→3，-100→0（近似）
    let c_slope = 1.0 + contrast.clamp(-100.0, 100.0) / 100.0 * 2.0;
    // saturation 因子：0→1，+100→2，-100→0
    let s_factor = 1.0 + saturation.clamp(-100.0, 100.0) / 100.0;

    for pixel in img.pixels_mut() {
        // 先亮度偏移
        let mut r = pixel[0] as f32 + b;
        let mut g = pixel[1] as f32 + b;
        let mut bl = pixel[2] as f32 + b;
        // 对比度（绕 128）
        r = (r - 128.0) * c_slope + 128.0;
        g = (g - 128.0) * c_slope + 128.0;
        bl = (bl - 128.0) * c_slope + 128.0;
        // 饱和度（绕灰度值缩放）
        let gray = 0.299 * r + 0.587 * g + 0.114 * bl;
        r = gray + (r - gray) * s_factor;
        g = gray + (g - gray) * s_factor;
        bl = gray + (bl - gray) * s_factor;
        pixel[0] = r.clamp(0.0, 255.0) as u8;
        pixel[1] = g.clamp(0.0, 255.0) as u8;
        pixel[2] = bl.clamp(0.0, 255.0) as u8;
    }
    encode_png(&img)
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

    // ── 边缘净化测试 ──
    // 构造一个 10×10 图：中心 6×6 红色不透明，四周透明
    fn make_centered_png(size: u32, inner: u32, color: [u8; 4]) -> Vec<u8> {
        let mut img = RgbaImage::new(size, size);
        let off = (size - inner) / 2;
        for y in off..off + inner {
            for x in off..off + inner {
                img.put_pixel(x, y, Rgba(color));
            }
        }
        encode_png(&img).unwrap()
    }

    #[test]
    fn test_erode_alpha_shrinks_opaque_region() {
        // 10×10，中心 6×6 不透明 → erode 1px 后中心应缩到约 4×4
        let bytes = make_centered_png(10, 6, [255, 0, 0, 255]);
        let result = erode_alpha(&bytes, 1).unwrap();
        let img = decode_rgba(&result).unwrap();
        // 边缘原本不透明的点（x=2）现在应变透明
        assert_eq!(img.get_pixel(2, 5)[3], 0, "erode 后边缘应透明");
        // 中心点仍不透明
        assert_eq!(img.get_pixel(5, 5)[3], 255, "中心仍不透明");
    }

    #[test]
    fn test_feather_alpha_creates_semitransparent() {
        // 硬边缘 → 羽化后应出现半透明像素
        let bytes = make_centered_png(10, 6, [255, 0, 0, 255]);
        let result = feather_alpha(&bytes, 1.5).unwrap();
        let img = decode_rgba(&result).unwrap();
        // 边界附近的像素应变成半透明（0 < alpha < 255）
        let edge_alpha = img.get_pixel(3, 5)[3];
        assert!(edge_alpha > 0 && edge_alpha < 255, "羽化后边缘应半透明，实际 alpha={}", edge_alpha);
    }

    #[test]
    fn test_decontaminate_changes_edge_color() {
        // 中心不透明红 [255,0,0]，边缘半透明且带蓝色色晕 [0,0,255,a=128]
        let mut img = RgbaImage::new(10, 10);
        for y in 3..7 { for x in 3..7 { img.put_pixel(x, y, Rgba([255, 0, 0, 255])); } }
        // 给中心边缘外包一圈带蓝晕的半透明像素
        for &p in &[(2u32,5u32),(7,5),(5,2),(5,7)] { img.put_pixel(p.0, p.1, Rgba([0, 0, 255, 128])); }
        let bytes = encode_png(&img).unwrap();
        let result = decontaminate(&bytes, 2).unwrap();
        let out = decode_rgba(&result).unwrap();
        // (2,5) 原是蓝色半透明，去色晕后 RGB 应偏向红色（邻域不透明像素是红）
        let p = out.get_pixel(2, 5);
        assert!(p[0] > 200, "去色晕后 R 应偏大，实际 {}", p[0]);
        assert!(p[2] < 100, "去色晕后 B 应偏小，实际 {}", p[2]);
    }

    #[test]
    fn test_inner_stroke_draws_at_boundary() {
        // 10×10，中心 6×6（offset=2，即 x,y∈[2,7]）红色不透明，内描边 1px 白
        let bytes = make_centered_png(10, 6, [255, 0, 0, 255]);
        let result = add_inner_stroke(&bytes, 1, [255, 255, 255]).unwrap();
        let img = decode_rgba(&result).unwrap();
        // 最外圈像素 (2,5) 应被描白：erode k=1 后它因邻居 (1,5) 透明而消失，命中边缘带
        let edge = img.get_pixel(2, 5);
        assert_eq!([edge[0], edge[1], edge[2]], [255, 255, 255], "外圈应描白");
        // 内层 (5,5) 仍红
        let center = img.get_pixel(5, 5);
        assert_eq!([center[0], center[1], center[2]], [255, 0, 0], "内层保持红色");
    }

    // ── 裁剪测试 ──
    #[test]
    fn test_trim_transparent_removes_borders() {
        // 10×10 中心 6×6 不透明 → trim 后应成 6×6
        let bytes = make_centered_png(10, 6, [255, 0, 0, 255]);
        let result = trim_transparent(&bytes, 0).unwrap();
        let img = decode_rgba(&result).unwrap();
        assert_eq!(img.width(), 6);
        assert_eq!(img.height(), 6);
    }

    #[test]
    fn test_trim_transparent_all_transparent_unchanged() {
        // 全透明图 → 原样返回（10×10）
        let img = RgbaImage::new(10, 10);
        let bytes = encode_png(&img).unwrap();
        let result = trim_transparent(&bytes, 0).unwrap();
        let out = decode_rgba(&result).unwrap();
        assert_eq!(out.width(), 10);
        assert_eq!(out.height(), 10);
    }

    #[test]
    fn test_crop_to_aspect_square() {
        // 100×50 宽图，按 1:1 裁 → 应成 50×50
        let bytes = make_test_png(100, 50);
        let result = crop_to_aspect(&bytes, 1, 1).unwrap();
        let img = decode_rgba(&result).unwrap();
        assert_eq!(img.width(), 50);
        assert_eq!(img.height(), 50);
    }

    // ── 形状遮罩测试 ──
    #[test]
    fn test_circle_mask_corners_transparent() {
        // 10×10 不透明图，应用圆形遮罩 → 四角应变透明
        let bytes = make_test_png(10, 10);
        let result = apply_circle_mask(&bytes).unwrap();
        let img = decode_rgba(&result).unwrap();
        assert_eq!(img.get_pixel(0, 0)[3], 0, "左上角应透明");
        assert_eq!(img.get_pixel(9, 9)[3], 0, "右下角应透明");
        // 中心仍不透明
        assert_eq!(img.get_pixel(5, 5)[3], 255, "中心不透明");
    }

    #[test]
    fn test_rounded_mask_corners_transparent() {
        // 10×10 不透明图，圆角 3 → 四角透明
        let bytes = make_test_png(10, 10);
        let result = apply_rounded_mask(&bytes, 3).unwrap();
        let img = decode_rgba(&result).unwrap();
        assert_eq!(img.get_pixel(0, 0)[3], 0, "圆角后左上角应透明");
        assert_eq!(img.get_pixel(5, 5)[3], 255, "中心不透明");
    }

    // ── 调色测试 ──
    #[test]
    fn test_brightness_increases() {
        // 中灰 (128) + 亮度 50 → 应明显变亮
        let img = RgbaImage::from_pixel(2, 2, Rgba([128, 128, 128, 255]));
        let bytes = encode_png(&img).unwrap();
        let result = adjust_brightness_contrast(&bytes, 50.0, 0.0, 0.0).unwrap();
        let out = decode_rgba(&result).unwrap();
        let p = out.get_pixel(0, 0);
        assert!(p[0] > 128, "亮度+50 后应变亮，实际 {}", p[0]);
    }

    #[test]
    fn test_saturation_zero_grayscale() {
        // 红色 (255,0,0) 饱和度 -100 → 应接近灰度
        let bytes = make_test_png(2, 2); // 纯红
        let result = adjust_brightness_contrast(&bytes, 0.0, 0.0, -100.0).unwrap();
        let out = decode_rgba(&result).unwrap();
        let p = out.get_pixel(0, 0);
        // 三通道应接近（去色后 R 会大幅下降）
        let (r, g, b) = (p[0], p[1], p[2]);
        assert!(g > 50, "去色后 G 应上升，实际 {}", g);
        assert!((r as i32 - g as i32).abs() < 30, "去色后 R≈G，差 {}", r as i32 - g as i32);
    }
}
