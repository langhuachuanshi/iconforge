//! 阿里云 OpenAPI V3 签名（ACS3-HMAC-SHA256）
//!
//! 参考：https://www.alibabacloud.com/help/en/sdk/product-overview/v3-request-structure-and-signature
//! 算法：
//!   CanonicalRequest = METHOD + \n + "/" + \n + CanonicalQueryString + \n
//!                      + CanonicalHeaders + \n + SignedHeaders + \n + hex(SHA256(body))
//!   StringToSign = "ACS3-HMAC-SHA256" + \n + hex(SHA256(CanonicalRequest))
//!   Signature = lowercase_hex(HMAC-SHA256(AccessKeySecret, StringToSign))
//!   Authorization = ACS3-HMAC-SHA256 Credential={AK},SignedHeaders={...},Signature={sig}
//!
//! RPC 风格接口：参数走 query string，URI 固定 "/"，body 为空。

use hmac::{Hmac, Mac};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use sha2::{Digest, Sha256};

use crate::error::AppError;

type HmacSha256 = Hmac<Sha256>;

/// RFC 3986 percent-encode（V3 query 编码用）：
/// - A-Z a-z 0-9 - _ . ~ 不编码
/// - 其他字符编码为 %XY（大写十六进制）
/// - 空格 → %20（不是 +），* → %2A
pub fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char);
            }
            _ => {
                out.push('%');
                out.push_str(&format!("{:02X}", b));
            }
        }
    }
    out
}

