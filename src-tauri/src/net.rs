//! 网络层：与上游站点对话。
//!
//! 站点把全部 `/api/**` 调用收敛成 `POST /api/edge/request` 的信封，
//! 我们只实现这一个入口，其余接口都是「填 path」。
//!
//! # 代理（用户反馈「浏览器能开、HGhub 打不开」的根因就在这里）
//!
//! 客户端的流量**全部**走本文件的 reqwest，跟 WebView2 无关，所以它既不共享
//! 浏览器的代理设置，也不共享浏览器的证书库：
//!
//! * reqwest 只有开了 `system-proxy` feature 才会读 Windows 系统代理
//!   （`HKCU\...\Internet Settings` 的 `ProxyEnable`/`ProxyServer`/`ProxyOverride`）。
//!   本 crate 用 `default-features = false`，早先漏了这个 feature ⇒ 永远直连，
//!   只有 TUN/虚拟网卡这种网络层接管才「看起来能用」。
//! * 即使开了 `system-proxy`，PAC（`AutoConfigURL`）也不被支持，只认固定代理。
//! * Android 没有注册表可读，系统「Wi-Fi 代理」同样不被 reqwest 采用；
//!   安卓上只有 VPN/TUN 模式（VpnService 抓全流量）才有效。
//!
//! 排查手段：环境变量 `HGHUB_PROXY=http://127.0.0.1:7897` 可强制指定代理，
//! `net_diag` 命令会输出 DNS/HTTP/环境变量快照。

use crate::network_policy;
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use once_cell::sync::Lazy;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

/// 默认上游（固定线路 1）。
pub const DEFAULT_BASE: &str = "https://huangguoai.com";

/// 当前上游基址（不带结尾斜杠）。可在设置里切到固定线路 2 或镜像线路，运行时立即生效。
static BASE: Lazy<RwLock<String>> = Lazy::new(|| RwLock::new(DEFAULT_BASE.to_string()));
/// 线路配置落盘位置（应用数据目录/line.json），由 [`bind_base`] 在启动时绑定。
static BASE_FILE: Lazy<RwLock<Option<PathBuf>>> = Lazy::new(|| RwLock::new(None));

/// 当前上游基址。
pub fn base() -> String {
    BASE.read()
        .map(|b| b.clone())
        .unwrap_or_else(|_| DEFAULT_BASE.to_string())
}

/// 线路必须是纯 HTTPS origin，不允许路径、认证、查询串或片段。
pub fn normalize_base(raw: &str) -> Option<String> {
    let parsed = network_policy::parse_target(raw.trim()).ok()?;
    if parsed.scheme() != "https"
        || parsed.path() != "/"
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return None;
    }
    Some(parsed.origin().ascii_serialization())
}

/// 远端发布页只提供游客镜像，不能扩展账号密码的信任范围。
pub fn trusted_auth_origin(origin: &str) -> bool {
    matches!(origin, "https://huangguoai.com" | "https://huangguoai.ai")
}

/// 启动时读回上次选择的线路（文件不存在 / 内容坏了就保持默认）。
pub fn bind_base(dir: &std::path::Path) {
    network_policy::bind_security(dir);
    let path = dir.join("line.json");
    if let Ok(text) = std::fs::read_to_string(&path) {
        if let Ok(v) = serde_json::from_str::<Value>(&text) {
            if let Some(next) = v
                .get("base")
                .and_then(|x| x.as_str())
                .and_then(normalize_base)
            {
                if let Ok(mut cur) = BASE.write() {
                    *cur = next;
                }
            }
        }
    }
    if let Ok(mut f) = BASE_FILE.write() {
        *f = Some(path);
    }
    if let Ok(text) = std::fs::read_to_string(dir.join("proxy.json")) {
        if let Ok(v) = serde_json::from_str::<Value>(&text) {
            if let Some(mode) = v.get("mode").and_then(Value::as_str) {
                let url = v.get("url").and_then(Value::as_str).unwrap_or("");
                if let Ok(valid) = validate_proxy(mode, url) {
                    if let Ok(mut current) = PROXY_SETTINGS.write() { *current = valid; }
                }
            }
        }
    }
}

/// 切换线路并落盘，返回规整后的地址。
pub fn set_base(url: &str) -> Result<String, String> {
    let next = normalize_base(url)
        .ok_or_else(|| "线路地址无效（需要纯 HTTPS 站点地址，禁止内网）".to_string())?;
    let previous = base();
    if let Ok(mut cur) = BASE.write() {
        *cur = next.clone();
    }
    if let Some(path) = BASE_FILE.read().ok().and_then(|p| p.clone()) {
        let _ = std::fs::write(path, json!({ "base": next }).to_string());
    }
    if previous != next {
        crate::playback_log::record(
            "line.changed",
            json!({
                "previous_host": url::Url::parse(&previous).ok().and_then(|u| u.host_str().map(str::to_owned)),
                "line_host": url::Url::parse(&next).ok().and_then(|u| u.host_str().map(str::to_owned)),
            }),
        );
    }
    Ok(next)
}

pub const UA: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

/// 用户选择的网络方式。默认系统代理；不会自动使用未知镜像。
#[derive(Clone)]
struct ProxySettings {
    mode: String,
    url: String,
}
impl Default for ProxySettings {
    fn default() -> Self {
        Self { mode: "system".to_string(), url: String::new() }
    }
}
static PROXY_SETTINGS: Lazy<RwLock<ProxySettings>> =
    Lazy::new(|| RwLock::new(ProxySettings::default()));

fn validate_proxy(mode: &str, raw: &str) -> Result<ProxySettings, String> {
    match mode {
        "system" | "direct" => Ok(ProxySettings { mode: mode.into(), url: String::new() }),
        "warp" if cfg!(target_os = "macos") => Ok(ProxySettings { mode: mode.into(), url: String::new() }),
        "custom" => {
            if raw.len() > 512 || raw.chars().any(char::is_control) {
                return Err("代理地址过长或包含控制字符".into());
            }
            let u = url::Url::parse(raw.trim()).map_err(|_| "代理地址格式不正确")?;
            if !matches!(u.scheme(), "http" | "https")
                || !u.username().is_empty() || u.password().is_some()
                || u.path() != "/" || u.query().is_some() || u.fragment().is_some()
                || u.host_str().is_none()
            {
                return Err("仅支持不带账号密码、路径或参数的 HTTP(S) 代理地址".into());
            }
            let host = u.host_str().unwrap_or("").to_ascii_lowercase();
            if matches!(host.as_str(), "tauri.localhost" | "hg.localhost" | "ipc.localhost") {
                return Err("不能使用应用内部主机名作为代理".into());
            }
            Ok(ProxySettings {
                mode: "custom".into(),
                url: u.as_str().trim_end_matches('/').into(),
            })
        }
        _ => Err("未知代理模式".into()),
    }
}

pub fn proxy_state() -> Value {
    let config = PROXY_SETTINGS.read().map(|x| x.clone()).unwrap_or_default();
    json!({"mode": config.mode, "url": config.url})
}

/// 先安全落盘再切换。代理仅改变传输方式，不修改登录、收藏和历史。
pub fn proxy_set(mode: &str, url: &str) -> Result<Value, String> {
    let config = validate_proxy(mode, url)?;
    // Do not let front-end callers enable a missing or stopped sidecar.
    if mode == "warp" && crate::warp::proxy_url().is_none() {
        return Err("内置 WARP 尚未连接；请先点击连接并完成检测".into());
    }
    let base_file = BASE_FILE.read().map_err(|_| "网络设置锁失败")?
        .clone().ok_or("应用数据目录尚未初始化")?;
    let path = base_file.with_file_name("proxy.json");
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, json!({"mode":config.mode,"url":config.url}).to_string())
        .and_then(|_| std::fs::rename(&temp, path))
        .map_err(|_| "代理设置保存失败")?;
    {
        let mut current = PROXY_SETTINGS.write().map_err(|_| "网络设置锁失败")?;
        *current = config;
    }
    refresh_client();
    Ok(proxy_state())
}

