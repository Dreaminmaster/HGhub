//! Loopback HTTP integration tests only. No external downloads or executable packages.
use super::update_integrity::VerifiedStream;
use sha2::{Digest,Sha256};
use std::{io::{Read,Write},net::{TcpListener,Shutdown},thread,time::{Duration,Instant}};
fn server(response:Vec<u8>,delay:Duration)->(String,thread::JoinHandle<()>) {
    let listener=TcpListener::bind("127.0.0.1:0").unwrap();
    let addr=listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let task=thread::spawn(move||{
        let deadline=Instant::now()+Duration::from_secs(4);
        let mut stream=loop {
            match listener.accept() {
                Ok((stream,_))=>break stream,
                Err(e) if e.kind()==std::io::ErrorKind::WouldBlock && Instant::now()<deadline=>thread::sleep(Duration::from_millis(5)),
                _=>return,
            }
        };
        stream.set_nonblocking(false).unwrap();
        stream.set_write_timeout(Some(Duration::from_secs(2))).unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let mut request=Vec::new();
        let mut byte=[0;1];
        while request.len()<8192 {
            match stream.read(&mut byte) {
                Ok(1)=>{request.push(byte[0]);if request.ends_with(b"\r\n\r\n"){break;}},
                _=>return,
            }
        }
        thread::sleep(delay);
        let _=stream.write_all(&response);
        let _=stream.shutdown(Shutdown::Both);
    });
    (format!("http://{addr}/fixture"),task)
}
fn retrieve(url:&str,expected:&[u8],timeout:Duration)->Result<Vec<u8>,String> {
    let client=reqwest::blocking::Client::builder().no_proxy()
        .redirect(reqwest::redirect::Policy::none()).timeout(timeout)
        .build().map_err(|e|e.to_string())?;
    let mut response=client.get(url).send().map_err(|e|e.to_string())?;
    if !response.status().is_success(){return Err(format!("HTTP {}",response.status().as_u16()));}
    let digest=format!("{:x}",Sha256::digest(expected));
    let mut verifier=VerifiedStream::new(expected.len() as u64,&digest)?;
    let mut output=Vec::new();
    let mut buf=[0;4];
    loop {
        let n=response.read(&mut buf).map_err(|e|e.to_string())?;
        if n==0{break;}
        verifier.write_chunk(&mut output,&buf[..n])?;
    }
    verifier.finish()?;
    Ok(output)
}
fn run(response:&[u8],expected:&[u8])->Result<Vec<u8>,String>{
    let (url,task)=server(response.to_vec(),Duration::ZERO);
    let result=retrieve(&url,expected,Duration::from_secs(3));
    task.join().unwrap();result
}
#[test]
fn exact_http_bytes_pass(){
    let result=run(b"HTTP/1.1 200 OK\r\nContent-Length: 6\r\nConnection: close\r\n\r\nabc123",b"abc123").unwrap();
    assert_eq!(result,b"abc123");
}
#[test]
fn incomplete_and_corrupt_http_bytes_fail(){
    assert!(run(b"HTTP/1.1 200 OK\r\nContent-Length: 6\r\nConnection: close\r\n\r\nabc",b"abc123").is_err());
    assert!(run(b"HTTP/1.1 200 OK\r\nContent-Length: 6\r\nConnection: close\r\n\r\nabc124",b"abc123").is_err());
}
#[test]
fn server_failure_and_redirect_are_rejected(){
    let result=run(b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",b"abc123");
    assert_eq!(result.unwrap_err(),"HTTP 503");
    let result=run(b"HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:1/never\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",b"abc123");
    assert_eq!(result.unwrap_err(),"HTTP 302");
}
#[test]
fn delayed_server_times_out(){
    let (url,task)=server(b"HTTP/1.1 200 OK\r\nContent-Length: 6\r\nConnection: close\r\n\r\nabc123".to_vec(),Duration::from_millis(750));
    assert!(retrieve(&url,b"abc123",Duration::from_millis(150)).is_err());
    task.join().unwrap();
}
