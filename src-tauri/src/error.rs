use serde::Serialize;

/// 统一错误类型，映射原 Python ProviderError 及各类运行时错误
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    ProviderError(String),

    #[error("未找到: {0}")]
    NotFound(String),

    #[error("数据库错误: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    #[error("图片处理错误: {0}")]
    Image(String),

    #[error("Base64 解码错误: {0}")]
    Base64(#[from] base64::DecodeError),

    #[error("HTTP 请求错误: {0}")]
    Http(String),

    #[error("序列化错误: {0}")]
    Serde(String),
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::Serde(e.to_string())
    }
}

impl From<reqwest::Error> for AppError {
    fn from(e: reqwest::Error) -> Self {
        AppError::Http(e.to_string())
    }
}

impl From<image::ImageError> for AppError {
    fn from(e: image::ImageError) -> Self {
        AppError::Image(e.to_string())
    }
}

impl From<zip::result::ZipError> for AppError {
    fn from(e: zip::result::ZipError) -> Self {
        AppError::Io(std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
    }
}

/// Tauri 要求错误类型实现 Serialize
impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}
