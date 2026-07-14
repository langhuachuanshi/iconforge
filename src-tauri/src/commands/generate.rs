use base64::Engine;
use tauri::State;

use crate::data::templates;
use crate::error::AppError;
use crate::models::{GenerateRequest, GenerateResponse, ProviderInfo, Template};
use crate::providers::OpenAiProvider;
use crate::AppState;

/// 列出所有 AI 服务商及配置状态
#[tauri::command]
pub async fn get_providers(state: State<'_, AppState>) -> Result<Vec<ProviderInfo>, AppError> {
    let storage = state.storage.lock();
    let providers = storage.list_providers()?;
    Ok(providers
        .iter()
        .filter(|p| p.enabled)
        .map(|p| ProviderInfo {
            name: p.name.clone(),
            display_name: p.name.clone(),
            config_key: p.id.clone(),
            supported_sizes: vec!["1024x1024".into()],
            configured: !p.api_key.is_empty(),
        })
        .collect())
}

/// 获取提示词风格模板列表
#[tauri::command]
pub fn get_templates() -> Vec<Template> {
    templates::TEMPLATES
        .iter()
        .map(|t| Template {
            id: t.id.to_string(),
            name: t.name.to_string(),
            description: t.description.to_string(),
        })
        .collect()
}

/// 生成图标
#[tauri::command]
pub async fn generate_icon(
    state: State<'_, AppState>,
    req: GenerateRequest,
) -> Result<GenerateResponse, AppError> {
    // 1. 组装 prompt：模板 + 步骤填充
    let tpl = templates::get_template(&req.style)
        .ok_or_else(|| AppError::NotFound(format!("风格模板 {} 不存在", req.style)))?;
    let mut prompt = tpl.prompt_prefix.replace("{concept}", &req.concept);
    // 额外指令追加到末尾（背景、细节、补充等）
    if let Some(extra) = &req.extra {
        if !extra.is_empty() {
            prompt.push_str(". ");
            prompt.push_str(extra);
        }
    }
    // 统一收尾：图标质量约束
    prompt.push_str(". Centered composition, professional app icon, readable at small sizes");

    // 2. 从 DB 获取服务商配置
    let config = {
        let storage = state.storage.lock();
        let all = storage.list_providers()?;
        all.into_iter()
            .find(|p| p.id == req.provider || p.name == req.provider)
            .ok_or_else(|| AppError::NotFound(format!("服务商 {} 不存在", req.provider)))?
    };

    if !config.enabled {
        return Err(AppError::ProviderError(format!(
            "{} 已禁用", config.name
        )));
    }

    // 3. 调用 OpenAI 兼容 API
    log::info!("[生成] 服务商={} endpoint={} model={} size={}", config.name, config.endpoint, config.model, req.size);
    log::info!("[生成] prompt={}", &prompt[..prompt.len().min(200)]);
    let result = OpenAiProvider::generate(&config, &prompt, &req.size).await?;

    // 4. 保存到历史
    let meta = {
        let storage = state.storage.lock();
        storage.save_icon(&result.image, &req.concept, &req.style, &req.provider)?
    };

    let image_b64 = base64::engine::general_purpose::STANDARD.encode(&result.image);

    Ok(GenerateResponse {
        image: image_b64,
        format: result.format,
        icon_id: meta.id,
    })
}
