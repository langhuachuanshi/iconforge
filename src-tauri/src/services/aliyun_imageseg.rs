//! 阿里云 VIAPI 分割抠图云端调用（SegmentCommonImage / SegmentCommodity）
//!
//! 流程（图片只接受 URL，所以先上传到阿里云官方临时 Bucket）：
//!   ① GetOssStsToken —— 拿临时 STS 凭证
//!   ② OSS PostObject 上传 —— 上传到官方 Bucket，拼出公网 URL
//!   ③ {Action} —— 用 URL 调抠图，下载返回的透明 PNG
//!
//! 两个 Action 同 Version/同入参 ImageURL/同返回 Data.ImageURL/同 cn-shanghai，
//! 仅 Action 名不同，故参数化 action 名复用整条流水线。
//!
//! 签名：RPC 调用用 V3（ACS3-HMAC-SHA256，见 aliyun_sign）；OSS PostObject 用 V1（HMAC-SHA1）。
//! 每步通过 logger 回调输出诊断信息（请求/响应/错误），便于调试云端流水线。
//!
//! 参考：
//!   - 小程序直传方案 https://help.aliyun.com/zh/viapi/developer-reference/small-application-scenario-called-directly
//!   - SegmentCommonImage https://help.aliyun.com/zh/viapi/developer-reference/api-k8cs8t
//!   - V3 签名 https://www.alibabacloud.com/help/en/sdk/product-overview/v3-request-structure-and-signature

use std::time::Duration;

use base64::Engine;
use hmac::{Hmac, Mac};
use reqwest::Client;
use serde_json::Value;
use sha1::Sha1;

use crate::error::AppError;
use crate::services::aliyun_sign;

type HmacSha1 = Hmac<Sha1>;
type Logger = dyn Fn(&str) + Send + Sync;

const UTILS_HOST: &str = "viapiutils.cn-shanghai.aliyuncs.com";
const SEG_HOST: &str = "imageseg.cn-shanghai.aliyuncs.com";
/// 阿里云 VIAPI 官方临时上传桶（GetOssStsToken 不返回，按官方 Demo 写死）
const OSS_BUCKET_HOST: &str = "https://viapi-customer-temp.oss-cn-shanghai.aliyuncs.com";
const OSS_BUCKET_NAME: &str = "viapi-customer-temp";

/// 云端抠图入口：本地图片字节 → 透明 PNG 字节
///
/// `logger`：诊断回调，每步关键信息（请求/响应/错误）会经它输出，传给前端 console。
pub async fn remove_background(
    image_bytes: &[u8],
    access_key_id: &str,
    access_key_secret: &str,
    model: &str,
    logger: &Logger,
) -> Result<Vec<u8>, AppError> {
    let action = action_for_model(model);
    logger(&format!("[Aliyun] 开始云端抠图，model={model} → action={action}"));

    // ① 拿 STS 凭证
    let sts = get_oss_sts_token(access_key_id, access_key_secret, logger).await?;
    logger("[Aliyun] ① GetOssStsToken 成功，拿到临时凭证");

    // ② 上传到官方临时 Bucket，拿公网 URL
    // 关键：object key 前缀必须用「调用 GetOssStsToken 时的永久 AK」，不是临时 STS AK。
    // 见 https://help.aliyun.com/zh/viapi/getting-started/the-file-url-processing
    // SessionPolicy 的 Resource 限定为 viapi-customer-temp/{永久AK}/*，用临时 AK 前缀会 ImplicitDeny。
    let image_url = upload_to_oss(access_key_id, &sts, image_bytes, logger).await?;
    logger(&format!("[Aliyun] ② OSS 上传成功，ImageURL={}", &image_url[..image_url.len().min(80)]));

    // ③ 调抠图，拿结果 URL
    let result_url = segment_image(&image_url, access_key_id, access_key_secret, action, logger).await?;
    logger(&format!("[Aliyun] ③ {action} 成功，结果 URL={}", &result_url[..result_url.len().min(80)]));

    // ④ 下载结果（透明 PNG）
    let png_bytes = download_result(&result_url, logger).await?;
    logger(&format!("[Aliyun] ④ 下载完成，{} bytes", png_bytes.len()));
    Ok(png_bytes)
}

/// 模型标识 → 阿里云 Action 名。未知值回落 common。
fn action_for_model(model: &str) -> &'static str {
    match model {
        "commodity" => "SegmentCommodity",
        _ => "SegmentCommonImage",
    }
}

// ────────────────────────────────────────────────────────────────
// ① GetOssStsToken（V3 签名）
// ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct OssSts {
    access_key_id: String,
    access_key_secret: String,
    security_token: String,
}

