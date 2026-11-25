import { useEffect, useState } from "react";
import { api, type ManualUpdateInfo, type DownloadedUpdate } from "./api";
import type { MaintenancePreferences as Preferences } from "./maintenanceBackup";
const KEY = "hghub_maintenance_preferences_v1";
let prefs: Preferences = { autoCheck: true, channel: "stable" };
try {
  const parsed = JSON.parse(localStorage.getItem(KEY) || "{}");
  prefs = { autoCheck: parsed.autoCheck !== false, channel: parsed.channel === "beta" ? "beta" : "stable" };
} catch { /* defaults */ }
let snapshot = { prefs, result: null as ManualUpdateInfo | null, busy: false, error: "", checked: "",
  downloading: false, downloaded: null as DownloadedUpdate | null, downloadError: "" };
const listeners = new Set<() => void>();
function set(p: Partial<typeof snapshot>) {
  snapshot = { ...snapshot, ...p };
  listeners.forEach(l => l());
}
export function setMaintenancePreferences(p: Partial<Preferences>): boolean {
  if (p.channel !== undefined && p.channel !== prefs.channel && (snapshot.busy || snapshot.downloading)) return false;
  const previousChannel = prefs.channel;
  const next = { ...prefs, ...p };
  let persisted = true;
  try { localStorage.setItem(KEY, JSON.stringify(next)); } catch { persisted = false; }
  // Keep existing session-only behavior for ordinary controls, but report persistence.
  prefs = next;
  if (prefs.channel !== previousChannel) set({ prefs, result: null, error: "", checked: "", downloaded: null, downloadError: "" });
  else set({ prefs });
  return persisted;
}
let pending: Promise<void> | null = null;
export function checkMaintenance() {
  if (pending) return pending;
  if (snapshot.downloading) return Promise.resolve();
  const channel = prefs.channel;
  set({ busy: true, error: "", result: null });
  pending = api.manualUpdateCheck(channel).then(result => {
    if (prefs.channel === channel) set({ result, checked: new Date().toISOString() });
  }).catch(e => { if (prefs.channel === channel) set({error: (e as Error).message || "检查失败"}); })
    .finally(() => { pending = null; set({busy:false}); });
  return pending;
}
export async function downloadMaintenance() {
  const {result, downloading}=snapshot;
  if (!result?.version || !result.digest || downloading || result.status!=="available") return;
  set({downloading:true,downloaded:null,downloadError:""});
  try { set({downloaded:await api.manualUpdateDownload(prefs.channel,result.version,result.digest)}); }
  catch(e) { set({downloadError:(e as Error).message || "下载失败"}); }
  finally { set({downloading:false}); }
}
export function useMaintenance() {
  const [value, update] = useState(snapshot);
  useEffect(() => { const fn=()=>update(snapshot); listeners.add(fn); fn(); return ()=>{listeners.delete(fn);}; }, []);
  return value;
}
export function useStartupUpdateCheck(platform: string) {
  useEffect(() => {
    if (platform !== "macos" || !prefs.autoCheck) return;
    const id = window.setTimeout(() => { if(prefs.autoCheck) void checkMaintenance(); }, 8000);
    return () => window.clearTimeout(id);
  }, [platform]);
}