/// 手动指定代理（排查 / 公司内网用）：`HGHUB_PROXY=http://127.0.0.1:7897`。
/// 不设时交给 reqwest 的 `system-proxy` 探测（注册表 + 环境变量）。
fn proxy_override() -> Option<String> {
    std::env::var("HGHUB_PROXY")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// 解析 macOS scutil 的显式 HTTPS/HTTP 系统代理。PAC/WPAD 不解析、不执行。
#[cfg(any(target_os = "macos", test))]
fn parse_macos_proxy(text: &str) -> Option<String> {
    let mut values: HashMap<&str, &str> = HashMap::new();
    for line in text.lines() {
        if let Some((key, value)) = line.trim().split_once(" : ") {
            values.insert(key.trim(), value.trim());
        }
    }
    for (enabled, host_key, port_key) in [
        ("HTTPSEnable", "HTTPSProxy", "HTTPSPort"),
        ("HTTPEnable", "HTTPProxy", "HTTPPort"),
    ] {
        if values.get(enabled).copied() != Some("1") {
            continue;
        }
        let Some(host) = values.get(host_key).map(|v| v.trim()) else {
            continue;
        };
        let Some(port) = values.get(port_key).and_then(|v| v.parse::<u16>().ok()) else {
            continue;
        };
        if port == 0 || host.is_empty() {
            continue;
        }
        let address = if host.contains(':') && !host.starts_with('[') {
            format!("http://[{host}]:{port}")
        } else {
            format!("http://{host}:{port}")
        };
        if let Ok(valid) = validate_proxy("custom", &address) {
            return Some(valid.url);
        }
    }
    None
}

#[cfg(target_os = "macos")]
static MACOS_PROXY_CACHE: Lazy<Mutex<Option<(Instant, Option<String>)>>> =
    Lazy::new(|| Mutex::new(None));

#[cfg(target_os = "macos")]
fn macos_system_proxy() -> Option<String> {
    use std::process::Command;
    // 不为每个 HLS 分片都启动 scutil；系统代理变化最多约 10 秒内生效。
    if let Ok(mut cache) = MACOS_PROXY_CACHE.lock() {
        if let Some((last_checked, result)) = cache.as_ref() {
            if last_checked.elapsed() < Duration::from_secs(10) {
                return result.clone();
            }
        }
        let result = Command::new("/usr/sbin/scutil")
            .arg("--proxy")
            .output()
            .ok()
            .and_then(|output| {
                if !output.status.success() || output.stdout.len() > 32768 {
                    return None;
                }
                String::from_utf8(output.stdout).ok()
            })
            .and_then(|text| parse_macos_proxy(&text));
        *cache = Some((Instant::now(), result.clone()));
        return result;
    }
    None
}

#[cfg(not(target_os = "macos"))]
fn macos_system_proxy() -> Option<String> {
    None
}

/// macOS 自动复用已启用的系统 HTTP/HTTPS 代理；不猜测未知本地端口。
fn effective_proxy(config: &ProxySettings) -> Option<String> {
    match config.mode.as_str() {
        "direct" => None,
        "custom" => Some(config.url.clone()),
        "warp" => crate::warp::proxy_url(),
        _ => proxy_override().or_else(macos_system_proxy),
    }
}

/// 按**当前**系统代理设置新建一个客户端。
///
/// 注意：reqwest 的 `system-proxy` 只在构建时读一次注册表 / 环境变量。代理软件被关掉后，
/// 旧客户端仍会往 `127.0.0.1:7897` 建隧道，症状是满屏 `tunnel error: failed to create
/// underlying connection`（浏览器却是通的，因为它每次都重读系统代理）。所以这里允许重建。
fn build_client(host: &str, addrs: &[SocketAddr], config: &ProxySettings, proxy_url: Option<&str>) -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder()
        .user_agent(UA)
        .timeout(Duration::from_secs(20))
        .connect_timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .referer(false)
        .gzip(true)
        .brotli(true)
        .pool_idle_timeout(Duration::from_secs(60));
    if !addrs.is_empty() {
        builder = builder.resolve_to_addrs(host, addrs);
    }
    if config.mode == "direct" {
        builder = builder.no_proxy();
    } else if let Some(proxy_url) = proxy_url {
        let proxy = reqwest::Proxy::all(proxy_url)
            .map_err(|_| "代理地址无效（详细信息已隐藏）")?;
        builder = builder.proxy(proxy);
    }
    builder
        .build()
        .map_err(|_| "无法创建 HTTP 客户端".to_string())
}

const CLIENT_TTL: Duration = Duration::from_secs(30);
type ClientCache = HashMap<String, (Instant, Arc<reqwest::Client>)>;
static CLIENTS: Lazy<Mutex<ClientCache>> = Lazy::new(|| Mutex::new(HashMap::new()));

fn cached_client(cache: &ClientCache, key: &str, now: Instant) -> Option<Arc<reqwest::Client>> {
    cache
        .get(key)
        .filter(|(expires, _)| now < *expires)
        .map(|(_, http)| http.clone())
}

/// 仅当系统 DNS 明确失败时启用的加密 DNS（DoH），不更改系统网络。
/// HTTPS 证书仍由 rustls 验证；为解析器使用公开的固定入口 IP 避免递归查询。
/// 向解析器发送的只有目标主机名，不包含请求路径、Cookie、Token 或账号。
#[derive(Clone, Copy)]
struct DohProvider {
    host: &'static str,
    url: &'static str,
    ips: &'static [&'static str],
}

const DOH_PROVIDERS: [DohProvider; 2] = [
    DohProvider {
        host: "cloudflare-dns.com",
        url: "https://cloudflare-dns.com/dns-query",
        ips: &["1.1.1.1:443", "1.0.0.1:443"],
    },
    DohProvider {
        host: "dns.google",
        url: "https://dns.google/resolve",
        ips: &["8.8.8.8:443", "8.8.4.4:443"],
    },
];

/// DoH 不允许将远端解析结果导向私网、回环或特殊地址。
fn parse_doh_answers(answer: &Value, qtype: u64, port: u16) -> Result<Vec<SocketAddr>, String> {
    if answer.get("Status").and_then(Value::as_u64) != Some(0) {
        return Err("加密 DNS 未返回成功状态".to_string());
    }
    let mut addrs = Vec::new();
    if let Some(rows) = answer.get("Answer").and_then(Value::as_array) {
        if rows.len() > 64 {
            return Err("加密 DNS 记录数量超出限制".into());
        }
        for row in rows {
            if row.get("type").and_then(Value::as_u64) != Some(qtype) {
                continue;
            }
            let Some(raw_ip) = row.get("data").and_then(Value::as_str) else {
                continue;
            };
            if let Ok(ip) = raw_ip.parse::<std::net::IpAddr>() {
                if (qtype == 1 && !ip.is_ipv4()) || (qtype == 28 && !ip.is_ipv6()) {
                    continue;
                }
                // 对所有答案执行私网地址检查；不能只挑选其中的公开 IP。
                // 外部公开 DNS 不应返回 FakeIP (198.18.0.0/15)。
                // 该例外只适用于本地可信 TUN，不能复用于远程 DNS 结果。
                let fake_ip = match ip {
                    std::net::IpAddr::V4(v4) => v4.octets()[0] == 198
                        && matches!(v4.octets()[1], 18 | 19),
                    std::net::IpAddr::V6(v6) => v6.to_ipv4_mapped().is_some_and(|v4|
                        v4.octets()[0] == 198 && matches!(v4.octets()[1], 18 | 19)),
                };
                if !network_policy::allowed_ip(ip) || fake_ip {
                    return Err("加密 DNS 返回了不允许的目标地址".into());
                }
                addrs.push(SocketAddr::new(ip, port));
            }
        }
    }
    Ok(addrs)
}

