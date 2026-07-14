use std::path::{Path, PathBuf};
use std::time::Duration;

use reqwest::Client;
use tauri::Emitter;
use tokio::io::AsyncWriteExt;

use crate::error::AppError;
use crate::models::BgDownloadProgress;

const MODEL_FILENAME: &str = "rmbg-2.0.onnx";
pub const DEFAULT_MODEL_URL: &str =
    "https://huggingface.co/briaai/RMBG-2.0/resolve/main/onnx/model.onnx";

/// 模型文件路径（存储在 models/ 子目录）
pub fn model_path(base_dir: &Path) -> PathBuf {
    base_dir.join("models").join(MODEL_FILENAME)
}

/// 检查模型是否已下载
pub fn model_exists(model_dir: &Path) -> bool {
    model_path(model_dir).exists()
}

/// 下载模型并报告进度
pub async fn download_model(
    window: &tauri::Window,
    model_dir: &Path,
    model_url: Option<&str>,
) -> Result<(), AppError> {
    let model_path = model_path(model_dir);
    if let Some(parent) = model_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let url = model_url.unwrap_or(DEFAULT_MODEL_URL);
    let tmp_path = model_dir.join(format!("{}.tmp", MODEL_FILENAME));

    let client = Client::builder()
        .timeout(Duration::from_secs(600))
        .build()?;

    let resp = client
        .get(url)
        .header("User-Agent", "IconForge/1.0")
        .send()
        .await?;
    if !resp.status().is_success() {
        return Err(AppError::Http(format!(
            "模型下载失败 (HTTP {})",
            resp.status().as_u16()
        )));
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
            let _ = window.emit("bg-download-progress", BgDownloadProgress {
                percent: pct,
                downloaded: downloaded as u64,
                total: total as u64,
            });
        }
    }

    file.flush().await?;
    tokio::fs::rename(&tmp_path, &model_path).await?;

    let _ = window.emit("bg-download-complete", ());

    Ok(())
}

/// 运行抠图推理（使用 ONNX Runtime）
pub fn run_inference(model_dir: &Path, image_bytes: &[u8], threshold: f64) -> Result<Vec<u8>, AppError> {
    let model_path = model_path(model_dir);
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

    // 预处理：CHW，像素归一化到 [0, 1]
    let mut input_data = Vec::with_capacity(3 * 1024 * 1024);
    for y in 0..1024 {
        for x in 0..1024 {
            let p = resized.get_pixel(x, y);
            input_data.push(p[0] as f32 / 255.0);
        }
    }
    for y in 0..1024 {
        for x in 0..1024 {
            let p = resized.get_pixel(x, y);
            input_data.push(p[1] as f32 / 255.0);
        }
    }
    for y in 0..1024 {
        for x in 0..1024 {
            let p = resized.get_pixel(x, y);
            input_data.push(p[2] as f32 / 255.0);
        }
    }

    // ONNX Runtime 推理
    let mask = run_ort_inference(&model_path, &input_data, 1024, 1024, threshold)?;

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

    let input_name = session.inputs()[0].name().to_string();
    let outputs = session
        .run(ort::inputs![input_name => input_tensor])
        .map_err(|e| AppError::Image(format!("推理失败: {e}")))?;

    let (_name, value) = outputs.iter().next()
        .ok_or_else(|| AppError::Image("模型无输出".into()))?;

    let (_shape, data) = value
        .try_extract_tensor::<f32>()
        .map_err(|e| AppError::Image(format!("输出解析失败: {e}")))?;

    let mut mask = Vec::with_capacity((w * h) as usize);
    for y in 0..h as usize {
        for x in 0..w as usize {
            let v = data[y * w as usize + x];
            let sig = 1.0f32 / (1.0 + (-v).exp());
            let alpha = if sig >= threshold as f32 {
                (sig * 255.0).clamp(0.0, 255.0) as u8
            } else {
                0u8
            };
            mask.push(alpha);
        }
    }

    Ok(mask)
}
