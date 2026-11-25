//! User-confirmed DMG download. No self-install, arbitrary URL, or signing secret.
use serde_json::{json, Value};
const REPO: &str = "Dreaminmaster/HGhub";
const MAX_BYTES: u64 = 256 * 1024 * 1024;
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
    let uri=format!("https://api.github.com/repos/{REPO}/releases?per_page=50");
    let res=crate::net::get(&uri,std::time::Duration::from_secs(15),Some("application/vnd.github+json"),None).await?;
    if !res.status().is_success() || res.url().scheme()!="https" || res.url().host_str()!=Some("api.github.com") || res.url().path()!=format!("/repos/{REPO}/releases") {return Err("仓库版本查询失败".into());}
    let list:Vec<Value>=res.json().await.map_err(|_|"版本列表格式无效")?;
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
        use sha2::{Digest,Sha256};
        use std::{io::Write,sync::atomic::{AtomicBool,Ordering},time::{Duration,SystemTime,UNIX_EPOCH}};
        use tauri::Manager;
        use tauri_plugin_opener::OpenerExt;
        static BUSY:AtomicBool=AtomicBool::new(false);
        if BUSY.swap(true,Ordering::AcqRel){return Err("已有下载正在进行".into());}
        struct Guard;
        impl Drop for Guard {fn drop(&mut self){BUSY.store(false,Ordering::Release);}}
        let _guard=Guard;
        let c=latest(&channel).await?.ok_or("没有可用的完整安装包")?;
        let current=self::version(env!("CARGO_PKG_VERSION")).ok_or("当前版本格式无效")?;
        if c.version!=version || c.digest!=digest || c.order<=current {return Err("版本或文件摘要已变化，请重新检查更新".into());}
        let dir=app.path().download_dir().map_err(|_|"无法找到下载目录")?;
        let temp=tempfile::Builder::new().prefix("HGhub-update-").tempdir_in(&dir).map_err(|_|"无法创建下载目录")?;
        let path=temp.path().join(&c.name);
        let mut out=std::fs::OpenOptions::new().write(true).create_new(true).open(&path).map_err(|_|"无法创建安装包")?;
        let mut res=crate::net::get(&c.url,Duration::from_secs(180),Some("application/octet-stream"),None).await?;
        if !res.status().is_success() || !download_host(res.url()) {return Err("拒绝非 GitHub 发布文件".into());}
        let mut hasher=Sha256::new();
        let mut total=0u64;
        tokio::time::timeout(Duration::from_secs(180),async {
            while let Some(chunk)=res.chunk().await.map_err(|_|"安装包传输中断")? {
                total=total.checked_add(chunk.len() as u64).ok_or("文件过大")?;
                if total>c.size || total>MAX_BYTES {return Err("文件大小与发布信息不一致");}
                hasher.update(&chunk);
                out.write_all(&chunk).map_err(|_|"写入安装包失败")?;
            }
            Ok::<(),&str>(())
        }).await.map_err(|_|"下载超时")??;
        if total!=c.size || format!("{:x}",hasher.finalize())!=c.digest {return Err("安装包 SHA-256 或大小校验失败，已删除临时文件".into());}
        out.sync_all().map_err(|_|"保存安装包失败")?;
        drop(out);
        // Mark as downloaded, not trusted. Never remove quarantine or change Gatekeeper.
        let now=SystemTime::now().duration_since(UNIX_EPOCH).map_err(|_|"系统时间无效")?.as_secs();
        let quarantine=format!("0081;{now:x};HGhub;");
        let mark=std::process::Command::new("/usr/bin/xattr").args(["-w","com.apple.quarantine",&quarantine]).arg(&path).status().map_err(|_|"无法标记下载文件")?;
        if !mark.success(){return Err("无法设置下载安全标记".into());}
        let _saved=temp.keep();
        let revealed=app.opener().reveal_item_in_dir(&path).is_ok();
        Ok(json!({"path":path.to_string_lossy(),"size":total,"sha256":c.digest,"revealed":revealed}))
    }
    #[cfg(not(target_os="macos"))]
    {let _=(app,version,digest);Err("当前平台不支持此下载方式".into())}
}
#[cfg(test)]
mod tests {
    use super::*;
    fn release(v:&str,beta:bool)->Value {
        let name=format!("HGhub_{v}_universal.dmg");
        json!({"tag_name":format!("v{v}-macos"),"draft":false,"prerelease":beta,"assets":[{"name":name,"state":"uploaded","size":100,"digest":format!("sha256:{}","a".repeat(64)),"browser_download_url":format!("https://github.com/{REPO}/releases/download/v{v}-macos/{name}")}]})
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