/// HTTPS DNS 请求独立于业务 HTTP 客户端，不走不可信代理设置。
/// 每个 DNS 服务分别限制 3 秒，两种记录并行查询；失败才尝试第二个服务。
async fn doh_query(provider: DohProvider, host: &str, port: u16) -> Result<Vec<SocketAddr>, String> {
    let endpoints: Vec<SocketAddr> = provider.ips.iter()
        .filter_map(|raw| raw.parse::<SocketAddr>().ok()).collect();
    let client = reqwest::Client::builder()
        .no_proxy()
        .resolve_to_addrs(provider.host, &endpoints)
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(3))
        .connect_timeout(Duration::from_secs(2))
        .build()
        .map_err(|_| "无法创建加密 DNS 客户端")?;
    let lookup = |record: &'static str| {
        let client = client.clone();
        async move {
            client.get(provider.url)
                .query(&[("name", host), ("type", record)])
                .header(reqwest::header::ACCEPT, "application/dns-json")
                .send()
                .await
                .map_err(|_| "加密 DNS 连接失败".to_string())?
                .error_for_status()
                .map_err(|_| "加密 DNS HTTP 错误".to_string())?
                .json::<Value>()
                .await
                .map_err(|_| "加密 DNS 返回无效数据".to_string())
        }
    };
    let (a, aaaa) = tokio::join!(lookup("A"), lookup("AAAA"));
    let mut results = Vec::new();
    let mut received = false;
    for (answer, qtype) in [(a, 1), (aaaa, 28)] {
        if let Ok(value) = answer {
            received = true;
            results.extend(parse_doh_answers(&value, qtype, port)?);
        }
    }
    if !received {
        return Err("加密 DNS 服务暂时不可达".to_string());
    }
    // 与原 DNS 解析路径一致：偏好 IPv6、去重，并整体检查答案。
    results.sort_unstable_by_key(|addr| (addr.is_ipv4(), *addr));
    results.dedup();
    network_policy::check_addresses_with_mode(&results, true)?;
    Ok(results)
}

async fn secure_dns_recovery(host: &str, port: u16) -> Result<Vec<SocketAddr>, String> {
    let mut last_error = String::from("加密 DNS 不可达");
    for provider in DOH_PROVIDERS {
        match doh_query(provider, host, port).await {
            Ok(addrs) => {
                crate::playback_log::record("network.secure_dns_recovery", json!({
                    "resolver": provider.host, "result_count": addrs.len(),
                }));
                return Ok(addrs);
            }
            Err(e) => last_error = e,
        }
    }
    Err(last_error)
}

fn local_dns_failure(error: &str) -> bool {
    matches!(error, "DNS 解析失败" | "DNS 解析超时" | "目标没有可用的 DNS 地址")
}

/// 缓存按模式、主机和端口隔离；有效期内仅复用已经检查并固定地址的客户端。
/// 到期后重新检查全部 DNS 答案，不使用旧连接绕过新的目标检查。
async fn client_for(raw: &str) -> Result<(url::Url, Arc<reqwest::Client>), String> {
    let strict = network_policy::strict_security();
    let target = network_policy::parse_target_with_mode(raw, strict)?;
    let host = target.host_str().ok_or("目标缺少主机名")?;
    let port = target.port_or_known_default().ok_or("目标端口无效")?;
    let config = PROXY_SETTINGS.read().map_err(|_| "网络设置锁失败")?.clone();
    let proxy_url = effective_proxy(&config);
    // Fail closed if the user explicitly selected the embedded tunnel.
    // Never leak HGhub traffic directly if usque exits or cannot reconnect.
    if config.mode == "warp" && proxy_url.is_none() {
        return Err("内置 WARP 连接未建立；请在设置中重新连接或关闭内置 WARP".into());
    }
    let key = format!("{strict}:{}:{}:{host}:{port}", config.mode, proxy_url.as_deref().unwrap_or("direct"));
    {
        let cache = CLIENTS.lock().map_err(|_| "HTTP 客户端锁失败")?;
        if let Some(http) = cached_client(&cache, &key, Instant::now()) {
            return Ok((target, http));
        }
    }
    // 用户显式代理时，由 HTTP CONNECT 代理解析域名，避免本地 DNS 被阻断。
    // 仍由 parse_target_with_mode 阻止字面私网 IP 和内部域名；代理必须可信。
    let addrs = if strict && proxy_url.is_none() {
        match network_policy::resolve_target(&target, strict).await {
            Ok(addrs) => addrs,
            Err(reason) if local_dns_failure(&reason) && target.scheme() == "https" => {
                // 本机 DNS 失败后自动查询公开的免费 DoH；证书与安全地址检查不降低。
                // 仅在域名目标上使用 DoH，字面 IP 和安全拒绝绝不走回退。
                if target.host().is_some_and(|h| matches!(h, url::Host::Domain(_))) {
                    match secure_dns_recovery(host, port).await {
                        Ok(addrs) => addrs,
                        Err(_) => return Err(reason),
                    }
                } else {
                    return Err(reason);
                }
            }
            Err(reason) => return Err(reason),
        }
    } else {
        Vec::new()
    };
    let http = Arc::new(build_client(host, &addrs, &config, proxy_url.as_deref())?);
    let now = Instant::now();
    let mut cache = CLIENTS.lock().map_err(|_| "HTTP 客户端锁失败")?;
    // 同时到达的请求可能已完成解析；不延长已有结果的有效期。
    let http = cached_client(&cache, &key, now).unwrap_or(http);
    if !cache.get(&key).is_some_and(|(expires, _)| now < *expires) {
        cache.retain(|_, (expires, _)| now < *expires);
        if cache.len() >= 64 {
            cache.clear();
        }
        cache.insert(key, (now + CLIENT_TTL, http.clone()));
    }
    Ok((target, http))
}

pub fn refresh_client() {
    if let Ok(mut cache) = CLIENTS.lock() {
        cache.clear();
    }
}

/// 不可信 GET 的统一入口；自动重定向被关闭，每一跳重新检查 URL、DNS 并固定解析地址。
pub async fn get(
    raw: &str,
    timeout: Duration,
    accept: Option<&str>,
    referer: Option<&str>,
) -> Result<reqwest::Response, String> {
    tokio::time::timeout(timeout, async {
        let policy_error = |phase: &str, error: String| {
            crate::playback_log::record("network.target_failed", json!({"resource": crate::playback_log::resource_id(raw), "phase": phase, "error_kind": crate::playback_log::error_kind(&error)}));
            error
        };
        let mut next = network_policy::parse_target(raw).map_err(|e| policy_error("parse", e))?;
        for hop in 0..=5 {
            let resolving = Instant::now();
            let (target, http) = client_for(next.as_str()).await.map_err(|e| policy_error("dns_or_client", e))?;
            crate::playback_log::record("network.target_ready", json!({"resource": crate::playback_log::resource_id(raw), "target_resource": crate::playback_log::resource_id(target.as_str()), "dns_or_client_ms": resolving.elapsed().as_millis() as u64, "hop": hop}));
            let mut request = http.get(target.clone()).timeout(timeout);
            if let Some(value) = accept {
                request = request.header("Accept", value);
            }
            if let Some(value) = referer {
                request = request.header("Referer", value);
            }
            let response = request.send().await.map_err(|e| err_text(&e))?;
            if !matches!(response.status().as_u16(), 301 | 302 | 303 | 307 | 308) {
                return Ok(response);
            }
            if hop == 5 {
                return Err("重定向次数超过限制".into());
            }
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|h| h.to_str().ok())
                .ok_or("重定向缺少有效 Location")?;
            next = network_policy::redirect_target(&target, location).map_err(|e| policy_error("redirect", e))?;
            crate::playback_log::record("network.redirect", json!({"resource": crate::playback_log::resource_id(raw), "from": crate::playback_log::resource_id(target.as_str()), "to": crate::playback_log::resource_id(next.as_str()), "status": response.status().as_u16(), "hop": hop + 1}));
        }
        Err("重定向次数超过限制".into())
    })
    .await
    .map_err(|_| "请求超时".to_string())?
}

