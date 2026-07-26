//! 阿里云 VIAPI 分割抠图（SegmentCommonImage）云端调用
//!
//! 流程（图片只接受 URL，所以先上传到阿里云官方临时 Bucket）：
//!   ① GetOssStsToken —— 拿临时 STS 凭证 + 上传地址
//!   ② OSS POST 上传 —— 把图片字节上传到官方 Bucket，拼出公网 URL
//!   ③ SegmentCommonImage —— 用 URL 调抠图，下载返回的透明 PNG
//!
//! 参考：
//!   - 小程序直传方案 https://help.aliyun.com/zh/viapi/developer-reference/small-application-scenario-called-directly
//!   - SegmentCommonImage https://help.aliyun.com/zh/viapi/developer-reference/api-k8cs8t
//!   - OSS 表单上传 https://help.aliyun.com/zh/oss/user-guide/form-upload

use std::time::Duration;

use base64::Engine;
use hmac::{Hmac, Mac};
use reqwest::Client;
use serde_json::Value;
use sha1::Sha1;

use crate::error::AppError;
use crate::services::aliyun_sign;

type HmacSha1 = Hmac<Sha1>;

const UTILS_ENDPOINT: &str = "https://viapiutils.cn-shanghai.aliyuncs.com";
const SEG_ENDPOINT: &str = "https://imageseg.cn-shanghai.aliyuncs.com";

/// 云端抠图入口：本地图片字节 → 透明 PNG 字节
pub async fn remove_background(
    image_bytes: &[u8],
    access_key_id: &str,
    access_key_secret: &str,
) -> Result<Vec<u8>, AppError> {
    // ① 拿 STS 凭证 + 上传地址
    let sts = get_oss_sts_token(access_key_id, access_key_secret).await?;

    // ② 上传到官方临时 Bucket，拿公网 URL
    let image_url = upload_to_oss(&sts, image_bytes).await?;

    // ③ 调抠图，拿结果 URL
    let result_url = segment_image(&image_url, access_key_id, access_key_secret).await?;

    // ④ 下载结果（透明 PNG）
    let png_bytes = download_result(&result_url).await?;
    Ok(png_bytes)
}

// ────────────────────────────────────────────────────────────────
// ① GetOssStsToken
// ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct OssSts {
    /// 临时 AK，用于 OSS 上传签名
    access_key_id: String,
    access_key_secret: String,
    /// STS 安全令牌，OSS 上传表单里的 x-oss-security-token
    security_token: String,
    /// 上传目标 Bucket 的公网 host（如 viapi-customer-temp.oss-cn-shanghai.aliyuncs.com）
    bucket_host: String,
    /// 对象存储路径前缀/完整 key
    object_key: String,
}

async fn get_oss_sts_token(ak: &str, sk: &str) -> Result<OssSts, AppError> {
    let nonce = uuid_str();
    let timestamp = now_iso8601();
    let mut params: Vec<(String, String)> = vec![
        ("Action".into(), "GetOssStsToken".into()),
        ("Version".into(), "2020-04-01".into()),
        ("Format".into(), "JSON".into()),
        ("AccessKeyId".into(), ak.into()),
        ("SignatureMethod".into(), "HMAC-SHA1".into()),
        ("SignatureVersion".into(), "1.0".into()),
        ("SignatureNonce".into(), nonce),
        ("Timestamp".into(), timestamp),
        ("RegionId".into(), "cn-shanghai".into()),
    ];
    let sig = aliyun_sign::sign(&params, "GET", sk);
    params.push(("Signature".into(), sig));

    let url = build_signed_url(UTILS_ENDPOINT, &params);
    log::info!("[Aliyun] GET GetOssStsToken");

    let client = Client::builder().timeout(Duration::from_secs(30)).build()?;
    let resp = client.get(&url).send().await
        .map_err(|e| AppError::ProviderError(format!("请求 GetOssStsToken 失败: {e}")))?;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(AppError::ProviderError(format!(
            "GetOssStsToken 返回错误 ({}): {}",
            status.as_u16(),
            &text[..text.len().min(500)]
        )));
    }
    let data: Value = serde_json::from_str(&text)
        .map_err(|e| AppError::ProviderError(format!("解析 GetOssStsToken 响应失败: {e}")))?;

    // 在 data.Data 下寻找上传凭证信息。字段命名按 VIAPI 通用约定。
    let d = data.get("Data").ok_or_else(|| {
        AppError::ProviderError(format!("GetOssStsToken 响应缺少 Data 字段: {}", &text[..text.len().min(300)]))
    })?;

    Ok(OssSts {
        access_key_id: field(d, &["AccessKeyId", "accessKeyId"]),
        access_key_secret: field(d, &["AccessKeySecret", "accessKeySecret"]),
        security_token: field(d, &["SecurityToken", "securityToken"]),
        bucket_host: field(d, &["Bucket", "bucket", "BucketHost"]),
        object_key: field(d, &["ObjectPath", "objectPath", "Path", "Key"]),
    })
}

