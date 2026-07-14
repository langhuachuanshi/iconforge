use std::path::{Path, PathBuf};
use std::time::Duration;

use reqwest::Client;
use tauri::Emitter;
use tokio::io::AsyncWriteExt;

use crate::error::AppError;
use crate::models::BgDownloadProgress;

/// 可用模型定义
pub struct BgModel {
    pub id: &'static str,
    pub name: &'static str,
    pub url: &'static str,
    pub filename: &'static str,
    pub size: &'static str,
}

pub const BG_MODELS: &[BgModel] = &[
    BgModel {
        id: "crispcut-quality",
        name: "CrispCut（推荐）",
        url: "https://hf-mirror.com/bowespublishing/crisp-cut/resolve/main/onnx/crispcut-quality.onnx",
        filename: "crispcut-quality.onnx",
        size: "约 25MB",
    },
    BgModel {
        id: "crispcut-fast",
        name: "CrispCut-快速版",
        url: "https://hf-mirror.com/bowespublishing/crisp-cut/resolve/main/onnx/crispcut-fast.onnx",
        filename: "crispcut-fast.onnx",
        size: "约 6.5MB",
    },
    BgModel {
        id: "rmbg-1.4",
        name: "RMBG-1.4",
        url: "https://modelscope.cn/models/briaai/RMBG-1.4/resolve/master/onnx/model.onnx",
        filename: "rmbg-1.4.onnx",
        size: "约 40MB",
    },
    BgModel {
        id: "rmbg-2.0",
        name: "RMBG-2.0",
        url: "https://modelscope.cn/models/briaai/RMBG-2.0/resolve/master/onnx/model.onnx",
        filename: "rmbg-2.0.onnx",
        size: "约 176MB",
    },
];

const DEFAULT_MODEL: &str = "crispcut-quality";

pub fn get_model(id: &str) -> &'static BgModel {
    BG_MODELS.iter().find(|m| m.id == id).unwrap_or(&BG_MODELS[0])
}

/// 模型文件路径（存储在 models/ 子目录）
pub fn model_path(base_dir: &Path, filename: &str) -> PathBuf {
    base_dir.join("models").join(filename)
}

/// 检查模型是否已下载（默认模型）
pub fn model_exists(model_dir: &Path, model_id: &str) -> bool {
    let m = get_model(model_id);
    model_path(model_dir, m.filename).exists()
}

/// 下载指定模型
pub async fn download_model(
    window: &tauri::Window,
    model_dir: &Path,
    model_id: &str,
) -> Result<(), AppError> {
    let m = get_model(model_id);
    let target = model_path(model_dir, m.filename);
    if let Some(parent) = target.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let tmp_path = model_dir.join(format!("{}.tmp", m.filename));

    let client = Client::builder()
        .timeout(Duration::from_secs(600))
        .build()?;

    let resp = client.get(m.url).header("User-Agent", "IconForge/1.0").send().await?;
    if !resp.status().is_success() {
        return Err(AppError::Http(format!("模型下载失败 (HTTP {})", resp.status().as_u16())));
    }

    let total = resp.content_length().unwrap_or(0) as f64;
    let mut downloaded = 0u64;
    let mut file = tokio::fs::File::create(&tmp_path).await?;
    let mut stream = resp.bytes_stream();

    use futures_util::StreamExt;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;
        if total > 0.0 {
            let pct = (downloaded as f64 / total * 100.0).min(100.0);
            let _ = window.emit("bg-download-progress", BgDownloadProgress { percent: pct, downloaded: downloaded as u64, total: total as u64 });
        }
    }

    file.flush().await?;
    tokio::fs::rename(&tmp_path, &target).await?;
    let _ = window.emit("bg-download-complete", ());
    Ok(())
}

