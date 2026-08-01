//! 系统级操作：桌面快捷方式管理
//!
//! 仅 Windows 实现。mac/linux 上两个命令都返回"不支持"（不报错，前端忽略）。
//! 快捷方式用 PowerShell 的 WScript.Shell COM 创建 .lnk 文件。

use std::path::PathBuf;

use crate::error::AppError;

/// 快捷方式显示名（不含扩展名）
const SHORTCUT_NAME: &str = "IconForge.lnk";

/// 当前用户桌面路径（兼容 OneDrive 重定向）
#[cfg(target_os = "windows")]
fn desktop_dir() -> Option<PathBuf> {
    // 优先 USERPROFILE\Desktop
    if let Ok(home) = std::env::var("USERPROFILE") {
        let p = PathBuf::from(home).join("Desktop");
        if p.is_dir() {
            return Some(p);
        }
    }
    // OneDrive 重定向情况
    if let Ok(home) = std::env::var("USERPROFILE") {
        let p = PathBuf::from(home).join("OneDrive").join("Desktop");
        if p.is_dir() {
            return Some(p);
        }
    }
    None
}

/// 桌面是否存在本应用的快捷方式
#[tauri::command]
pub async fn has_desktop_shortcut() -> Result<bool, AppError> {
    #[cfg(target_os = "windows")]
    {
        let Some(desktop) = desktop_dir() else {
            return Ok(false);
        };
        return Ok(desktop.join(SHORTCUT_NAME).exists());
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok(false)
    }
}

/// 在桌面创建指向当前 exe 的快捷方式。返回是否成功创建（已存在时也返回 true）。
#[tauri::command]
pub async fn create_desktop_shortcut() -> Result<bool, AppError> {
    #[cfg(target_os = "windows")]
    {
        let Some(desktop) = desktop_dir() else {
            return Ok(false);
        };
        // 当前 exe 路径
        let exe = std::env::current_exe()?;
        let exe_path = exe.to_string_lossy().to_string();
        let lnk_path = desktop.join(SHORTCUT_NAME).to_string_lossy().to_string();

        // 用 PowerShell + WScript.Shell 创建 .lnk
        // -WindowStyle Hidden：不弹窗；目标 exe 作为 TargetPath
        let script = format!(
            "$s=(New-Object -COM WScript.Shell).CreateShortcut('{lnk}');\
             $s.TargetPath='{exe}';\
             $s.WorkingDirectory='{}';\
             $s.Description='IconForge';\
             $s.Save()",
            exe.parent().map(|p| p.to_string_lossy().to_string()).unwrap_or_default(),
            lnk = lnk_path.replace('\'', "''"),
            exe = exe_path.replace('\'', "''"),
        );
        let out = {
            #[cfg(target_os = "windows")]
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            let mut cmd = std::process::Command::new("powershell");
            cmd.args(["-NoProfile", "-NonInteractive", "-Command", &script]);
            // CREATE_NO_WINDOW：彻底避免控制台黑窗闪现（-WindowStyle Hidden 不够）
            #[cfg(target_os = "windows")]
            cmd.creation_flags(CREATE_NO_WINDOW);
            cmd.output()
        };
        match out {
            Ok(o) if o.status.success() => Ok(true),
            Ok(o) => {
                log::warn!("创建快捷方式失败: {}", String::from_utf8_lossy(&o.stderr));
                Ok(false)
            }
            Err(e) => {
                log::warn!("启动 PowerShell 失败: {e}");
                Ok(false)
            }
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok(false)
    }
}
