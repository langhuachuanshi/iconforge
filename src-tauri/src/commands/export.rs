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
