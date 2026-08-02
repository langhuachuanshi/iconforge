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
            supported_sizes: parse_supported_sizes(&p.supported_sizes),
            configured: !p.api_key.is_empty(),
        })
        .collect())
}

/// 把 "1024x1024,720x1280" 解析成 Vec，空则回退默认
fn parse_supported_sizes(s: &str) -> Vec<String> {
    let v: Vec<String> = s
        .split(',')
        .map(|x| x.trim().to_string())
        .filter(|x| !x.is_empty())
        .collect();
    if v.is_empty() { vec!["1024x1024".into()] } else { v }
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
            category: t.category.to_string(),
            prompt_prefix: t.prompt_prefix.to_string(),
        })
        .collect()
}

/// 引导式 prompt 组装：模板前缀 + extra + 统一收尾
fn assemble_guided_prompt(req: &GenerateRequest) -> Result<String, AppError> {
    let tpl = templates::get_template(&req.style)
        .ok_or_else(|| AppError::NotFound(format!("风格模板 {} 不存在", req.style)))?;
    let mut prompt = tpl.prompt_prefix.replace("{concept}", &req.concept);
    if let Some(extra) = &req.extra {
        if !extra.is_empty() {
            prompt.push_str(". ");
            prompt.push_str(extra);
        }
    }
    prompt.push_str(". Centered composition, professional app icon, readable at small sizes");
    Ok(prompt)
}

/// 生成图标
#[tauri::command]
pub async fn generate_icon(
    state: State<'_, AppState>,
    req: GenerateRequest,
) -> Result<GenerateResponse, AppError> {
    // 1. 组装 prompt
    let prompt = if let Some(raw) = &req.raw_prompt {
        if !raw.trim().is_empty() {
            // 专家模式：用户直接传完整提示词，跳过模板/概念拼接
            let mut p = raw.trim().to_string();
            p.push_str(". Centered composition, professional app icon, readable at small sizes");
            p
        } else {
            assemble_guided_prompt(&req)?
        }
    } else {
        assemble_guided_prompt(&req)?
    };

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
    log::info!("[生成] prompt={}", prompt.chars().take(200).collect::<String>());
    let result = OpenAiProvider::generate(
        &config,
        &prompt,
        &req.size,
        req.negative_prompt.as_deref(),
        req.seed,
    ).await?;

    // 4. 保存到历史
    let meta = {
        let storage = state.storage.lock();
        storage.save_icon(&result.image, &req.concept, &req.style, &req.provider, &prompt)?
    };

    let image_b64 = base64::engine::general_purpose::STANDARD.encode(&result.image);

    Ok(GenerateResponse {
        image: image_b64,
        format: result.format,
        icon_id: meta.id,
    })
}
