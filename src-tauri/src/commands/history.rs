use base64::Engine;
use tauri::State;

use crate::error::AppError;
use crate::models::{IconListResponse, ImageResponse};
use crate::AppState;

/// 列出图标历史
#[tauri::command]
pub async fn list_icons(
    state: State<'_, AppState>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<IconListResponse, AppError> {
    let storage = state.storage.lock();
    let limit = limit.unwrap_or(100);
    let offset = offset.unwrap_or(0);
    let icons = storage.list_icons(limit, offset)?;
    let count = storage.count_icons()?;
    Ok(IconListResponse { icons, count })
}

/// 获取图标 base64（从历史加载到编辑页）
#[tauri::command]
pub async fn get_icon_base64(
    state: State<'_, AppState>,
    icon_id: String,
) -> Result<ImageResponse, AppError> {
    let storage = state.storage.lock();
    let bytes = storage
        .get_icon_bytes(&icon_id)?
        .ok_or_else(|| AppError::NotFound(format!("图标 {} 不存在", icon_id)))?;

    Ok(ImageResponse {
        image: base64::engine::general_purpose::STANDARD.encode(&bytes),
        format: "PNG".into(),
    })
}

/// 获取图标文件路径（用于 convertFileSrc 直接展示）
#[tauri::command]
pub async fn get_icon_path(
    state: State<'_, AppState>,
    icon_id: String,
) -> Result<String, AppError> {
    let storage = state.storage.lock();
    let path = storage
        .get_icon_path(&icon_id)?
        .ok_or_else(|| AppError::NotFound(format!("图标 {} 不存在", icon_id)))?;

    Ok(path.to_string_lossy().to_string())
}

/// 删除图标
#[tauri::command]
pub async fn delete_icon(
    state: State<'_, AppState>,
    icon_id: String,
) -> Result<(), AppError> {
    let storage = state.storage.lock();
    if !storage.delete_icon(&icon_id)? {
        return Err(AppError::NotFound(format!("图标 {} 不存在", icon_id)));
    }
    Ok(())
}
