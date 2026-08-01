use base64::Engine;
use tauri::{Emitter, State, Window};

use crate::error::AppError;
use crate::models::{CropRequest, ImageResponse, RemoveBgRequest, RemoveColorRequest, EdgeRefineRequest, SmartCropRequest, ShapeMaskRequest, AdjustColorRequest, BgModelEntry};
use crate::services;
use crate::AppState;

/// 裁剪图片
#[tauri::command]
pub async fn crop_image(req: CropRequest) -> Result<ImageResponse, AppError> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(&req.image)?;

    let result = tokio::task::spawn_blocking(move || {
        services::image::crop(&bytes, req.x as u32, req.y as u32, req.width, req.height)
    })
    .await
    .map_err(|e| AppError::Image(e.to_string()))??;

    Ok(ImageResponse {
        image: base64::engine::general_purpose::STANDARD.encode(&result),
        format: "PNG".into(),
    })
}

/// 保存当前图片到指定路径
#[tauri::command]
pub async fn save_image_file(save_path: String, image: String) -> Result<(), AppError> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(&image)?;
    std::fs::write(&save_path, &bytes)?;
    Ok(())
}

/// 导入本地模型文件
#[tauri::command]
pub async fn import_bg_model(
    state: State<'_, AppState>,
    source_path: String,
    model_id: Option<String>,
) -> Result<(), AppError> {
    let mid = model_id.unwrap_or_else(|| "rmbg-1.4".into());
    let m = services::remove_bg::get_model(&mid);
    let target = {
        let storage = state.storage.lock();
        services::remove_bg::model_path(storage.base_dir(), m.filename)
    };
    std::fs::copy(&source_path, &target)?;
    Ok(())
}

fn get_model_id(storage: &crate::services::storage::Storage) -> String {
    let id = storage.get_config("bg_model", "crispcut-quality");
    if id.is_empty() { "crispcut-quality".into() } else { id }
}

/// 检查抠图模型是否已下载
#[tauri::command]
pub async fn check_bg_model(state: State<'_, AppState>) -> Result<serde_json::Value, AppError> {
    let storage = state.storage.lock();
    let mid = get_model_id(&storage);
    let has = services::remove_bg::model_exists(storage.base_dir(), &mid);
    Ok(serde_json::json!({"downloaded": has, "model": mid}))
}

/// 列出所有抠图模型及其下载状态
#[tauri::command]
pub async fn list_bg_models(state: State<'_, AppState>) -> Result<Vec<BgModelEntry>, AppError> {
    let storage = state.storage.lock();
    let base_dir = storage.base_dir().to_path_buf();
    let current = get_model_id(&storage);
    drop(storage);

    let mut list = Vec::with_capacity(services::remove_bg::BG_MODELS.len());
    for m in services::remove_bg::BG_MODELS {
        let p = services::remove_bg::model_path(&base_dir, m.filename);
        let downloaded = p.exists();
        list.push(BgModelEntry {
            id: m.id.to_string(),
            name: m.name.to_string(),
            size: m.size.to_string(),
            downloaded,
            path: if downloaded { Some(p.to_string_lossy().to_string()) } else { None },
            current: m.id == current,
        });
    }
    Ok(list)
}

/// 删除已下载的模型文件
#[tauri::command]
pub async fn delete_bg_model(
    state: State<'_, AppState>,
    model_id: String,
) -> Result<(), AppError> {
    let storage = state.storage.lock();
    let base_dir = storage.base_dir().to_path_buf();
    drop(storage);

    let m = services::remove_bg::get_model(&model_id);
    let p = services::remove_bg::model_path(&base_dir, m.filename);
    if !p.exists() {
        return Err(AppError::NotFound(format!("模型 {} 未下载", model_id)));
    }
    std::fs::remove_file(&p)?;
    Ok(())
}

/// 在系统资源管理器中打开模型所在位置（Windows 选中文件，其他平台打开目录）
#[tauri::command]
pub async fn open_model_location(
    state: State<'_, AppState>,
    model_id: String,
) -> Result<(), AppError> {
    let storage = state.storage.lock();
    let base_dir = storage.base_dir().to_path_buf();
    drop(storage);

    let m = services::remove_bg::get_model(&model_id);
    let p = services::remove_bg::model_path(&base_dir, m.filename);
    if !p.exists() {
        return Err(AppError::NotFound(format!("模型 {} 未下载", model_id)));
    }

    // 平台分支：Windows 用 explorer /select 选中文件；其他平台打开父目录
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(format!("/select,{}", p.display()))
            .spawn()?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(p.parent().unwrap_or(&p))
            .spawn()?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(p.parent().unwrap_or(&p))
            .spawn()?;
    }
    Ok(())
}

/// 下载抠图模型（含进度事件）
#[tauri::command]
pub async fn download_bg_model(
    window: tauri::Window,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    let (model_dir, mid) = {
        let storage = state.storage.lock();
        (storage.base_dir().to_path_buf(), get_model_id(&storage))
    };
    services::remove_bg::download_model(&window, &model_dir, &mid).await
}

/// 移除背景
#[tauri::command]
pub async fn remove_background(
    state: State<'_, AppState>,
    req: RemoveBgRequest,
) -> Result<ImageResponse, AppError> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(&req.image)?;
    let (model_dir, mid) = {
        let storage = state.storage.lock();
        (storage.base_dir().to_path_buf(), get_model_id(&storage))
    };
    let threshold = req.threshold.clamp(0.0, 1.0);

    let result = tokio::task::spawn_blocking(move || {
        services::remove_bg::run_inference(&model_dir, &bytes, threshold, &mid)
    })
    .await
    .map_err(|e| AppError::Image(e.to_string()))??;

    Ok(ImageResponse {
        image: base64::engine::general_purpose::STANDARD.encode(&result),
        format: "PNG".into(),
    })
}

