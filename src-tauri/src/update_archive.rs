//! Local receipts for verified downloads. This module never runs or installs an app.
use serde::{Deserialize,Serialize};
use serde_json::{json,Value};
use sha2::{Digest,Sha256};
use std::{fs,io::{Read,Write},path::{Path,PathBuf},sync::Mutex};
use tauri::Manager;
use tauri_plugin_opener::OpenerExt;
const LIMIT:usize=20;
const MAX_BYTES:u64=256*1024*1024;
static LOCK:Mutex<()>=Mutex::new(());

#[derive(Clone,Serialize,Deserialize)]
struct Receipt {
    id:String, version:String, size:u64, digest:String, saved_at:String,
}
fn valid(r:&Receipt)->bool {
    r.id.starts_with("HGhub-update-") && r.id.len()>13 && r.id.len()<=80
        && r.id.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'-')
        && {let p:Vec<&str>=r.version.split('.').collect();p.len()==3&&p.iter().all(|s|!s.is_empty()&&s.len()<=20&&s.bytes().all(|b|b.is_ascii_digit())&&s.parse::<u64>().is_ok())}
        && r.size>0 && r.size<=MAX_BYTES && r.digest.len()==64
        && r.digest.bytes().all(|b|b.is_ascii_hexdigit()) && r.saved_at.len()<=64
}
fn receipt_path(app:&tauri::AppHandle)->Result<PathBuf,String> {
    let dir=app.path().app_data_dir().map_err(|_|"无法找到应用数据目录")?;
    fs::create_dir_all(&dir).map_err(|_|"无法创建应用数据目录")?;
    Ok(dir.join("verified-update-history-v1.json"))
}
fn load(path:&Path)->Result<Vec<Receipt>,String> {
    match fs::symlink_metadata(path) {
        Err(e) if e.kind()==std::io::ErrorKind::NotFound=>return Ok(vec![]),
        Ok(m) if m.file_type().is_file() && m.len()<=65536=>{},
        _=>return Err("下载记录格式或大小异常".into()),
    }
    let f=fs::File::open(path).map_err(|_|"无法读取下载记录")?;
    let mut bytes=Vec::new();
    f.take(65537).read_to_end(&mut bytes).map_err(|_|"无法读取下载记录")?;
    if bytes.len()>65536{return Err("下载记录过大".into());}
    let items:Vec<Receipt>=serde_json::from_slice(&bytes).map_err(|_|"下载记录无法解析")?;
    if items.len()>LIMIT||items.iter().any(|r|!valid(r)){return Err("下载记录校验失败".into());}
    Ok(items)
}
fn local_file(root:&Path,r:&Receipt)->Result<PathBuf,String> {
    if !valid(r){return Err("下载记录无效".into());}
    let root=root.canonicalize().map_err(|_|"下载目录不可用")?;
    let dir=root.join(&r.id);
    let dm=fs::symlink_metadata(&dir).map_err(|_|"本地安装包目录已移动或删除")?;
    if !dm.file_type().is_dir() || dir.canonicalize().ok().as_ref()!=Some(&dir) {return Err("安装包目录不符合安全要求".into());}
    let file=dir.join(format!("HGhub_{}_universal.dmg",r.version));
    let fm=fs::symlink_metadata(&file).map_err(|_|"本地安装包已移动或删除")?;
    if !fm.file_type().is_file() || fm.len()!=r.size || file.canonicalize().ok().as_ref()!=Some(&file) {return Err("本地安装包路径或大小已变化".into());}
    Ok(file)
}
fn verify(root:&Path,r:&Receipt)->Result<PathBuf,String> {
    let file=local_file(root,r)?;
    let f=fs::File::open(&file).map_err(|_|"无法读取本地安装包")?;
    let mut reader=f.take(r.size+1);
    let mut sha=Sha256::new();
    let mut buf=[0u8;65536];
    let mut count=0u64;
    loop{let n=reader.read(&mut buf).map_err(|_|"读取安装包失败")?;if n==0{break;}count+=n as u64;sha.update(&buf[..n]);}
    if count!=r.size||format!("{:x}",sha.finalize())!=r.digest.to_ascii_lowercase(){return Err("本地安装包摘要不一致，拒绝定位".into());}
    // Revalidate the path before showing the file. Never open or execute the DMG.
    local_file(root,r)
}
pub fn record(app:&tauri::AppHandle,path:&Path,version:&str,size:u64,digest:&str)->Result<(),String> {
    let id=path.parent().and_then(|p|p.file_name()).and_then(|s|s.to_str()).ok_or("安装包目录无效")?;
    let r=Receipt{id:id.to_owned(),version:version.to_owned(),size,digest:digest.to_owned(),saved_at:crate::logging::utc_timestamp().unwrap_or_default()};
    let root=app.path().download_dir().map_err(|_|"无法找到下载目录")?;
    if local_file(&root,&r)?!=path.canonicalize().map_err(|_|"安装包路径无效")? {return Err("安装包不在指定下载目录".into());}
    let _lock=LOCK.lock().map_err(|_|"下载记录暂不可用")?;
    let file=receipt_path(app)?;
    let mut items=load(&file)?;
    items.retain(|x|x.id!=r.id);
    items.insert(0,r);
    items.truncate(LIMIT);
    let parent=file.parent().ok_or("记录目录无效")?;
    let mut temp=tempfile::NamedTempFile::new_in(parent).map_err(|_|"无法创建记录文件")?;
    let bytes=serde_json::to_vec(&items).map_err(|_|"下载记录序列化失败")?;
    temp.write_all(&bytes).map_err(|_|"下载记录写入失败")?;
    temp.as_file().sync_all().map_err(|_|"下载记录保存失败")?;
    temp.persist(&file).map_err(|_|"下载记录替换失败")?;
    Ok(())
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
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(root:&Path)->Receipt {
        let r=Receipt{id:"HGhub-update-test123".into(),version:"1.2.9".into(),size:3,digest:format!("{:x}",Sha256::digest(b"abc")),saved_at:"2026-11-25T00:00:00Z".into()};
        let d=root.join(&r.id);fs::create_dir(&d).unwrap();fs::write(d.join("HGhub_1.2.9_universal.dmg"),b"abc").unwrap();r
    }
    #[test]
    fn verifies_local_bytes_and_detects_changes() {
        let d=tempfile::tempdir().unwrap();let r=fixture(d.path());
        let f=verify(d.path(),&r).unwrap();fs::write(&f,b"abd").unwrap();
        assert!(verify(d.path(),&r).is_err());
        fs::remove_file(&f).unwrap();assert!(verify(d.path(),&r).is_err());
    }
    #[test]
    fn rejects_traversal_and_symlink() {
        let d=tempfile::tempdir().unwrap();let mut r=fixture(d.path());
        let f=local_file(d.path(),&r).unwrap();fs::remove_file(&f).unwrap();
        let other=d.path().join("other.dmg");fs::write(&other,b"abc").unwrap();
        std::os::unix::fs::symlink(&other,&f).unwrap();assert!(verify(d.path(),&r).is_err());
        r.id="../outside".into();assert!(local_file(d.path(),&r).is_err());
    }
}
