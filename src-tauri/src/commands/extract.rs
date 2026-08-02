use base64::Engine;

use crate::error::AppError;
use crate::models::{ExtractIconsFromBytesRequest, ExtractIconsRequest, ExtractedIcon};
use crate::services;

/// 从 PE 文件提取所有图标（按文件路径）
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

/// 从 PE 文件字节提取所有图标（拖拽场景，前端传 base64）
#[tauri::command]
pub async fn extract_icons_from_bytes(
    req: ExtractIconsFromBytesRequest,
) -> Result<Vec<ExtractedIcon>, AppError> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&req.data)
        .map_err(|e| AppError::Image(format!("base64 解码失败: {e}")))?;
    let result = tokio::task::spawn_blocking(move || services::extract::extract_pe_icons(&bytes))
        .await
        .map_err(|e| AppError::Image(e.to_string()))??;
    Ok(result)
}
