import { useState } from "react";
import { api, APP_VERSION, platform, type DownloadedUpdate } from "../api";
import { checkMaintenance, setMaintenancePreferences, useMaintenance } from "../maintenance";
import { Toggle } from "./ui";
export default function UpdateCenter() {
  const {prefs,result,busy,error,checked}=useMaintenance();
  const [downloading,setDownloading]=useState(false);
  const [downloaded,setDownloaded]=useState<DownloadedUpdate|null>(null);
  const [downloadError,setDownloadError]=useState("");
  const download=async()=>{
    if(!result?.version || !result.digest || downloading)return;
    setDownloading(true);setDownloaded(null);setDownloadError("");
    try{setDownloaded(await api.manualUpdateDownload(prefs.channel,result.version,result.digest));}
    catch(e){setDownloadError((e as Error).message||"下载失败");}
    finally{setDownloading(false);}
  };
  if(platform!=="macos")return <div className="p-3 text-[12px] text-mut">此下载中心目前仅支持 macOS。</div>;
  return <div className="p-3.5 space-y-3">
    <div className="flex justify-between items-center"><div><div className="text-[13px] font-semibold text-on">软件更新</div><div className="text-[11px] text-mut">当前 v{APP_VERSION} · Dreaminmaster/HGhub</div></div>
    <button disabled={busy||downloading} onClick={()=>{void checkMaintenance();}} className="px-3 py-2 rounded-(--r-sm) bg-s3 text-primary text-[12px] disabled:opacity-60">{busy?"检查中…":"检查更新"}</button></div>
    <div className="flex justify-between items-center"><div className="text-[12px] text-on">启动时检查更新</div><Toggle on={prefs.autoCheck} disabled={downloading} onChange={autoCheck=>setMaintenancePreferences({autoCheck})}/></div>
    <div className="flex items-center justify-between gap-2"><div className="text-[12px] text-on">更新渠道</div><select disabled={busy||downloading} className="bg-s3 text-on text-[12px] rounded p-2" value={prefs.channel} onChange={e=>{setDownloaded(null);setMaintenancePreferences({channel:e.target.value==="beta"?"beta":"stable"});void checkMaintenance();}}><option value="stable">稳定版</option><option value="beta">含测试版</option></select></div>
    <div className="text-[11px] text-mut">{checked?"最近检查："+new Date(checked).toLocaleString():"尚未检查"}</div>
    {error&&<p role="alert" className="text-[12px] text-mut">{error}</p>}
    {result?.status==="no_release"&&<p className="text-[12px] text-mut">没有找到此渠道中带有效文件摘要的 macOS 安装包。</p>}
    {result?.status==="current"&&<p className="text-[12px] text-on">当前版本不低于此渠道可下载版本。</p>}
    {result?.status==="available"&&<div className="rounded bg-s3 p-3 space-y-2"><div className="text-[13px] text-on">发现 v{result.version} · {((result.size||0)/1048576).toFixed(1)} MB</div><div className="text-[11px] text-mut whitespace-pre-wrap max-h-36 scroll-y">{result.notes}</div><p className="text-[11px] text-mut">从自己的 GitHub 仓库下载并校验 SHA-256，随后由你在 Finder 中安装。不是自动安装，也不验证独立发布签名。</p><button disabled={downloading||busy} onClick={()=>void download()} className="px-3 py-2 rounded bg-primary text-on-primary text-[12px] disabled:opacity-50">{downloading?"下载并校验中…":"下载安装包"}</button></div>}
    {downloaded&&<div className="rounded bg-s3 p-3 text-[12px] text-on space-y-1"><p>文件完整性校验通过，{downloaded.revealed?"已在 Finder 中定位":"请在下载目录中查看"}。请退出 HGhub，再手动安装。</p><p className="text-[10px] text-mut break-all">{downloaded.path}</p><p className="text-[10px] text-mut break-all">SHA-256: {downloaded.sha256}</p><p className="text-[11px] text-mut">保留 macOS 安全提示，请先确认安装包来源。不要全局关闭系统安全保护。</p></div>}
    {downloadError&&<p role="alert" className="text-[12px] text-mut">{downloadError}</p>}
  </div>;
}
