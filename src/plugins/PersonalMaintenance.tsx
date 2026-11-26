import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useStore } from "../store";
import { useHost } from "./HostContext";
import { platform } from "../environment";

type Prefs = { schema: 1; theme: "light" | "dark"; muteOnStartup: boolean; autoNext: boolean };
export function parsePersonalPrefs(text: string): Prefs {
  if (text.length > 4096) throw new Error("偏好文件过大");
  const p: unknown = JSON.parse(text);
  if (!p || typeof p !== "object" || Array.isArray(p)) throw new Error("偏好格式无效");
  const x = p as Record<string, unknown>;
  if (Object.keys(x).sort().join(",") !== "autoNext,muteOnStartup,schema,theme" ||
      x.schema !== 1 || (x.theme !== "light" && x.theme !== "dark") ||
      typeof x.muteOnStartup !== "boolean" || typeof x.autoNext !== "boolean") throw new Error("偏好格式无效");
  return { schema: 1, theme: x.theme, muteOnStartup: x.muteOnStartup, autoNext: x.autoNext };
}
export default function PersonalMaintenance() {
  const app = useStore(), host = useHost();
  const [text, setText] = useState(""), [preview, setPreview] = useState<Prefs | null>(null);
  const [status, setStatus] = useState(""), [media, setMedia] = useState<{url:string; name:string; audio:boolean} | null>(null);
  const [busy, setBusy] = useState(false);
  const urlRef = useRef<string | null>(null);
  useEffect(() => () => { if (urlRef.current) URL.revokeObjectURL(urlRef.current); }, []);
  const select = (file?: File) => {
    if (urlRef.current) URL.revokeObjectURL(urlRef.current);
    urlRef.current = null; setMedia(null); setStatus("");
    if (!file) return;
    if (!(file.type.startsWith("video/") || file.type.startsWith("audio/") || /\.(mp4|m4v|mov|webm|mp3|m4a|wav|ogg)$/i.test(file.name))) {setStatus("请选择音视频文件");return;}
    const url = URL.createObjectURL(file); urlRef.current = url;
    setMedia({url,name:file.name,audio:file.type.startsWith("audio/") || /\.(mp3|m4a|wav|ogg)$/i.test(file.name)});
  };
  const folder = async (kind: "data" | "tools") => {
    if (busy) return;
    setBusy(true); setStatus("");
    try { await invoke("maintenance_open_folder", { kind }); }
    catch { setStatus("无法打开维护目录，请在 macOS 应用中重试"); }
    finally { setBusy(false); }
  };
  const btn = "rounded-md bg-s3 px-3 py-2 text-xs text-on hover:bg-s1 disabled:opacity-50";
  return <div className="px-3.5 py-3 space-y-3 text-xs text-on">
    <h3 className="font-semibold">本地媒体</h3>
    <p className="text-mut">选择自己的音视频；文件不会上传或加入第三方来源。</p>
    <input aria-label="打开本地媒体" type="file" accept="video/*,audio/*,.mkv,.m4v" onChange={e=>select(e.currentTarget.files?.[0])}/>
    {media && <div className="space-y-2"><p className="break-all">{media.name}</p>{media.audio ? <audio key={media.url} src={media.url} controls preload="metadata" onError={()=>setStatus("系统不支持此媒体编码")}/> : <video key={media.url} src={media.url} controls playsInline preload="metadata" className="w-full max-h-64 bg-black" onError={()=>setStatus("系统不支持此媒体编码")}/>}<button className={btn} onClick={()=>select()}>关闭媒体</button></div>}
    <h3 className="font-semibold">偏好备份</h3>
    <p className="text-mut">仅保存外观、启动静音、自动下一条。不会导出账号、代理、插件或权限。</p>
    <div className="flex flex-wrap gap-2"><button className={btn} onClick={()=>{setText(JSON.stringify({schema:1,theme:app.theme,muteOnStartup:app.muteOnStartup,autoNext:host.prefs.autoNext},null,2));setPreview(null);setStatus("请将下方文本保存到本地");}}>导出偏好</button><button className={btn} onClick={()=>{try{setPreview(parsePersonalPrefs(text));setStatus("请检查预览后确认");}catch{setPreview(null);setStatus("偏好格式无效");}}}>预览导入</button></div>
    <textarea aria-label="偏好备份文本" maxLength={4096} rows={4} value={text} onChange={e=>{setText(e.target.value);setPreview(null);}} className="w-full rounded bg-s1 p-2 font-mono"/>
    {preview && <div className="space-y-2"><p>外观：{preview.theme==="dark"?"深色":"浅色"} · 启动静音：{preview.muteOnStartup?"是":"否"} · 自动下一条：{preview.autoNext?"是":"否"}</p><button className={btn} onClick={()=>{app.setTheme(preview.theme);app.setMuteOnStartup(preview.muteOnStartup);host.setPrefs({autoNext:preview.autoNext});setPreview(null);setStatus("偏好已应用");}}>确认导入</button></div>}
    {platform==="macos" && <><h3 className="font-semibold">完整备份与诊断</h3><p className="text-mut">先打开工具目录。退出应用后再备份；完整系统数据另用 Time Machine。工具不会自动恢复或覆盖。</p><div className="flex flex-wrap gap-2"><button className={btn} disabled={busy} onClick={()=>void folder("data")}>打开应用数据</button><button className={btn} disabled={busy} onClick={()=>void folder("tools")}>打开维护工具</button></div></>}
    <h3 className="font-semibold">应用更新</h3><p className="text-mut">本独立版本尚无正式更新渠道。不会把旧版 HGhub 或上游发行包识别成可安装更新。</p>
    {status && <p role="status" className="break-words text-mut">{status}</p>}
  </div>;
}
