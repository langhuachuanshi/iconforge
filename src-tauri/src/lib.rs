mod commands;
mod data;
mod error;
mod models;
mod providers;
mod services;

use std::sync::Arc;
use parking_lot::Mutex;
use tauri::Manager;
use services::storage::Storage;

pub struct AppState {
    pub storage: Arc<Mutex<Storage>>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]

/// 目录可写探测：能建目录且能写入探测文件
fn is_dir_writable(dir: &std::path::Path) -> bool {
    if std::fs::create_dir_all(dir).is_err() {
        return false;
    }
    let probe = dir.join(".write-probe");
    match std::fs::write(&probe, b"ok") {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

/// 旧 %APPDATA% 数据迁移到运行目录：icons.db / icons / icon_versions / models 全量搬移。
/// 同卷 rename 直移，跨卷 copy+delete；已迁移（目标有 icons.db）则跳过。
fn migrate_legacy_data(old_dir: &std::path::Path, new_dir: &std::path::Path) {
    use std::path::Path;
    if !old_dir.join("icons.db").exists() || new_dir.join("icons.db").exists() {
        return;
    }
    fn move_rec(src: &Path, dst: &Path) -> std::io::Result<()> {
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            let from = entry.path();
            let to = dst.join(entry.file_name());
            if from.is_dir() {
                std::fs::create_dir_all(&to)?;
                move_rec(&from, &to)?;
                std::fs::remove_dir(&from).ok();
            } else {
                // 先试 rename（同卷瞬时），失败（跨卷）退回复制+删除
                if std::fs::rename(&from, &to).is_err() {
                    std::fs::copy(&from, &to)?;
                    std::fs::remove_file(&from)?;
                }
            }
        }
        Ok(())
    }
    std::fs::create_dir_all(new_dir).ok();
    match move_rec(old_dir, new_dir) {
        Ok(_) => log::info!(
            "[存储] 历史数据已迁移至运行目录: {}",
            new_dir.display()
        ),
        Err(e) => log::warn!("[存储] 迁移旧数据失败（继续用旧目录逻辑无效，按新目录空数据运行）: {e}"),
    }
}
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // 数据目录：优先 exe 同级的「运行目录」（用户可自行查找/备份/管理）；
            // 运行目录不可写（如装在 Program Files）时回退 %APPDATA%。
            let exe_dir = std::env::current_exe()?
                .parent()
                .map(|p| p.to_path_buf())
                .ok_or_else(|| tauri::Error::Io(std::io::Error::new(std::io::ErrorKind::NotFound, "无法定位程序目录")))?;
            let run_dir = exe_dir.join("IconForge 数据");
            let data_dir = if is_dir_writable(&run_dir) {
                // 首次切到运行目录：把旧 %APPDATA% 数据整体迁移过来
                let old = app.path().app_data_dir()?;
                migrate_legacy_data(&old, &run_dir);
                run_dir
            } else {
                app.path().app_data_dir()?
            };
            std::fs::create_dir_all(&data_dir)?;

            let storage = Storage::new(data_dir)
                .expect("无法初始化存储");

            // 预置默认服务商
            storage.seed_default_providers()
                .expect("无法预置默认服务商");

            app.manage(AppState {
                storage: Arc::new(Mutex::new(storage)),
            });

            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::edit::crop_image,
            commands::edit::check_bg_model,
            commands::edit::list_bg_models,
            commands::edit::delete_bg_model,
            commands::edit::open_model_location,
            commands::edit::download_bg_model,
            commands::edit::import_bg_model,
            commands::edit::save_image_file,
            commands::edit::remove_background,
            commands::edit::remove_background_cloud,
            commands::edit::remove_color,
            commands::edit::edge_refine,
            commands::edit::smart_crop,
            commands::edit::apply_shape_mask,
            commands::edit::adjust_color,
            commands::export::export_icon_to_file,
            commands::export::export_icons_to_dir,
            commands::generate::get_providers,
            commands::generate::get_templates,
            commands::generate::generate_icon,
            commands::generate::test_provider,
            commands::edit::test_aliyun_matting,
            commands::edit::add_custom_model,
            commands::edit::list_inpaint_models,
            commands::edit::set_inpaint_model,
            commands::edit::download_inpaint_model,
            commands::edit::delete_inpaint_model,
            commands::edit::import_inpaint_model,
            commands::edit::open_inpaint_location,
            commands::edit::inpaint_region,
            commands::config::get_config,
            commands::config::set_config,
            commands::config::list_providers,
            commands::config::add_provider,
            commands::config::update_provider,
            commands::config::delete_provider,
            commands::config::toggle_provider,
            commands::config::reorder_providers,
            commands::history::list_icons,
            commands::history::get_icon_base64,
            commands::history::get_icon_path,
            commands::history::delete_icon,
            commands::history::save_icon_version,
            commands::history::list_icon_versions,
            commands::history::load_icon_version,
            commands::history::delete_icon_version,
            commands::extract::extract_icons,
            commands::extract::extract_icons_from_bytes,
            commands::convert::convert_images_to_ico,
            commands::system::has_desktop_shortcut,
            commands::system::create_desktop_shortcut,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
