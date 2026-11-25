//! Native, signed update boundary. No arbitrary URL, key or install path from IPC.
use serde_json::{json, Value};
const KEY: Option<&str> = option_env!("HGHUB_UPDATER_PUBLIC_KEY");

fn key() -> Option<&'static str> { KEY.map(str::trim).filter(|v| !v.is_empty()) }
fn endpoint(channel: &str) -> Result<&'static str, String> {
    match channel {
        "stable" => Ok("https://github.com/Dreaminmaster/HGhub/releases/download/macos-stable/latest.json"),
        "beta" => Ok("https://github.com/Dreaminmaster/HGhub/releases/download/macos-beta/latest.json"),
        _ => Err("无效更新渠道".into()),
    }
}

#[cfg(target_os = "macos")]
pub fn register(app: &tauri::AppHandle) -> tauri::Result<()> {
    if let Some(k) = key() {
        app.plugin(tauri_plugin_updater::Builder::new().pubkey(k).build())?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn updater(app: &tauri::AppHandle, channel: &str) -> Result<tauri_plugin_updater::Updater, String> {
    use tauri_plugin_updater::UpdaterExt;
    let url = endpoint(channel)?.parse().map_err(|_| "更新地址无效")?;
    let k = key().ok_or("尚未配置可信更新公钥")?;
    let (direct, proxy) = crate::net::updater_proxy()?;
    let mut b = app.updater_builder().pubkey(k).target("macos-universal")
        .timeout(std::time::Duration::from_secs(90))
        .endpoints(vec![url]).map_err(|_| "更新配置无效")?;
    if direct { b = b.no_proxy(); }
    if let Some(p) = proxy { b = b.proxy(p); }
    b.build().map_err(|_| "无法初始化签名更新")
}

#[cfg(target_os = "macos")]
fn allowed_asset(u: &url::Url) -> bool {
    u.scheme()=="https" && u.host_str()==Some("github.com") && u.port().is_none()
        && u.username().is_empty() && u.password().is_none()
        && u.query().is_none() && u.fragment().is_none()
        && u.path().starts_with("/Dreaminmaster/HGhub/releases/download/")
        && u.path().ends_with(".app.tar.gz")
}

#[tauri::command]
pub async fn signed_update_check(app: tauri::AppHandle, channel: String) -> Result<Value,String> {
    endpoint(&channel)?;
    #[cfg(target_os = "macos")]
    {
        if key().is_none() { return Ok(json!({"configured":false,"status":"key_missing"})); }
        let update = updater(&app,&channel)?.check().await.map_err(|_| "签名更新清单暂不可用")?;
        match update {
            Some(u) if allowed_asset(&u.download_url) => Ok(json!({"configured":true,"status":"available","version":u.version,"notes":u.body})),
            Some(_) => Err("拒绝非指定仓库的更新文件".into()),
            None => Ok(json!({"configured":true,"status":"current"})),
        }
    }
    #[cfg(not(target_os = "macos"))]
    { let _ = app; Ok(json!({"configured":false,"status":"unsupported"})) }
}

#[tauri::command]
pub async fn signed_update_install(app: tauri::AppHandle, channel:String, version:String) -> Result<(),String> {
    endpoint(&channel)?;
    #[cfg(target_os = "macos")]
    {
        use std::sync::atomic::{AtomicBool, Ordering};
        static BUSY: AtomicBool=AtomicBool::new(false);
        if BUSY.swap(true,Ordering::AcqRel) { return Err("已有更新正在进行".into()); }
        struct Guard;
        impl Drop for Guard { fn drop(&mut self) { BUSY.store(false,Ordering::Release); } }
        let _guard=Guard;
        let update=updater(&app,&channel)?.check().await.map_err(|_| "无法重新验证更新")?
            .ok_or("更新已不可用，请重新检查")?;
        if update.version != version || !allowed_asset(&update.download_url) { return Err("更新版本或地址已变化，请重新检查".into()); }
        // The official updater verifies the pinned-key signature before install.
        update.download_and_install(|_,_|{},||{}).await.map_err(|_| "下载、签名验证或安装失败；请保留当前版本")?;
        app.restart();
    }
    #[cfg(not(target_os = "macos"))]
    { let _=(app,version); Err("当前平台不支持应用内签名安装".into()) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn channels_are_fixed() {
        assert!(endpoint("stable").is_ok()); assert!(endpoint("beta").is_ok());
        assert!(endpoint("https://other.invalid").is_err());
    }
    #[cfg(target_os="macos")]
    #[test]
    fn rejects_external_or_unexpected_assets() {
        assert!(allowed_asset(&url::Url::parse("https://github.com/Dreaminmaster/HGhub/releases/download/v1.2.9-macos/HGhub.app.tar.gz").unwrap()));
        for s in ["https://github.com/other/HGhub/releases/download/v1/HGhub.app.tar.gz","http://github.com/Dreaminmaster/HGhub/releases/download/v1/HGhub.app.tar.gz","https://github.com/Dreaminmaster/HGhub/releases/download/v1/HGhub.dmg"] {
            assert!(!allowed_asset(&url::Url::parse(s).unwrap()));
        }
    }
}