/// 只返回代理的协议/主机/端口；解析失败时隐藏整个值，绝不回传认证或查询参数。
pub fn redact_proxy(raw: &str) -> String {
    let parsed = url::Url::parse(raw.trim())
        .ok()
        .filter(|u| u.host_str().is_some());
    match parsed {
        Some(url) => {
            let mut safe = format!("{}://{}", url.scheme(), url.host_str().unwrap());
            if let Some(port) = url.port() {
                safe.push_str(&format!(":{port}"));
            }
            safe
        }
        None => "[已配置，原始值已隐藏]".into(),
    }
}

/// 看起来像代理 / 隧道问题（而不是目标站点本身不可达）。
pub fn is_proxy_error(e: &str) -> bool {
    let e = e.to_ascii_lowercase();
    e.contains("tunnel") || e.contains("proxy")
}

static LAST_HEAL: Lazy<std::sync::Mutex<Option<Instant>>> =
    Lazy::new(|| std::sync::Mutex::new(None));

/// 请求失败后按需重建客户端：代理隧道错误、连接被拒 / 超时都算「网络环境可能变了」。
/// 3 秒内只重建一次，批量探测不会反复重建。
pub fn heal_client(reason: &str) {
    let connectish = reason.contains("Connect")
        || reason.contains("timed out")
        || reason.contains("refused")
        || reason.contains("dns");
    if !is_proxy_error(reason) && !connectish {
        return;
    }
    let mut last = match LAST_HEAL.lock() {
        Ok(l) => l,
        Err(_) => return,
    };
    if let Some(t) = *last {
        if t.elapsed() < Duration::from_secs(3) {
            return;
        }
    }
    *last = Some(Instant::now());
    drop(last);
    refresh_client();
}

/// reqwest 的 `Display` 只有「error sending request for url (...)」，
/// 真正的原因（dns error / connection refused / invalid peer certificate）
/// 藏在 `source()` 链里。展开成一行，界面和 debug.log 才有排查价值。
pub fn err_text(e: &reqwest::Error) -> String {
    let mut out = e.to_string();
    let mut src = std::error::Error::source(e);
    while let Some(s) = src {
        let t = s.to_string();
        if !t.is_empty() && !out.contains(&t) {
            out.push_str(" ← ");
            out.push_str(&t);
        }
        src = s.source();
    }
    out
}

fn dns_summary(addrs: impl IntoIterator<Item = SocketAddr>) -> Value {
    let mut ips: Vec<_> = addrs.into_iter().map(|addr| addr.ip()).collect();
    ips.sort_unstable();
    ips.dedup();
    let has_fake_ip = ips.iter().any(|ip| {
        let v4 = match ip {
            std::net::IpAddr::V4(v4) => Some(*v4),
            std::net::IpAddr::V6(v6) => v6.to_ipv4_mapped(),
        };
        v4.is_some_and(|ip| {
            let [a, b, _, _] = ip.octets();
            a == 198 && (b == 18 || b == 19)
        })
    });
    json!({ "count": ips.len(), "has_fake_ip": has_fake_ip })
}

/// 网络自检：DNS / HTTP / 代理相关环境变量，用于远程定位「打不开」。
/// One anonymous, bounded, read-only legacy API check, with no fallback or retries.
pub async fn probe_content(origin: &str) -> Value {
    let started = Instant::now();
    let response_meta = Mutex::new(Value::Null);
    let result = tokio::time::timeout(
        Duration::from_secs(12),
        edge_once(origin, "GET", "/api/videos?page=1&size=1", None, None, false, Some(&response_meta)),
    ).await;
    let ms = started.elapsed().as_millis() as u64;
    let mut diagnostic = match result {
        Ok(Ok(value)) if valid_feed(&value) => json!({"ok":true,"ms":ms}),
        Ok(Ok(_)) => json!({"ok":false,"ms":ms,"error_kind":"unexpected_feed_format"}),
        Ok(Err(error)) => json!({"ok":false,"ms":ms,
            "error_kind":crate::playback_log::error_kind(&error),
            "status":http_error_status(&error)}),
        Err(_) => json!({"ok":false,"ms":ms,"error_kind":"timeout"}),
    };
    diagnostic["response"] = response_meta.into_inner().unwrap_or(Value::Null);
    diagnostic
}

// Export only bounded, classified response metadata; never raw headers or bodies.
fn diagnostic_response(headers: &reqwest::header::HeaderMap, status: u16) -> Value {
    let content_type = headers.get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok()).unwrap_or("").to_ascii_lowercase();
    let cf_error = headers.get("cf-error-type").and_then(|v| v.to_str().ok())
        .filter(|v| (3..=4).contains(&v.len()) && v.bytes().all(|b| b.is_ascii_digit()));
    let cf_generated = status >= 400 && (cf_error.is_some() || headers.contains_key("cf-error-origin"));
    json!({
        "status": status,
        "format": if content_type.contains("json") { "json" } else if content_type.contains("html") { "html" } else { "other" },
        "gateway": if headers.contains_key("x-amz-cf-id") { "cloudfront" } else if headers.contains_key("cf-ray") { "cloudflare" } else { "unknown" },
        "cloudflare_generated_error": cf_generated,
        "cloudflare_error_type": cf_error,
    })
}

fn valid_feed(value: &Value) -> bool {
    value.get("items").and_then(Value::as_array).is_some()
}

pub async fn diagnose() -> Value {
    let cur = base();
    let host = url::Url::parse(&cur)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_string()))
        .unwrap_or_default();

    // All probes are independently bounded and run concurrently. Local DNS
    // failure does not prove a proxy-side DNS failure.
    let mode_before = proxy_state();
    let warp_running = if mode_before["mode"] == "warp" {
        Some(crate::warp::proxy_url().is_some())
    } else {
        None
    };
    let warp_url_before = if mode_before["mode"] == "warp" { crate::warp::proxy_url() } else { None };
    let (dns, http, api_check, backup_check, warp_egress) = tokio::join!(
        async {
            match tokio::time::timeout(
                Duration::from_secs(4),
                tokio::net::lookup_host((host.as_str(), 443u16)),
            ).await {
                Ok(Ok(iter)) => dns_summary(iter),
                Ok(Err(_)) => json!({"error":"DNS 解析失败"}),
                Err(_) => json!({"error":"DNS 解析超时"}),
            }
        },
        async {
            let started = Instant::now();
            match get(&format!("{cur}/"), Duration::from_secs(8), None, None).await {
                Ok(r) => json!({"status":r.status().as_u16(),"ms":started.elapsed().as_millis() as u64}),
                Err(e) => json!({"error_kind":crate::playback_log::error_kind(&e),"ms":started.elapsed().as_millis() as u64}),
            }
        },
        probe_content(&cur),
        async {
            if let Some(origin) = automatic_backup(&cur) {
                json!({"origin":origin,"api":probe_content(origin).await})
            } else {
                Value::Null
            }
        },
        async {
            if mode_before["mode"] == "warp" {
                crate::warp::diagnose_egress().await
            } else {
                Value::Null
            }
        }
    );
    let settings_changed = cur != base() || mode_before != proxy_state()
        || (mode_before["mode"] == "warp" && warp_url_before != crate::warp::proxy_url());

    json!({
        "version": env!("CARGO_PKG_VERSION"),
        "checked_at_utc": crate::logging::utc_timestamp().ok(),
        "os": std::env::consts::OS,
        "base": cur,
        "host": host,
        "api": api_check,
        "backup": backup_check,
        "settings_changed": settings_changed,
        "auto_line_recovery": "仅匿名 GET 遇到网络或临时网关故障时尝试两条内置固定线路",
        "secure_dns": {
            "mode": "automatic_fallback_on_local_dns_error",
            "providers": ["Cloudflare DNS", "Google Public DNS"],
            "note": "仅向加密 DNS 提交域名；DNS 恢复不代表网站或视频服务可访问",
        },
        "proxy": {
            "mode": mode_before["mode"],
            "warp_running": warp_running,
            "warp_egress": warp_egress,
            "address": PROXY_SETTINGS.read().ok().and_then(|p| {
                let selected = effective_proxy(&p);
                selected.map(|s| redact_proxy(&s))
            }),
            "note": "手动代理模式由代理端解析域名，本地 DNS 失败不代表代理失败",
        },
        "dns": dns,
        "http": http,
        "env": {
            "HTTP_PROXY": std::env::var("HTTP_PROXY").ok().map(|p| redact_proxy(&p)),
            "HTTPS_PROXY": std::env::var("HTTPS_PROXY").ok().map(|p| redact_proxy(&p)),
            "ALL_PROXY": std::env::var("ALL_PROXY").ok().map(|p| redact_proxy(&p)),
            "NO_PROXY": std::env::var("NO_PROXY").ok().map(|_| "[已配置，列表已隐藏]"),
            "HGHUB_PROXY": proxy_override().map(|p| redact_proxy(&p)),
        },
    })
}