/// 构造一个带 V3 签名的 RPC 请求。
///
/// 参数走 query string（RPC 风格），URI 固定 "/"，body 为空。
/// 返回 (HeaderMap, 完整 URL)。调用方用 `client.method(url).headers(headers).send()`。
///
/// - `host`：如 "imageseg.cn-shanghai.aliyuncs.com"（不带协议）
/// - `method`：GET / POST
/// - `params`：业务参数（不含公共参数，本函数自动加 RegionId 等不需，公共项由 header 承担）
/// - `security_token`：STS 临时凭证时传 Some，长期 AK 传 None
pub fn build_v3_request(
    host: &str,
    method: &str,
    action: &str,
    version: &str,
    params: &[(&str, String)],
    ak: &str,
    sk: &str,
    security_token: Option<&str>,
) -> Result<(HeaderMap, String), AppError> {
    // ── 1. CanonicalQueryString：参数名字典序，RFC3986 编码 ──
    let mut sorted: Vec<(&str, String)> = params.iter().map(|(k, v)| (*k, v.clone())).collect();
    sorted.sort_by(|a, b| a.0.cmp(b.0));
    let canonical_query = sorted
        .iter()
        .map(|(k, v)| format!("{}={}", percent_encode(k), percent_encode(v)))
        .collect::<Vec<_>>()
        .join("&");

    // ── 2. 组装要参与签名的 header（host + x-acs-* ） ──
    // body 为空，x-acs-content-sha256 = SHA256("") 固定值
    let empty_hash = hex_sha256(b"");
    let now_iso8601 = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let nonce = uuid::Uuid::new_v4().to_string();

    // 参与签名的 header（小写 key），按 key 字典序排列
    // 必含：host / x-acs-action / x-acs-version / x-acs-date / x-acs-signature-nonce / x-acs-content-sha256
    // STS 时加 x-acs-security-token
    let mut signed_headers_vec: Vec<(&str, String)> = vec![
        ("host", host.to_string()),
        ("x-acs-action", action.to_string()),
        ("x-acs-version", version.to_string()),
        ("x-acs-date", now_iso8601.clone()),
        ("x-acs-signature-nonce", nonce.clone()),
        ("x-acs-content-sha256", empty_hash.clone()),
    ];
    if let Some(tok) = security_token {
        signed_headers_vec.push(("x-acs-security-token", tok.to_string()));
    }
    // 按 header key 字典序（已是基本有序，这里确保 STS 插入后仍有序）
    signed_headers_vec.sort_by(|a, b| a.0.cmp(b.0));

    // 每个 header 末尾都要 \n（含最后一个），V3 规范要求
    let canonical_headers = signed_headers_vec
        .iter()
        .map(|(k, v)| format!("{k}:{v}\n"))
        .collect::<Vec<_>>()
        .join("");
    let signed_headers = signed_headers_vec
        .iter()
        .map(|(k, _)| *k)
        .collect::<Vec<_>>()
        .join(";");

    // ── 3. CanonicalRequest ──
    let canonical_request = format!(
        "{method}\n/\n{canonical_query}\n{canonical_headers}\n{signed_headers}\n{empty_hash}",
    );
    // 签名诊断：打印 CanonicalRequest，便于和服务端报错里的对比
    log::info!("[Aliyun V3] CanonicalRequest:\n{}", canonical_request);

    // ── 4. StringToSign ──
    let hashed_canonical = hex_sha256(canonical_request.as_bytes());
    let string_to_sign = format!("ACS3-HMAC-SHA256\n{hashed_canonical}");
    log::info!("[Aliyun V3] StringToSign:\n{}", string_to_sign);

    // ── 5. Signature（密钥=裸 AccessKeySecret）──
    let mut mac = HmacSha256::new_from_slice(sk.as_bytes())
        .map_err(|e| AppError::ProviderError(format!("HMAC 初始化失败: {e}")))?;
    mac.update(string_to_sign.as_bytes());
    let signature = hex::encode(mac.finalize().into_bytes());

    // ── 6. 组装 HeaderMap ──
    let authorization = format!(
        "ACS3-HMAC-SHA256 Credential={ak},SignedHeaders={signed_headers},Signature={signature}"
    );

    let mut headers = HeaderMap::new();
    headers.insert("host", HeaderValue::from_str(host).map_err(|e| AppError::ProviderError(format!("host header: {e}")))?);
    headers.insert(HeaderName::from_static("x-acs-action"), HeaderValue::from_str(action).map_err(|e| AppError::ProviderError(format!("action header: {e}")))?);
    headers.insert(HeaderName::from_static("x-acs-version"), HeaderValue::from_str(version).map_err(|e| AppError::ProviderError(format!("version header: {e}")))?);
    headers.insert(HeaderName::from_static("x-acs-date"), HeaderValue::from_str(&now_iso8601).map_err(|e| AppError::ProviderError(format!("date header: {e}")))?);
    headers.insert(HeaderName::from_static("x-acs-signature-nonce"), HeaderValue::from_str(&nonce).map_err(|e| AppError::ProviderError(format!("nonce header: {e}")))?);
    headers.insert(HeaderName::from_static("x-acs-content-sha256"), HeaderValue::from_str(&empty_hash).map_err(|e| AppError::ProviderError(format!("content-sha256 header: {e}")))?);
    headers.insert("authorization", HeaderValue::from_str(&authorization).map_err(|e| AppError::ProviderError(format!("authorization header: {e}")))?);
    if let Some(tok) = security_token {
        headers.insert(HeaderName::from_static("x-acs-security-token"), HeaderValue::from_str(tok).map_err(|e| AppError::ProviderError(format!("security-token header: {e}")))?);
    }

    let url = if canonical_query.is_empty() {
        format!("https://{host}/")
    } else {
        format!("https://{host}/?{canonical_query}")
    };

    Ok((headers, url))
}

fn hex_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 3986 编码规则（V2/V3 通用）
    #[test]
    fn test_percent_encode_basic() {
        assert_eq!(percent_encode("hello"), "hello");
        assert_eq!(percent_encode("a b"), "a%20b"); // 空格 → %20
        assert_eq!(percent_encode("a*b"), "a%2Ab"); // * → %2A
        assert_eq!(percent_encode("~"), "~"); // ~ 保留
        assert_eq!(percent_encode("/"), "%2F");
        assert_eq!(percent_encode("="), "%3D");
        assert_eq!(percent_encode("&"), "%26");
    }

    /// 空 body 的 SHA256 固定值（V3 签名里 x-acs-content-sha256 用到）
    #[test]
    fn test_empty_body_hash() {
        assert_eq!(
            hex_sha256(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
