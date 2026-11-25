#!/usr/bin/env python3
"""Build-time integration of pinned, publicly available noncommercial source.
No service credentials, raw response bodies or new mirrors are distributed here.
"""
import json
import pathlib
import sys

base = pathlib.Path(__file__).resolve().parents[1]
up = pathlib.Path(sys.argv[1]).resolve()
src = base / 'src-tauri' / 'src'
usrc = up / 'src-tauri' / 'src'
tauri = src.parent
notice = 'Required Notice: Copyright (c) 2026 HGhubX contributors (https://github.com/zxsos/HGhubX)'

def read(p): return p.read_text(encoding='utf-8')
def write(p, s): p.write_text(s, encoding='utf-8')
def once(s, old, new):
    if s.count(old) != 1:
        raise RuntimeError('Pinned source anchor mismatch: ' + old[:70])
    return s.replace(old, new, 1)
def part(s, start, stop, after=0):
    a = s.index(start, after)
    b = s.index(stop, a)
    return a, b, s[a:b]

license_src = read(up / 'LICENSE')
license_base = read(base / 'LICENSE')
if notice not in license_src or 'PolyForm Noncommercial License 1.0.0' not in license_src or 'PolyForm Noncommercial License 1.0.0' not in license_base:
    raise RuntimeError('License mismatch')
license_base = once(license_base, '\n## Acceptance\n', '\n'+notice+'\n\n## Acceptance\n')
resources = tauri / 'resources'
resources.mkdir(exist_ok=True)
write(resources / 'LICENSE-HGhubX.txt', license_src)
write(resources / 'LICENSE-HGhub.txt', license_base)
write(resources / 'NOTICE-HGhubX.txt', notice+'\nSource: https://github.com/zxsos/HGhubX/tree/6370c4412f8c7179018e1dcbb84ffb084c70b796\nNoncommercial use only. Respect content permissions and applicable law. No bulk downloads or redistribution of content.\n')
cfg_path = tauri / 'tauri.macos.conf.json'
cfg = json.loads(read(cfg_path))
cfg['bundle']['resources'] += ['resources/LICENSE-HGhubX.txt','resources/LICENSE-HGhub.txt','resources/NOTICE-HGhubX.txt']
write(cfg_path, json.dumps(cfg,ensure_ascii=False,indent=2)+'\n')
write(src / 'edge_crypto.rs', '// '+notice+'\n'+read(usrc / 'edge_crypto.rs'))
p = tauri / 'Cargo.toml'
write(p, once(read(p), 'url = "2"\n', 'url = "2"\nring = "=0.17.14"\n'))
p = tauri / 'Cargo.lock'
s = read(p)
a,b,entry = part(s, 'name = "huangguo-client"', '\n[[package]]')
if 'name = "ring"\nversion = "0.17.14"' not in s: raise RuntimeError('ring version mismatch')
write(p, s[:a]+once(entry,' "reqwest 0.12.28",\n',' "reqwest 0.12.28",\n "ring",\n')+s[b:])
p = src / 'lib.rs'
write(p, once(read(p),'mod api;\n','mod api;\nmod edge_crypto;\n'))

p = src / 'net.rs'
n = read(p)
u = read(usrc / 'net' / 'edge.rs')
# Preserve and strengthen the existing credential-origin guard.
n = once(n, 'if network_policy::strict_security()\n        && (token.is_some()', 'if (token.is_some()')
a,b,_ = part(n, '    let url = format!("{cur}/api/edge/request");','    let referer = format!("{cur}/");',n.index('async fn edge_once('))
_,_,v = part(u,'    let scope = crate::edge_crypto::scope_for(method, path);','    let referer = format!("{cur}/");')
n=n[:a]+v+n[b:]
a,b,_ = part(n,'        let envelope = envelope.clone();','        async move {',n.index('async fn edge_once('))
_,_,v = part(u,'        let cur = cur.clone();','        async move {',u.index('pub async fn edge('))
n=n[:a]+v+n[b:]
a,b,_ = part(n,'            let result = async {','            crate::playback_log::record("api.end"',n.index('async fn edge_once('))
_,_,logic = part(u,'            let result = async {','                status_code = Some(reply.status);',u.index('pub async fn edge('))
# Scoped requests cannot redirect; only anonymous legacy reads may use the old redirect policy.
logic=once(logic,'None, true, guest_get, started).await?','None, true, false, started, diagnostic).await?')
logic=once(logic,'None, false, guest_get, started).await }','None, false, false, started, diagnostic).await }')
logic=once(logic,'None => post_edge_json(&legacy_url, &referer, &envelope, token.as_deref(), false, guest_get, started).await?,','None if guest_get => post_edge_json(&legacy_url, &referer, &envelope, None, false, true, started, diagnostic).await?,\n                    None => return Err("HTTP 404: 当前服务不支持此接口".into()),')
n=n[:a]+logic+'''                status_code = Some(reply.status);
                headers_ms = Some(reply.headers_ms);
                target_host = reply.target_host;
                gateway = reply.gateway;
                let ctype = reply.ctype.to_ascii_lowercase();
                response_kind = if ctype.contains("json") { "json" } else if ctype.contains("html") { "html" } else { "other" };
                if reply.status == 401 {
                    return Err(if allow_retry { "401".to_string() } else { "HTTP 401".to_string() });
                }
                if !(200..300).contains(&reply.status) {
                    return Err(format!("HTTP {}", reply.status));
                }
                if response_kind == "html" {
                    return Err("内容接口返回网页而非 JSON".into());
                }
                if reply.text.trim().is_empty() { return Ok(Value::Null); }
                serde_json::from_str::<Value>(&reply.text)
                    .map_err(|_| "内容接口返回无效 JSON".to_string())
            }.await;
'''+n[b:]
_,_,helper=part(u,'async fn post_edge_json(','/// 拉原始字节')
helper=once(helper,'    started: Instant,\n)','    started: Instant,\n    diagnostic: Option<&Mutex<Value>>,\n)')
helper=once(helper,'        let status = resp.status();\n','''        let status = resp.status();
        if let Some(meta) = diagnostic {
            if let Ok(mut value) = meta.lock() {
                *value = diagnostic_response(resp.headers(), status.as_u16());
            }
        }
''')
a=n.index('/// 拉原始字节',n.index('async fn edge_once('))
write(p,n[:a]+helper+n[a:])
# Preserve access denials; only unsupported playback endpoints are compatibility signals.
p=src/'api.rs'
write(p,once(read(p),'''    if let Ok(info) = &primary {
        if has_play_url(info) {
            return primary;
        }
    }
    match state
''','''    if !matches!(&primary, Err(error) if error.starts_with("HTTP 404") || error.starts_with("HTTP 405")) {
        return primary;
    }
    match state
'''))
print('Pinned compatibility overlay prepared. Native compilation and tests required.')
