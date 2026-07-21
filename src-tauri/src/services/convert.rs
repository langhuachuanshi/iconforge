use crate::error::AppError;
use super::export::assemble_ico;
use super::image;

/// 多张图 × 多尺寸 → 单个 ICO 文件
///
/// 每张图按所选尺寸缩放成方形 PNG，全部拼进同一个 ICO。
pub fn images_to_ico(images: &[Vec<u8>], sizes: &[u32]) -> Result<Vec<u8>, AppError> {
    if images.is_empty() {
        return Err(AppError::Image("请至少添加一张图片".into()));
    }
    if sizes.is_empty() {
        return Err(AppError::Image("请至少选择一个尺寸".into()));
    }

    let mut entries: Vec<(u32, Vec<u8>)> = Vec::new();
    for img in images {
        for &size in sizes {
            entries.push((size, image::make_square(img, size)?));
        }
    }
    Ok(assemble_ico(entries))
}
