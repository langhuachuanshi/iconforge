//! LaMa ONNX 本地图像修复（去水印）
//!
//! 模型：Carve/LaMa-ONNX（big-lama 导出，经 hf-mirror 下载）
//! 输入：image [1,3,H,W] f32 [0,1]；mask [1,1,H,W] f32（1 = 待修复区域）
//! 输出：[1,3,H,W] f32，范围 [0,1] 或 [-1,1]（运行时按最小值自适应判定）
//! 尺寸要求：H/W 为 8 的倍数（不足则边缘复制填充，推理后裁回）

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use parking_lot::Mutex;
use reqwest::Client;
use tauri::Emitter;
use tokio::io::AsyncWriteExt;

use crate::error::AppError;
use crate::models::BgDownloadProgress;

/// 推理会话缓存：模型路径 + 文件元数据（换/重导模型后自动失效）。
/// 208MB 模型每次重建要几秒，缓存后只有首次擦除付这个成本。
static SESSION_CACHE: OnceLock<Mutex<Option<(PathBuf, u64, ort::session::Session)>>> =
    OnceLock::new();

/** 取缓存的推理会话；未缓存/已失效则从磁盘加载。返回前保持锁，调用期间独占。 */
fn with_cached_session<T>(
    model_path: &Path,
    f: impl FnOnce(&mut ort::session::Session) -> Result<T, AppError>,
) -> Result<T, AppError> {
    use ort::session::Session;

    let stamp = std::fs::metadata(model_path)
        .map(|m| {
            m.len().wrapping_mul(31)
                ^ m.modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0)
        })
        .unwrap_or(0);

    let cache = SESSION_CACHE.get_or_init(|| Mutex::new(None));
    let mut guard = cache.lock();
    let stale = match guard.as_ref() {
        Some((p, s, _)) => p != model_path || *s != stamp,
        None => true,
    };
    if stale {
        let start = std::time::Instant::now();
        let sess = Session::builder()
            .map_err(|e| AppError::Image(format!("创建 Session 失败: {e}")))?
            .commit_from_file(model_path)
            .map_err(|e| AppError::Image(format!("加载模型失败: {e}")))?;
        log::info!("[Inpaint] 模型加载 {}ms", start.elapsed().as_millis());
        *guard = Some((model_path.to_path_buf(), stamp, sess));
    }
    f(&mut guard.as_mut().expect("cache just filled").2)
}

/// 可用修复模型定义
pub struct InpaintModel {
    pub id: &'static str,
    pub name: &'static str,
    pub url: &'static str,
    pub filename: &'static str,
    pub size: &'static str,
}

pub const INPAINT_MODELS: &[InpaintModel] = &[InpaintModel {
    id: "lama",
    name: "LaMa 智能擦除",
    url: "https://hf-mirror.com/Carve/LaMa-ONNX/resolve/main/lama_fp32.onnx",
    filename: "lama_fp32.onnx",
    size: "约 208MB",
}];

pub fn get_inpaint_model(id: &str) -> &'static InpaintModel {
    INPAINT_MODELS
        .iter()
        .find(|m| m.id == id)
        .unwrap_or(&INPAINT_MODELS[0])
}

/// 模型文件路径（与抠图模型共用 models/ 子目录）
pub fn model_path(base_dir: &Path, filename: &str) -> PathBuf {
    base_dir.join("models").join(filename)
}

pub fn is_downloaded(base_dir: &Path, model_id: &str) -> bool {
    model_path(base_dir, get_inpaint_model(model_id).filename).exists()
}

pub fn delete_model(base_dir: &Path, model_id: &str) -> Result<(), AppError> {
    let target = model_path(base_dir, get_inpaint_model(model_id).filename);
    if target.exists() {
        std::fs::remove_file(&target)?;
    }
    Ok(())
}

