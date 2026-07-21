use std::io::Cursor;

use base64::Engine;
use pelite::pe64::Pe as Pe64;
use pelite::pe32::Pe as Pe32;
use pelite::resources::group::GroupResource;

use crate::error::AppError;
use crate::models::ExtractedIcon;

/// 从 PE 文件字节提取所有图标
///
/// 自动探测 32/64 位：先试 pe64，失败再试 pe32。
/// 每个图标组（如 MAINICON）的每个尺寸都展开为独立的 PNG，可单独导出；
/// 同时保留整组的 ICO base64，支持「导出整组为 ICO」。
pub fn extract_pe_icons(pe_bytes: &[u8]) -> Result<Vec<ExtractedIcon>, AppError> {
    match pelite::pe64::PeFile::from_bytes(pe_bytes) {
        Ok(file) => {
            let resources = file.resources()?;
            return collect_from_resources(resources);
        }
        Err(e) => log::info!("[EXTRACT] pe64 解析失败，尝试 pe32: {}", e),
    }
    let file = pelite::pe32::PeFile::from_bytes(pe_bytes)?;
    let resources = file.resources()?;
    collect_from_resources(resources)
}

/// 从 Resources 收集所有图标组并展开为 PNG
fn collect_from_resources<'a>(
    resources: pelite::resources::Resources<'a>,
) -> Result<Vec<ExtractedIcon>, AppError> {
    let mut out = Vec::new();
    for item in resources.icons() {
        let (name, group) = match item {
            Ok(v) => v,
            Err(e) => {
                log::warn!("[EXTRACT] 跳过图标组: {}", e);
                continue;
            }
        };

        // 整组 ICO bytes（保留用于「导出整组 ICO」）
        let mut ico_buf = Vec::new();
        group
            .write(&mut ico_buf)
            .map_err(|e| AppError::Image(format!("ICO 拼装失败: {e}")))?;
        let group_ico_b64 = base64::engine::general_purpose::STANDARD.encode(&ico_buf);
        let group_name = name.to_string();

        // 展开组内每个尺寸为 PNG
        match expand_group_to_pngs(&ico_buf) {
            Ok(entries) => {
                for (w, h, bpp, png_b64) in entries {
                    out.push(ExtractedIcon {
                        name: group_name.clone(),
                        width: w,
                        height: h,
                        bit_depth: bpp,
                        png_base64: png_b64,
                        ico_base64: group_ico_b64.clone(),
                    });
                }
            }
            Err(e) => {
                // 整组解析失败：至少保留一个占位条目，前端能看到组名和导出 ICO
                log::warn!("[EXTRACT] 组 {} PNG 展开失败: {}", group_name, e);
                let (w, h, bpp) = best_entry(&group);
                out.push(ExtractedIcon {
                    name: group_name.clone(),
                    width: w,
                    height: h,
                    bit_depth: bpp,
                    png_base64: String::new(),
                    ico_base64: group_ico_b64,
                });
            }
        }
    }
    Ok(out)
}

/// 用 ico crate 解析 ICO bytes，把每个尺寸转成 PNG base64
///
/// 自动处理 DIB 和 PNG 两种 RT_ICON 编码。
fn expand_group_to_pngs(ico_bytes: &[u8]) -> Result<Vec<(u32, u32, u32, String)>, AppError> {
    let icon_dir = ico::IconDir::read(Cursor::new(ico_bytes))
        .map_err(|e| AppError::Image(format!("ICO 解析失败: {e}")))?;

    let mut out = Vec::new();
    for entry in icon_dir.entries() {
        let w = entry.width();
        let h = entry.height();
        let bpp = entry.bits_per_pixel() as u32;

        let png_b64 = match entry.decode() {
            Ok(img) => {
                let mut buf = Vec::new();
                img.write_png(&mut buf)
                    .map_err(|e| AppError::Image(format!("PNG 编码失败: {e}")))?;
                base64::engine::general_purpose::STANDARD.encode(&buf)
            }
            Err(e) => {
                log::warn!("[EXTRACT] 跳过单尺寸 {}x{}: {}", w, h, e);
                continue;
            }
        };
        out.push((w, h, bpp, png_b64));
    }
    Ok(out)
}

/// 取图标组里分辨率最高、位深最大的条目作为代表尺寸（用于占位）
fn best_entry<'a>(group: &GroupResource<'a>) -> (u32, u32, u32) {
    let mut best = (0u32, 0u32, 0u32);
    for entry in group.entries() {
        let w = if entry.bWidth == 0 { 256 } else { entry.bWidth as u32 };
        let h = if entry.bHeight == 0 { 256 } else { entry.bHeight as u32 };
        let bpp = entry.wBitCount as u32;
        if w > best.0 || (w == best.0 && bpp > best.2) {
            best = (w, h, bpp);
        }
    }
    best
}
