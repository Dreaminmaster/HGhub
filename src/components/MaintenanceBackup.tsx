import { useState } from "react";
import { useMaintenance,setMaintenancePreferences } from "../maintenance";
import { encodeMaintenanceBackup,decodeMaintenanceBackup,type MaintenancePreferences } from "../maintenanceBackup";
export default function MaintenanceBackup(){
  const {prefs,busy,downloading}=useMaintenance();
  const [open,setOpen]=useState(false);
  const [raw,setRaw]=useState("");
  const [preview,setPreview]=useState<MaintenancePreferences|null>(null);
  const [message,setMessage]=useState("");
  const locked=busy||downloading;
  return <div className="border-t border-s3 pt-3 space-y-2">
    <button className="text-[12px] text-on" aria-expanded={open} onClick={()=>{setOpen(!open);setPreview(null);setMessage("");}}>更新偏好备份</button>
    {open&&<div className="space-y-2">
      <p className="text-[11px] text-mut">仅备份更新渠道与启动检查开关。不会导出账号、播放记录、代理地址、私钥或应用文件。</p>
      <div className="flex gap-3">
        <button className="text-primary text-[12px]" onClick={()=>{setRaw(encodeMaintenanceBackup(prefs));setPreview(null);setMessage("备份已生成，请复制并自行保存在本地。");}}>生成备份</button>
        <button className="text-primary text-[12px] disabled:opacity-50" disabled={!raw} onClick={async()=>{try{await navigator.clipboard.writeText(raw);setMessage("已复制备份。");}catch{setMessage("复制失败，可手动复制文本。");}}}>复制备份</button>
      </div>
      <textarea className="w-full rounded bg-s3 p-2 text-[11px] text-on" aria-label="备份 JSON" rows={5} maxLength={4096} value={raw} onChange={e=>{setRaw(e.target.value);setPreview(null);setMessage("");}}/>
      <button className="text-primary text-[12px] disabled:opacity-50" disabled={!raw||locked} onClick={()=>{const p=decodeMaintenanceBackup(raw);setPreview(p);setMessage(p?"":"备份格式无效或含有不支持的字段。");}}>检查备份</button>
      {preview&&<div className="rounded bg-s3 p-2 space-y-2"><p className="text-[12px] text-on">将恢复：{preview.channel==="beta"?"含测试版":"稳定版"}；{preview.autoCheck?"启动时检查":"启动时不检查"}。</p><button disabled={locked} className="text-[12px] text-primary disabled:opacity-50" onClick={()=>{if(locked)return;const saved=setMaintenancePreferences(preview);setPreview(null);setMessage(saved?"更新偏好已恢复。":"无法持久保存偏好；本次运行的设置可能已改变。");}}>确认恢复偏好</button></div>}
      {message&&<p role="status" className="text-[11px] text-mut">{message}</p>}
      <p className="text-[11px] text-mut">完整备份仍需单独保留当前应用和本地数据。此处不会自动替换或降级应用。</p>
    </div>}
  </div>;
}
