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
///
/// 若传入的是 Windows 快捷方式（.lnk）字节，先解析出目标 exe 再递归提取，
/// 这样「拖拽」和「选文件」两条入口都能直接支持快捷方式。
pub fn extract_pe_icons(pe_bytes: &[u8]) -> Result<Vec<ExtractedIcon>, AppError> {
    if is_shell_link(pe_bytes) {
        let target = resolve_lnk_target_path(pe_bytes)?;
        let target_bytes = std::fs::read(&target)?;
        return extract_pe_icons(&target_bytes);
    }
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

// ── 快捷方式（.lnk）解析 ──

/// 判断字节是否为 Windows Shell Link（.lnk）：首 4 字节 HeaderSize == 0x0000004C
fn is_shell_link(bytes: &[u8]) -> bool {
    bytes.len() >= 4 && bytes[0..4] == [0x4C, 0x00, 0x00, 0x00]
}

/// 解析 .lnk 字节，返回目标 exe 的绝对路径（仅 Windows）。
/// 把字节落临时 .lnk，再用 PowerShell WScript.Shell COM 读 TargetPath。
#[cfg(target_os = "windows")]
fn resolve_lnk_target_path(lnk_bytes: &[u8]) -> Result<String, AppError> {
    use std::io::Write;

    // 落临时 .lnk 文件（tempfile 关闭时自动清理；扩展名必须为 .lnk，COM 才认）
    let mut tmp = tempfile::Builder::new()
        .suffix(".lnk")
        .tempfile()
        .map_err(|e| AppError::Image(format!("创建临时文件失败: {e}")))?;
    tmp.write_all(lnk_bytes)
        .map_err(|e| AppError::Image(format!("写入临时文件失败: {e}")))?;
    let lnk_path = tmp.path().to_string_lossy().replace('\'', "''");

    // PowerShell + WScript.Shell 读 TargetPath（与 system.rs 创建快捷方式同套机制）
    let script = format!(
        "(New-Object -COM WScript.Shell).CreateShortcut('{lnk}').TargetPath",
        lnk = lnk_path,
    );
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let out = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .creation_flags(CREATE_NO_WINDOW)
        .output();

    // 显式释放临时文件句柄（早于函数返回，避免 Windows 上文件占用）
    let _ = tmp.close();

    match out {
        Ok(o) if o.status.success() => {
            let target = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if target.is_empty() {
                Err(AppError::Image("快捷方式没有目标".to_string()))
            } else {
                Ok(target)
            }
        }
        Ok(o) => Err(AppError::Image(format!(
            "无法解析快捷方式目标: {}",
            String::from_utf8_lossy(&o.stderr).trim()
        ))),
        Err(e) => Err(AppError::Image(format!("启动 PowerShell 失败: {e}"))),
    }
}

/// 非 Windows：不支持解析 .lnk
#[cfg(not(target_os = "windows"))]
fn resolve_lnk_target_path(_lnk_bytes: &[u8]) -> Result<String, AppError> {
    Err(AppError::Image(
        "当前系统不支持解析 Windows 快捷方式".to_string(),
    ))
}
