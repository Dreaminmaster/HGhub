// Small portable preference backup; no secrets, source addresses, executable code or paths.
export type MaintenancePreferences = { autoCheck: boolean; channel: "stable" | "beta" };
const exactKeys=(o:unknown,keys:string[]): o is Record<string,unknown>=>{
  if(!o||typeof o!=="object"||Array.isArray(o))return false;
  const k=Object.keys(o);
  return k.length===keys.length&&keys.every(x=>Object.prototype.hasOwnProperty.call(o,x));
};
export function encodeMaintenanceBackup(p:MaintenancePreferences):string {
  return JSON.stringify({schema:1,kind:"maintenance-preferences",preferences:{autoCheck:p.autoCheck,channel:p.channel}},null,2);
}
export function decodeMaintenanceBackup(raw:string):MaintenancePreferences|null {
  if(raw.length>4096)return null;
  try{
    const o:unknown=JSON.parse(raw);
    if(!exactKeys(o,["schema","kind","preferences"])||o.schema!==1||o.kind!=="maintenance-preferences")return null;
    const p=o.preferences;
    if(!exactKeys(p,["autoCheck","channel"])||typeof p.autoCheck!=="boolean"||(p.channel!=="stable"&&p.channel!=="beta"))return null;
    return {autoCheck:p.autoCheck,channel:p.channel};
  }catch{return null;}
}
