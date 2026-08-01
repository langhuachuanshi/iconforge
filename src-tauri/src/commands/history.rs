use base64::Engine;
use tauri::State;

use crate::error::AppError;
use crate::models::{IconListResponse, ImageResponse, SaveVersionRequest, VersionMeta};
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

// ── 图标编辑版本（工程文件）──

/// 保存当前编辑状态为该图标的新版本（存档点）
#[tauri::command]
pub async fn save_icon_version(
    state: State<'_, AppState>,
    req: SaveVersionRequest,
) -> Result<VersionMeta, AppError> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(&req.image)?;
    let icon_id = req.icon_id.clone();
    let note = req.note.clone();
    let meta = {
        let storage = state.storage.lock();
        storage.save_version(&icon_id, &bytes, &note)?
    };
    Ok(meta)
}

/// 列出某图标所有编辑版本（最新在前）
#[tauri::command]
pub async fn list_icon_versions(
    state: State<'_, AppState>,
    icon_id: String,
) -> Result<Vec<VersionMeta>, AppError> {
    let storage = state.storage.lock();
    Ok(storage.list_versions(&icon_id)?)
}

/// 按 version_id 加载某版本的图（用于回溯历史版本继续编辑）
#[tauri::command]
pub async fn load_icon_version(
    state: State<'_, AppState>,
    version_id: String,
) -> Result<ImageResponse, AppError> {
    let bytes = {
        let storage = state.storage.lock();
        storage
            .version_bytes_by_id(&version_id)?
            .ok_or_else(|| AppError::NotFound(format!("版本 {} 不存在", version_id)))?
    };
    Ok(ImageResponse {
        image: base64::engine::general_purpose::STANDARD.encode(&bytes),
        format: "PNG".into(),
    })
}

/// 删除某编辑版本（原图不可删，需用 delete_icon）
#[tauri::command]
pub async fn delete_icon_version(
    state: State<'_, AppState>,
    version_id: String,
) -> Result<(), AppError> {
    let storage = state.storage.lock();
    if !storage.delete_version(&version_id)? {
        return Err(AppError::NotFound(format!("版本 {} 不存在", version_id)));
    }
    Ok(())
}
