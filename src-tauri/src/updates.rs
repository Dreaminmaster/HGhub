//! User-confirmed DMG download. No self-install, arbitrary URL, or signing secret.
use serde_json::{json, Value};
const REPO: &str = "Dreaminmaster/HGhub";
const MAX_BYTES: u64 = 256 * 1024 * 1024;
#[cfg(target_os="macos")]
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
#[cfg(target_os="macos")]
static BUSY: AtomicBool = AtomicBool::new(false);
#[cfg(target_os="macos")]
static RECEIVED: AtomicU64 = AtomicU64::new(0);
#[cfg(target_os="macos")]
static TOTAL: AtomicU64 = AtomicU64::new(0);
#[cfg(target_os="macos")]
static VERIFYING: AtomicBool = AtomicBool::new(false);

#[cfg(target_os="macos")]
fn api_host(u:&url::Url)->bool {
    u.scheme()=="https" && u.host_str()==Some("api.github.com") && u.port().is_none()
        && u.username().is_empty() && u.password().is_none() && u.fragment().is_none()
}

#[cfg(target_os="macos")]
async fn github_json(path:&str)->Result<Value,String> {
    let uri=format!("https://api.github.com/{path}");
    tokio::time::timeout(std::time::Duration::from_secs(20),async {
        let mut res=crate::net::get_scoped(&uri,std::time::Duration::from_secs(15),
            Some("application/vnd.github+json"),None,Some(api_host)).await?;
        if !res.status().is_success() {return Err(format!("GitHub HTTP {}",res.status()));}
        if !api_host(res.url()) || res.url().path()!=format!("/{}",path.split('?').next().unwrap_or("")) {
            return Err("拒绝非指定版本信息".into());
        }
        let mut body=Vec::new();
        while let Some(chunk)=res.chunk().await.map_err(|_|"读取版本信息失败")? {
            if body.len().saturating_add(chunk.len())>2*1024*1024 {return Err("版本信息超过大小限制".into());}
            body.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&body).map_err(|_|"版本信息格式无效".into())
    }).await.map_err(|_|"版本查询超时".to_string())?
}

#[tauri::command]
pub fn manual_update_progress()->Value {
    #[cfg(target_os="macos")]
    {json!({"busy":BUSY.load(Ordering::Acquire),"received":RECEIVED.load(Ordering::Relaxed),
        "total":TOTAL.load(Ordering::Relaxed),"verifying":VERIFYING.load(Ordering::Relaxed)})}
    #[cfg(not(target_os="macos"))]
    {json!({"busy":false,"received":0,"total":0,"verifying":false})}
}