/// 下载模型（含进度事件，与抠图模型同模式）
pub async fn download_model(
    window: &tauri::Window,
    model_dir: &Path,
    model_id: &str,
) -> Result<(), AppError> {
    let m = get_inpaint_model(model_id);
    let target = model_path(model_dir, m.filename);
    if target.exists() {
        return Ok(());
    }
    if let Some(parent) = target.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let tmp_path = model_dir.join(format!("{}.tmp", m.filename));

    let client = Client::builder().timeout(Duration::from_secs(1800)).build()?;
    let resp = client.get(m.url).header("User-Agent", "IconForge/1.0").send().await?;
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
            let _ = window.emit(
                "inpaint-download-progress",
                BgDownloadProgress {
                    percent: pct,
                    downloaded: downloaded as u64,
                    total: total as u64,
                },
            );
        }
    }

    file.flush().await?;
    tokio::fs::rename(&tmp_path, &target).await?;
    let _ = window.emit("inpaint-download-complete", ());
    Ok(())
}

/// 智能擦除：PNG 图片字节 + 遮罩（白=擦除；未提供时用 rect 生成）→ 修复后 PNG 字节。
/// 只替换遮罩内像素，其余保持原图。
pub fn run_inpaint(
    model_dir: &Path,
    image_bytes: &[u8],
    mask_bytes: Option<&[u8]>,
    rect: (f64, f64, f64, f64),
    model_id: &str,
) -> Result<Vec<u8>, AppError> {
    const MODEL_SIZE: usize = 512;

    let model_path = model_path(model_dir, get_inpaint_model(model_id).filename);
    if !model_path.exists() {
        return Err(AppError::Image("擦除模型未下载".into()));
    }

    let img = image::load_from_memory(image_bytes)
        .map_err(|e| AppError::Image(format!("解码图片失败: {e}")))?
        .to_rgba8();
    let (w, h) = img.dimensions();
    let (w, h) = (w as usize, h as usize);

    // 遮罩图（白=擦除）：优先涂抹遮罩，否则由矩形生成
    let mask_img: image::GrayImage = if let Some(mb) = mask_bytes {
        let m = image::load_from_memory(mb)
            .map_err(|e| AppError::Image(format!("解码遮罩失败: {e}")))?
            .to_luma8();
        if m.dimensions() != (w as u32, h as u32) {
            image::imageops::resize(
                &m,
                w as u32,
                h as u32,
                image::imageops::FilterType::Nearest,
            )
        } else {
            m
        }
    } else {
        let (rx, ry, rw, rh) = rect;
        let x = (rx.clamp(0.0, 1.0) * w as f64).round() as u32;
        let y = (ry.clamp(0.0, 1.0) * h as f64).round() as u32;
        let rw2 = (rw.clamp(0.0, 1.0) * w as f64).round() as u32;
        let rh2 = (rh.clamp(0.0, 1.0) * h as f64).round() as u32;
        let mut m = image::GrayImage::new(w as u32, h as u32);
        for yy in y..(y + rh2).min(h as u32) {
            for xx in x..(x + rw2).min(w as u32) {
                m.put_pixel(xx, yy, image::Luma([255u8]));
            }
        }
        m
    };

    // 擦除像素的包围盒
    let mut x0 = w;
    let mut y0 = h;
    let mut x1 = 0usize;
    let mut y1 = 0usize;
    for (x, y, p) in mask_img.enumerate_pixels() {
        if p[0] >= 128 {
            let (px, py) = (x as usize, y as usize);
            x0 = x0.min(px);
            y0 = y0.min(py);
            x1 = x1.max(px + 1);
            y1 = y1.max(py + 1);
        }
    }
    if x1 <= x0 || y1 <= y0 {
        return Err(AppError::Image("擦除区域为空，请先涂抹或框选".into()));
    }

    // 选区外加 25% 上下文（帮助 LaMa 理解周边内容），裁剪后缩放到模型固定的 512×512
    let ctx = ((x1 - x0).max(y1 - y0)) / 4;
    let ctx = ctx.max(16);
    let ex0 = x0.saturating_sub(ctx);
    let ey0 = y0.saturating_sub(ctx);
    let ex1 = (x1 + ctx).min(w);
    let ey1 = (y1 + ctx).min(h);
    let (cw, ch) = (ex1 - ex0, ey1 - ey0);

    let crop = image::imageops::crop_imm(&img, ex0 as u32, ey0 as u32, cw as u32, ch as u32).to_image();
    let input_img = image::imageops::resize(
        &crop,
        MODEL_SIZE as u32,
        MODEL_SIZE as u32,
        image::imageops::FilterType::CatmullRom,
    );
    // 遮罩同步裁剪缩放（最近邻保持二值）
    let crop_mask = image::imageops::crop_imm(&mask_img, ex0 as u32, ey0 as u32, cw as u32, ch as u32)
        .to_image();
    let mask_512 = image::imageops::resize(
        &crop_mask,
        MODEL_SIZE as u32,
        MODEL_SIZE as u32,
        image::imageops::FilterType::Nearest,
    );

    // 构造张量：image [0,1] CHW；mask 0/1（1 = 待修复）
    let mut img_data = vec![0f32; 3 * MODEL_SIZE * MODEL_SIZE];
    let mut mask_data = vec![0f32; MODEL_SIZE * MODEL_SIZE];
    for y in 0..MODEL_SIZE {
        for x in 0..MODEL_SIZE {
            let p = input_img.get_pixel(x as u32, y as u32);
            img_data[y * MODEL_SIZE + x] = p[0] as f32 / 255.0;
            img_data[MODEL_SIZE * MODEL_SIZE + y * MODEL_SIZE + x] = p[1] as f32 / 255.0;
            img_data[2 * MODEL_SIZE * MODEL_SIZE + y * MODEL_SIZE + x] = p[2] as f32 / 255.0;
            if mask_512.get_pixel(x as u32, y as u32)[0] >= 128 {
                mask_data[y * MODEL_SIZE + x] = 1.0;
            }
        }
    }

    log::info!(
        "[Inpaint] 选区 {}x{}，上下文裁剪 {}x{} → 模型 {}x{}",
        x1 - x0,
        y1 - y0,
        cw,
        ch,
        MODEL_SIZE,
        MODEL_SIZE
    );

    let image_tensor = ort::value::Tensor::from_array((
        vec![1i64, 3, MODEL_SIZE as i64, MODEL_SIZE as i64],
        img_data,
    ))
    .map_err(|e| AppError::Image(format!("创建 image Tensor 失败: {e}")))?;
    let mask_tensor = ort::value::Tensor::from_array((
        vec![1i64, 1, MODEL_SIZE as i64, MODEL_SIZE as i64],
        mask_data,
    ))
    .map_err(|e| AppError::Image(format!("创建 mask Tensor 失败: {e}")))?;

    // 推理：会话全局缓存（208MB 模型每次重建要数秒，缓存后只有首次加载）
    let infer_start = std::time::Instant::now();
    let data: Vec<f32> = with_cached_session(&model_path, |session| {
        let image_name = session
            .inputs()
            .iter()
            .find(|i| i.name() == "image")
            .map(|i| i.name().to_string())
            .unwrap_or_else(|| session.inputs()[0].name().to_string());
        let mask_name = session
            .inputs()
            .iter()
            .find(|i| i.name() == "mask")
            .map(|i| i.name().to_string())
            .unwrap_or_else(|| {
                session
                    .inputs()
                    .get(1)
                    .map(|i| i.name().to_string())
                    .unwrap_or_else(|| "mask".into())
            });

        let outputs = session
            .run(ort::inputs![image_name.as_str() => image_tensor, mask_name.as_str() => mask_tensor])
            .map_err(|e| AppError::Image(format!("智能擦除推理失败: {e}")))?;

        let (_name, value) = outputs
            .iter()
            .next()
            .ok_or_else(|| AppError::Image("模型无输出".into()))?;
        let (_oshape, view) = value
            .try_extract_tensor::<f32>()
            .map_err(|e| AppError::Image(format!("输出解析失败: {e}")))?;
        Ok(view.to_vec())
    })?;
    log::info!("[Inpaint] 推理 {}ms", infer_start.elapsed().as_millis());

    // 输出范围自适应（该导出实际输出 0..255；兼容 0..1 与 [-1,1] 两种导出习惯）
    let out_min = data.iter().cloned().fold(f32::INFINITY, f32::min);
    let out_max = data.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    log::info!("[Inpaint] 输出范围 min={out_min} max={out_max}");
    let to_byte = |v: f32| -> u8 {
        let x = if out_max > 1.5 {
            v.clamp(0.0, 255.0)
        } else if out_min < -0.05 {
            ((v + 1.0) / 2.0).clamp(0.0, 1.0) * 255.0
        } else {
            v.clamp(0.0, 1.0) * 255.0
        };
        x.round() as u8
    };

    // 模型输出 → 512 RGBA → 缩放回上下文尺寸
    let mut out512 = image::RgbaImage::new(MODEL_SIZE as u32, MODEL_SIZE as u32);
    for y in 0..MODEL_SIZE {
        for x in 0..MODEL_SIZE {
            let idx = y * MODEL_SIZE + x;
            let r = to_byte(data[idx]);
            let g = to_byte(data[MODEL_SIZE * MODEL_SIZE + idx]);
            let b = to_byte(data[2 * MODEL_SIZE * MODEL_SIZE + idx]);
            out512.put_pixel(x as u32, y as u32, image::Rgba([r, g, b, 255]));
        }
    }
    let out_full = image::imageops::resize(
        &out512,
        cw as u32,
        ch as u32,
        image::imageops::FilterType::CatmullRom,
    );

    // 写回：仅遮罩内像素取模型输出，遮罩外保留原图
    let mut out = img;
    for y in y0..y1 {
        for x in x0..x1 {
            if mask_img.get_pixel(x as u32, y as u32)[0] < 128 {
                continue;
            }
            let p = out_full.get_pixel((x - ex0) as u32, (y - ey0) as u32);
            out.put_pixel(x as u32, y as u32, image::Rgba([p[0], p[1], p[2], 255]));
        }
    }

    let mut png = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut png);
    out.write_to(&mut cursor, image::ImageFormat::Png)
        .map_err(|e| AppError::Image(format!("编码 PNG 失败: {e}")))?;
    Ok(png)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 端到端验证：需模型已下载 + 本地水印样图，手动运行 `cargo test -- --ignored`
    #[test]
    #[ignore = "需要已下载模型与本地样图"]
    fn inpaint_watermark_sample() {
        let model_dir = Path::new(r"C:\Users\silas\AppData\Roaming\com.iconforge.app");
        let img = std::fs::read(r"C:\Users\silas\Pictures\image_498889043065379.png").unwrap();
        let out = run_inpaint(model_dir, &img, None, (0.60, 0.82, 0.40, 0.18), "lama").unwrap();
        std::fs::write(r"C:\Users\silas\AppData\Local\Temp\inpaint-out.png", &out).unwrap();
    }

    /// 涂抹遮罩路径：黑底白块模拟笔刷遮罩；连跑两次验证会话缓存
    #[test]
    #[ignore = "需要已下载模型与本地样图"]
    fn inpaint_mask_sample() {
        let model_dir = Path::new(r"C:\Users\silas\AppData\Roaming\com.iconforge.app");
        let img = std::fs::read(r"C:\Users\silas\Pictures\image_498889043065379.png").unwrap();
        let mut m = image::GrayImage::new(1024, 1024);
        for yy in 880..1000 {
            for xx in 640..980 {
                m.put_pixel(xx, yy, image::Luma([255u8]));
            }
        }
        let mut png = Vec::new();
        image::DynamicImage::ImageLuma8(m)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let t0 = std::time::Instant::now();
        let out = run_inpaint(model_dir, &img, Some(&png), (0.0, 0.0, 0.0, 0.0), "lama").unwrap();
        std::fs::write(r"C:\Users\silas\AppData\Local\Temp\inpaint-mask-out.png", &out).unwrap();
        eprintln!("第一次（含模型加载）: {}ms", t0.elapsed().as_millis());
        let t1 = std::time::Instant::now();
        let _ = run_inpaint(model_dir, &img, Some(&png), (0.0, 0.0, 0.0, 0.0), "lama").unwrap();
        eprintln!("第二次（会话缓存）: {}ms", t1.elapsed().as_millis());
    }
}