async fn get_oss_sts_token(ak: &str, sk: &str, logger: &Logger) -> Result<OssSts, AppError> {
    let params = [("RegionId", "cn-shanghai".to_string())];
    let (headers, url) = aliyun_sign::build_v3_request(
        UTILS_HOST, "POST", "GetOssStsToken", "2020-04-01", &params, ak, sk, None,
    )?;

    logger("[Aliyun] ① POST GetOssStsToken");
    let client = Client::builder().timeout(Duration::from_secs(30)).build()?;
    let resp = client
        .post(&url)
        .headers(headers)
        .send()
        .await
        .map_err(|e| AppError::ProviderError(format!("请求 GetOssStsToken 失败: {e}")))?;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    logger(&format!("[Aliyun] ① GetOssStsToken 响应 ({}): {}", status.as_u16(), &text[..text.len().min(800)]));
    if !status.is_success() {
        return Err(AppError::ProviderError(format!(
            "GetOssStsToken 返回错误 ({}): {}",
            status.as_u16(),
            &text[..text.len().min(500)]
        )));
    }
    let data: Value = serde_json::from_str(&text)
        .map_err(|e| AppError::ProviderError(format!("解析 GetOssStsToken 响应失败: {e}")))?;

    let d = data.get("Data").unwrap_or(&data);
    // 诊断：枚举 Data 的所有字段名，并打印非凭证字段（凭证字段值太长，跳过）
    if let Some(obj) = d.as_object() {
        let keys: Vec<&String> = obj.keys().collect();
        let non_secret: Vec<String> = obj.iter()
            .filter(|(k, _)| !k.ends_with("SecurityToken") && !k.ends_with("Token"))
            .filter_map(|(k, v)| {
                let s = match v {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                if s.len() > 100 { format!("{k}={:.80}...(len={})", s, s.len()) } else { format!("{k}={s}") }.into()
            })
            .collect();
        logger(&format!("[Aliyun] ① Data 字段: {:?}", keys));
        logger(&format!("[Aliyun] ① Data 非凭证字段: {}", non_secret.join(" | ")));
    }
    let sts = OssSts {
        access_key_id: field(d, &["AccessKeyId", "accessKeyId"]),
        access_key_secret: field(d, &["AccessKeySecret", "accessKeySecret"]),
        security_token: field(d, &["SecurityToken", "securityToken"]),
    };
    if sts.access_key_id.is_empty() || sts.security_token.is_empty() {
        return Err(AppError::ProviderError(format!(
            "GetOssStsToken 未返回有效凭证（字段名可能不符）: {}",
            &text[..text.len().min(500)]
        )));
    }
    Ok(sts)
}

// ────────────────────────────────────────────────────────────────
// ② OSS PutObject 上传（V1 签名 Authorization header）
// 注意：必须用 PutObject（PUT），不能用 PostObject（POST 表单）。
// VIAPI 签发的临时凭证 SessionPolicy 只放行 oss:PutObject，PostObject 会被 ImplicitDeny。
//
// 关键：鉴权时间必须走 x-oss-date header（不是普通 Date）。照 ali-oss SDK 的实际行为：
//   - createRequest.js 永远发 x-oss-date，不发 Date
//   - signUtils.buildCanonicalString 第 4 项取 expires || headers['x-oss-date']，
//     且 x-oss-date / x-oss-security-token 都会再进 CanonicalizedOSSHeaders 各自带 \n
// 用普通 Date 会导致 AccessDenied（OSS 拿不到合法鉴权时间，走不到策略评估）。
// ────────────────────────────────────────────────────────────────

async fn upload_to_oss(
    caller_ak: &str,
    sts: &OssSts,
    image_bytes: &[u8],
    logger: &Logger,
) -> Result<String, AppError> {
    // object key 前缀用「调用 GetOssStsToken 时的永久 AK」（LTAI 开头），不是临时 STS AK。
    // 临时 AK 当前缀会被 SessionPolicy ImplicitDeny。
    let object_key = format!("{}/{}/{}.png", caller_ak, uuid_str(), "iconforge");
    let x_oss_date = chrono::Utc::now().format("%a, %d %b %Y %H:%M:%S GMT").to_string();

    // OSS V1 签名 StringToSign（6 行，照 ali-oss signUtils.buildCanonicalString）：
    //   VERB\nContent-MD5\nContent-Type\n(expires||x-oss-date)\nCanonicalizedOSSHeaders\nCanonicalizedResource
    // 这里：Content-MD5 空；CanonicalizedOSSHeaders = x-oss-date 和 x-oss-security-token 各一行带 \n（字典序）。
    let canonical_oss_headers = format!(
        "x-oss-date:{x_oss_date}\nx-oss-security-token:{}\n",
        sts.security_token
    );
    let canonical_resource = format!("/{}/{}", OSS_BUCKET_NAME, object_key);
    let string_to_sign = format!(
        "PUT\n\nimage/png\n{x_oss_date}\n{canonical_oss_headers}{canonical_resource}"
    );
    logger(&format!("[Aliyun] ② PutObject StringToSign:\n{}", string_to_sign));

    // signature = base64(HMAC-SHA1(AccessKeySecret, StringToSign))，STS 凭证用裸 secret
    let mut mac = HmacSha1::new_from_slice(sts.access_key_secret.as_bytes())
        .map_err(|e| AppError::ProviderError(format!("HMAC 初始化失败: {e}")))?;
    mac.update(string_to_sign.as_bytes());
    let signature = base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes());
    let authorization = format!("OSS {}:{}", sts.access_key_id, signature);

    let url = format!("{}/{}", OSS_BUCKET_HOST.trim_end_matches('/'), object_key);
    logger(&format!("[Aliyun] ② PUT OSS 上传 ({} bytes), key={}", image_bytes.len(), object_key));

    let client = Client::builder().timeout(Duration::from_secs(60)).build()?;
    let resp = client
        .put(&url)
        .header("Authorization", &authorization)
        .header("x-oss-date", &x_oss_date)
        .header("Content-Type", "image/png")
        .header("x-oss-security-token", &sts.security_token)
        .header("Content-Length", image_bytes.len().to_string())
        .body(image_bytes.to_vec())
        .send()
        .await
        .map_err(|e| AppError::ProviderError(format!("OSS 上传请求失败: {e}")))?;
    let status = resp.status();
    // 200 成功时 body 为空；失败时 body 是 XML，记录用于诊断
    let text = resp.text().await.unwrap_or_default();
    logger(&format!("[Aliyun] ② OSS 响应 ({}): {}", status.as_u16(), &text[..text.len().min(800)]));
    if !status.is_success() {
        return Err(AppError::ProviderError(format!(
            "OSS 上传失败 ({}): {}",
            status.as_u16(),
            &text[..text.len().min(500)]
        )));
    }

    logger(&format!("[Aliyun] ② OSS 上传完成，ImageURL={}", &url[..url.len().min(80)]));
    Ok(url)
}

