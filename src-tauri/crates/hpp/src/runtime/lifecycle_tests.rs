use super::*;

#[tokio::test]
async fn healthy_runtime_disposes_once_with_no_io_and_failed_hook_does_not_block_stop() {
    for hook in [
        "for(const op of [()=>hg.http.fetch({url:'https://fixture.invalid/never'}),()=>hg.storage.set('late',true),()=>hg.html.parse('<p>x</p>'),()=>setTimeout(()=>{},0)]){try{op();throw new Error('allowed');}catch(e){if(e.code!=='CANCELLED')throw e;}}hg.log.info('cleaned');",
        "throw new Error('dispose failed');",
        "while(true){}",
        "return new Promise(()=>{});",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let meta = Meta::parse(json!({"id":"fixture.lifecycle","name":"Lifecycle","version":"1.0.0","protocol":1})).unwrap();
        let store = Arc::new(Store::open(dir.path().join("store.json")).unwrap());
        let network = Arc::new(Network::new(meta.clone(), dir.path().into(), Some(false), Default::default()).unwrap());
        let logs: Logs = Default::default();
        let source = format!("export const meta={};export default hg=>({{media:{{getDetail(){{}},resolvePlayback(){{}}}},dispose(){{hg.log.info('dispose');{hook}}}}});", serde_json::to_string(&meta).unwrap());
        let worker = Worker::spawn(source, meta, json!({}), Value::Null, store.clone(), network, logs.clone(), Default::default(), None).await.unwrap();
        let started = Instant::now();
        worker.stop_and_wait().await.unwrap();
        assert!(started.elapsed() < Duration::from_secs(2));
        let entries = logs.lock().unwrap().snapshot();
        assert_eq!(entries.iter().filter(|entry| entry["text"] == "dispose").count(), 1);
        if hook.starts_with("for") { assert!(entries.iter().any(|entry| entry["text"] == "cleaned")); }
        if hook.starts_with("throw") || hook.starts_with("while") {
            assert!(entries.iter().any(|entry| entry["text"] == "插件卸载通知失败"));
        }
        assert_eq!(store.operation("keys", &Transfer::from_json(json!({"prefix":""}))).unwrap().json(), json!([]));
    }
}

#[tokio::test]
async fn interrupted_runtime_skips_dispose() {
    let dir = tempfile::tempdir().unwrap();
    let logs: Logs = Default::default();
    let stop = Arc::new(AtomicBool::new(false));
    let engine = Engine::new(logs.clone(), Default::default(), stop).unwrap();
    let meta = engine.inspect("export const meta={id:'fixture.skip',name:'Skip',version:'1.0.0',protocol:1};export default hg=>({media:{getDetail(){while(true){}},resolvePlayback(){}},dispose(){hg.log.info('must not run');}});").unwrap();
    engine
        .install(&meta, &json!({}), &Value::Null, &Default::default(), false)
        .unwrap();
    engine
        .invoke(
            "start",
            (
                "loop",
                "media.getDetail",
                serde_json::to_string(&Transfer::from_json(json!({}))).unwrap(),
                now() + 10000,
            ),
        )
        .unwrap();
    assert!(engine.pump().is_err());
    drop(engine);
    assert!(!logs
        .lock()
        .unwrap()
        .snapshot()
        .iter()
        .any(|entry| entry["text"] == "must not run"));
    drop(dir);
}

#[tokio::test]
async fn consecutive_runtime_fault_is_visible_before_third_error_reply() {
    let dir = tempfile::tempdir().unwrap();
    let meta = Meta::parse(json!({
        "id": "fixture.terminal-fault", "name": "Terminal fault",
        "version": "1.0.0", "protocol": 1
    })).unwrap();
    let store = Arc::new(Store::open(dir.path().join("store.json")).unwrap());
    let network = Arc::new(Network::new(
        meta.clone(), dir.path().into(), Some(false), Default::default(),
    ).unwrap());
    let source = format!(
        "export const meta={};export default hg=>({{media:{{getDetail(){{while(true){{}}}},resolvePlayback(){{return {{ok:true}};}}}}}});",
        serde_json::to_string(&meta).unwrap()
    );
    let worker = Worker::spawn(
        source, meta, json!({}), Value::Null, store, network,
        Default::default(), Default::default(), None,
    ).await.unwrap();
    let invoke = |id: String, method: &str| {
        worker.call(id, method.to_owned(), Transfer::from_json(json!({})), Duration::from_secs(10))
    };
    // Two interruptions are recoverable, and a healthy call resets the streak.
    for n in 0..2 {
        assert!(tokio::time::timeout(Duration::from_secs(15),
            invoke(format!("before-{n}"), "media.getDetail")).await.unwrap().is_err());
        assert!(worker.fault().is_none());
    }
    assert!(tokio::time::timeout(Duration::from_secs(15),
        invoke("healthy".to_owned(), "media.resolvePlayback")).await.unwrap().is_ok());
    for n in 0..3 {
        assert!(tokio::time::timeout(Duration::from_secs(15),
            invoke(format!("after-{n}"), "media.getDetail")).await.unwrap().is_err());
        if n < 2 {
            assert!(worker.fault().is_none(), "healthy call must reset the failure streak");
        } else {
            // No polling or delay: callers must see the terminal state immediately.
            assert!(worker.fault().is_some(), "terminal state must precede the third reply");
        }
    }
    let terminal = worker.fault().expect("terminal fault");
    // The failure remains attributable even as the worker thread closes.
    let rejected = tokio::time::timeout(Duration::from_secs(2),
        invoke("post-fault".to_owned(), "media.resolvePlayback"))
        .await.unwrap().err().expect("terminal runtime must reject new calls");
    assert_eq!(rejected.code, terminal.code);
    assert_eq!(rejected.message, terminal.message);
    worker.stop_and_wait().await.unwrap();
    let rejected_after_close = invoke("post-close".to_owned(), "media.resolvePlayback")
        .await.err().expect("closed terminal runtime must reject new calls");
    assert_eq!(rejected_after_close.code, terminal.code);
    assert_eq!(rejected_after_close.message, terminal.message);
}
