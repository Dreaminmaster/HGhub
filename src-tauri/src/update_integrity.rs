//! Bounded, fail-closed byte stream verification. No network or installation logic.
use sha2::{Digest,Sha256};
use std::io::Write;

const MAX_BYTES:u64=256*1024*1024;
pub struct VerifiedStream {
    expected:u64,
    digest:String,
    received:u64,
    sha:Sha256,
    failed:bool,
}
impl VerifiedStream {
    pub fn new(expected:u64,digest:&str)->Result<Self,&'static str> {
        if expected==0||expected>MAX_BYTES||digest.len()!=64||!digest.bytes().all(|b|b.is_ascii_hexdigit()){
            return Err("安装包大小或摘要无效");
        }
        Ok(Self{expected,digest:digest.to_ascii_lowercase(),received:0,sha:Sha256::new(),failed:false})
    }
    pub fn write_chunk<W:Write>(&mut self,out:&mut W,bytes:&[u8])->Result<u64,&'static str>{
        if self.failed{return Err("下载校验已失败");}
        let next=match self.received.checked_add(bytes.len() as u64){
            Some(n) if n<=self.expected&&n<=MAX_BYTES=>n,
            _=>{self.failed=true;return Err("文件大小与发布信息不一致");}
        };
        if out.write_all(bytes).is_err(){self.failed=true;return Err("写入安装包失败");}
        self.sha.update(bytes);
        self.received=next;
        Ok(next)
    }
    pub fn finish(self)->Result<u64,&'static str>{
        if self.failed||self.received!=self.expected||format!("{:x}",self.sha.finalize())!=self.digest{
            return Err("安装包 SHA-256 或大小校验失败，已删除临时文件");
        }
        Ok(self.received)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn digest(data:&[u8])->String {format!("{:x}",Sha256::digest(data))}
    #[test]
    fn accepts_chunked_identical_bytes(){
        let data=b"harmless fixture bytes";
        let mut stream=VerifiedStream::new(data.len() as u64,&digest(data)).unwrap();
        let mut out=Vec::new();
        assert_eq!(stream.write_chunk(&mut out,&data[..4]).unwrap(),4);
        stream.write_chunk(&mut out,&data[4..]).unwrap();
        assert_eq!(stream.finish().unwrap(),data.len() as u64);
        assert_eq!(out,data);
    }
    #[test]
    fn rejects_corruption_and_truncation(){
        let mut changed=VerifiedStream::new(3,&digest(b"abc")).unwrap();
        changed.write_chunk(&mut Vec::new(),b"abd").unwrap();
        assert!(changed.finish().is_err());
        let mut short=VerifiedStream::new(3,&digest(b"abc")).unwrap();
        short.write_chunk(&mut Vec::new(),b"ab").unwrap();
        assert!(short.finish().is_err());
    }
    #[test]
    fn rejects_oversized_chunk_without_writing_and_poisoned_stream(){
        let mut stream=VerifiedStream::new(3,&digest(b"abc")).unwrap();
        let mut out=Vec::new();
        assert!(stream.write_chunk(&mut out,b"abcd").is_err());
        assert!(out.is_empty());
        assert!(stream.write_chunk(&mut out,b"abc").is_err());
        assert!(stream.finish().is_err());
    }
    #[test]
    fn rejects_malformed_metadata(){
        assert!(VerifiedStream::new(0,&digest(b"")).is_err());
        assert!(VerifiedStream::new(MAX_BYTES+1,&digest(b"a")).is_err());
        assert!(VerifiedStream::new(1,"invalid").is_err());
        assert!(VerifiedStream::new(1,&"z".repeat(64)).is_err());
    }
    #[test]
    fn rejects_writer_failure_even_after_partial_write(){
        struct Broken {wrote:bool}
        impl Write for Broken {
            fn write(&mut self,bytes:&[u8])->std::io::Result<usize>{
                if self.wrote{return Err(std::io::Error::other("fixture disk failure"));}
                self.wrote=true;Ok(bytes.len().min(1))
            }
            fn flush(&mut self)->std::io::Result<()>{Ok(())}
        }
        let mut stream=VerifiedStream::new(3,&digest(b"abc")).unwrap();
        assert!(stream.write_chunk(&mut Broken{wrote:false},b"abc").is_err());
        assert!(stream.finish().is_err());
    }
}