// ────────────────────────────────────────────────────────────────
// ② OSS POST 表单上传
// ────────────────────────────────────────────────────────────────

async fn upload_to_oss(sts: &OssSts, image_bytes: &[u8]) -> Result<String, AppError> {
    // OSS POST 表单签名（V1，HMAC-SHA1）
    //   policy = base64(JSON{expiration, conditions})
    //   signature = base64(hmac_sha1(access_key_secret, policy))
    // 表单字段：key / policy / OSSAccessKeyId / signature / x-oss-security-token / success_action_status / file

    let expiration = now_iso8601_plus_hours(1);
    let policy_json = serde_json::json!({
        "expiration": expiration,
        "conditions": [
            ["content-length-range", 0, 10_485_760] // 0 ~ 10MB
        ]
    })
    .to_string();

    let policy_b64 = base64::engine::general_purpose::STANDARD.encode(policy_json.as_bytes());

    let mut mac = HmacSha1::new_from_slice(sts.access_key_secret.as_bytes())
        .map_err(|e| AppError::ProviderError(format!("HMAC 初始化失败: {e}")))?;
    mac.update(policy_b64.as_bytes());
    let signature = base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes());

    let host = if sts.bucket_host.starts_with("http") {
        sts.bucket_host.clone()
    } else {
        format!("https://{}", sts.bucket_host)
    };
    // object_key 可能是带前缀的路径，补一个时间戳文件名避免冲突
    let object_key = if sts.object_key.is_empty() {
        format!("iconforge/{}.png", uuid_str())
    } else if sts.object_key.ends_with('/') {
        format!("{}{}.png", sts.object_key, uuid_str())
    } else {
        sts.object_key.clone()
    };

    let form = reqwest::multipart::Form::new()
        .text("key", object_key.clone())
        .text("policy", policy_b64)
        .text("OSSAccessKeyId", sts.access_key_id.clone())
        .text("signature", signature)
        .text("x-oss-security-token", sts.security_token.clone())
        .text("success_action_status", "200".to_string())
        .part(
            "file",
            reqwest::multipart::Part::bytes(image_bytes.to_vec())
                .file_name("image.png")
                .mime_str("image/png")
                .map_err(|e| AppError::Http(format!("mime 设置失败: {e}")))?,
        );

    log::info!("[Aliyun] POST OSS 上传 ({} bytes)", image_bytes.len());

    let client = Client::builder().timeout(Duration::from_secs(60)).build()?;
    let resp = client.post(&host).multipart(form).send().await
        .map_err(|e| AppError::ProviderError(format!("OSS 上传请求失败: {e}")))?;
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(AppError::ProviderError(format!(
            "OSS 上传失败 ({}): {}",
            status.as_u16(),
            &text[..text.len().min(500)]
        )));
    }

    // 拼公网 URL：host/key
    let image_url = format!("{}/{}", host.trim_end_matches('/'), object_key);
    log::info!("[Aliyun] OSS 上传完成: {}", &image_url[..image_url.len().min(80)]);
    Ok(image_url)
}

