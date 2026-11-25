import { useEffect,useState } from "react";
import { api, APP_VERSION, type UpdateArchiveEntry } from "../api";
export default function UpdateArchive({refreshKey}:{refreshKey:string}) {
  const [open,setOpen]=useState(false);
  const [items,setItems]=useState<UpdateArchiveEntry[]>([]);
  const [loading,setLoading]=useState(false);
  const [checking,setChecking]=useState("");
  const [message,setMessage]=useState("");
  useEffect(()=>{
    if(!open)return;
    let active=true;setLoading(true);setMessage("");
    api.manualUpdateHistory().then(x=>{if(active)setItems(x);}).catch(e=>{if(active){setItems([]);setMessage((e as Error).message||"无法读取记录");}}).finally(()=>{if(active)setLoading(false);});
    return()=>{active=false;};
  },[open,refreshKey]);
  const reveal=async(id:string)=>{
    if(checking)return;setChecking(id);setMessage("");
    try{await api.manualUpdateReveal(id);setMessage("已重新验证文件并在 Finder 中定位。安装前请确认版本与来源。");}
    catch(e){setMessage((e as Error).message||"本地安装包检查失败");}
    finally{setChecking("");}
  };
  return <div className="border-t border-s3 pt-3 space-y-2">
    <button className="w-full flex justify-between text-[12px] text-on" onClick={()=>setOpen(!open)} aria-expanded={open}><span>本地安装包与恢复记录</span><span className="text-primary">{open?"收起":"查看"}</span></button>
    {open&&<div className="space-y-2">
      <p className="text-[11px] text-mut">当前 v{APP_VERSION}。这里只记录此前成功校验并下载的文件，不代表已安装或已验证运行。更新前请另行保留当前可用应用或安装包。</p>
      {loading?<p className="text-[11px] text-mut">读取中…</p>:items.length===0?<p className="text-[11px] text-mut">暂无记录；旧版手动下载的文件不会自动导入。</p>:items.map(item=><div key={item.id} className="rounded bg-s3 p-2 space-y-1">
        <div className="flex justify-between items-center"><span className="text-[12px] text-on">v{item.version}</span><span className="text-[11px] text-mut">{(item.size/1048576).toFixed(1)} MB</span></div>
        <div className="text-[10px] text-mut">{item.saved_at} · {item.present?"文件仍在原目录":"文件已移动、删除或大小改变"}</div>
        <button disabled={!item.present||!!checking} onClick={()=>void reveal(item.id)} className="text-[11px] text-primary disabled:opacity-50">{checking===item.id?"重新校验中…":"重新校验并定位文件"}</button>
      </div>)}
      {message&&<p role="status" className="text-[11px] text-mut">{message}</p>}
      <p className="text-[11px] text-mut">手动恢复：退出 APP 后，使用之前自己确认可用的安装包。不要自动降级或清除本地数据；旧版本可能无法读取新版数据。系统安全设置保持不变。</p>
    </div>}
  </div>;
}
