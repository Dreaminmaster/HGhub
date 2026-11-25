//! Local receipt storage and file verification, independent of application runtime.
use serde::{Deserialize,Serialize};
use sha2::{Digest,Sha256};
use std::{fs,io::{Read,Write},path::{Path,PathBuf}};
const LIMIT:usize=20;
const MAX_BYTES:u64=256*1024*1024;
#[derive(Clone,Serialize,Deserialize)]
pub(crate) struct Receipt {
    pub(crate) id:String, pub(crate) version:String, pub(crate) size:u64,
    pub(crate) digest:String, pub(crate) saved_at:String,
}
fn valid(r:&Receipt)->bool {
    r.id.starts_with("HGhub-update-") && r.id.len()>13 && r.id.len()<=80
        && r.id.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'-')
        && {let p:Vec<&str>=r.version.split('.').collect();p.len()==3&&p.iter().all(|s|!s.is_empty()&&s.len()<=20&&s.bytes().all(|b|b.is_ascii_digit())&&s.parse::<u64>().is_ok())}
        && r.size>0 && r.size<=MAX_BYTES && r.digest.len()==64
        && r.digest.bytes().all(|b|b.is_ascii_hexdigit()) && r.saved_at.len()<=64
}
pub(crate) fn load(path:&Path)->Result<Vec<Receipt>,String> {
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
    let mut ids=std::collections::HashSet::new();
    if items.len()>LIMIT||items.iter().any(|r|!valid(r)||!ids.insert(r.id.as_str())){return Err("下载记录校验失败".into());}
    Ok(items)
}
pub(crate) fn local_file(root:&Path,r:&Receipt)->Result<PathBuf,String> {
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
pub(crate) fn verify(root:&Path,r:&Receipt)->Result<PathBuf,String> {
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
pub(crate) fn store_receipt(file:&Path,r:Receipt)->Result<(),String> {
    if !valid(&r){return Err("下载记录无效".into());}
    let mut items=load(file)?;
    items.retain(|x|x.id!=r.id);
    items.insert(0,r);
    items.truncate(LIMIT);
    let parent=file.parent().ok_or("记录目录无效")?;
    let mut temp=tempfile::NamedTempFile::new_in(parent).map_err(|_|"无法创建记录文件")?;
    let bytes=serde_json::to_vec(&items).map_err(|_|"下载记录序列化失败")?;
    temp.write_all(&bytes).map_err(|_|"下载记录写入失败")?;
    temp.as_file().sync_all().map_err(|_|"下载记录保存失败")?;
    temp.persist(file).map_err(|_|"下载记录替换失败")?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(root:&Path)->Receipt {
        let r=Receipt{id:"HGhub-update-test123".into(),version:"1.2.9".into(),size:3,digest:format!("{:x}",Sha256::digest(b"abc")),saved_at:"2026-11-25T00:00:00Z".into()};
        let d=root.join(&r.id);fs::create_dir(&d).unwrap();fs::write(d.join("HGhub_1.2.9_universal.dmg"),b"abc").unwrap();r
    }
    #[test]
    fn receipt_roundtrip_replaces_duplicate_and_bounds_history() {
        let d=tempfile::tempdir().unwrap();
        let r=fixture(d.path());
        let file=d.path().join("history.json");
        assert!(load(&file).unwrap().is_empty());
        store_receipt(&file,r.clone()).unwrap();
        let mut changed=r.clone();
        changed.saved_at="later".into();
        store_receipt(&file,changed).unwrap();
        assert_eq!(load(&file).unwrap().len(),1);
        assert_eq!(load(&file).unwrap()[0].saved_at,"later");
        for i in 0..25 {
            let mut next=r.clone();next.id=format!("HGhub-update-{i}");
            store_receipt(&file,next).unwrap();
        }
        let rows=load(&file).unwrap();
        assert_eq!(rows.len(),20);
        assert_eq!(rows[0].id,"HGhub-update-24");
        assert!(verify(d.path(),&r).is_ok(),"evicted receipt never deletes the installer");
    }
    #[test]
    fn rejects_corrupt_and_duplicate_history_without_overwriting() {
        let d=tempfile::tempdir().unwrap();
        let r=fixture(d.path());let file=d.path().join("history.json");
        fs::write(&file,b"not json").unwrap();
        assert!(store_receipt(&file,r.clone()).is_err());
        assert_eq!(fs::read(&file).unwrap(),b"not json");
        fs::write(&file,serde_json::to_vec(&vec![r.clone(),r.clone()]).unwrap()).unwrap();
        assert!(load(&file).is_err());
        fs::write(&file,vec![b' ';65537]).unwrap();
        assert!(load(&file).is_err());
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
