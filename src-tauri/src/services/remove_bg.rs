use std::path::{Path, PathBuf};
use std::time::Duration;

use reqwest::Client;
use tauri::Emitter;
use tokio::io::AsyncWriteExt;

use crate::error::AppError;
use crate::models::BgDownloadProgress;

pub use crate::services::model_registry::Norm;

// 模型种子已迁至 model_registry.rs（内置 + 自定义统一注册表）

/// 模型文件路径（存储在 models/ 子目录）
pub fn model_path(base_dir: &Path, filename: &str) -> PathBuf {
    base_dir.join("models").join(filename)
}

/// 下载指定模型（url/filename 来自注册表条目）
pub async fn download_model(
    window: &tauri::Window,
    model_dir: &Path,
    filename: &str,
    url: &str,
) -> Result<(), AppError> {
    let target = model_path(model_dir, filename);
    if let Some(parent) = target.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let tmp_path = model_dir.join(format!("{filename}.tmp"));

    let client = Client::builder()
        .timeout(Duration::from_secs(600))
        .build()?;

    let resp = client.get(url).header("User-Agent", "IconForge/1.0").send().await?;
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

/// 运行抠图推理（模型定义来自注册表）
pub fn run_inference(model_dir: &Path, image_bytes: &[u8], threshold: f64, model: &crate::services::model_registry::ModelDef) -> Result<Vec<u8>, AppError> {
    let crate::services::model_registry::ModelParams::RemoveBg { .. } = &model.params else {
        return Err(AppError::Image("该模型不是抠图模型".into()));
    };
    let model_path = model_path(model_dir, &model.filename);
    if !model_path.exists() {
        return Err(AppError::NotFound("抠图模型未下载，请先下载模型".into()));
    }

    // 加载原图
    let img = image::load_from_memory(image_bytes)?;
    let (orig_w, orig_h) = (img.width(), img.height());

    // 转换为 RGB 并缩放到 1024x1024（各模型官方预处理都是直接拉伸到 1024²）
    let rgb = img.to_rgb8();
    let resized = image::imageops::resize(
        &rgb, 1024, 1024,
        image::imageops::FilterType::Lanczos3,
    );

    // 按模型官方预处理策略归一化
    let crate::services::model_registry::ModelParams::RemoveBg { norm, .. } = &model.params else { unreachable!() };
    let input_data = preprocess(&resized, *norm);

    // ONNX Runtime 推理（传入模型配置：输入名、是否需手动 sigmoid）
    let mask = run_ort_inference(&model_path, &input_data, 1024, 1024, model)?;

    // mask 缩放回原始尺寸（用 Bilinear，避免 Lanczos 在 mask 上产生振铃半透明边）
    let mask_img = image::GrayImage::from_raw(1024, 1024, mask)
        .ok_or_else(|| AppError::Image("mask 构造失败".into()))?;
    let mask_resized = image::imageops::resize(
        &mask_img, orig_w, orig_h,
        image::imageops::FilterType::Triangle,  // Triangle = bilinear kernel
    );

    // 合成 RGBA + 回填空洞：mask 判为透明、但原图明显非背景白的像素，强制不透明
    // 解决 logo/图标类「主体内部白色填充被误判为背景挖空」的问题
    let mut rgba = img.to_rgba8();
    for y in 0..orig_h {
        for x in 0..orig_w {
            let alpha = mask_resized.get_pixel(x, y)[0];
            let pixel = rgba.get_pixel_mut(x, y);
            if alpha < 128 {
                // mask 判为透明：再检查原图像素，非背景白则回填（保留物体内部填充）
                let [r, g, b, _] = pixel.0;
                let is_background_white = r >= 240 && g >= 240 && b >= 240;
                pixel[3] = if is_background_white { 0 } else { 255 };
            } else {
                pixel[3] = alpha;
            }
        }
    }

    let mut buf = Vec::new();
    rgba.write_to(
        &mut std::io::Cursor::new(&mut buf),
        image::ImageFormat::Png,
    )?;
    Ok(buf)
}

/// 按模型官方预处理策略归一化 RGB 图（输出 NCHW [1,3,1024,1024] 的扁平数据）
fn preprocess(resized: &image::RgbImage, norm: Norm) -> Vec<f32> {
    let mean = [0.485f32, 0.456, 0.406];
    let std = [0.229f32, 0.224, 0.225];
    let mut out = Vec::with_capacity(3 * 1024 * 1024);
    for c in 0..3u8 {
        for y in 0..1024 {
            for x in 0..1024 {
                let v = resized.get_pixel(x, y)[c as usize] as f32 / 255.0;
                let n = match norm {
                    Norm::ImageNet => (v - mean[c as usize]) / std[c as usize],
                    Norm::Unit => v,
                    Norm::Centered => v - 0.5,
                };
                out.push(n);
            }
        }
    }
    out
}

fn run_ort_inference(
    model_path: &Path,
    input_data: &[f32],
    w: u32,
    h: u32,
    m: &crate::services::model_registry::ModelDef,
) -> Result<Vec<u8>, AppError> {
    let crate::services::model_registry::ModelParams::RemoveBg { norm, sigmoid_output, input_name } = &m.params else {
        return Err(AppError::Image("该模型不是抠图模型".into()));
    };
    use ort::session::Session;

    let mut session = Session::builder()
        .map_err(|e| AppError::Image(format!("创建 Session 失败: {e}")))?
        .commit_from_file(model_path)
        .map_err(|e| AppError::Image(format!("加载模型失败: {e}")))?;

    // [1, 3, H, W] tensor
    let shape = vec![1i64, 3, h as i64, w as i64];
    let input_tensor = ort::value::Tensor::from_array((shape, input_data.to_vec()))
        .map_err(|e| AppError::Image(format!("创建 Tensor 失败: {e}")))?;

    // 优先用模型配置的输入名，找不到回退到首个输入
    let configured = input_name.clone();
    let input_name = if session.inputs().iter().any(|i| i.name() == configured) {
        configured
    } else {
        session.inputs()[0].name().to_string()
    };
    log::info!("[RMBG] 模型={} 输入名={} 归一化={:?} 手动sigmoid={}",
        m.id, input_name, norm, !sigmoid_output);

    let outputs = session
        .run(ort::inputs![input_name.as_str() => input_tensor])
        .map_err(|e| AppError::Image(format!("推理失败: {e}")))?;

    let (_name, value) = outputs.iter().next()
        .ok_or_else(|| AppError::Image("模型无输出".into()))?;

    let (_shape, data) = value
        .try_extract_tensor::<f32>()
        .map_err(|e| AppError::Image(format!("输出解析失败: {e}")))?;

    // 后处理：CrispCut 输出是 logits，需手动 sigmoid；RMBG/ISNet 输出已 sigmoid
    let probs: Vec<f32> = if *sigmoid_output {
        data.to_vec()
    } else {
        data.iter().map(|&v| 1.0 / (1.0 + (-v).exp())).collect()
    };

    // ISNet 官方 Inference.py 在 sigmoid 后还做 min-max 归一化（拉伸到 [0,1] 提升对比度）
    let probs: Vec<f32> = if m.id == "isnet-general-use" {
        let mn = probs.iter().fold(f32::INFINITY, |a, &b| a.min(b));
        let mx = probs.iter().fold(f32::NEG_INFINITY, |a, &b| a.max(b));
        let range = (mx - mn).max(1e-6);
        probs.iter().map(|&v| (v - mn) / range).collect()
    } else {
        probs
    };

    // ── 调试输出 ──
    let total = probs.len();
    let (mut dmin, mut dmax) = (f32::MAX, f32::MIN);
    let mut sum = 0f64;
    for &v in probs.iter() { dmin = dmin.min(v); dmax = dmax.max(v); sum += v as f64; }
    let avg = sum / total as f64;
    let corners = [
        ("左上", probs[0]),
        ("右上", probs[(w - 1) as usize]),
        ("左下", probs[(h - 1) as usize * w as usize]),
        ("右下", probs[(h * w - 1) as usize]),
        ("中心", probs[(h / 2 * w + w / 2) as usize]),
    ];
    log::info!("[RMBG] output range=[{:.4},{:.4}] avg={:.4}", dmin, dmax, avg);
    for (label, v) in &corners {
        log::info!("[RMBG]   {}: {:.4}", label, v);
    }

    // ── 调试：保存原始 mask ──
    let raw: Vec<u8> = probs.iter().map(|&v| (v * 255.0).clamp(0.0, 255.0) as u8).collect();
    if let Some(mask_img) = image::GrayImage::from_raw(w, h, raw) {
        let mut debug_path = model_path.to_path_buf();
        debug_path.set_extension("debug.png");
        let _ = mask_img.save(&debug_path);
        log::info!("[RMBG] 保存调试 mask: {:?}", debug_path);
    }

    // 输出直接作为 alpha（已 sigmoid，范围 [0,1]）
    let mask: Vec<u8> = probs.iter().map(|&v| (v * 255.0).clamp(0.0, 255.0) as u8).collect();
    Ok(mask)
}
