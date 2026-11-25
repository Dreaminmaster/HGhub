import { useEffect, useState } from "react";
import { api, APP_VERSION, platform, type UpdateProgress, type UpstreamReport } from "../api";
import { checkMaintenance, downloadMaintenance, setMaintenancePreferences, useMaintenance } from "../maintenance";
import { Toggle } from "./ui";
import UpdateArchive from "./UpdateArchive";
import MaintenanceBackup from "./MaintenanceBackup";
export default function UpdateCenter() {
  const {prefs,result,busy,error,checked,downloading,downloaded,downloadError}=useMaintenance();
  const [progress,setProgress]=useState<UpdateProgress|null>(null);
  const [upstreams,setUpstreams]=useState<UpstreamReport|null>(null);
  const [tracking,setTracking]=useState(false);
  const [trackError,setTrackError]=useState("");
  useEffect(()=>{
    if(!downloading){setProgress(null);return;}
    let active=true;
    let pending=false;
    const refresh=()=>{if(pending)return;pending=true;void api.manualUpdateProgress().then(p=>{if(active)setProgress(p);}).catch(()=>{}).finally(()=>{pending=false;});};
    refresh();const id=window.setInterval(refresh,750);
    return ()=>{active=false;window.clearInterval(id);};
  },[downloading]);
  const track=async()=>{
    if(tracking)return;setTracking(true);setTrackError("");
    try{setUpstreams(await api.maintenanceUpstreams());}
    catch{setTrackError("上游版本查询失败，请稍后重试。");}
    finally{setTracking(false);}
  };
  if(platform!=="macos")return <div className="p-3 text-[12px] text-mut">此下载中心目前仅支持 macOS。</div>;
  return <div className="p-3.5 space-y-3">
    <div className="flex justify-between items-center"><div><div className="text-[13px] font-semibold text-on">软件更新</div><div className="text-[11px] text-mut">当前 v{APP_VERSION} · Dreaminmaster/HGhub</div></div>
    <button disabled={busy||downloading} onClick={()=>{void checkMaintenance();}} className="px-3 py-2 rounded-(--r-sm) bg-s3 text-primary text-[12px] disabled:opacity-60">{busy?"检查中…":"检查更新"}</button></div>
    <div className="flex justify-between items-center"><div className="text-[12px] text-on">启动时检查更新</div><Toggle on={prefs.autoCheck} disabled={downloading} onChange={autoCheck=>setMaintenancePreferences({autoCheck})}/></div>
    <div className="flex items-center justify-between gap-2"><div className="text-[12px] text-on">更新渠道</div><select disabled={busy||downloading} className="bg-s3 text-on text-[12px] rounded p-2" value={prefs.channel} onChange={e=>{setMaintenancePreferences({channel:e.target.value==="beta"?"beta":"stable"});void checkMaintenance();}}><option value="stable">稳定版</option><option value="beta">含测试版</option></select></div>
    <div className="text-[11px] text-mut">{checked?"最近检查："+new Date(checked).toLocaleString():"尚未检查"}</div>
    {error&&<p role="alert" className="text-[12px] text-mut">{error}</p>}
    {result?.status==="no_release"&&<p className="text-[12px] text-mut">没有找到此渠道中带有效文件摘要的 macOS 安装包。</p>}
    {result?.status==="current"&&<p className="text-[12px] text-on">当前版本不低于此渠道可下载版本。</p>}
    {result?.status==="available"&&<div className="rounded bg-s3 p-3 space-y-2"><div className="text-[13px] text-on">发现 v{result.version} · {((result.size||0)/1048576).toFixed(1)} MB</div><div className="text-[11px] text-mut whitespace-pre-wrap max-h-36 scroll-y">{result.notes}</div><p className="text-[11px] text-mut">从自己的 GitHub 仓库下载并校验 SHA-256，随后由你在 Finder 中安装。不是自动安装，也不验证独立发布签名。</p><button disabled={downloading||busy} onClick={()=>void downloadMaintenance()} className="px-3 py-2 rounded bg-primary text-on-primary text-[12px] disabled:opacity-50">{downloading?"下载并校验中…":"下载安装包"}</button></div>}
    {downloading&&<div className="space-y-1" role="status"><div className="text-[11px] text-mut">{progress?.verifying?"正在验证文件完整性…":progress?.total?((progress.received/1048576).toFixed(1)+" / "+(progress.total/1048576).toFixed(1)+" MB"):"正在检查安装包…"}</div>{!!progress?.total&&<progress className="w-full h-2" max={progress.total} value={Math.min(progress.received,progress.total)} />}</div>}
    {downloaded&&<div className="rounded bg-s3 p-3 text-[12px] text-on space-y-1"><p>文件完整性校验通过，{downloaded.revealed?"已在 Finder 中定位":"请在下载目录中查看"}。请退出 HGhub，再手动安装。</p><p className="text-[10px] text-mut break-all">{downloaded.path}</p><p className="text-[10px] text-mut break-all">SHA-256: {downloaded.sha256}</p>{downloaded.archived===false&&<p className="text-[11px] text-mut">文件已保存，但下载记录保存失败，请自行保留安装包。</p>}<p className="text-[11px] text-mut">保留 macOS 安全提示，请先确认安装包来源。不要全局关闭系统安全保护。</p></div>}
    {downloadError&&<p role="alert" className="text-[12px] text-mut">{downloadError}</p>}
    <UpdateArchive refreshKey={downloaded?.sha256||""} />
    <MaintenanceBackup />
    <div className="border-t border-s3 pt-3 space-y-2">
      <div className="flex justify-between items-center"><div><div className="text-[12px] text-on">维护组件版本</div><div className="text-[11px] text-mut">仅查询公开版本信息，不自动同步代码</div></div><button disabled={tracking} onClick={()=>void track()} className="text-primary text-[12px] disabled:opacity-50">{tracking?"查询中…":"检查上游"}</button></div>
      {upstreams&&<div className="space-y-2">{upstreams.items.map(item=><div key={item.repo} className="rounded bg-s3 p-2"><div className="text-[12px] text-on">{item.repo}</div><div className="text-[11px] text-mut">当前基准 {item.baseline} · {item.latest?"上游 "+item.latest:"无法确认最新版本"}</div><div className="text-[11px] text-mut">{item.status==="newer"?"发现较新版本，待维护者评估":item.status==="not_newer"?"未发现更高版本":"查询未完成"}</div></div>)}<div className="text-[10px] text-mut">{upstreams.checked_at_utc||""}</div></div>}
      {trackError&&<p role="alert" className="text-[11px] text-mut">{trackError}</p>}
      <button className="text-[12px] text-primary" onClick={async()=>{try{const report={version:APP_VERSION,channel:prefs.channel,autoCheck:prefs.autoCheck,lastChecked:checked||null,updateStatus:result?.status||"unknown",availableVersion:result?.version||null,upstreams};await navigator.clipboard.writeText(JSON.stringify(report,null,2));setTrackError("维护报告已复制（不含账号、地址或凭据）。");}catch{setTrackError("复制失败。");}}}>复制维护报告</button>
    </div>
  </div>;
}