// ────────────────────────────────────────────────────────────────
// ③ 抠图 Action（V3 签名）
// ────────────────────────────────────────────────────────────────

async fn segment_image(
    image_url: &str,
    ak: &str,
    sk: &str,
    action: &str,
    logger: &Logger,
) -> Result<String, AppError> {
    let params = [
        ("RegionId", "cn-shanghai".to_string()),
        ("ImageURL", image_url.to_string()),
    ];
    let (headers, url) =
        aliyun_sign::build_v3_request(SEG_HOST, "POST", action, "2019-12-30", &params, ak, sk, None)?;

    logger(&format!("[Aliyun] ③ POST {action}"));
    let client = Client::builder().timeout(Duration::from_secs(120)).build()?;
    let resp = client
        .post(&url)
        .headers(headers)
        .send()
        .await
        .map_err(|e| AppError::ProviderError(format!("请求 {action} 失败: {e}")))?;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    logger(&format!("[Aliyun] ③ {action} 响应 ({}): {}", status.as_u16(), &text[..text.len().min(800)]));
    if !status.is_success() {
        return Err(AppError::ProviderError(format!(
            "{action} 返回错误 ({}): {}",
            status.as_u16(),
            &text[..text.len().min(500)]
        )));
    }
    let data: Value = serde_json::from_str(&text)
        .map_err(|e| AppError::ProviderError(format!("解析 {action} 响应失败: {e}")))?;
    let result_url = data
        .get("Data")
        .and_then(|d| d.get("ImageURL"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            AppError::ProviderError(format!(
                "{action} 响应缺少 Data.ImageURL: {}",
                &text[..text.len().min(300)]
            ))
        })?
        .to_string();
    Ok(result_url)
}

// ────────────────────────────────────────────────────────────────
// ④ 下载结果透明 PNG
// ────────────────────────────────────────────────────────────────

async fn download_result(url: &str, logger: &Logger) -> Result<Vec<u8>, AppError> {
    logger("[Aliyun] ④ 下载抠图结果");
    let client = Client::builder().timeout(Duration::from_secs(60)).build()?;
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| AppError::ProviderError(format!("下载抠图结果失败: {e}")))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(AppError::ProviderError(format!(
            "下载抠图结果失败 ({})",
            status.as_u16()
        )));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| AppError::ProviderError(format!("读取抠图结果失败: {e}")))?;
    Ok(bytes.to_vec())
}

// ────────────────────────────────────────────────────────────────
// 工具函数
// ────────────────────────────────────────────────────────────────

fn field(d: &Value, keys: &[&str]) -> String {
    for k in keys {
        if let Some(v) = d.get(*k).and_then(|v| v.as_str()) {
            return v.to_string();
        }
    }
    String::new()
}

fn uuid_str() -> String {
    uuid::Uuid::new_v4().to_string()
}