/// 运行抠图推理（使用 ONNX Runtime）
pub fn run_inference(model_dir: &Path, image_bytes: &[u8], threshold: f64, model_id: &str) -> Result<Vec<u8>, AppError> {
    let m = get_model(model_id);
    let model_path = model_path(model_dir, m.filename);
    if !model_path.exists() {
        return Err(AppError::NotFound("抠图模型未下载，请先下载模型".into()));
    }

    // 加载原图
    let img = image::load_from_memory(image_bytes)?;
    let (orig_w, orig_h) = (img.width(), img.height());

    // 转换为 RGB 并缩放到 1024x1024
    let rgb = img.to_rgb8();
    let resized = image::imageops::resize(
        &rgb, 1024, 1024,
        image::imageops::FilterType::Lanczos3,
    );

    // 预处理：官方 ImageNet 归一化 RGB (0,1,2)
    let mean = [0.485f32, 0.456, 0.406];
    let std = [0.229f32, 0.224, 0.225];
    let mut input_data = Vec::with_capacity(3 * 1024 * 1024);
    for c in 0..3 {
        for y in 0..1024 {
            for x in 0..1024 {
                let v = resized.get_pixel(x, y)[c] as f32 / 255.0;
                input_data.push((v - mean[c as usize]) / std[c as usize]);
            }
        }
    }

    // ONNX Runtime 推理
    let mask = run_ort_inference(&model_path, &input_data, 1024, 1024, threshold, model_id)?;

    // mask 缩放回原始尺寸
    let mask_img = image::GrayImage::from_raw(1024, 1024, mask)
        .ok_or_else(|| AppError::Image("mask 构造失败".into()))?;
    let mask_resized = image::imageops::resize(
        &mask_img, orig_w, orig_h,
        image::imageops::FilterType::Lanczos3,
    );

    // 合成 RGBA
    let mut rgba = img.to_rgba8();
    for y in 0..orig_h {
        for x in 0..orig_w {
            let alpha = mask_resized.get_pixel(x, y)[0];
            let pixel = rgba.get_pixel_mut(x, y);
            pixel[3] = alpha;
        }
    }

    let mut buf = Vec::new();
    rgba.write_to(
        &mut std::io::Cursor::new(&mut buf),
        image::ImageFormat::Png,
    )?;
    Ok(buf)
}

fn run_ort_inference(
    model_path: &Path,
    input_data: &[f32],
    w: u32,
    h: u32,
    threshold: f64,
    model_id: &str,
) -> Result<Vec<u8>, AppError> {
    use ort::session::Session;

    let mut session = Session::builder()
        .map_err(|e| AppError::Image(format!("创建 Session 失败: {e}")))?
        .commit_from_file(model_path)
        .map_err(|e| AppError::Image(format!("加载模型失败: {e}")))?;

    // [1, 3, H, W] tensor
    let shape = vec![1i64, 3, h as i64, w as i64];
    let input_tensor = ort::value::Tensor::from_array((shape, input_data.to_vec()))
        .map_err(|e| AppError::Image(format!("创建 Tensor 失败: {e}")))?;

    // 尝试官方名称 "pixel_values"，不行则用第一个输入名
    let input_name = if session.inputs().iter().any(|i| i.name() == "pixel_values") {
        "pixel_values".to_string()
    } else {
        session.inputs()[0].name().to_string()
    };
    log::info!("[RMBG] 模型={} 输入名={}", model_id, input_name);

    let outputs = session
        .run(ort::inputs![input_name.as_str() => input_tensor])
        .map_err(|e| AppError::Image(format!("推理失败: {e}")))?;

    let (_name, value) = outputs.iter().next()
        .ok_or_else(|| AppError::Image("模型无输出".into()))?;

    let (_shape, data) = value
        .try_extract_tensor::<f32>()
        .map_err(|e| AppError::Image(format!("输出解析失败: {e}")))?;

    // ── 调试输出 ──
    let total = data.len();
    let (mut dmin, mut dmax) = (f32::MAX, f32::MIN);
    let mut sum = 0f64;
    for &v in data.iter() { dmin = dmin.min(v); dmax = dmax.max(v); sum += v as f64; }
    let avg = sum / total as f64;
    let corners = [
        ("左上", data[0]),
        ("右上", data[(w - 1) as usize]),
        ("左下", data[(h - 1) as usize * w as usize]),
        ("右下", data[(h * w - 1) as usize]),
        ("中心", data[(h / 2 * w + w / 2) as usize]),
    ];
    log::info!("[RMBG] output range=[{:.4},{:.4}] avg={:.4} threshold={}", dmin, dmax, avg, threshold);
    for (label, v) in &corners {
        log::info!("[RMBG]   {}: {:.4}", label, v);
    }

    // ── 调试：保存原始 mask ──
    let raw: Vec<u8> = data.iter().map(|&v| (v * 255.0).clamp(0.0, 255.0) as u8).collect();
    if let Some(mask_img) = image::GrayImage::from_raw(w, h, raw) {
        let mut debug_path = model_path.to_path_buf();
        debug_path.set_extension("debug.png");
        let _ = mask_img.save(&debug_path);
        log::info!("[RMBG] 保存调试 mask: {:?}", debug_path);
    }

    // 默认官方方案：模型输出直接作为 alpha。阈值 > 0 时启用裁切。
    let mask: Vec<u8> = if threshold > 0.01 {
        data.iter().map(|&v| {
            if v >= threshold as f32 { (v * 255.0).clamp(0.0, 255.0) as u8 } else { 0u8 }
        }).collect()
    } else {
        data.iter().map(|&v| (v * 255.0).clamp(0.0, 255.0) as u8).collect()
    };
    Ok(mask)
}
