use base64::Engine;
use tauri::State;

use crate::error::AppError;
use crate::models::{CropRequest, ImageResponse, RemoveBgRequest};
use crate::services;
use crate::AppState;

/// 裁剪图片
#[tauri::command]
pub async fn crop_image(req: CropRequest) -> Result<ImageResponse, AppError> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(&req.image)?;

    let result = tokio::task::spawn_blocking(move || {
        services::image::crop(&bytes, req.x as u32, req.y as u32, req.width, req.height)
    })
    .await
    .map_err(|e| AppError::Image(e.to_string()))??;

    Ok(ImageResponse {
        image: base64::engine::general_purpose::STANDARD.encode(&result),
        format: "PNG".into(),
    })
}

/// 保存当前图片到指定路径
#[tauri::command]
pub async fn save_image_file(save_path: String, image: String) -> Result<(), AppError> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(&image)?;
    std::fs::write(&save_path, &bytes)?;
    Ok(())
}

/// 导入本地模型文件
#[tauri::command]
pub async fn import_bg_model(
    state: State<'_, AppState>,
    source_path: String,
    model_id: Option<String>,
) -> Result<(), AppError> {
    let mid = model_id.unwrap_or_else(|| "rmbg-1.4".into());
    let m = services::remove_bg::get_model(&mid);
    let target = {
        let storage = state.storage.lock();
        services::remove_bg::model_path(storage.base_dir(), m.filename)
    };
    std::fs::copy(&source_path, &target)?;
    Ok(())
}

fn get_model_id(storage: &crate::services::storage::Storage) -> String {
    let id = storage.get_config("bg_model", "crispcut-quality");
    if id.is_empty() { "crispcut-quality".into() } else { id }
}

/// 检查抠图模型是否已下载
#[tauri::command]
pub async fn check_bg_model(state: State<'_, AppState>) -> Result<serde_json::Value, AppError> {
    let storage = state.storage.lock();
    let mid = get_model_id(&storage);
    let has = services::remove_bg::model_exists(storage.base_dir(), &mid);
    Ok(serde_json::json!({"downloaded": has, "model": mid}))
}

/// 下载抠图模型（含进度事件）
#[tauri::command]
pub async fn download_bg_model(
    window: tauri::Window,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    let (model_dir, mid) = {
        let storage = state.storage.lock();
        (storage.base_dir().to_path_buf(), get_model_id(&storage))
    };
    services::remove_bg::download_model(&window, &model_dir, &mid).await
}

/// 移除背景
#[tauri::command]
pub async fn remove_background(
    state: State<'_, AppState>,
    req: RemoveBgRequest,
) -> Result<ImageResponse, AppError> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(&req.image)?;
    let (model_dir, mid) = {
        let storage = state.storage.lock();
        (storage.base_dir().to_path_buf(), get_model_id(&storage))
    };
    let threshold = req.threshold.clamp(0.0, 1.0);

    let result = tokio::task::spawn_blocking(move || {
        services::remove_bg::run_inference(&model_dir, &bytes, threshold, &mid)
    })
    .await
    .map_err(|e| AppError::Image(e.to_string()))??;

    Ok(ImageResponse {
        image: base64::engine::general_purpose::STANDARD.encode(&result),
        format: "PNG".into(),
    })
}
