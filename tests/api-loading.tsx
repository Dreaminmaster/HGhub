import React, { useState } from "react";
import { createRoot } from "react-dom/client";
import Home from "../src/pages/Home";
import Category from "../src/pages/Category";
import { AppProvider } from "../src/store";
import "../src/index.css";

type Request = { cmd: string; args: Record<string, unknown>; resolve: (value: unknown) => void; reject: (error: string) => void };
const pending: Request[] = [];
const fixture = {
  history: [] as { cmd: string; args: Record<string, unknown> }[],
  debug: [] as string[],
  ready: false,
  diagnosticDelay: false,
  finishDiagnostic: null as null | (() => void),
  setActive: (_active: boolean) => {},
  complete(cmd: string, key: string, result: "404" | "503" | "empty" | "item") {
    const index = pending.findIndex(r => r.cmd === cmd && (!key || r.args.cate === key));
    if (index < 0) throw new Error(`No pending request: ${cmd}/${key}`);
    const request = pending.splice(index, 1)[0];
    if (result === "404" || result === "503") {
      request.reject(`HTTP ${result} Gateway: <html><h1>nginx error</h1></html>`);
      return;
    }
    request.resolve({
      items: result === "empty" ? [] : [{
        id: 991, title: `已加载${key || cmd}`, cover: "", tags: [], score: 0, hot: 0,
        episode_count: 1, total_episodes: 1, is_finished: true, is_original: 0,
        like_count: 0, comment_count: 0, collect_count: 0,
      }],
      pagination: { page: 1, pages: 1, size: 8, total: result === "empty" ? 0 : 1 },
    });
  },
};

// Preserve the real api.call error formatting and React pages; replace only Tauri IPC.
Object.assign(window, {
  apiLoadingFixture: fixture,
  __TAURI_INTERNALS__: { invoke(cmd: string, args: Record<string, unknown> = {}) {
    if (cmd === "app_boot") return Promise.resolve({ user: null, logged_in: false, base: "https://fixture.invalid" });
    if (cmd === "debug_log") { fixture.debug.push(String(args.msg)); return Promise.resolve(); }
    if (cmd === "net_diag") {
      const result = {
        http: { status: 200 }, api: { ok: false, status: 503, error_kind: "http_error" },
        proxy: { mode: "warp", warp_running: true, warp_egress: { ok: true } },
        backup: { origin: "https://fixture.invalid", api: { ok: false, status: 502 } },
      };
      if (!fixture.diagnosticDelay) return Promise.resolve(result);
      return new Promise(resolve => { fixture.finishDiagnostic = () => { fixture.finishDiagnostic = null; resolve(result); }; });
    }
    if (!["api_feed", "api_category", "api_posts"].includes(cmd)) return Promise.reject(`Unexpected IPC: ${cmd}`);
    fixture.history.push({ cmd, args });
    return new Promise((resolve, reject) => pending.push({ cmd, args, resolve, reject }));
  } },
});

function Fixture() {
  const [active, setActive] = useState(false);
  fixture.setActive = setActive;
  fixture.ready = true;
  const home = new URLSearchParams(location.search).get("page") === "home";
  return <AppProvider>
    <div style={{ position: "relative", width: 900, height: 1000 }}>
      {home ? <Home active bottomInset={0} /> : <Category wide active={active} />}
    </div>
  </AppProvider>;
}
createRoot(document.querySelector("#root")!).render(<React.StrictMode><Fixture /></React.StrictMode>);