/// Metadata only. No code import, endpoint discovery or automatic update.
#[tauri::command]
pub async fn maintenance_upstreams()->Result<Value,String> {
    #[cfg(target_os="macos")]
    {
        async fn check(repo:&str,baseline:&str)->Value {
            let path=format!("repos/{repo}/releases/latest");
            let mut result=json!({"repo":repo,"baseline":baseline,"status":"unavailable"});
            if let Ok(r)=github_json(&path).await {
                if r["draft"]==false && r["prerelease"]==false {
                    if let Some(tag)=r["tag_name"].as_str().and_then(|s|s.strip_prefix('v')) {
                        if let (Some(latest),Some(current))=(version(tag),version(baseline)) {
                            result["latest"]=json!(tag);
                            result["status"]=json!(if latest>current {"newer"} else {"not_newer"});
                        }
                    }
                }
            }
            result
        }
        let (a,b)=tokio::join!(check("zxsos/HGhubX","2.2.10"),check("Diniboy1123/usque","4.2.1"));
        Ok(json!({"checked_at_utc":crate::logging::utc_timestamp().ok(),"items":[a,b]}))
    }
    #[cfg(not(target_os="macos"))]
    {Err("当前平台暂不支持上游版本查询".into())}
}
fn version(s: &str) -> Option<[u64; 3]> {
    let p: Vec<&str> = s.split('.').collect();
    if p.len()!=3 || p.iter().any(|x| x.is_empty() || !x.bytes().all(|b|b.is_ascii_digit()) || (x.len()>1 && x.starts_with('0'))) { return None; }
    Some([p[0].parse().ok()?,p[1].parse().ok()?,p[2].parse().ok()?])
}
fn channel_ok(s: &str) -> Result<(), String> {
    if matches!(s,"stable"|"beta") { Ok(()) } else { Err("无效更新渠道".into()) }
}
#[derive(Debug)]
struct Candidate { version: String, order: [u64;3], notes: String, url:String, digest:String, size:u64, name:String }
fn pick(releases:&[Value], channel:&str) -> Option<Candidate> {
    releases.iter().filter_map(|r| {
        if r["draft"] != false || !r["prerelease"].is_boolean() || (channel=="stable" && r["prerelease"]!=false) {return None;}
        let tag=r["tag_name"].as_str()?;
        let ver=tag.strip_prefix('v')?.strip_suffix("-macos")?;
        let order=version(ver)?;
        let name=format!("HGhub_{ver}_universal.dmg");
        let url=format!("https://github.com/{REPO}/releases/download/{tag}/{name}");
        let asset=r["assets"].as_array()?.iter().find(|a|a["name"].as_str()==Some(name.as_str()) && a["state"]=="uploaded" && a["browser_download_url"].as_str()==Some(url.as_str()))?;
        let size=asset["size"].as_u64()?;
        if size==0 || size>MAX_BYTES {return None;}
        let digest=asset["digest"].as_str()?.strip_prefix("sha256:")?;
        if digest.len()!=64 || !digest.bytes().all(|b|b.is_ascii_hexdigit()) {return None;}
        Some(Candidate{version:ver.to_owned(),order,notes:r["body"].as_str().unwrap_or("").chars().take(2000).collect(),url,digest:digest.to_ascii_lowercase(),size,name})
    }).max_by_key(|c|c.order)
}
#[cfg(target_os="macos")]
async fn latest(channel:&str)->Result<Option<Candidate>,String> {
    channel_ok(channel)?;
    let list:Vec<Value>=serde_json::from_value(github_json(&format!("repos/{REPO}/releases?per_page=50")).await?)
        .map_err(|_|"版本列表格式无效")?;
    Ok(pick(&list,channel))
}
#[tauri::command]
pub async fn manual_update_check(channel:String)->Result<Value,String> {
    channel_ok(&channel)?;
    #[cfg(target_os="macos")]
    {
        let current=version(env!("CARGO_PKG_VERSION")).ok_or("当前版本格式无效")?;
        let Some(c)=latest(&channel).await? else {return Ok(json!({"status":"no_release"}));};
        Ok(json!({"status":if c.order>current {"available"} else {"current"},"version":c.version,"notes":c.notes,"size":c.size,"digest":c.digest}))
    }
    #[cfg(not(target_os="macos"))]
    {Ok(json!({"status":"unsupported"}))}
}
#[cfg(target_os="macos")]
fn download_host(u:&url::Url)->bool {
    u.scheme()=="https" && u.port().is_none() && u.username().is_empty() && u.password().is_none()
      && matches!(u.host_str(),Some("github.com"|"release-assets.githubusercontent.com"|"objects.githubusercontent.com"))
}
#[tauri::command]
pub async fn manual_update_download(app:tauri::AppHandle,channel:String,version:String,digest:String)->Result<Value,String> {
    channel_ok(&channel)?;
    #[cfg(target_os="macos")]
    {
        use crate::update_integrity::VerifiedStream;
        use std::time::{Duration,SystemTime,UNIX_EPOCH};
        use tauri::Manager;
        use tauri_plugin_opener::OpenerExt;
        if BUSY.swap(true,Ordering::AcqRel){return Err("已有下载正在进行".into());}
        RECEIVED.store(0,Ordering::Relaxed);
        TOTAL.store(0,Ordering::Relaxed);
        VERIFYING.store(false,Ordering::Relaxed);
        struct Guard;
        impl Drop for Guard {fn drop(&mut self){BUSY.store(false,Ordering::Release);}}
        let _guard=Guard;
        let c=latest(&channel).await?.ok_or("没有可用的完整安装包")?;
        let current=self::version(env!("CARGO_PKG_VERSION")).ok_or("当前版本格式无效")?;
        if c.version!=version || c.digest!=digest || c.order<=current {return Err("版本或文件摘要已变化，请重新检查更新".into());}
        TOTAL.store(c.size,Ordering::Relaxed);
        let dir=app.path().download_dir().map_err(|_|"无法找到下载目录")?;
        let temp=tempfile::Builder::new().prefix("HGhub-update-").tempdir_in(&dir).map_err(|_|"无法创建下载目录")?;
        let path=temp.path().join(&c.name);
        let mut out=std::fs::OpenOptions::new().write(true).create_new(true).open(&path).map_err(|_|"无法创建安装包")?;
        let mut res=crate::net::get_scoped(&c.url,Duration::from_secs(180),Some("application/octet-stream"),None,Some(download_host)).await?;
        if !res.status().is_success() || !download_host(res.url()) {return Err("拒绝非 GitHub 发布文件".into());}
        let mut verifier=VerifiedStream::new(c.size,&c.digest)?;
        tokio::time::timeout(Duration::from_secs(180),async {
            while let Some(chunk)=res.chunk().await.map_err(|_|"安装包传输中断")? {
                let received=verifier.write_chunk(&mut out,&chunk)?;
                RECEIVED.store(received,Ordering::Relaxed);
            }
            Ok::<(),&str>(())
        }).await.map_err(|_|"下载超时")??;
        VERIFYING.store(true,Ordering::Relaxed);
        let total=verifier.finish()?;
        out.sync_all().map_err(|_|"保存安装包失败")?;
        drop(out);
        // Mark as downloaded, not trusted. Never remove quarantine or change Gatekeeper.
        let now=SystemTime::now().duration_since(UNIX_EPOCH).map_err(|_|"系统时间无效")?.as_secs();
        let quarantine=format!("0081;{now:x};HGhub;");
        let mark=std::process::Command::new("/usr/bin/xattr").args(["-w","com.apple.quarantine",&quarantine]).arg(&path).status().map_err(|_|"无法标记下载文件")?;
        if !mark.success(){return Err("无法设置下载安全标记".into());}
        let _saved=temp.keep();
        let archived=crate::update_archive::record(&app,&path,&c.version,total,&c.digest).is_ok();
        let revealed=app.opener().reveal_item_in_dir(&path).is_ok();
        Ok(json!({"path":path.to_string_lossy(),"size":total,"sha256":c.digest,"revealed":revealed,"archived":archived}))
    }
    #[cfg(not(target_os="macos"))]
    {let _=(app,version,digest);Err("当前平台不支持此下载方式".into())}
}
/// Local download receipts only; no remote historical download or automatic downgrade.
#[tauri::command]
pub async fn manual_update_history(app:tauri::AppHandle)->Result<Value,String> {
    #[cfg(target_os="macos")]
    {tauri::async_runtime::spawn_blocking(move||crate::update_archive::history(&app)).await.map_err(|_|"读取下载记录失败".to_string())?}
    #[cfg(not(target_os="macos"))]
    {let _=app;Ok(json!([]))}
}
#[tauri::command]
pub async fn manual_update_reveal(app:tauri::AppHandle,id:String)->Result<(),String> {
    #[cfg(target_os="macos")]
    {tauri::async_runtime::spawn_blocking(move||crate::update_archive::reveal(&app,&id)).await.map_err(|_|"检查本地安装包失败".to_string())?}
    #[cfg(not(target_os="macos"))]
    {let _=(app,id);Err("当前平台暂不支持".into())}
}

