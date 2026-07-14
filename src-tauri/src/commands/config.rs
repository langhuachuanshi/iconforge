use tauri::State;

use crate::error::AppError;
use crate::models::{ProviderEntry, ProviderUpsertRequest};
use crate::AppState;

/// 获取所有配置（API Key 等）
#[tauri::command]
pub async fn get_config(state: State<'_, AppState>) -> Result<std::collections::HashMap<String, String>, AppError> {
    let storage = state.storage.lock();
    storage.get_all_config()
}

/// 设置配置值
#[tauri::command]
pub async fn set_config(state: State<'_, AppState>, key: String, value: String) -> Result<(), AppError> {
    let storage = state.storage.lock();
    storage.set_config(&key, &value)
}

/// 列出所有服务商
#[tauri::command]
pub async fn list_providers(state: State<'_, AppState>) -> Result<Vec<ProviderEntry>, AppError> {
    let storage = state.storage.lock();
    storage.list_providers()
}

/// 新增服务商
#[tauri::command]
pub async fn add_provider(state: State<'_, AppState>, req: ProviderUpsertRequest) -> Result<ProviderEntry, AppError> {
    let storage = state.storage.lock();
    storage.add_provider(&req)
}

/// 更新服务商
#[tauri::command]
pub async fn update_provider(state: State<'_, AppState>, id: String, req: ProviderUpsertRequest) -> Result<(), AppError> {
    let storage = state.storage.lock();
    storage.update_provider(&id, &req)
}

/// 删除服务商（仅自定义）
#[tauri::command]
pub async fn delete_provider(state: State<'_, AppState>, id: String) -> Result<(), AppError> {
    let storage = state.storage.lock();
    storage.delete_provider(&id)
}

/// 重新排序服务商
#[tauri::command]
pub async fn reorder_providers(state: State<'_, AppState>, ids: Vec<String>) -> Result<(), AppError> {
    let storage = state.storage.lock();
    storage.reorder_providers(&ids)
}

/// 启用/禁用服务商
#[tauri::command]
pub async fn toggle_provider(state: State<'_, AppState>, id: String, enabled: bool) -> Result<(), AppError> {
    let storage = state.storage.lock();
    storage.toggle_provider(&id, enabled)
}
