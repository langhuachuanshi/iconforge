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
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
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
            commands::export::export_icon_to_file,
            commands::generate::get_providers,
            commands::generate::get_templates,
            commands::generate::generate_icon,
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
            commands::extract::extract_icons,
            commands::convert::convert_images_to_ico,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
