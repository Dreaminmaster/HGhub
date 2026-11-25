import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { IChevR, IClose, ILink } from "../icons";
import { pluginCommand, pluginError } from "./client";
import { useHost } from "./HostContext";
import { registerBackHandler } from "../back";

type Mode = "system" | "custom" | "off" | "warp";
type WarpStatus = { available: boolean; registered: boolean; running: boolean; active: boolean; transport?: string };
type Config = { mode: Mode; url: string };
type Probe = { ok: boolean; ms: number; status?: number; error?: string };
const labels: Record<Mode, string> = { system: "跟随系统", custom: "自定义", off: "直连", warp: "内置 WARP" };

export default function ProxySettings() {
  const [saved, setSaved] = useState<Config | null>(null), [open, setOpen] = useState(false);
  useEffect(() => { let live = true; void pluginCommand<Config>("proxy.get").then(v => { if(live)setSaved(v); }).catch(()=>{}); return()=>{live=false;}; }, []);
  return <><button className="hg-proxy-setting" onClick={()=>setOpen(true)} aria-haspopup="dialog"><span className="hg-proxy-icon"><ILink size={17}/></span><span><b>代理访问</b><small>{saved ? labels[saved.mode] : "设置应用使用的代理"}</small></span><IChevR size={16}/></button>
    {open&&<ProxyDialog onClose={()=>setOpen(false)} onSaved={v=>{setSaved(v);setOpen(false);}}/>}</>;
}

function ProxyDialog({ onClose, onSaved }: { onClose: () => void; onSaved: (v: Config) => void }) {
  const { sources } = useHost(), ref = useRef<HTMLDialogElement>(null), pending = useRef(false);
  const [mode, setMode] = useState<Mode>("system"), [url, setUrl] = useState("");
  const [ready, setReady] = useState(false), [busy, setBusy] = useState(""), [error, setError] = useState("");
  const [probe, setProbe] = useState<Probe | null>(null), [visible, setVisible] = useState(false);
  const [warp, setWarp] = useState<WarpStatus | null>(null), [consent, setConsent] = useState(false);
  useEffect(() => {
    let live = true;
    const check = () => { void invoke<WarpStatus>("warp_status").then(v => { if(live) setWarp(v); }).catch(()=>{}); };
    check();
    const timer = window.setInterval(check, 4000);
    return () => { live=false; window.clearInterval(timer); };
  }, []);
  const targets = sources.filter(p=>p.endpoint?.url);
  const [target, setTarget] = useState(targets[0]?.endpoint?.url || "");
  const close = () => { if(!pending.current)onClose(); };
  const load = async () => { try { const v = await pluginCommand<Config>("proxy.get");setMode(v.mode);setUrl(v.url);setReady(true);setError(""); } catch(e){setError(pluginError(e));} };
  useEffect(() => { ref.current!.showModal(); void load(); const un = registerBackHandler(()=>{close();return true;}); return()=>{un();}; }, []);
  const run = async (action: "test" | "save") => {
    if(pending.current)return;pending.current=true;setBusy(action);setError("");setProbe(null);
    try {
      if(action==="test") setProbe(await pluginCommand<Probe>("proxy.test",{mode,url:mode==="custom"?url:"",target}));
      else {
        if (mode==="warp") {
          if (!warp?.available) throw new Error("本安装包不支持内置 WARP");
          await invoke("warp_enable", { acceptTerms: consent });
          onSaved(await pluginCommand<Config>("proxy.get"));
        } else {
          const v=await pluginCommand<Config>("proxy.set",{mode,url:mode==="custom"?url:""});
          onSaved(v);
        }
      }
    } catch(e){setError(pluginError(e));} finally {pending.current=false;setBusy("");}
  };
  const disabled = !ready || !!busy;
  return <dialog className="hg-proxy-dialog" ref={ref} aria-labelledby="proxy-title" onCancel={e=>{e.preventDefault();close();}} onClick={e=>{if(e.target===e.currentTarget)close();}}>
    <header><h2 id="proxy-title"><ILink size={20}/>代理访问</h2><button aria-label="关闭代理设置" disabled={!!busy} onClick={close}><IClose size={20}/></button></header>
    <p>仅应用于 HGhub 的插件、图片与视频请求。测试当前设置后，保存即可生效。</p>
    <div className="hg-proxy-modes" aria-label="代理模式">{(["system","custom","off",...(warp?.available || mode==="warp" ? ["warp" as const] : [])] as Mode[]).map(m=><button key={m} disabled={disabled} aria-pressed={mode===m} onClick={()=>{setMode(m);setProbe(null);setError("");}}>{labels[m]}</button>)}</div>
    {mode==="warp" ? <>
      <p>仅 HGhub 内容请求使用 Cloudflare 网络。首次启用需注册独立设备；不保证来源服务可用，不提供指定国家出口。隧道断开时不会自动切回直连。</p>
      <p role="status">{warp?.running ? "本地组件运行中（出口状态需实际测试）" : "尚未连接"}</p>
      {!warp?.registered && <label><input type="checkbox" checked={consent} disabled={disabled} onChange={e=>setConsent(e.target.checked)}/> 我已阅读 Cloudflare WARP 使用条款，同意注册设备及第三方流量转发。</label>}
      <button type="button" disabled={disabled} onClick={()=>void invoke("open_url",{url:"https://www.cloudflare.com/terms/"}).catch(e=>setError(pluginError(e)))}>查看 Cloudflare 条款</button>
    </> : mode==="custom" ? <><label>代理地址<input aria-label="代理地址" type={visible?"text":"password"} autoComplete="off" spellCheck={false} placeholder="http://127.0.0.1:7890" value={url} disabled={disabled} onChange={e=>{setUrl(e.target.value);setProbe(null);}}/></label><button className="hg-proxy-reveal" onClick={()=>setVisible(v=>!v)}>{visible?"隐藏地址":"显示地址"}</button><p>支持 HTTP、HTTPS、SOCKS。手机上的 127.0.0.1 指手机自身；使用电脑代理请填写电脑局域网地址。</p></> : <p>{mode==="system"?"沿用系统固定代理与环境变量。VPN / TUN 由操作系统接管。":"不使用应用层代理和代理环境变量；系统 VPN / TUN 仍由操作系统接管。"}已保存的自定义地址会保留。</p>}
    <label>测试地址<input aria-label="测试地址" list="proxy-test-targets" type="url" placeholder="https://你的片源地址" value={target} disabled={disabled} onChange={e=>{setTarget(e.target.value);setProbe(null);}}/></label>
    <datalist id="proxy-test-targets">{targets.map(p=><option key={p.meta.id} value={p.endpoint!.url}>{p.meta.name}</option>)}</datalist>
    <p>测试仅检查所填地址的 HTTP 连通性，不修改已保存的代理。</p>
    {error&&<p role="alert" className="hg-proxy-error">{error}{!ready&&<button onClick={()=>void load()}>重新读取</button>}</p>}
    <p role="status">{busy==="test"?"正在测试连接…":probe?`${probe.ok?"连接正常":probe.error||"连接失败"} · ${probe.ms} ms`:"保存后，新请求立即使用所选模式。"}</p>
    <footer><button disabled={disabled||!target.trim()||(mode==="warp"&&!warp?.running)} onClick={()=>void run("test")}>测试连接</button><button className="hg-watch-action" disabled={disabled||(mode==="warp"&&(!warp?.available||(!warp.registered&&!consent)))} onClick={()=>void run("save")}>{busy==="save"?"保存中…":"保存代理"}</button></footer>
  </dialog>;
}
