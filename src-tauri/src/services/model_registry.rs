//! 模型基座：可插拔模型注册表（抠图 + 智能擦除）
//!
//! 内置种子（代码内，带下载 URL）+ 自定义模型（DB custom_models 表，用户导入 .onnx）
//! 统一成 ModelDef。增删模型不改代码：导入即插、删除即拔。
//! 删除语义：内置 → 只删文件（条目保留，可重新下载）；自定义 → 删文件 + 删记录。

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::services::storage::Storage;

/// 擦除模型 IO 约定
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Io {
    /// float 图 [0,1] + float mask（1=擦除）@512，输出 float 需范围自适应（LaMa 型）
    Float01,
    /// uint8 图 + uint8 mask（255=擦除），输出 uint8，预处理内置图内（MI-GAN pipeline 型）
    Uint8,
}

/// 抠图归一化策略（严格对应各模型官方预处理）
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Norm {
    /// ImageNet: (x/255 - mean) / std，RMBG 系用
    ImageNet,
    /// 仅 /255 → [0,1]，CrispCut 系用
    Unit,
    /// x/255 - 0.5 → [-0.5, 0.5]，ISNet 系用
    Centered,
}

/// 模型参数（按类别，存 DB params JSON）
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum ModelParams {
    Inpaint { io: Io },
    RemoveBg { norm: Norm, sigmoid_output: bool, input_name: String },
}

/// 注册表里的一个模型（内置或自定义）
#[derive(Clone, Debug)]
pub struct ModelDef {
    pub id: String,
    pub name: String,
    pub filename: String,
    /// 展示用体积/耗时说明
    pub size_label: String,
    /// 下载 URL（内置有；自定义为 None）
    pub url: Option<String>,
    pub builtin: bool,
    pub params: ModelParams,
}

pub const CATEGORY_REMOVE_BG: &str = "remove_bg";
pub const CATEGORY_INPAINT: &str = "inpaint";

/// 内置种子（顺序 = 展示顺序）
fn builtin_seeds(category: &str) -> Vec<ModelDef> {
    match category {
        CATEGORY_INPAINT => vec![ModelDef {
            id: "lama".into(),
            name: "LaMa（质量优先）".into(),
            filename: "lama_fp32.onnx".into(),
            size_label: "约 208MB · CPU 约 5s".into(),
            url: Some("https://hf-mirror.com/Carve/LaMa-ONNX/resolve/main/lama_fp32.onnx".into()),
            builtin: true,
            params: ModelParams::Inpaint { io: Io::Float01 },
        }],
        _ => vec![
            ModelDef {
                id: "crispcut-quality".into(),
                name: "CrispCut（推荐）".into(),
                filename: "crispcut-quality.onnx".into(),
                size_label: "约 25MB".into(),
                url: Some("https://hf-mirror.com/bowespublishing/crisp-cut/resolve/main/onnx/crispcut-quality.onnx".into()),
                builtin: true,
                params: ModelParams::RemoveBg { norm: Norm::Unit, sigmoid_output: false, input_name: "input".into() },
            },
            ModelDef {
                id: "crispcut-fast".into(),
                name: "CrispCut-快速版".into(),
                filename: "crispcut-fast.onnx".into(),
                size_label: "约 6.5MB".into(),
                url: Some("https://hf-mirror.com/bowespublishing/crisp-cut/resolve/main/onnx/crispcut-fast.onnx".into()),
                builtin: true,
                params: ModelParams::RemoveBg { norm: Norm::Unit, sigmoid_output: false, input_name: "input".into() },
            },
            ModelDef {
                id: "rmbg-1.4".into(),
                name: "RMBG-1.4".into(),
                filename: "rmbg-1.4.onnx".into(),
                size_label: "约 40MB".into(),
                url: Some("https://modelscope.cn/models/briaai/RMBG-1.4/resolve/master/onnx/model.onnx".into()),
                builtin: true,
                params: ModelParams::RemoveBg { norm: Norm::ImageNet, sigmoid_output: true, input_name: "input".into() },
            },
            ModelDef {
                id: "rmbg-2.0".into(),
                name: "RMBG-2.0".into(),
                filename: "rmbg-2.0.onnx".into(),
                size_label: "约 176MB".into(),
                url: Some("https://modelscope.cn/models/briaai/RMBG-2.0/resolve/master/onnx/model.onnx".into()),
                builtin: true,
                params: ModelParams::RemoveBg { norm: Norm::ImageNet, sigmoid_output: true, input_name: "input".into() },
            },
            ModelDef {
                id: "isnet-general-use".into(),
                name: "ISNet (ModelScope)".into(),
                filename: "isnet-general-use.onnx".into(),
                size_label: "约 176MB".into(),
                url: Some("https://hf-mirror.com/x-Liola-x/isnet-general-use-onnx/resolve/main/isnet-general-use.onnx".into()),
                builtin: true,
                params: ModelParams::RemoveBg { norm: Norm::Centered, sigmoid_output: true, input_name: "input".into() },
            },
        ],
    }
}

