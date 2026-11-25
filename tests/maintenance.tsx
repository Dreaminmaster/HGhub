import React from "react";
import { createRoot } from "react-dom/client";
import { api, type ManualUpdateInfo, type DownloadedUpdate } from "../src/api";
import UpdateCenter from "../src/components/UpdateCenter";
import { checkMaintenance, setMaintenancePreferences, downloadMaintenance } from "../src/maintenance";
function deferred<T>() { let resolve!: (v:T)=>void; let reject!: (reason:unknown)=>void; const promise=new Promise<T>((r,j)=>{resolve=r;reject=j;}); return {promise,resolve,reject}; }
const info:ManualUpdateInfo={status:"available",version:"1.2.99",digest:"a".repeat(64),size:1048576,notes:"Local test version"};
const download:DownloadedUpdate={path:"/fixture/installer.dmg",size:1048576,sha256:info.digest!,revealed:true,archived:true};
let checks=0, downloads=0, reveals=0;
let check=deferred<ManualUpdateInfo>(), file=deferred<DownloadedUpdate>();
api.manualUpdateCheck=()=>{checks++;return check.promise;};
api.manualUpdateDownload=()=>{downloads++;return file.promise;};
api.manualUpdateProgress=async()=>({busy:true,received:524288,total:1048576,verifying:false});
api.manualUpdateHistory=async()=>[{id:"fixture",version:"1.2.98",size:1048576,digest:"a".repeat(64),saved_at:"2026-11-25",present:true}];
api.manualUpdateReveal=async()=>{reveals++;};
const root=createRoot(document.getElementById("root")!);
root.render(<UpdateCenter/>);
const fixture={
  counts:()=>({checks,downloads,reveals}),
  check:()=>{void checkMaintenance();void checkMaintenance();},
  finishCheck:()=>check.resolve(info),
  noRelease:()=>check.resolve({status:"no_release"}),
  current:()=>check.resolve({status:"current",version:"1.2.9"}),
  invalidDigest:()=>check.resolve({...info,digest:"not-a-digest"}),
  unknownStatus:()=>check.resolve({status:"unexpected"}),
  newCheck:()=>{check=deferred<ManualUpdateInfo>();},
  failCheck:()=>check.reject(new Error("fixture unavailable")),
  switchChannel:()=>setMaintenancePreferences({channel:"beta"}),
  downloadAgain:()=>{void downloadMaintenance();},
  finishDownload:()=>file.resolve(download),
  newDownload:()=>{file=deferred<DownloadedUpdate>();},
  failDownload:()=>file.reject(new Error("fixture download failed")),
};
Object.assign(window,{maintenanceFixture:fixture});
