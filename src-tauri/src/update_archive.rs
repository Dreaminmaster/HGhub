//! Tauri wrapper for local receipts. Never runs or installs an app.
use serde_json::{json,Value};
use std::{fs,path::{Path,PathBuf},sync::Mutex};
use tauri::Manager;
use tauri_plugin_opener::OpenerExt;
use crate::update_archive_core::{Receipt,load,local_file,verify,store_receipt};
static LOCK:Mutex<()>=Mutex::new(());
fn receipt_path(app:&tauri::AppHandle)->Result<PathBuf,String> {
    let dir=app.path().app_data_dir().map_err(|_|"无法找到应用数据目录")?;
    fs::create_dir_all(&dir).map_err(|_|"无法创建应用数据目录")?;
    Ok(dir.join("verified-update-history-v1.json"))
}
pub fn record(app:&tauri::AppHandle,path:&Path,version:&str,size:u64,digest:&str)->Result<(),String> {
    let id=path.parent().and_then(|p|p.file_name()).and_then(|s|s.to_str()).ok_or("安装包目录无效")?;
    let r=Receipt{id:id.to_owned(),version:version.to_owned(),size,digest:digest.to_owned(),saved_at:crate::logging::utc_timestamp().unwrap_or_default()};
    let root=app.path().download_dir().map_err(|_|"无法找到下载目录")?;
    if local_file(&root,&r)?!=path.canonicalize().map_err(|_|"安装包路径无效")? {return Err("安装包不在指定下载目录".into());}
    let _lock=LOCK.lock().map_err(|_|"下载记录暂不可用")?;
    let file=receipt_path(app)?;
    store_receipt(&file,r)
}
pub fn history(app:&tauri::AppHandle)->Result<Value,String> {
    let _lock=LOCK.lock().map_err(|_|"下载记录暂不可用")?;
    let items=load(&receipt_path(app)?)?;
    let root=app.path().download_dir().map_err(|_|"无法找到下载目录")?;
    Ok(json!(items.iter().map(|r|json!({"id":r.id,"version":r.version,"size":r.size,"digest":r.digest,"saved_at":r.saved_at,"present":local_file(&root,r).is_ok()})).collect::<Vec<_>>()))
}
pub fn reveal(app:&tauri::AppHandle,id:&str)->Result<(),String> {
    let r={
        let _lock=LOCK.lock().map_err(|_|"下载记录暂不可用")?;
        load(&receipt_path(app)?)?.into_iter().find(|r|r.id==id).ok_or("未找到此下载记录")?
    };
    let root=app.path().download_dir().map_err(|_|"无法找到下载目录")?;
    let path=verify(&root,&r)?;
    app.opener().reveal_item_in_dir(&path).map_err(|_|"无法在 Finder 中定位安装包".into())
}