/// 列出某类别全部模型：内置种子 + 自定义记录
pub fn list_models(storage: &Storage, category: &str) -> Vec<ModelDef> {
    let mut out = builtin_seeds(category);
    if let Ok(rows) = storage.list_custom_models(category) {
        for (id, name, filename, params) in rows {
            if let Ok(p) = serde_json::from_str::<ModelParams>(&params) {
                out.push(ModelDef {
                    id,
                    name,
                    filename,
                    size_label: "自定义".into(),
                    url: None,
                    builtin: false,
                    params: p,
                });
            }
        }
    }
    out
}

/// 按 id 解析模型（内置优先，其次自定义）
pub fn get_model(storage: &Storage, category: &str, id: &str) -> Option<ModelDef> {
    list_models(storage, category).into_iter().find(|m| m.id == id)
}

/// 导入自定义模型：复制 .onnx 进 models/，写 DB 记录
pub fn add_custom_model(
    storage: &Storage,
    category: &str,
    name: &str,
    params: &ModelParams,
    src_path: &Path,
) -> Result<ModelDef, AppError> {
    if !src_path.is_file() {
        return Err(AppError::NotFound(format!("文件不存在: {}", src_path.display())));
    }
    let is_onnx = src_path
        .extension()
        .map(|e| e.eq_ignore_ascii_case("onnx"))
        .unwrap_or(false);
    if !is_onnx {
        return Err(AppError::ProviderError("请选择 .onnx 模型文件".into()));
    }
    let id = format!("custom-{}", uuid::Uuid::new_v4().simple());
    let filename = format!("{id}.onnx");
    let dst = storage.base_dir().join("models").join(&filename);
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::copy(src_path, &dst)?;
    let params_json = serde_json::to_string(params)?;
    storage.insert_custom_model(&id, category, name, &filename, &params_json)?;
    Ok(ModelDef {
        id,
        name: name.into(),
        filename,
        size_label: format!("自定义 · {:.0}MB", dst.metadata().map(|m| m.len()).unwrap_or(0) as f64 / 1048576.0),
        url: None,
        builtin: false,
        params: params.clone(),
    })
}

/// 删除模型：内置只删文件；自定义删文件 + 记录
pub fn delete_model(storage: &Storage, category: &str, id: &str) -> Result<(), AppError> {
    let def = get_model(storage, category, id)
        .ok_or_else(|| AppError::NotFound(format!("模型 {id} 不存在")))?;
    let file = storage.base_dir().join("models").join(&def.filename);
    if file.exists() {
        std::fs::remove_file(&file)?;
    }
    if !def.builtin {
        storage.delete_custom_model(id)?;
    }
    Ok(())
}

/// 模型文件是否已下载
pub fn is_downloaded(storage: &Storage, def: &ModelDef) -> bool {
    storage.base_dir().join("models").join(&def.filename).exists()
}

/// 模型文件完整路径
pub fn model_file(storage: &Storage, def: &ModelDef) -> std::path::PathBuf {
    storage.base_dir().join("models").join(&def.filename)
}