/// Accept both reqwest's "HTTP 503 Service Unavailable" and "HTTP 503: ...".
fn http_error_status(error: &str) -> Option<u16> {
    let suffix = error.strip_prefix("HTTP ")?;
    let code = suffix.get(..3)?;
    if !code.bytes().all(|b| b.is_ascii_digit()) ||
        suffix.as_bytes().get(3).is_some_and(|b| b.is_ascii_digit()) {
        return None;
    }
    code.parse().ok()
}

fn retryable_error(error: &str) -> bool {
    if error == "401" {
        return false;
    }
    if let Some(status) = http_error_status(error) {
        // A rate limit is not a request to retry immediately.
        return matches!(status, 408 | 425 | 500..=599);
    }
    !matches!(
        crate::playback_log::error_kind(error),
        "https_downgrade_denied"
            | "private_address_denied"
            | "target_policy_denied"
            | "redirect_error"
            | "unexpected_api_response"
    )
}

/// 只重试可能恢复的错误；404/权限错误/安全拒绝直接结束，避免无效等待。
/// 站点偶发 TLS 抖动；碰上代理隧道错误就先重建客户端再试
/// （代理软件开关后旧客户端会一直卡在 tunnel error 上）。
async fn with_retry<F, Fut, T>(retry: bool, mut f: F) -> Result<T, String>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, String>>,
{
    let mut last = String::from("network error");
    let attempts = if retry { 3 } else { 1 };
    for attempt in 0..attempts {
        match f().await {
            Ok(v) => return Ok(v),
            Err(e) => {
                last = e;
                let unavailable = http_error_status(&last) == Some(503);
                // 503: at most one retry on the same origin. A new origin is still
                // subject to the existing anonymous-read/trusted-source policy.
                if !retryable_error(&last) || (unavailable && attempt >= 1) {
                    return Err(last);
                }
                heal_client(&last);
                if attempt + 1 < attempts {
                    let delay_ms = if unavailable { 1500 } else { 350 * (attempt as u64 + 1) };
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                }
            }
        }
    }
    Err(last)
}

// 只记录固定的操作类型，不写搜索词、账号、内容 ID 或查询参数。
fn edge_operation(path: &str) -> &'static str {
    let path = path.split('?').next().unwrap_or("");
    if path.starts_with("/api/videos/category/") {
        "category"
    } else if path.starts_with("/api/videos/") && (path.ends_with("/play") || path.ends_with("/playback")) {
        "play"
    } else if path == "/api/videos" {
        "feed"
    } else if path.starts_with("/api/videos/") {
        "video"
    } else if path == "/api/posts" {
        "posts"
    } else if path.starts_with("/api/posts/") {
        "post"
    } else if path.starts_with("/api/ranks/") {
        "rank"
    } else if path == "/api/search" {
        "search"
    } else if path.starts_with("/api/account/") {
        "account"
    } else {
        "other"
    }
}

/// 仅无请求体、无凭据的逻辑 GET 可以迁移网关；信封本身仍使用 POST。
fn edge_redirect_target(
    current: &url::Url,
    status: u16,
    location: &str,
    guest_get: bool,
) -> Result<url::Url, String> {
    if !guest_get || !matches!(status, 301 | 302 | 303) {
        return Err("业务接口重定向已拒绝".into());
    }
    network_policy::redirect_target(current, location)
}

/// 只有这两条源代码内置的固定线路可以互为备用。不读取第三方发布页作自动跳转。
fn automatic_backup(origin: &str) -> Option<&'static str> {
    match origin {
        "https://huangguoai.com" => Some("https://huangguoai.ai"),
        "https://huangguoai.ai" => Some("https://huangguoai.com"),
        _ => None,
    }
}

/// 仅网络或临时网关故障可触发备用，不把鉴权失败、业务错误或 TLS 证书拒绝当作“被屏蔽”。
fn temporary_route_error(error: &str) -> bool {
    if let Some(status) = http_error_status(error) {
        return matches!(status, 408 | 502 | 503 | 504 | 520..=524);
    }
    let msg = error.to_ascii_lowercase();
    if msg.contains("certificate")
        || msg.contains("tls")
        || msg.contains("ssl")
        || msg.contains("authorization")
        || msg.contains("401")
        || msg.contains("403")
        || msg.contains("forbidden")
        || error.contains("禁止")
        || error.contains("拒绝")
    {
        return false;
    }
    ["dns", "connect", "connection", "network", "timeout", "timed out", "tunnel"]
        .iter().any(|key| msg.contains(key))
        || error.contains("解析失败")
        || error.contains("解析超时")
        || error.contains("网络超时")
        || error.contains("连接超时")
}

fn may_recover_with_backup(
    origin: &str, method: &str, path: &str, has_body: bool, has_token: bool, error: &str,
) -> bool {
    // 只读播放地址查询虽然使用 POST，但无副作用（项目原有重试策略已明确）。
    let safe_read = (method.eq_ignore_ascii_case("GET") && !has_body)
        || (method.eq_ignore_ascii_case("POST") && edge_operation(path) == "play");
    let incompatible_feed = edge_operation(path) == "feed"
        && method.eq_ignore_ascii_case("GET")
        && error.starts_with("内容接口返回");
    automatic_backup(origin).is_some()
        && safe_read && !has_token
        && (temporary_route_error(error) || incompatible_feed)
}

/// 默认自动恢复：匿名读取或只读的播放地址查询失败时，尝试另一条内置固定线路。
/// 不更改用户选择的线路或本地登录会话，不携带账号、Bearer token 或写入请求。
pub async fn edge(
    origin: &str,
    method: &str,
    path: &str,
    body: Option<Value>,
    token: Option<&str>,
) -> Result<Value, String> {
    let first = edge_once(origin, method, path, body.clone(), token, true, None).await;
    match first {
        Err(first_error) if may_recover_with_backup(origin, method, path, body.is_some(), token.is_some(), &first_error) => {
            let backup = automatic_backup(origin).expect("经过备用线路白名单验证");
            crate::playback_log::record("api.trusted_fallback", json!({
                "operation": edge_operation(path), "from": origin, "to": backup,
            }));
            match edge_once(backup, method, path, body, None, true, None).await {
                Ok(value) => {
                    crate::playback_log::record("api.trusted_fallback_success", json!({
                        "operation": edge_operation(path), "source": backup,
                    }));
                    Ok(value)
                }
                Err(backup_error) => {
                    crate::playback_log::record("api.trusted_fallback_failed", json!({
                        "operation": edge_operation(path),
                        "primary_status": http_error_status(&first_error),
                        "backup_status": http_error_status(&backup_error),
                        "backup_error_kind": crate::playback_log::error_kind(&backup_error),
                    }));
                    // 避免把备用站点的未知错误覆盖到用户实际选择的线路。
                    Err(first_error)
                }
            }
        }
        other => other,
    }
}

