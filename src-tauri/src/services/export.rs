use std::io::Cursor;

use zip::ZipWriter;
use zip::write::FileOptions;

use crate::error::AppError;
use super::image::{self, decode_rgba, encode_png};

/// 默认 PNG 导出尺寸
pub const DEFAULT_PNG_SIZES: &[u32] = &[16, 32, 48, 64, 128, 256, 512];
/// 默认 ICO 导出尺寸
pub const DEFAULT_ICO_SIZES: &[u32] = &[16, 32, 48, 64, 128, 256];

/// 导出文件
pub struct ExportFile {
    pub filename: String,
    pub content: Vec<u8>,
}

/// 生成多尺寸 PNG
pub fn export_pngs(image_bytes: &[u8], sizes: &[u32]) -> Result<Vec<ExportFile>, AppError> {
    let mut files = Vec::new();
    for &size in sizes {
        let png_bytes = image::make_square(image_bytes, size)?;
        files.push(ExportFile {
            filename: format!("icon-{}x{}.png", size, size),
            content: png_bytes,
        });
    }
    Ok(files)
}

/// 生成原始尺寸的 PNG 副本
pub fn export_original(image_bytes: &[u8]) -> Result<ExportFile, AppError> {
    // 确保是 RGBA PNG
    let img = decode_rgba(image_bytes)?;
    let content = encode_png(&img)?;
    Ok(ExportFile {
        filename: "icon-original.png".into(),
        content,
    })
}

/// 生成多尺寸 ICO 文件
///
/// ICO 格式：6 字节头 + N×16 字节目录项 + 图像数据
/// Windows Vista+ 支持 PNG 编码的 ICO 条目（支持 alpha 通道）
pub fn export_ico(image_bytes: &[u8], sizes: &[u32]) -> Result<ExportFile, AppError> {
    // 为每个尺寸生成方形 PNG
    let mut entries: Vec<(u32, Vec<u8>)> = Vec::new();
    for &size in sizes {
        let png_bytes = image::make_square(image_bytes, size)?;
        entries.push((size, png_bytes));
    }

    // 手动构造 ICO 文件
    // ICO 头：2 字节保留(0) + 2 字节类型(1=ICO) + 2 字节图像数量
    let num_images = entries.len() as u16;
    let header_size = 6 + 16 * num_images as usize;
    let mut data_offset = header_size as u32;

    // 计算所有条目数据的总偏移量
    let mut dir_entries: Vec<Vec<u8>> = Vec::new();
    for &(size, ref png_data) in &entries {
        let mut dir = Vec::with_capacity(16);
        let actual_size = size.min(256) as u8; // ICO 目录存 0=256
        dir.push(actual_size);            // 宽度 (0 = 256)
        dir.push(actual_size);            // 高度 (0 = 256)
        dir.push(0);                       // 调色板颜色数
        dir.push(0);                       // 保留
        dir.extend_from_slice(&1u16.to_le_bytes());   // 色彩平面数（ICO 中始终为 1）
        dir.extend_from_slice(&32u16.to_le_bytes());  // 位深度（32 = RGBA）
        dir.extend_from_slice(&(png_data.len() as u32).to_le_bytes()); // 图像数据大小
        dir.extend_from_slice(&data_offset.to_le_bytes()); // 图像数据偏移
        data_offset += png_data.len() as u32;
        dir_entries.push(dir);
    }

    // 组装 ICO
    let mut buf = Vec::new();
    // 头
    buf.extend_from_slice(&0u16.to_le_bytes());   // 保留
    buf.extend_from_slice(&1u16.to_le_bytes());   // 类型：ICO
    buf.extend_from_slice(&num_images.to_le_bytes()); // 图像数量
    // 目录项
    for dir in &dir_entries {
        buf.extend_from_slice(dir);
    }
    // 图像数据（PNG 格式）
    for (_size, png_data) in &entries {
        buf.extend_from_slice(png_data);
    }

    Ok(ExportFile {
        filename: "icon.ico".into(),
        content: buf,
    })
}

/// 将所有导出文件打包为 ZIP
pub fn export_zip(files: &[ExportFile]) -> Result<Vec<u8>, AppError> {
    let mut buf = Cursor::new(Vec::new());
    {
        let mut zip = ZipWriter::new(&mut buf);
        let options = FileOptions::<()>::default()
            .compression_method(zip::CompressionMethod::Deflated);

        for file in files {
            zip.start_file(&file.filename, options)?;
            std::io::Write::write_all(&mut zip, &file.content)?;
        }
        zip.finish()?;
    }
    Ok(buf.into_inner())
}

/// 完整导出流程：原始 + PNG 多尺寸 + ICO → ZIP
pub fn export_all(
    image_bytes: &[u8],
    png_sizes: Option<&[u32]>,
    ico_sizes: Option<&[u32]>,
) -> Result<Vec<u8>, AppError> {
    let png_sizes = png_sizes.unwrap_or(DEFAULT_PNG_SIZES);
    let ico_sizes = ico_sizes.unwrap_or(DEFAULT_ICO_SIZES);

    let mut files = Vec::new();

    // 原始图
    files.push(export_original(image_bytes)?);

    // PNG 多尺寸
    let pngs = export_pngs(image_bytes, png_sizes)?;
    files.extend(pngs);

    // ICO
    files.push(export_ico(image_bytes, ico_sizes)?);

    // ZIP
    export_zip(&files)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_png() -> Vec<u8> {
        let img = ::image::RgbaImage::from_pixel(200, 100, ::image::Rgba([255, 0, 0, 255]));
        let mut buf = Vec::new();
        let mut cursor = std::io::Cursor::new(&mut buf);
        img.write_to(&mut cursor, ::image::ImageFormat::Png).unwrap();
        buf
    }

    #[test]
    fn test_export_all() {
        let bytes = make_test_png();
        let zip_bytes = export_all(&bytes, None, None).unwrap();
        assert!(!zip_bytes.is_empty());
        // 验证是有效的 ZIP
        let cursor = Cursor::new(zip_bytes);
        let archive = zip::ZipArchive::new(cursor).unwrap();
        // 原始 + 7 PNG + 1 ICO = 9 个文件
        assert_eq!(archive.len(), 9);
    }

    #[test]
    fn test_export_ico_contains_correct_sizes() {
        let bytes = make_test_png();
        let ico = export_ico(&bytes, &[16, 32, 64]).unwrap();
        // 验证 ICO 头
        assert_eq!(&ico.content[0..2], &[0, 0]);       // 保留
        assert_eq!(&ico.content[2..4], &[1, 0]);       // 类型 ICO
        assert_eq!(&ico.content[4..6], &[3, 0]);       // 3 个图像
    }
}
