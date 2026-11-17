//! HGhub optional macOS-only WARP transport.
//! A user must explicitly accept Cloudflare's terms before first registration.
//! The bundled MIT-licensed usque executable is compiled from a pinned upstream revision;
//! it only listens on loopback and never installs a system VPN or changes system routes.

use serde_json::{json, Value};

#[cfg(target_os = "macos")]
mod native {
    use super::*;
    use once_cell::sync::Lazy;
    use std::fs;
    use std::net::{SocketAddr, TcpListener};
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    use std::path::{Path, PathBuf};
    use std::process::{Child, Command, Stdio};
    use std::sync::Mutex;
    use std::time::Duration;
    use tauri::Manager;

    #[derive(Default)]
    struct Running {
        child: Option<Child>,
        port: Option<u16>,
    }

    static RUNNING: Lazy<Mutex<Running>> = Lazy::new(|| Mutex::new(Running::default()));
    static START: Lazy<tokio::sync::Mutex<()>> = Lazy::new(|| tokio::sync::Mutex::new(()));

    fn config_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
        let dir = app.path().app_data_dir().map_err(|_| "应用数据目录不可用")?.join("warp");
        fs::create_dir_all(&dir).map_err(|_| "无法创建 WARP 数据目录")?;
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))
            .map_err(|_| "无法保护 WARP 数据目录")?;
        Ok(dir.join("config.json"))
    }

    fn binary_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
        let root = app.path().resource_dir().map_err(|_| "无法定位安装包资源")?;
        let path = root.join("resources").join("usque");
        let meta = fs::symlink_metadata(&path).map_err(|_| "本安装包未包含 WARP 组件，请更新 macOS 版本")?;
        if meta.file_type().is_symlink() || !meta.is_file() || meta.permissions().mode() & 0o111 == 0 {
            return Err("内置 WARP 组件不存在或无法执行".into());
        }
        Ok(path)
    }

    fn registered(config: &Path) -> bool {
        let Ok(data) = fs::read(config) else { return false };
        if data.len() > 65536 { return false; }
        serde_json::from_slice::<Value>(&data).ok().is_some_and(|v| {
            ["private_key", "access_token", "endpoint_pub_key"].iter().all(|key|
                v.get(*key).and_then(Value::as_str).is_some_and(|s| !s.is_empty())
            )
        })
    }

    pub fn proxy_url() -> Option<String> {
        let mut state = RUNNING.lock().ok()?;
        if let Some(child) = state.child.as_mut() {
            if child.try_wait().ok().flatten().is_some() {
                state.child = None;
                state.port = None;
            }
        }
        if state.child.is_some() {
            state.port.map(|p| format!("http://127.0.0.1:{p}"))
        } else {
            None
        }
    }

    pub fn shutdown() {
        if let Ok(mut state) = RUNNING.lock() {
            if let Some(mut child) = state.child.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
            state.port = None;
        }
        crate::net::refresh_client();
    }

    pub fn status(app: &tauri::AppHandle) -> Value {
        let registered = config_path(app).ok().is_some_and(|p| registered(&p));
        let available = binary_path(app).is_ok();
        let running = proxy_url().is_some();
        json!({
            "available": available,
            "registered": registered,
            "running": running,
            "active": running && crate::net::proxy_state()["mode"] == "warp",
            "mode": crate::net::proxy_state()["mode"],
        })
    }

    async fn register(binary: &Path, config: &Path) -> Result<(), String> {
        // Pre-create the credential file with owner-only access; usque's os.Create
        // will reuse the 0600 permissions when it saves the token and private key.
        let created = fs::OpenOptions::new()
            .write(true).create_new(true).mode(0o600)
            .open(config).map_err(|_| "WARP 凭据文件已存在但无效；为防止覆盖，请先备份并处理该文件")?;
        drop(created);

        let command = tokio::process::Command::new(binary)
            .arg("-c").arg(config)
            .arg("register").arg("--accept-tos")
            .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null())
            .kill_on_drop(true)
            .status();
        let result = tokio::time::timeout(Duration::from_secs(50), command).await;
        let ok = matches!(result, Ok(Ok(code)) if code.success()) && registered(config);
        if !ok {
            let _ = fs::remove_file(config);
            return Err("WARP 设备注册未完成：可能是当前网络阻断、服务限制或注册请求超时".into());
        }
        fs::set_permissions(config, fs::Permissions::from_mode(0o600))
            .map_err(|_| "WARP 凭据权限保护失败")?;
        Ok(())
    }

    async fn verify_tunnel(url: &str) -> Result<(), String> {
        // Require independent evidence of WARP egress before activating it.
        // Otherwise an HTTP listener alone would be mistakenly reported as connected.
        let proxy = reqwest::Proxy::all(url).map_err(|_| "WARP 本地地址错误")?;
        let client = reqwest::Client::builder()
            .no_proxy().proxy(proxy)
            .connect_timeout(Duration::from_secs(8))
            .timeout(Duration::from_secs(14))
            .build().map_err(|_| "无法创建 WARP 检测请求")?;
        let resp = client.get("https://www.cloudflare.com/cdn-cgi/trace")
            .send().await.map_err(|_| "无法通过 WARP 隧道访问 Cloudflare 检测地址")?;
        if !resp.status().is_success() {
            return Err("WARP 出口检测未通过".into());
        }
        let body = resp.text().await.map_err(|_| "WARP 检测结果无效")?;
        if !body.lines().any(|line| matches!(line.trim(), "warp=on" | "warp=plus")) {
            return Err("未验证到 WARP 出口，拒绝切换流量".into());
        }
        Ok(())
    }

    pub async fn enable(app: tauri::AppHandle, accept_terms: bool) -> Result<Value, String> {
        let _start_guard = START.lock().await;
        let binary = binary_path(&app)?;
        let config = config_path(&app)?;
        if !registered(&config) {
            if !accept_terms {
                return Err("首次使用须确认 Cloudflare WARP 服务条款与第三方转发说明".into());
            }
            register(&binary, &config).await?;
        }
        if proxy_url().is_some() {
            crate::net::set_proxy("warp", "")?;
            return Ok(status(&app));
        }

        let listener = TcpListener::bind("127.0.0.1:0")
            .map_err(|_| "无法分配 WARP 本地端口")?;
        let port = listener.local_addr().map_err(|_| "无法读取 WARP 本地端口")?.port();
        drop(listener);
        let mut child = Command::new(&binary)
            .arg("-c").arg(&config)
            .arg("http-proxy")
            .arg("-b").arg("127.0.0.1")
            .arg("-p").arg(port.to_string())
            .arg("--always-reconnect")
            .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null())
            .spawn()
            .map_err(|_| "无法启动内置 WARP 组件")?;

        let address = SocketAddr::from(([127, 0, 0, 1], port));
        let mut ready = false;
        for _ in 0..40 {
            if child.try_wait().map_err(|_| "无法读取 WARP 进程状态")?.is_some() {
                break;
            }
            if tokio::net::TcpStream::connect(address).await.is_ok() {
                ready = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(125)).await;
        }

        let proxy = format!("http://127.0.0.1:{port}");
        let checked = if ready { verify_tunnel(&proxy).await } else {
            Err("WARP 本地连接未启动".into())
        };
        if let Err(error) = checked {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }

        {
            let mut state = RUNNING.lock().map_err(|_| "WARP 进程锁失败")?;
            state.child = Some(child);
            state.port = Some(port);
        }
        if let Err(error) = crate::net::set_proxy("warp", "") {
            shutdown();
            return Err(error);
        }
        Ok(status(&app))
    }

    pub async fn disable(app: tauri::AppHandle) -> Result<Value, String> {
        let _start_guard = START.lock().await;
        // Reset transport first so we never route more requests to a stopped proxy.
        crate::net::set_proxy("system", "")?;
        shutdown();
        Ok(status(&app))
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn registration_validation_does_not_return_credentials() {
            let tmp = std::env::temp_dir().join(format!("hghub-warp-check-{}", std::process::id()));
            let _ = fs::remove_file(&tmp);
            assert!(!registered(&tmp));
            fs::write(&tmp, r#"{"private_key":"secret","access_token":"token","endpoint_pub_key":"public"}"#).unwrap();
            assert!(registered(&tmp));
            fs::write(&tmp, r#"{"private_key":"secret","access_token":"","endpoint_pub_key":"public"}"#).unwrap();
            assert!(!registered(&tmp));
            fs::remove_file(tmp).unwrap();
        }
    }
}

pub fn proxy_url() -> Option<String> {
    #[cfg(target_os = "macos")]
    { return native::proxy_url(); }
    #[cfg(not(target_os = "macos"))]
    { None }
}

pub fn shutdown() {
    #[cfg(target_os = "macos")]
    native::shutdown();
}

pub fn restore(app: tauri::AppHandle) {
    #[cfg(target_os = "macos")]
    if crate::net::proxy_state()["mode"] == "warp" {
        tauri::async_runtime::spawn(async move {
            let _ = native::enable(app, false).await;
        });
        return;
    }
    let _ = app;
}

#[tauri::command]
pub async fn warp_status(app: tauri::AppHandle) -> Result<Value, String> {
    #[cfg(target_os = "macos")]
    { return Ok(native::status(&app)); }
    #[cfg(not(target_os = "macos"))]
    { let _ = app; Ok(json!({"available":false,"registered":false,"running":false,"active":false})) }
}

#[tauri::command]
pub async fn warp_enable(app: tauri::AppHandle, accept_terms: bool) -> Result<Value, String> {
    #[cfg(target_os = "macos")]
    { return native::enable(app, accept_terms).await; }
    #[cfg(not(target_os = "macos"))]
    { let _ = (app, accept_terms); Err("内置 WARP 目前仅支持 macOS 桌面版".into()) }
}

#[tauri::command]
pub async fn warp_disable(app: tauri::AppHandle) -> Result<Value, String> {
    #[cfg(target_os = "macos")]
    { return native::disable(app).await; }
    #[cfg(not(target_os = "macos"))]
    { let _ = app; Err("内置 WARP 目前仅支持 macOS 桌面版".into()) }
}
