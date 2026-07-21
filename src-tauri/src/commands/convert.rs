use base64::Engine;

use crate::error::AppError;
use crate::models::ConvertIcoRequest;
use crate::services;

/// 多张图片转单个 ICO 文件，写入用户指定路径
#[tauri::command]
pub async fn convert_images_to_ico(
    req: ConvertIcoRequest,
    save_path: String,
) -> Result<(), AppError> {
    // base64 解码所有图片
    let images: Vec<Vec<u8>> = req
        .images
        .iter()
        .map(|b| base64::engine::general_purpose::STANDARD.decode(b))
        .collect::<Result<_, _>>()?;
    let sizes = req.sizes.clone();

    let result = tokio::task::spawn_blocking(move || {
        services::convert::images_to_ico(&images, &sizes)
    })
    .await
    .map_err(|e| AppError::Image(e.to_string()))??;

    std::fs::write(&save_path, &result)?;
    Ok(())
}
