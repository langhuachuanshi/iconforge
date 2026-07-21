use crate::error::AppError;
use crate::models::{ExtractIconsRequest, ExtractedIcon};
use crate::services;

/// 从 PE 文件提取所有图标
#[tauri::command]
pub async fn extract_icons(req: ExtractIconsRequest) -> Result<Vec<ExtractedIcon>, AppError> {
    let path = req.file_path.clone();
    let result = tokio::task::spawn_blocking(move || {
        let bytes = std::fs::read(&path)?;
        services::extract::extract_pe_icons(&bytes)
    })
    .await
    .map_err(|e| AppError::Image(e.to_string()))??;
    Ok(result)
}