/// 业务请求：单条线路走边缘信封。
async fn edge_once(
    origin: &str,
    method: &str,
    path: &str,
    body: Option<Value>,
    token: Option<&str>,
    allow_retry: bool,
    diagnostic: Option<&Mutex<Value>>,
) -> Result<Value, String> {
    let cur = normalize_base(origin).ok_or("业务请求必须使用安全 HTTPS 线路")?;
    if network_policy::strict_security()
        && (token.is_some()
            || matches!(
                path,
                "/api/account/loginByPassword" | "/api/account/registerByPassword"
            ))
        && !trusted_auth_origin(&cur)
    {
        return Err("镜像线路仅供游客浏览；登录/注册请切换到固定线路".into());
    }
    let (encoded, ctype) = match &body {
        Some(v) => (
            B64.encode(serde_json::to_vec(v).map_err(|e| e.to_string())?),
            "application/json",
        ),
        None => (String::new(), ""),
    };
    let envelope = json!({
        "method": method,
        "path": path,
        "body": encoded,
        "content_type": ctype,
    });

    let url = format!("{cur}/api/edge/request");
    let referer = format!("{cur}/");
    let token = token.map(|s| s.to_string());

    static NEXT_EDGE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let request = NEXT_EDGE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let line_host = url::Url::parse(&cur)
        .unwrap()
        .host_str()
        .unwrap()
        .to_string();
    let operation = edge_operation(path);
    let mut attempt = 0;
    let guest_get = method.eq_ignore_ascii_case("GET") && body.is_none() && token.is_none();
    // 取播放地址的 POST 没有副作用，和 GET 一样允许重试
    let can_retry = allow_retry && ((method.eq_ignore_ascii_case("GET") && body.is_none())
        || (method.eq_ignore_ascii_case("POST") && operation == "play"));
    let value = with_retry(can_retry, || {
        attempt += 1;
        let attempt = attempt;
        let envelope = envelope.clone();
        let token = token.clone();
        let url = url.clone();
        let referer = referer.clone();
        let line_host = line_host.clone();
        async move {
            let started = Instant::now();
            crate::playback_log::record("api.begin", json!({"request": request, "attempt": attempt, "operation": operation, "method": method, "line_host": line_host}));
            let mut status_code = None;
            let mut headers_ms = None;
            let mut target_host = line_host.clone();
            let mut response_kind = "unknown";
            let mut gateway = "other";
            let result = async {
                let mut next = network_policy::parse_target(&url)?;
                for hop in 0..=5 {
                    let (target, client) = client_for(next.as_str()).await?;
                    target_host = target.host_str().unwrap_or("").to_string();
                    let mut req = client
                        .post(target.clone())
                        .header("Accept", "application/json")
                        .header("Referer", &referer)
                        .json(&envelope);
                    if let Some(t) = token.as_deref() {
                        if !t.is_empty() {
                            req = req.header("Authorization", format!("Bearer {t}"));
                        }
                    }
                    let resp = req.send().await.map_err(|e| err_text(&e))?;
                    let status = resp.status();
                    status_code = Some(status.as_u16());
                    if let Some(meta) = diagnostic {
                        if let Ok(mut value) = meta.lock() {
                            *value = diagnostic_response(resp.headers(), status.as_u16());
                        }
                    }
                    headers_ms = Some(started.elapsed().as_millis() as u64);
                    let ctype = resp.headers().get(reqwest::header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("");
                    response_kind = if ctype.contains("json") { "json" } else if ctype.contains("html") { "html" } else { "other" };
                    gateway = if resp.headers().contains_key("x-amz-cf-id") { "cloudfront" } else if resp.headers().contains_key("cf-ray") { "cloudflare" } else { "other" };
                    // 逐跳重建并检查客户端；不自动把信封 POST 改成无信封 GET。
                    if status.is_redirection() {
                        if hop == 5 {
                            return Err("重定向次数超过限制".into());
                        }
                        let location = resp
                            .headers()
                            .get(reqwest::header::LOCATION)
                            .and_then(|h| h.to_str().ok())
                            .ok_or("重定向缺少有效 Location")?;
                        next = edge_redirect_target(&target, status.as_u16(), location, guest_get)?;
                        continue;
                    }
                    if status.as_u16() == 401 || status.as_u16() == 403 {
                        // Preserve the actual status for the read-only diagnostic;
                        // normal business calls retain the legacy session handling.
                        return Err(if allow_retry { "401".to_string() } else { format!("HTTP {status}") });
                    }
                    if !status.is_success() {
                        // Never log raw upstream HTML or response bodies, which may contain user data.
                        return Err(format!("HTTP {status}"));
                    }
                    if response_kind == "html" {
                        return Err("内容接口返回网页而非 JSON".into());
                    }
                    let text = resp.text().await.map_err(|e| err_text(&e))?;
                    if text.trim().is_empty() {
                        return Ok(Value::Null);
                    }
                    return serde_json::from_str::<Value>(&text)
                        .map_err(|_| "内容接口返回无效 JSON".to_string());
                }
                Err("重定向次数超过限制".into())
            }.await;
            crate::playback_log::record("api.end", json!({
                "request": request, "attempt": attempt, "operation": operation, "method": method,
                "line_host": line_host, "target_host": target_host, "status": status_code,
                "headers_ms": headers_ms, "elapsed_ms": started.elapsed().as_millis() as u64,
                "response_kind": response_kind, "gateway": gateway,
                "business_status": result.as_ref().ok().and_then(|v: &Value| v.get("status")).and_then(Value::as_i64),
                "error_kind": result.as_ref().err().map(|e: &String| crate::playback_log::error_kind(e)),
                "retryable": can_retry && result.as_ref().err().is_some_and(|e| retryable_error(e)),
            }));
            result
        }
    })
    .await?;

    if value.is_null() {
        if operation == "feed" {
            return Err("内容接口返回空响应，无法读取首页".into());
        }
        return Ok(Value::Null);
    }

    if value.get("status").and_then(|v| v.as_i64()) == Some(1) {
        let data = value.get("data").cloned().unwrap_or(Value::Null);
        if operation == "feed" && !valid_feed(&data) {
            return Err("内容接口返回不兼容的首页数据格式".into());
        }
        Ok(data)
    } else {
        Err(value
            .get("msg")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("接口返回失败")
            .to_string())
    }
}

/// 拉原始字节（图片 / m3u8 / 分片）。
pub async fn fetch_bytes(url: &str) -> Result<(Option<String>, Vec<u8>), String> {
    static NEXT_REQUEST: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let request = NEXT_REQUEST.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let resource = crate::playback_log::resource_id(url);
    let mut attempt = 0;
    let url = url.to_string();
    let referer = format!("{}/", base());
    with_retry(true, || {
        attempt += 1;
        let attempt = attempt;
        let resource = resource.clone();
        let url = url.clone();
        let referer = referer.clone();
        async move {
            let started = Instant::now();
            crate::playback_log::record("network.begin", json!({"request": request, "resource": resource, "attempt": attempt}));
            let mut status = None;
            let mut headers_ms = None;
            let result = async {
                let resp = get(&url, Duration::from_secs(20), None, Some(&referer)).await?;
                status = Some(resp.status().as_u16());
                headers_ms = Some(started.elapsed().as_millis() as u64);
                if !resp.status().is_success() {
                    return Err(format!("HTTP {}", resp.status()));
                }
                let ctype = resp
                    .headers()
                    .get(reqwest::header::CONTENT_TYPE)
                    .and_then(|v| v.to_str().ok())
                    .map(|s| s.to_string());
                let bytes = resp.bytes().await.map_err(|e| err_text(&e))?.to_vec();
                Ok((ctype, bytes))
            }.await;
            crate::playback_log::record("network.end", json!({"request": request, "resource": resource, "attempt": attempt, "status": status, "headers_ms": headers_ms, "elapsed_ms": started.elapsed().as_millis() as u64, "bytes": result.as_ref().ok().map(|(_, bytes)| bytes.len()), "error_kind": result.as_ref().err().map(|e: &String| crate::playback_log::error_kind(e))}));
            result
        }
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_response_classifies_without_exporting_raw_headers() {
        use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("Text/HTML; Charset=UTF-8"));
        headers.insert("cf-ray", HeaderValue::from_static("private-ray"));
        headers.insert("set-cookie", HeaderValue::from_static("secret=value"));
        let forwarded = diagnostic_response(&headers, 503);
        assert_eq!(forwarded["gateway"], "cloudflare");
        assert_eq!(forwarded["format"], "html");
        assert_eq!(forwarded["cloudflare_generated_error"], false);
        headers.insert("cf-error-type", HeaderValue::from_static("1102"));
        headers.insert("cf-error-origin", HeaderValue::from_static("internal-detail"));
        let generated = diagnostic_response(&headers, 503);
        assert_eq!(generated["cloudflare_generated_error"], true);
        assert_eq!(generated["cloudflare_error_type"], "1102");
        let exported = generated.to_string();
        for private in ["private-ray", "secret=value", "internal-detail"] {
            assert!(!exported.contains(private));
        }
        headers.insert("cf-error-type", HeaderValue::from_static("unexpected-private-value"));
        assert!(diagnostic_response(&headers, 503)["cloudflare_error_type"].is_null());
    }

    #[tokio::test]
    async fn unavailable_origin_is_not_retried_indefinitely() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let calls = AtomicUsize::new(0);
        let result = with_retry(true, || async {
            calls.fetch_add(1, Ordering::SeqCst);
            Err::<(), _>("HTTP 503 Service Unavailable".to_string())
        }).await;
        assert!(result.is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn feed_shape_requires_items_array_and_fallback_respects_credentials() {
        assert!(valid_feed(&json!({"items":[]})));
        assert!(!valid_feed(&Value::Null));
        assert!(!valid_feed(&json!({"items":{}})));
        assert!(!valid_feed(&json!({"other":[]})));
        let path = "/api/videos?page=1&size=1";
        let error = "内容接口返回不兼容的首页数据格式";
        assert!(may_recover_with_backup("https://huangguoai.com", "GET", path, false, false, error));
        assert!(!may_recover_with_backup("https://huangguoai.com", "GET", path, false, true, error));
        assert!(!may_recover_with_backup("https://huangguoai.com", "POST", path, true, false, error));
        assert!(!may_recover_with_backup("https://untrusted.invalid", "GET", path, false, false, error));
    }

    #[test]
    fn status_parsing_accepts_colon_and_reason_phrase() {
        assert_eq!(http_error_status("HTTP 503: gateway"), Some(503));
        assert_eq!(http_error_status("HTTP 503 Service Unavailable"), Some(503));
        assert_eq!(http_error_status("HTTP 5030"), None);
        assert!(!retryable_error("HTTP 403: denied"));
        assert!(!retryable_error("HTTP 404: missing"));
        assert!(retryable_error("HTTP 503: unavailable"));
        assert!(!retryable_error("内容接口返回网页而非 JSON"));
        assert!(!retryable_error("内容接口返回无效 JSON"));
    }

    #[tokio::test]
    async fn terminal_errors_do_not_retry_but_transient_failures_can_recover() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        for error in [
            "HTTP 404 Not Found: <html>nginx</html>",
            "HTTP 403 Forbidden",
            "HTTP 429 Too Many Requests",
            "401",
            "禁止访问内网",
            "重定向次数超过限制",
        ] {
            let calls = AtomicUsize::new(0);
            assert!(with_retry(true, || async {
                calls.fetch_add(1, Ordering::SeqCst);
                Err::<(), _>(error.to_string())
            })
            .await
            .is_err());
            assert_eq!(calls.load(Ordering::SeqCst), 1, "{error}");
        }
        for error in ["HTTP 503 Service Unavailable", "请求超时"] {
            let calls = AtomicUsize::new(0);
            let value = with_retry(true, || {
                let attempt = calls.fetch_add(1, Ordering::SeqCst);
                async move {
                    if attempt == 0 {
                        Err(error.to_string())
                    } else {
                        Ok(42)
                    }
                }
            })
            .await
            .unwrap();
            assert_eq!(value, 42);
            assert_eq!(calls.load(Ordering::SeqCst), 2);
        }
    }

    #[test]
    fn api_operation_labels_exclude_user_input() {
        for (path, expected) in [
            ("/api/videos?page=1&size=10", "feed"),
            ("/api/videos/category/private?x=secret", "category"),
            ("/api/videos/42/play?ep=1", "play"),
            ("/api/videos/42/episodes/1/playback", "play"),
            ("/api/search?q=private-search", "search"),
            ("/api/account/loginByPassword", "account"),
            ("/unknown/private", "other"),
        ] {
            assert_eq!(edge_operation(path), expected);
        }
    }

    #[tokio::test]
    async fn non_idempotent_operations_are_attempted_once() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        for (retry, expected) in [(false, 1), (true, 3)] {
            let calls = AtomicUsize::new(0);
            let result = with_retry(retry, || async {
                calls.fetch_add(1, Ordering::SeqCst);
                Err::<(), _>("rejected".to_string())
            })
            .await;
            assert!(result.is_err());
            assert_eq!(calls.load(Ordering::SeqCst), expected);
        }
    }

    #[test]
    fn dns_diagnostics_only_expose_counts_and_fake_ip_presence() {
        let addrs: Vec<SocketAddr> = ["10.0.0.1:443", "198.18.0.2:443", "[::ffff:198.18.0.3]:443"]
            .into_iter()
            .map(|raw| raw.parse().unwrap())
            .collect();
        let summary = dns_summary(addrs);
        assert_eq!(summary, json!({"count": 3, "has_fake_ip": true}));
        assert!(!summary.to_string().contains("10.0.0.1"));
        assert!(!summary.to_string().contains("198.18"));
        assert_eq!(
            dns_summary(Vec::<SocketAddr>::new()),
            json!({"count":0,"has_fake_ip":false})
        );
    }

    #[test]
    fn pinned_clients_expire_without_extending_the_dns_ttl() {
        let now = Instant::now();
        let client = Arc::new(reqwest::Client::new());
        let mut cache = ClientCache::new();
        cache.insert(
            "true:example.com:443".into(),
            (now + CLIENT_TTL, client.clone()),
        );
        assert!(Arc::ptr_eq(
            &cached_client(
                &cache,
                "true:example.com:443",
                now + Duration::from_secs(29)
            )
            .unwrap(),
            &client
        ));
        assert!(cached_client(&cache, "true:example.com:443", now + CLIENT_TTL).is_none());
        assert!(cached_client(&cache, "false:example.com:443", now).is_none());
        assert!(cached_client(&cache, "true:example.com:8443", now).is_none());
    }

    #[test]
    fn edge_redirects_only_replay_anonymous_reads() {
        let current = url::Url::parse("https://huangguoai.ai/api/edge/request").unwrap();
        assert_eq!(
            edge_redirect_target(
                &current,
                301,
                "https://huangguoai.com/api/edge/request",
                true
            )
            .unwrap()
            .host_str(),
            Some("huangguoai.com")
        );
        assert!(edge_redirect_target(&current, 302, "/next", true).is_ok());
        for status in [301, 302, 303, 307, 308] {
            assert!(edge_redirect_target(&current, status, "/next", false).is_err());
        }
        for status in [307, 308] {
            assert!(edge_redirect_target(&current, status, "/next", true).is_err());
        }
        assert!(edge_redirect_target(&current, 301, "http://example.com/", true).is_err());
        assert!(edge_redirect_target(&current, 301, "https://127.0.0.1/", true).is_err());
    }

    #[test]
    fn parses_explicit_macos_system_proxy_without_pac() {
        let enabled = r#"<dictionary> {
            HTTPEnable : 1
            HTTPProxy : 127.0.0.1
            HTTPPort : 7890
            HTTPSEnable : 1
            HTTPSProxy : 127.0.0.1
            HTTPSPort : 7897
            ProxyAutoConfigEnable : 0
        }"#;
        assert_eq!(parse_macos_proxy(enabled).as_deref(), Some("http://127.0.0.1:7897"));
        assert_eq!(parse_macos_proxy("ProxyAutoConfigEnable : 1\nProxyAutoConfigURLString : https://config.example/pac"), None);
        assert_eq!(parse_macos_proxy("HTTPSEnable : 0\nHTTPSProxy : 127.0.0.1\nHTTPSPort : 7897"), None);
        assert_eq!(parse_macos_proxy("HTTPSEnable : 1\nHTTPSProxy : user:secret@bad.example\nHTTPSPort : 7897"), None);
    }

    #[test]
    fn secure_dns_rejects_private_ip_and_mixed_responses() {
        let valid = json!({"Status":0,"Answer":[{"type":1,"data":"1.2.3.4"}]});
        assert_eq!(parse_doh_answers(&valid, 1, 443).unwrap(),
            vec!["1.2.3.4:443".parse::<SocketAddr>().unwrap()]);
        let bad = json!({"Status":0,"Answer":[
            {"type":1,"data":"1.2.3.4"}, {"type":1,"data":"127.0.0.1"}
        ]});
        assert!(parse_doh_answers(&bad, 1, 443).is_err());
        let fake = json!({"Status":0,"Answer":[{"type":1,"data":"198.18.0.2"}]});
        assert!(parse_doh_answers(&fake, 1, 443).is_err());
        let invalid6 = json!({"Status":0,"Answer":[{"type":28,"data":"::1"}]});
        assert!(parse_doh_answers(&invalid6, 28, 443).is_err());
        assert!(parse_doh_answers(&json!({"Status":3}), 1, 443).is_err());
        assert!(local_dns_failure("DNS 解析超时"));
        assert!(!local_dns_failure("禁止访问普通内网、回环或保留地址"));
        assert!(!local_dns_failure("TLS certificate invalid"));
    }

    #[test]
    fn auto_recovery_only_allows_anonymous_read_on_trusted_fixed_sources() {
        assert_eq!(automatic_backup("https://huangguoai.ai"), Some("https://huangguoai.com"));
        assert_eq!(automatic_backup("https://huangguoai.com"), Some("https://huangguoai.ai"));
        assert_eq!(automatic_backup("https://unknown.example"), None);
        let read_path = "/api/videos?page=1&size=1";
        let playback = "/api/videos/123/episodes/1/playback";
        assert!(may_recover_with_backup("https://huangguoai.ai", "GET", read_path, false, false, "DNS 解析失败"));
        assert!(may_recover_with_backup("https://huangguoai.ai", "get", read_path, false, false, "HTTP 503 Service Unavailable"));
        assert!(may_recover_with_backup("https://huangguoai.ai", "POST", playback, true, false, "DNS 解析失败"));
        assert!(!may_recover_with_backup("https://huangguoai.ai", "POST", playback, true, true, "DNS 解析失败"));
        assert!(!may_recover_with_backup("https://huangguoai.ai", "GET", read_path, false, true, "DNS 解析失败"));
        assert!(!may_recover_with_backup("https://huangguoai.ai", "POST", "/api/videos/123/comments", true, false, "DNS 解析失败"));
        assert!(!may_recover_with_backup("https://huangguoai.ai", "POST", "/api/account/loginByPassword", true, false, "DNS 解析失败"));
        assert!(!may_recover_with_backup("https://huangguoai.ai", "GET", read_path, true, false, "DNS 解析失败"));
        assert!(!may_recover_with_backup("https://unknown.example", "GET", read_path, false, false, "DNS 解析失败"));
        for err in ["401", "HTTP 403 Forbidden", "HTTP 404 Not Found", "HTTP 429 Too Many Requests",
                    "certificate verify failed", "TLS handshake error", "禁止访问内网地址",
                    "业务接口返回失败"] {
            assert!(!may_recover_with_backup("https://huangguoai.ai", "GET", read_path, false, false, err), "{err}");
        }
    }

    #[test]
    fn validates_user_proxy_without_secrets() {
        assert!(validate_proxy("custom", "http://127.0.0.1:7890").is_ok());
        assert!(validate_proxy("custom", "https://proxy.example.com:443").is_ok());
        assert_eq!(validate_proxy("direct", "ignored").unwrap().url, "");
        for raw in [
            "socks5h://127.0.0.1:1080",
            "http://user:password@127.0.0.1:7890",
            "http://hg.localhost:7890",
            "http://127.0.0.1:7890/path",
            "http://127.0.0.1:7890/?token=secret",
        ] {
            assert!(validate_proxy("custom", raw).is_err(), "{raw}");
        }
    }

    #[test]
    fn proxy_credentials_and_query_are_redacted() {
        assert_eq!(
            redact_proxy("http://alice:p%40ss@127.0.0.1:7897/path?token=secret#hidden"),
            "http://127.0.0.1:7897"
        );
        assert_eq!(
            redact_proxy("socks5://alice:secret@[::1]:1080"),
            "socks5://[::1]:1080"
        );
        for raw in ["alice:secret@127.0.0.1:7897", "not a url secret", ""] {
            assert_eq!(redact_proxy(raw), "[已配置，原始值已隐藏]");
        }
    }

    #[test]
    fn base_requires_https_origin_and_auth_trust_is_static() {
        assert_eq!(
            normalize_base("https://HUANGGUOAI.COM:443/"),
            Some(DEFAULT_BASE.into())
        );
        for raw in [
            "http://huangguoai.com",
            "https://user:pass@huangguoai.com",
            "https://huangguoai.com/path",
            "https://huangguoai.com/?secret=1",
            "https://huangguoai.com/#fragment",
            "https://192.168.1.1",
        ] {
            assert_eq!(normalize_base(raw), None, "{raw}");
        }
        assert!(trusted_auth_origin(DEFAULT_BASE));
        assert!(trusted_auth_origin("https://huangguoai.ai"));
        assert!(!trusted_auth_origin("https://mirror.example"));
        assert!(!trusted_auth_origin("https://huangguoai.com.evil.example"));
    }

    #[tokio::test]
    async fn untrusted_origins_reject_credentials_before_network() {
        for (path, body, token) in [
            (
                "/api/account/loginByPassword",
                Some(json!({"password":"secret"})),
                None,
            ),
            (
                "/api/account/registerByPassword",
                Some(json!({"password":"secret"})),
                None,
            ),
            ("/api/videos", None, Some("secret-token")),
        ] {
            let err = edge("https://mirror.invalid", "POST", path, body, token)
                .await
                .unwrap_err();
            assert!(err.contains("镜像线路仅供游客浏览"));
        }
    }

    #[tokio::test]
    async fn private_get_is_rejected_before_network() {
        let err = get(
            "http://127.0.0.1:1/private",
            Duration::from_secs(1),
            None,
            None,
        )
        .await
        .unwrap_err();
        assert!(err.contains("禁止访问"));
    }

    /// 手动跑：`cargo test --lib -- --ignored net::tests::diag -- --nocapture`
    /// 关掉 TUN、只留系统代理时跑一遍，能直接看出客户端有没有走代理。
    #[tokio::test]
    #[ignore = "联网测试：请求上游做网络自检，手动用 --ignored 运行"]
    async fn diag() {
        println!(
            "{}",
            serde_json::to_string_pretty(&diagnose().await).expect("json")
        );
    }
}
