use base64::Engine;

use crate::error::AppError;
use crate::models::ExportRequest;
use crate::services;

/// 导出图标到指定文件路径（ZIP 格式）
/// 前端使用 dialog save 获取路径后调用此命令
#[tauri::command]
pub async fn export_icon_to_file(
    req: ExportRequest,
    save_path: String,
) -> Result<(), AppError> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(&req.image)?;

    let png_sizes = req.png_sizes.clone();
    let ico_sizes = req.ico_sizes.clone();

    let result = tokio::task::spawn_blocking(move || {
        services::export::export_all(
            &bytes,
            png_sizes.as_deref(),
            ico_sizes.as_deref(),
        )
    })
    .await
    .map_err(|e| AppError::Image(e.to_string()))??;

    std::fs::write(&save_path, &result)?;
    Ok(())
}

/// 批量导出多个历史图标到指定目录（每个图标一个 ZIP）
/// 前端用 dialog open(目录) 拿到 dir 后调用
#[tauri::command]
pub async fn export_icons_to_dir(
    state: tauri::State<'_, crate::AppState>,
    icon_ids: Vec<String>,
    dir: String,
    png_sizes: Option<Vec<u32>>,
    ico_sizes: Option<Vec<u32>>,
) -> Result<usize, AppError> {
    // 一次加锁，批量取 (id, concept, bytes)，避免循环里反复加锁
    let items: Vec<(String, String, Vec<u8>)> = {
        let storage = state.storage.lock();
        let mut out = Vec::new();
        for id in &icon_ids {
            if let Some(bytes) = storage.get_icon_bytes(id)? {
                // concept 从 list_icons 拿不到单条，直接查
                let concept = storage.get_icon_concept(id)?.unwrap_or_default();
                out.push((id.clone(), concept, bytes));
            }
        }
        out
    };

    let png = png_sizes;
    let ico = ico_sizes;
    let dir = std::path::PathBuf::from(&dir);

    let count = tokio::task::spawn_blocking(move || -> Result<usize, AppError> {
        std::fs::create_dir_all(&dir)?;
        let mut ok = 0;
        for (id, concept, bytes) in &items {
            match services::export::export_all(bytes, png.as_deref(), ico.as_deref()) {
                Ok(zip_bytes) => {
                    // 文件名：concept（清理非法字符）_id前8位.zip
                    let safe_name = sanitize_filename(concept);
                    let fname = if safe_name.is_empty() {
                        format!("{}.zip", &id[..id.len().min(8)])
                    } else {
                        format!("{}_{}.zip", safe_name, &id[..id.len().min(8)])
                    };
                    let path = dir.join(fname);
                    if std::fs::write(&path, &zip_bytes).is_ok() {
                        ok += 1;
                    }
                }
                Err(e) => log::warn!("[导出] 图标 {} 失败: {}", id, e),
            }
        }
        Ok(ok)
    })
    .await
    .map_err(|e| AppError::Image(e.to_string()))??;

    Ok(count)
}

/// 把 concept 转成安全的文件名（去 Windows 非法字符，限长）
fn sanitize_filename(s: &str) -> String {
    s.chars()
        .filter(|c| !matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
        .take(30)
        .collect::<String>()
        .trim()
        .to_string()
}
