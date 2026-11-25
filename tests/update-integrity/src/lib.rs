// Compile the same code used by the native updater, using inert byte fixtures.
#[path = "../../../src-tauri/src/update_integrity.rs"]
pub mod update_integrity;

#[path = "../../../src-tauri/src/update_archive_core.rs"]
mod update_archive_core;

#[cfg(test)]
mod local_lifecycle {
    use super::{update_integrity::VerifiedStream,update_archive_core::{Receipt,load,store_receipt,verify}};
    use sha2::{Digest,Sha256};
    use std::{fs,io::{self,Read,Write},path::Path};
    fn digest(data:&[u8])->String {format!("{:x}",Sha256::digest(data))}
    // Inert fixture only. Exercise the actual shared verifier and receipt core
    // with real files and the same temporary-directory lifetime as the updater.
    fn stage(root:&Path,mut source:impl Read,expected:&[u8])->Result<Receipt,String>{
        let temp=tempfile::Builder::new().prefix("HGhub-update-").tempdir_in(root).map_err(|e|e.to_string())?;
        let mut file=fs::OpenOptions::new().write(true).create_new(true)
            .open(temp.path().join("HGhub_1.2.9_universal.dmg")).map_err(|e|e.to_string())?;
        let mut verifier=VerifiedStream::new(expected.len() as u64,&digest(expected))?;
        let mut buf=[0;4];
        loop {
            let n=source.read(&mut buf).map_err(|e|e.to_string())?;
            if n==0{break;}
            verifier.write_chunk(&mut file,&buf[..n])?;
        }
        let size=verifier.finish()?;
        file.flush().map_err(|e|e.to_string())?;
        file.sync_all().map_err(|e|e.to_string())?;
        drop(file);
        let id=temp.path().file_name().unwrap().to_str().unwrap().to_owned();
        let receipt=Receipt{id,version:"1.2.9".into(),size,digest:digest(expected),saved_at:"fixture".into()};
        let _saved=temp.keep();
        verify(root,&receipt)?;
        store_receipt(&root.join("history.json"),receipt.clone())?;
        Ok(receipt)
    }
    #[test]
    fn verified_file_survives_scope_and_receipt_reload(){
        let dir=tempfile::tempdir().unwrap();
        let data=b"not an executable; test bytes";
        let r=stage(dir.path(),&data[..],data).unwrap();
        let receipts=load(&dir.path().join("history.json")).unwrap();
        assert_eq!(receipts.len(),1);
        assert_eq!(receipts[0].id,r.id);
        let file=verify(dir.path(),&receipts[0]).unwrap();
        assert_eq!(fs::read(file).unwrap(),data);
    }
    #[test]
    fn truncated_corrupt_and_oversized_streams_leave_no_files_or_receipts(){
        let dir=tempfile::tempdir().unwrap();
        let expected=b"abcdef";
        for data in [&b"abc"[..],&b"abcdeg"[..],&b"abcdefg"[..]] {
            assert!(stage(dir.path(),data,expected).is_err());
            assert_eq!(fs::read_dir(dir.path()).unwrap().count(),0);
        }
        assert!(load(&dir.path().join("history.json")).unwrap().is_empty());
    }
    #[test]
    fn interrupted_stream_cleans_up_and_retry_preserves_prior_good_file(){
        struct InterruptedOnce(bool);
        impl Read for InterruptedOnce {
            fn read(&mut self,buf:&mut [u8])->io::Result<usize>{
                if self.0{return Err(io::Error::new(io::ErrorKind::ConnectionReset,"fixture interruption"));}
                self.0=true;buf[..3].copy_from_slice(b"abc");Ok(3)
            }
        }
        let dir=tempfile::tempdir().unwrap();
        let data=b"abcdef";
        let first=stage(dir.path(),&data[..],data).unwrap();
        assert!(stage(dir.path(),InterruptedOnce(false),data).is_err());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(),2);
        assert_eq!(load(&dir.path().join("history.json")).unwrap().len(),1);
        let second=stage(dir.path(),&data[..],data).unwrap();
        assert_ne!(first.id,second.id);
        assert!(verify(dir.path(),&first).is_ok());
        assert!(verify(dir.path(),&second).is_ok());
        assert_eq!(load(&dir.path().join("history.json")).unwrap().len(),2);
    }
}
