import { useState } from "react";
import { api, APP_VERSION, platform } from "../api";
import { checkMaintenance, setMaintenancePreferences, useMaintenance } from "../maintenance";
import { Toggle } from "./ui";
export default function UpdateCenter() {
  const {prefs,result,busy,error,checked} = useMaintenance();
  const [installing,setInstalling]=useState(false);
  const [confirm,setConfirm]=useState(false);
  const [installError,setInstallError]=useState("");
  const [checkingPublic,setCheckingPublic]=useState(false);
  const [publicMessage,setPublicMessage]=useState("");
  const install=async()=>{
    if(!result?.version || installing || !confirm) return;
    setInstalling(true); setInstallError("");
    try { await api.signedUpdateInstall(prefs.channel,result.version); }
    catch(e) { setInstallError((e as Error).message || "安装失败"); }
    finally {setInstalling(false);setConfirm(false);}
  };
  if(platform!=="macos") return <div className="p-3 text-[12px] text-mut">应用内签名更新目前仅支持 macOS。</div>;
  return <div className="p-3.5 space-y-3">
    <div className="flex justify-between items-center"><div><div className="text-[13px] font-semibold text-on">软件更新</div><div className="text-[11px] text-mut">当前 v{APP_VERSION} · Dreaminmaster/HGhub</div></div>
      <button disabled={busy||installing} onClick={()=>{setConfirm(false);void checkMaintenance();}} className="px-3 py-2 rounded-(--r-sm) bg-s3 text-primary text-[12px] disabled:opacity-60">{busy?"检查中…":"检查更新"}</button></div>
    <div className="flex justify-between items-center"><div className="text-[12px] text-on">启动时检查更新</div><Toggle on={prefs.autoCheck} disabled={installing} onChange={autoCheck=>setMaintenancePreferences({autoCheck})}/></div>
    <div className="flex items-center justify-between gap-2"><div className="text-[12px] text-on">更新渠道</div><select disabled={busy||installing} className="bg-s3 text-on text-[12px] rounded p-2" value={prefs.channel} onChange={e=>{setConfirm(false);setMaintenancePreferences({channel:e.target.value==="beta"?"beta":"stable"});void checkMaintenance();}}><option value="stable">稳定版</option><option value="beta">测试版</option></select></div>
    <div className="text-[11px] text-mut">{checked?"最近检查："+new Date(checked).toLocaleString():"尚未检查"}</div>
    {error&&<p role="alert" className="text-[12px] text-mut">{error}</p>}
    {result?.status==="key_missing"&&<p className="text-[12px] text-mut">可信签名密钥尚未配置。为保证安全，应用内安装暂时关闭。</p>}
    {result?.status==="current"&&<p className="text-[12px] text-on">当前已是此渠道最新版本。</p>}
    {result?.status==="available"&&<div className="rounded bg-s3 p-3 space-y-2"><div className="text-[13px] text-on">发现 v{result.version}</div><div className="text-[11px] text-mut whitespace-pre-wrap max-h-36 scroll-y">{result.notes}</div>
      {!confirm?<button onClick={()=>setConfirm(true)} disabled={installing} className="text-primary text-[12px]">准备安装</button>:<div><p className="text-[12px] text-mut">请先结束播放。安装成功后应用将重新启动；不会主动删除用户数据。</p><div className="flex gap-3 mt-2"><button disabled={installing} onClick={()=>setConfirm(false)} className="text-mut text-[12px]">取消</button><button disabled={installing} onClick={()=>void install()} className="text-primary text-[12px]">{installing?"下载并验证中…":"确认安装并重启"}</button></div></div>}
    </div>}
    {installError&&<p role="alert" className="text-[12px] text-mut">{installError}</p>}
    <div className="border-t border-s3 pt-2"><button disabled={checkingPublic||installing} onClick={async()=>{setCheckingPublic(true);try {const info=await api.checkUpdate();setPublicMessage(info.has_update?"发现公开发布版 "+info.latest+"；未经应用内签名验证，不会自动安装。":info.latest?"公开 macOS 版本未高于当前版本。":"未找到适用的公开版本。");}catch(e){setPublicMessage((e as Error).message);}finally{setCheckingPublic(false);}}} className="text-[11px] text-primary">{checkingPublic?"查询中…":"查看仓库发布状态（只读）"}</button>{publicMessage&&<p className="text-[11px] text-mut mt-1">{publicMessage}</p>}</div>
  </div>;
}