#[cfg(test)]
mod tests {
    use super::*;
    fn release(v:&str,beta:bool)->Value {
        let name=format!("HGhub_{v}_universal.dmg");
        json!({"tag_name":format!("v{v}-macos"),"draft":false,"prerelease":beta,"assets":[{"name":name,"state":"uploaded","size":100,"digest":format!("sha256:{}","a".repeat(64)),"browser_download_url":format!("https://github.com/{REPO}/releases/download/v{v}-macos/{name}")}]})
    }
    #[cfg(target_os="macos")]
    #[test]
    fn update_destination_allowlists_reject_foreign_redirects() {
        for url in ["https://evil.invalid/file.dmg","http://github.com/file.dmg","https://github.com.evil.invalid/file.dmg","https://user@github.com/file.dmg"] {
            assert!(!download_host(&url::Url::parse(url).unwrap()));
        }
        assert!(download_host(&url::Url::parse("https://release-assets.githubusercontent.com/example").unwrap()));
        assert!(api_host(&url::Url::parse("https://api.github.com/repos/example/project/releases").unwrap()));
        assert!(!api_host(&url::Url::parse("https://github.com/example/project").unwrap()));
    }
    #[test]
    fn stable_channel_rejects_preview_and_other_platform() {
        let mut ios=release("9.0.0",false);ios["tag_name"]=json!("v9.0.0-ios");
        let list=vec![release("1.2.10",false),release("2.0.0",true),ios];
        assert_eq!(pick(&list,"stable").unwrap().version,"1.2.10");
        assert_eq!(pick(&list,"beta").unwrap().version,"2.0.0");
        assert!(channel_ok("other").is_err());
    }
    #[test]
    fn reject_missing_digest_wrong_url_and_size() {
        let mut r=release("2.0.0",false);r["assets"][0]["digest"]=Value::Null;assert!(pick(&[r],"stable").is_none());
        let mut r=release("2.0.0",false);r["assets"][0]["browser_download_url"]=json!("https://other.invalid/file.dmg");assert!(pick(&[r],"stable").is_none());
        let mut r=release("2.0.0",false);r["assets"][0]["size"]=json!(MAX_BYTES+1);assert!(pick(&[r],"stable").is_none());
        assert!(version("1.02.9").is_none());
    }
}
