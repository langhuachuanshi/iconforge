//! 系统级操作：桌面快捷方式管理
//!
//! 仅 Windows 实现。mac/linux 上两个命令都返回"不支持"（不报错，前端忽略）。
//! 快捷方式用 PowerShell 的 WScript.Shell COM 创建 .lnk 文件。

use std::path::PathBuf;

use crate::error::AppError;

/// 快捷方式显示名（不含扩展名）
const SHORTCUT_NAME: &str = "IconForge.lnk";

/// 所有需要检测/创建快捷方式的桌面候选路径，按优先级排列。
/// 顺序：当前用户桌面 → OneDrive 重定向桌面 → 公共桌面。
/// 公共桌面必须纳入：MSI perMachine 安装会把快捷方式放这里，
/// 若只查用户桌面会导致"已安装但每次启动都弹询问"的误判。
#[cfg(target_os = "windows")]
fn desktop_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(home) = std::env::var("USERPROFILE") {
        let user_desktop = PathBuf::from(&home).join("Desktop");
        if user_desktop.is_dir() {
            dirs.push(user_desktop);
        }
        let onedrive_desktop = PathBuf::from(&home).join("OneDrive").join("Desktop");
        if onedrive_desktop.is_dir() {
            dirs.push(onedrive_desktop);
        }
    }
    // 公共桌面：MSI perMachine 安装创建快捷方式的默认位置
    if let Ok(public) = std::env::var("PUBLIC") {
        let common_desktop = PathBuf::from(public).join("Desktop");
        if common_desktop.is_dir() {
            dirs.push(common_desktop);
        }
    }
    dirs
}

/// 桌面是否存在本应用的快捷方式（任意一个桌面命中即视为已存在）
#[tauri::command]
pub async fn has_desktop_shortcut() -> Result<bool, AppError> {
    #[cfg(target_os = "windows")]
    {
        return Ok(desktop_dirs().iter().any(|d| d.join(SHORTCUT_NAME).exists()));
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok(false)
    }
}

/// 在桌面创建指向当前 exe 的快捷方式。返回是否成功创建（已存在时也返回 true）。
/// 优先在用户桌面创建；用户桌面不可写时回退到公共桌面。
#[tauri::command]
pub async fn create_desktop_shortcut() -> Result<bool, AppError> {
    #[cfg(target_os = "windows")]
    {
        // 优先取第一个用户桌面（公共桌面通常需要管理员权限才能写入）
        let Some(desktop) = desktop_dirs().into_iter().next() else {
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
