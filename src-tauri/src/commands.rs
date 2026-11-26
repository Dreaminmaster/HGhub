use serde_json::{json, Value};
use tauri::Manager;
use tauri_plugin_opener::OpenerExt;

#[cfg(target_os = "android")]
pub struct OrientationPlugin(pub tauri::plugin::PluginHandle<tauri::Wry>);

#[cfg(target_os = "android")]
#[tauri::command]
pub async fn set_orientation(landscape: bool, plugin: tauri::State<'_, OrientationPlugin>) -> Result<(), String> {
    plugin.0.run_mobile_plugin_async::<()>("setOrientation", serde_json::json!({"landscape": landscape})).await
        .map_err(|error| error.to_string())
}

#[cfg(not(target_os = "android"))]
#[tauri::command]
pub fn set_orientation(landscape: bool) -> Result<(), String> {
    let _ = landscape;
    Err("当前平台不支持原生屏幕方向切换".into())
}

#[tauri::command]
pub fn app_boot() -> Value {
    json!({"strict_security":crate::network_policy::strict_security()})
}
#[tauri::command]
pub fn security_set(strict: bool) -> Result<bool, String> {
    crate::network_policy::set_strict_security(strict)?;
    Ok(crate::network_policy::strict_security())
}
#[tauri::command]
pub fn debug_log(app: tauri::AppHandle, msg: String) -> Result<(), String> {
    crate::logging::append_debug(&app.path().app_data_dir().map_err(|e| e.to_string())?, &msg)
}
#[tauri::command]
pub fn open_url(app: tauri::AppHandle, url: String) -> Result<(), String> {
    let target = crate::network_policy::parse_target(&url)?;
    app.opener()
        .open_url(target.as_str(), None::<&str>)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn open_external_media(app: tauri::AppHandle, url: String) -> Result<(), String> {
    hghub_hpp::model::url(&url, true, true).map_err(|e| e.message)?;
    app.opener().open_url(url, None::<&str>).map_err(|e| e.to_string())
}

// Only fixed application-owned locations; never accepts an arbitrary path or command.
#[tauri::command]
pub fn maintenance_open_folder(app: tauri::AppHandle, kind: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let dir = match kind.as_str() {
            "data" => app.path().app_data_dir().map_err(|e| e.to_string())?,
            "tools" => app.path().resource_dir().map_err(|e| e.to_string())?.join("maintenance"),
            _ => return Err("未知维护目录".into()),
        };
        if !dir.is_dir() { return Err("维护目录不存在".into()); }
        let status = std::process::Command::new("/usr/bin/open").arg(dir).status().map_err(|_| "无法打开 Finder".to_string())?;
        if !status.success() { return Err("无法打开 Finder".into()); }
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    { let _ = (app, kind); Err("此维护入口仅支持 macOS".into()) }
}
