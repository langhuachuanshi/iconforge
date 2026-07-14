use std::time::Duration;

use reqwest::Client;
use serde_json::Value;
use tokio::time::sleep;

use crate::error::AppError;
use crate::models::ProviderEntry;

#[derive(Debug)]
pub struct GenerateResult {
    pub image: Vec<u8>,
    pub format: String,
}

pub struct OpenAiProvider;

impl OpenAiProvider {
    pub async fn generate(
        config: &ProviderEntry,
        prompt: &str,
        size: &str,
    ) -> Result<GenerateResult, AppError> {
        let api_key = config.api_key.trim();
        if api_key.is_empty() {
            return Err(AppError::ProviderError(format!("{} 的 API Key 未配置", config.name)));
        }
        if config.endpoint.trim().is_empty() {
            return Err(AppError::ProviderError(format!("{} 的请求地址未配置", config.name)));
        }

        // 自动检测协议：maas → DashScope 异步，dashscope → DashScope 异步，其他 → OpenAI 兼容
        if config.endpoint.contains("maas") {
            return Self::generate_dashscope_maas(config, prompt, size, api_key).await;
        }
        if config.endpoint.contains("dashscope") {
            return Self::generate_dashscope(config, prompt, size, api_key).await;
        }

        Self::generate_openai(config, prompt, size, api_key).await
    }

    /// OpenAI 兼容格式
    async fn generate_openai(
        config: &ProviderEntry,
        prompt: &str,
        size: &str,
        api_key: &str,
    ) -> Result<GenerateResult, AppError> {
        let client = Client::builder().timeout(Duration::from_secs(120)).build()?;
        let url = config.endpoint.trim();
        let model = if config.model.is_empty() { "default" } else { config.model.as_str() };

        let body = serde_json::json!({
            "model": model,
            "prompt": prompt,
            "n": 1,
            "size": size,
            "response_format": "b64_json"
        });

        log::info!("[OpenAI] POST {} model={}", url, model);

        let resp = client.post(url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&body).send().await
            .map_err(|e| AppError::ProviderError(format!("请求失败: {e}")))?;

        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            log::error!("[OpenAI] HTTP {} body={}", status, &text[..text.len().min(500)]);
            return Err(AppError::ProviderError(format!("{} 返回错误 ({}): {}", config.name, status, &text[..text.len().min(300)])));
        }

        let data: Value = resp.json().await
            .map_err(|e| AppError::ProviderError(format!("解析响应失败: {e}")))?;

        if let Some(b64) = data["data"][0]["b64_json"].as_str() {
            use base64::Engine;
            let image = base64::engine::general_purpose::STANDARD.decode(b64)
                .map_err(|e| AppError::ProviderError(format!("base64 解码失败: {e}")))?;
            return Ok(GenerateResult { image, format: "PNG".into() });
        }

        if let Some(img_url) = data["data"][0]["url"].as_str() {
            let img_resp = client.get(img_url).timeout(Duration::from_secs(60)).send().await
                .map_err(|e| AppError::ProviderError(format!("下载图片失败: {e}")))?;
            let image = img_resp.bytes().await
                .map_err(|e| AppError::ProviderError(format!("读取图片失败: {e}")))?;
            return Ok(GenerateResult { image: image.to_vec(), format: "PNG".into() });
        }