/// 云端抠图（阿里云分割抠图：通用分割 SegmentCommonImage / 商品分割 SegmentCommodity）
#[tauri::command]
pub async fn remove_background_cloud(
    state: State<'_, AppState>,
    window: Window,
    req: RemoveBgRequest,
) -> Result<ImageResponse, AppError> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(&req.image)?;
    let (ak, sk, cloud_model) = {
        let storage = state.storage.lock();
        (
            storage.get_config("aliyun_ak", ""),
            storage.get_config("aliyun_sk", ""),
            storage.get_config("cloud_model", "common"),
        )
    };
    if ak.is_empty() || sk.is_empty() {
        return Err(AppError::ProviderError(
            "未配置阿里云 AccessKey，请在设置中填写".into(),
        ));
    }
    // 诊断日志通过事件发到前端 console
    let logger = move |msg: &str| {
        let _ = window.emit("aliyun-log", msg);
    };
    let result =
        services::aliyun_imageseg::remove_background(&bytes, &ak, &sk, &cloud_model, &logger)
            .await?;
    Ok(ImageResponse {
        image: base64::engine::general_purpose::STANDARD.encode(&result),
        format: "PNG".into(),
    })
}

/// 按颜色去底（魔棒/色键）
#[tauri::command]
pub async fn remove_color(req: RemoveColorRequest) -> Result<ImageResponse, AppError> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(&req.image)?;
    let color = req.color;
    let tolerance = req.tolerance.clamp(0.0, 442.0);

    let result = tokio::task::spawn_blocking(move || {
        services::image::remove_color(&bytes, color, tolerance)
    })
    .await
    .map_err(|e| AppError::Image(e.to_string()))??;

    Ok(ImageResponse {
        image: base64::engine::general_purpose::STANDARD.encode(&result),
        format: "PNG".into(),
    })
}

/// 边缘净化：一个命令覆盖 erode(收缩) / feather(羽化) / decontaminate(去色晕) / stroke(内描边)
#[tauri::command]
pub async fn edge_refine(req: EdgeRefineRequest) -> Result<ImageResponse, AppError> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(&req.image)?;
    let op = req.op;
    let amount = req.amount.max(0.0);
    let color = req.color;

    let result = tokio::task::spawn_blocking(move || -> Result<Vec<u8>, AppError> {
        match op.as_str() {
            "erode" => services::image::erode_alpha(&bytes, amount.round() as u32),
            "feather" => services::image::feather_alpha(&bytes, amount as f32),
            "decontaminate" => services::image::decontaminate(&bytes, amount.round().max(1.0) as u32),
            "stroke" => services::image::add_inner_stroke(&bytes, amount.round() as u32, color),
            other => Err(AppError::Image(format!("未知边缘净化操作: {other}"))),
        }
    })
    .await
    .map_err(|e| AppError::Image(e.to_string()))??;

    Ok(ImageResponse {
        image: base64::engine::general_purpose::STANDARD.encode(&result),
        format: "PNG".into(),
    })
}

/// 智能裁剪：mode=trim(去透明边距) / aspect(按宽高比)
#[tauri::command]
pub async fn smart_crop(req: SmartCropRequest) -> Result<ImageResponse, AppError> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(&req.image)?;
    let result = tokio::task::spawn_blocking(move || -> Result<Vec<u8>, AppError> {
        if req.ratio_w > 0 || req.ratio_h > 0 {
            services::image::crop_to_aspect(&bytes, req.ratio_w.max(1), req.ratio_h.max(1))
        } else {
            // 默认走 trim
            services::image::trim_transparent(&bytes, (req.threshold.min(255) as u8))
        }
    })
    .await
    .map_err(|e| AppError::Image(e.to_string()))??;
    Ok(ImageResponse {
        image: base64::engine::general_purpose::STANDARD.encode(&result),
        format: "PNG".into(),
    })
}

/// 应用形状遮罩：shape=rounded(圆角矩形,带radius) / circle(圆形)
#[tauri::command]
pub async fn apply_shape_mask(req: ShapeMaskRequest) -> Result<ImageResponse, AppError> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(&req.image)?;
    let shape = req.shape;
    let radius = req.radius;
    let result = tokio::task::spawn_blocking(move || -> Result<Vec<u8>, AppError> {
        match shape.as_str() {
            "circle" => services::image::apply_circle_mask(&bytes),
            _ => services::image::apply_rounded_mask(&bytes, radius),
        }
    })
    .await
    .map_err(|e| AppError::Image(e.to_string()))??;
    Ok(ImageResponse {
        image: base64::engine::general_purpose::STANDARD.encode(&result),
        format: "PNG".into(),
    })
}

/// 调色：亮度/对比度/饱和度
#[tauri::command]
pub async fn adjust_color(req: AdjustColorRequest) -> Result<ImageResponse, AppError> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(&req.image)?;
    let (b, c, s) = (req.brightness, req.contrast, req.saturation);
    let result = tokio::task::spawn_blocking(move || {
        services::image::adjust_brightness_contrast(&bytes, b, c, s)
    })
    .await
    .map_err(|e| AppError::Image(e.to_string()))??;
    Ok(ImageResponse {
        image: base64::engine::general_purpose::STANDARD.encode(&result),
        format: "PNG".into(),
    })
}