// ────────────────────────────────────────────────────────────────
// ③ SegmentCommonImage
// ────────────────────────────────────────────────────────────────

async fn segment_image(
    image_url: &str,
    ak: &str,
    sk: &str,
) -> Result<String, AppError> {
    let nonce = uuid_str();
    let timestamp = now_iso8601();
    let mut params: Vec<(String, String)> = vec![
        ("Action".into(), "SegmentCommonImage".into()),
        ("Version".into(), "2019-12-30".into()),
        ("Format".into(), "JSON".into()),
        ("AccessKeyId".into(), ak.into()),
        ("SignatureMethod".into(), "HMAC-SHA1".into()),
        ("SignatureVersion".into(), "1.0".into()),
        ("SignatureNonce".into(), nonce),
        ("Timestamp".into(), timestamp),
        ("RegionId".into(), "cn-shanghai".into()),
        ("ImageURL".into(), image_url.into()),
        // 不传 ReturnForm，默认返回四通道透明 PNG
    ];
    let sig = aliyun_sign::sign(&params, "GET", sk);
    params.push(("Signature".into(), sig));

    let url = build_signed_url(SEG_ENDPOINT, &params);
    log::info!("[Aliyun] GET SegmentCommonImage");

    let client = Client::builder().timeout(Duration::from_secs(120)).build()?;
    let resp = client.get(&url).send().await
        .map_err(|e| AppError::ProviderError(format!("请求 SegmentCommonImage 失败: {e}")))?;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(AppError::ProviderError(format!(
            "SegmentCommonImage 返回错误 ({}): {}",
            status.as_u16(),
            &text[..text.len().min(500)]
        )));
    }
    let data: Value = serde_json::from_str(&text)
        .map_err(|e| AppError::ProviderError(format!("解析 SegmentCommonImage 响应失败: {e}")))?;
    let result_url = data
        .get("Data")
        .and_then(|d| d.get("ImageURL"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            AppError::ProviderError(format!(
                "SegmentCommonImage 响应缺少 Data.ImageURL: {}",
                &text[..text.len().min(300)]
            ))
        })?
        .to_string();
    log::info!("[Aliyun] 抠图结果 URL: {}", &result_url[..result_url.len().min(80)]);
    Ok(result_url)
}

// ────────────────────────────────────────────────────────────────
// ④ 下载结果透明 PNG
// ────────────────────────────────────────────────────────────────

async fn download_result(url: &str) -> Result<Vec<u8>, AppError> {
    let client = Client::builder().timeout(Duration::from_secs(60)).build()?;
    let resp = client.get(url).send().await
        .map_err(|e| AppError::ProviderError(format!("下载抠图结果失败: {e}")))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(AppError::ProviderError(format!(
            "下载抠图结果失败 ({})",
            status.as_u16()
        )));
    }
    let bytes = resp.bytes().await
        .map_err(|e| AppError::ProviderError(format!("读取抠图结果失败: {e}")))?;
    Ok(bytes.to_vec())
}

// ────────────────────────────────────────────────────────────────
// 工具函数
// ────────────────────────────────────────────────────────────────

fn build_signed_url(endpoint: &str, params: &[(String, String)]) -> String {
    let query: String = params
        .iter()
        .map(|(k, v)| {
            format!(
                "{}={}",
                aliyun_sign::percent_encode(k),
                aliyun_sign::percent_encode(v)
            )
        })
        .collect::<Vec<_>>()
        .join("&");
    format!("{}/?{}", endpoint, query)
}

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

fn now_iso8601() -> String {
    // UTC，格式 yyyy-MM-ddTHH:mm:ssZ
    let now = chrono::Utc::now();
    now.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

fn now_iso8601_plus_hours(hours: i64) -> String {
    let t = chrono::Utc::now() + chrono::Duration::hours(hours);
    // OSS policy expiration 需要 毫秒精度
    t.format("%Y-%m-%dT%H:%M:%S.000Z").to_string()
}
