use serde::{Deserialize, Serialize};

/// AI 服务商信息
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInfo {
    pub name: String,
    pub display_name: String,
    pub config_key: String,
    pub supported_sizes: Vec<String>,
    pub configured: bool,
}

/// 风格模板
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Template {
    pub id: String,
    pub name: String,
    pub description: String,
}

/// 图标生成请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateRequest {
    /// 图标概念，如"咖啡杯"
    pub concept: String,
    /// 风格模板 id
    #[serde(default = "default_style")]
    pub style: String,
    /// 尺寸，如 1024x1024
    #[serde(default = "default_size")]
    pub size: String,
    /// Provider 名称，如 tongyi/doubao/cogview
    pub provider: String,
    /// 额外指令，追加到模板提示词末尾（可选）
    #[serde(default)]
    pub extra: Option<String>,
}

fn default_style() -> String {
    "flat-design".into()
}

fn default_size() -> String {
    "1024x1024".into()
}

/// 图标生成响应
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateResponse {
    /// base64 编码的 PNG
    pub image: String,
    pub format: String,
    pub icon_id: String,
}

/// 裁剪请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CropRequest {
    /// base64 编码的原图
    pub image: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// 抠图请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveBgRequest {
    /// base64 编码的原图
    pub image: String,
    /// alpha 阈值 0.0-1.0（默认 0.5，越高越激进抠掉更多）
    #[serde(default = "default_threshold")]
    pub threshold: f64,
}

fn default_threshold() -> f64 {
    0.45
}

/// 图片响应（裁剪、抠图等操作共用）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageResponse {
    /// base64 编码的 PNG
    pub image: String,
    pub format: String,
}

/// 导出请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportRequest {
    /// base64 编码的原图
    pub image: String,
    pub png_sizes: Option<Vec<u32>>,
    pub ico_sizes: Option<Vec<u32>>,
}

/// 图标元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IconMeta {
    pub id: String,
    pub created_at: String,
    pub concept: String,
    pub style: String,
    pub provider: String,
}

/// 配置项
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigEntry {
    pub key: String,
    pub value: String,
}

/// 服务商配置（DB 存储）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderEntry {
    pub id: String,
    pub name: String,
    pub notes: String,
    pub website: String,
    pub api_key: String,
    pub endpoint: String,
    pub model: String,
    pub is_builtin: bool,
    pub enabled: bool,
}

/// 抠图模型下载进度
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BgDownloadProgress {
    pub percent: f64,
    pub downloaded: u64,
    pub total: u64,
}

/// 抠图模型清单条目
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BgModelEntry {
    pub id: String,
    pub name: String,
    pub size: String,
    pub downloaded: bool,
    /// 已下载时的完整文件路径，未下载为 None
    pub path: Option<String>,
    /// 是否为当前选中模型
    pub current: bool,
}

/// 新增/更新服务商请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderUpsertRequest {
    pub id: Option<String>, // 新增时为 None，更新时必填
    pub name: String,
    pub notes: Option<String>,
    pub website: Option<String>,
    pub api_key: String,
    pub endpoint: String,
    #[serde(default)]
    pub model: String,
}

/// 历史列表响应
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IconListResponse {
    pub icons: Vec<IconMeta>,
    pub count: usize,
}

/// 配置状态响应
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigStatusResponse {
    pub providers: Vec<ProviderInfo>,
}

// ── 图标提取（PE → ICO）──

/// 图标提取请求：传入 PE 文件绝对路径
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractIconsRequest {
    pub file_path: String,
}

/// 提取出的单个图标（每个尺寸一条记录）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractedIcon {
    /// 所属图标组名（如 "MAINICON"）——同组的条目共享此名
    pub name: String,
    /// 宽（0 表示 256）
    pub width: u32,
    /// 高
    pub height: u32,
    /// 位深
    pub bit_depth: u32,
    /// 该尺寸的 PNG base64（前端用 <img> 直接显示）
    pub png_base64: String,
    /// 整组的 ICO base64（同组共享，用于「导出整组为 ICO」）
    pub ico_base64: String,
}

// ── 多图转 ICO ──

/// 图片转 ICO 请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConvertIcoRequest {
    /// base64 编码的原图数组
    pub images: Vec<String>,
    /// 目标 ICO 尺寸列表
    pub sizes: Vec<u32>,
}