        Err(AppError::ProviderError(format!("{} 返回数据中未找到图片", config.name)))
    }

    /// MaaS 工作空间 API（multimodal-generation）
    async fn generate_dashscope_maas(
        config: &ProviderEntry,
        prompt: &str,
        size: &str,
        api_key: &str,
    ) -> Result<GenerateResult, AppError> {
        let client = Client::builder().timeout(Duration::from_secs(120)).build()?;
        let model = if config.model.is_empty() { "qwen-image-2.0-pro" } else { config.model.as_str() };
        let url = format!("{}/api/v1/services/aigc/multimodal-generation/generation", config.endpoint.trim_end_matches('/'));
        let maas_size = size.replace('x', "*");

        let body = serde_json::json!({
            "model": model,
            "input": {
                "messages": [{"role": "user", "content": [{"text": prompt}]}]
            },
            "parameters": {"size": maas_size}
        });

        log::info!("[MaaS] POST {} model={}", url, model);

        let resp = client.post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&body).send().await
            .map_err(|e| AppError::ProviderError(format!("请求失败: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let text = resp.text().await.unwrap_or_default();
            log::error!("[MaaS] HTTP {} body={}", status, &text[..text.len().min(500)]);
            return Err(AppError::ProviderError(format!("{} 返回错误 ({}): {}", config.name, status, &text[..text.len().min(300)])));
        }

        let data: Value = resp.json().await
            .map_err(|e| AppError::ProviderError(format!("解析响应失败: {e}")))?;

        let img_url = data["output"]["choices"][0]["message"]["content"][0]["image"]
            .as_str()
            .ok_or_else(|| AppError::ProviderError(format!("{} 返回数据中未找到图片", config.name)))?;

        log::info!("[MaaS] 下载图片: {}", &img_url[..img_url.len().min(80)]);

        let dl = Client::builder().timeout(Duration::from_secs(60)).build()?;
        let img = dl.get(img_url).send().await
            .map_err(|e| AppError::ProviderError(format!("下载图片失败: {e}")))?;
        let bytes = img.bytes().await
            .map_err(|e| AppError::ProviderError(format!("读取图片失败: {e}")))?;

        Ok(GenerateResult { image: bytes.to_vec(), format: "PNG".into() })
    }

    /// DashScope 原生异步 API（提交 + 轮询）
    async fn generate_dashscope(
        config: &ProviderEntry,
        prompt: &str,
        size: &str,
        api_key: &str,
    ) -> Result<GenerateResult, AppError> {
        let client = Client::builder().timeout(Duration::from_secs(30)).build()?;
        let model = if config.model.is_empty() { "qwen-image-2.0-pro" } else { config.model.as_str() };
        let ds_size = size.replace('x', "*");

        // 1. 提交任务
        let submit_url = "https://dashscope.aliyuncs.com/api/v1/services/aigc/text2image/image-synthesis";
        let body = serde_json::json!({
            "model": model,
            "input": { "prompt": prompt },
            "parameters": { "size": ds_size, "n": 1 }
        });

        log::info!("[DashScope] POST {} model={}", submit_url, model);

        let resp = client.post(submit_url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("X-DashScope-Async", "enable")
            .header("Content-Type", "application/json")
            .json(&body).send().await
            .map_err(|e| AppError::ProviderError(format!("提交任务失败: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let text = resp.text().await.unwrap_or_default();
            return Err(AppError::ProviderError(format!("提交失败 ({}): {}", status, &text[..text.len().min(300)])));
        }

        let task_id = resp.json::<Value>().await
            .map_err(|e| AppError::ProviderError(format!("解析响应失败: {e}")))?
            .get("output").and_then(|o| o.get("task_id")).and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| AppError::ProviderError("未获取到 task_id".into()))?;

        log::info!("[DashScope] task_id={}", task_id);

        // 2. 轮询任务
        let poll_url = format!("https://dashscope.aliyuncs.com/api/v1/tasks/{}", task_id);
        let deadline = tokio::time::Instant::now() + Duration::from_secs(180);

        loop {
            if tokio::time::Instant::now() > deadline {
                return Err(AppError::ProviderError("任务超时 (180s)".into()));
            }

            let r = client.get(&poll_url)
                .header("Authorization", format!("Bearer {}", api_key))
                .send().await
                .map_err(|e| AppError::ProviderError(format!("查询任务失败: {e}")))?;

            if !r.status().is_success() {
                let s = r.status().as_u16();
                let b = r.text().await.unwrap_or_default();
                return Err(AppError::ProviderError(format!("查询失败 ({}): {}", s, &b[..b.len().min(200)])));
            }

            let data = r.json::<Value>().await
                .map_err(|e| AppError::ProviderError(format!("解析失败: {e}")))?;

            match data.get("output").and_then(|o| o.get("task_status")).and_then(|v| v.as_str()) {
                Some("SUCCEEDED") => {
                    let img_url = data.get("output").and_then(|o| o.get("results"))
                        .and_then(|r| r.get(0)).and_then(|r| r.get("url"))
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| AppError::ProviderError("任务完成但未返回图片 URL".into()))?;

                    let dl = Client::builder().timeout(Duration::from_secs(60)).build()?;
                    let img = dl.get(img_url).send().await
                        .map_err(|e| AppError::ProviderError(format!("下载图片失败: {e}")))?;
                    let bytes = img.bytes().await
                        .map_err(|e| AppError::ProviderError(format!("读取图片失败: {e}")))?;

                    return Ok(GenerateResult { image: bytes.to_vec(), format: "PNG".into() });
                }
                Some("FAILED") => {
                    let msg = data.get("output").and_then(|o| o.get("message"))
                        .and_then(|v| v.as_str()).unwrap_or("未知错误");
                    return Err(AppError::ProviderError(format!("任务失败: {}", msg)));
                }
                Some("PENDING" | "RUNNING") => { sleep(Duration::from_secs(3)).await; }
                Some(s) => return Err(AppError::ProviderError(format!("未知状态: {}", s))),
                None => { sleep(Duration::from_secs(3)).await; }
            }
        }
    }
}
