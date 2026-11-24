import { useEffect, useState, type ReactNode } from "react";
import { api, describeNetworkDiagnostic, APP_VERSION, hostOf, logLine, platform, type UpdateInfo, type ProxyState, type WarpStatus } from "../api";
import { useStore } from "../store";
import { Modal, Toggle } from "../components/ui";
import LinePicker from "../components/LinePicker";
import { copyPlaybackLog, initPlaybackLog, setPlaybackLog, usePlaybackLog } from "../playbackDiagnostics";
import { IBack, IBoltF, IBroom, IChevR, IHeart, IInfo, ILink, ILockF, IMoon, IRefresh, ISettings, ISun, IVolMute } from "../icons";

/* 独立设置页：线路 / 外观 / 播放 / 本地数据 / 清理缓存 / 网络诊断 / 关于
   入口：Windows 侧栏「设置」、Android「我的」右上角齿轮 */
export default function Settings({ onBack, wide }: { onBack: () => void; wide: boolean }) {
  const { theme, setTheme, favs, history, muteOnStartup, setMuteOnStartup, strictSecurity, strictSecurityReady, securityPending, cliRelaxed, setStrictSecurity, toast } = useStore();
  const [about, setAbout] = useState(false);
  const [clearOpen, setClearOpen] = useState(false);
  const [clearing, setClearing] = useState(false);
  const [lineOpen, setLineOpen] = useState(false);
  const [lineBase, setLineBase] = useState("");
  const [proxyMode, setProxyMode] = useState<ProxyState["mode"]>("system");
  const [proxyUrl, setProxyUrl] = useState("");
  const [proxyReady, setProxyReady] = useState(false);
  const [proxyBusy, setProxyBusy] = useState(false);
  const [proxyResult, setProxyResult] = useState("");
  const [warp, setWarp] = useState<WarpStatus | null>(null);
  const [warpBusy, setWarpBusy] = useState(false);
  const [warpMessage, setWarpMessage] = useState("");
  const [warpTermsOpen, setWarpTermsOpen] = useState(false);
  const [warpAgreed, setWarpAgreed] = useState(false);
  const [checking, setChecking] = useState(false);
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const playbackLog = usePlaybackLog();
  const [copyingLog, setCopyingLog] = useState(false);

  const readLine = () => {
    api.lineState().then((s) => setLineBase(s.base)).catch(() => {});
  };
  useEffect(readLine, []);
  useEffect(() => {
    let mounted = true;
    api.proxyState().then(s => {
      if (!mounted) return;
      setProxyMode(s.mode);
      setProxyUrl(s.url);
      setProxyReady(true);
    }).catch(() => {
      if (mounted) setProxyResult("读取代理设置失败，请重新启动应用");
    });
    return () => { mounted = false; };
  }, []);

  useEffect(() => {
    if (platform !== "macos") return;
    let active = true;
    const refresh = () => {
      void api.warpStatus().then(status => {
        if (active) setWarp(status);
      }).catch(() => {
        if (active) setWarp(null);
      });
    };
    refresh();
    const timer = window.setInterval(refresh, 6000);
    return () => { active = false; window.clearInterval(timer); };
  }, []);

  const enableWarp = async (acceptTerms: boolean) => {
    setWarpBusy(true);
    setWarpMessage("正在连接，首次使用需要注册设备并检测安全出口...");
    try {
      const result = await api.warpEnable(acceptTerms);
      setWarp(result);
      setProxyMode("warp");
      setProxyUrl("");
      setWarpMessage("内置 WARP 已连接并验证出口，仅 HGhub 请求使用该连接。");
      toast("内置 WARP 已连接", "ok");
    } catch (e) {
      setWarpMessage((e as Error).message || "内置 WARP 启动失败");
      void api.warpStatus().then(setWarp).catch(() => {});
    } finally {
      setWarpBusy(false);
    }
  };

  const disableWarp = async () => {
    setWarpBusy(true);
    setWarpMessage("");
    try {
      const result = await api.warpDisable();
      setWarp(result);
      setProxyMode("system");
      setProxyUrl("");
      setWarpMessage("WARP 已断开，HGhub 恢复跟随系统。");
      toast("已关闭内置 WARP", "ok");
    } catch (e) {
      setWarpMessage((e as Error).message || "WARP 断开失败");
    } finally {
      setWarpBusy(false);
    }
  };

  const saveProxy = async () => {
    setProxyBusy(true);
    setProxyResult("");
    try {
      if (warp?.running && proxyMode !== "warp") {
        setWarp(await api.warpDisable());
      }
      const result = await api.proxySet(proxyMode, proxyMode === "custom" ? proxyUrl.trim() : "");
      setProxyMode(result.mode);
      setProxyUrl(result.url);
      setProxyResult("设置已保存，后续请求将通过新网络方式发送。");
      toast("网络访问方式已更新", "ok");
    } catch (e) {
      setProxyResult((e as Error).message || "保存代理设置失败");
    } finally {
      setProxyBusy(false);
    }
  };
  useEffect(() => { void initPlaybackLog(); }, []);

  const checkUpdate = async () => {
    setChecking(true);
    try {
      const info = await api.checkUpdate();
      if (info.has_update) setUpdate(info);
      else toast(`已是最新版本 v${info.current}`, "ok");
    } catch (e) {
      toast((e as Error).message || "检查更新失败", "info");
    } finally {
      setChecking(false);
    }
  };

  const Section = ({ title, children }: { title: string; children: ReactNode }) => (
    <>
      <div className="text-[10.5px] font-semibold text-dim tracking-wide px-1 mt-4 mb-1.5">{title}</div>
      <div className="rounded-(--r-md) bg-s2 divide-y divide-s3">{children}</div>
    </>
  );

  return (
    <div className="absolute inset-0 bg-bg flex flex-col">
      <div className="shrink-0 relative overflow-hidden bg-s1" style={{ paddingTop: "var(--sat, 0px)" }}>
        <div className="absolute inset-0 pointer-events-none" style={{ background: "radial-gradient(90% 120% at 100% 0%, var(--p-pc) 0%, transparent 60%)", opacity: 0.32 }} />
        <div className="relative px-3 pt-2 pb-4">
          <button onClick={onBack} aria-label="返回" className="w-10 h-10 rounded-full flex items-center justify-center text-on hover:bg-s3 pressable"><IBack size={20} /></button>
          <div className="px-2 mt-1 flex items-end justify-between">
            <div>
              <div className="flex items-center gap-2">
                <span className="w-9 h-9 rounded-(--r-sm) bg-pc text-on-pc flex items-center justify-center"><ISettings size={20} strokeWidth={1.7} /></span>
                <span className="text-[24px] font-bold tracking-wide text-on">设置</span>
              </div>
              <div className="text-[11px] text-mut mt-1">外观 · 播放 · 本地数据 · 诊断</div>
            </div>
          </div>
        </div>
      </div>

      <div className="flex-1 scroll-y px-4 pt-1 pb-10">
        <div className={wide ? "max-w-[720px] mx-auto" : ""}>
          <Section title="外观">
            <div className="flex items-center gap-3 px-3.5 py-3">
              <span className="w-8 h-8 rounded-full bg-s3 flex items-center justify-center text-primary">{theme === "dark" ? <IMoon size={16} /> : <ISun size={16} />}</span>
              <div className="flex-1">
                <div className="text-[12.5px] text-on">深色外观</div>
                <div className="text-[10.5px] text-mut">跟随设计稿的 Fluent 深色</div>
              </div>
              <Toggle on={theme === "dark"} onChange={(on) => setTheme(on ? "dark" : "light")} label="深色外观" />
            </div>
          </Section>

          <Section title="播放">
            <div className="flex items-center gap-3 px-3.5 py-3">
              <span className="w-8 h-8 rounded-full bg-s3 flex items-center justify-center text-primary"><IVolMute size={16} /></span>
              <div className="flex-1 min-w-0">
                <div className="text-[12.5px] text-on">启动时静音</div>
                <div className="text-[10.5px] text-mut">默认开启，每次启动均静音；关闭后记住上次静音状态。下次启动生效。</div>
              </div>
              <Toggle on={muteOnStartup} onChange={setMuteOnStartup} label="启动时静音" />
            </div>
          </Section>

          <Section title="本地数据">
            <div className="flex items-center gap-3 px-3.5 py-3">
              <span className="w-8 h-8 rounded-full bg-s3 flex items-center justify-center text-primary"><IHeart size={16} /></span>
              <div className="flex-1">
                <div className="text-[12.5px] text-on">本地喜欢</div>
                <div className="text-[10.5px] text-mut">{favs.length} 部收藏 · {history.length} 条观看记录</div>
              </div>
            </div>
            <button onClick={() => setClearOpen(true)} className="w-full flex items-center gap-3 px-3.5 py-3 text-left pressable">
              <span className="w-8 h-8 rounded-full bg-s3 flex items-center justify-center text-primary"><IBroom size={16} /></span>
              <div className="flex-1">
                <div className="text-[12.5px] text-on">清理缓存</div>
                <div className="text-[10.5px] text-mut">封面 / 播放缓存与本地记录</div>
              </div>
              <IChevR size={15} className="text-mut" />
            </button>
          </Section>

          <Section title="网络">
            {platform === "macos" && (
              <div className="px-3.5 pt-3 pb-3 border-b border-s3">
                <div className="flex items-center gap-2">
                  <span className="w-8 h-8 flex items-center justify-center rounded-full bg-s3 text-primary">
                    <IBoltF size={18} />
                  </span>
                  <div className="flex-1">
                    <div className="text-[12.5px] font-semibold text-on">内置 WARP · 免费网络增强</div>
                    <div className="text-[10.5px] text-mut">
                      {warp?.active ? "内置代理运行中 · " + (warp.transport || "出口已在连接时验证") :
                        warp?.registered ? "已有设备凭据 · 未连接" :
                        "首次启用需要确认服务条款"}
                    </div>
                  </div>
                  <span className={"text-[10px] px-2 py-1 rounded-full " +
                    (warp?.active ? "bg-primary/20 text-primary" : "bg-s3 text-mut")}>
                    {warp?.active ? "已开启" : "未开启"}
                  </span>
                </div>
                <p className="text-[10.5px] text-mut leading-relaxed mt-2">
                  内置开源 usque；不需要另装软件、购买服务器或填写代理端口。只转发 HGhub 内的 HTTP(S) 请求，
                  不修改 Mac 的全局网络设置。启用后首次注册设备，并在每次连接时检查真实 WARP 出口。
                </p>
                <div className="flex gap-2 mt-3">
                  {warp?.active || warp?.running ? (
                    <button type="button" disabled={warpBusy}
                      onClick={() => { void disableWarp(); }}
                      className="h-9 px-4 rounded-(--r-ctl) bg-s3 text-on text-[11.5px] font-semibold pressable disabled:opacity-50">
                      {warpBusy ? "正在处理…" : "断开内置 WARP"}
                    </button>
                  ) : (
                    <button type="button" disabled={warpBusy || !warp?.available}
                      onClick={() => {
                        if (!warp?.registered) {
                          setWarpAgreed(false);
                          setWarpTermsOpen(true);
                        } else {
                          void enableWarp(false);
                        }
                      }}
                      className="h-9 px-4 rounded-(--r-ctl) bg-primary text-on-primary text-[11.5px] font-semibold pressable disabled:opacity-50">
                      {warpBusy ? "正在连接…" : "一键启用 WARP"}
                    </button>
                  )}
                  <button type="button" disabled={warpBusy}
                    onClick={() => { void api.warpStatus().then(setWarp).catch(() => setWarp(null)); }}
                    className="h-9 px-3 rounded-(--r-ctl) bg-s3 text-on text-[11.5px] pressable disabled:opacity-50">
                    刷新状态
                  </button>
                </div>
                {warp?.available === false && (
                  <p role="status" className="text-[10.5px] text-danger mt-2">
                    安装包缺少内置组件，请使用官方仓库发布的 macOS 完整安装包。
                  </p>
                )}
                {warpMessage && <p role="status" className="text-[10.5px] text-mut mt-2 break-words">{warpMessage}</p>}
                <p className="text-[10px] text-dim mt-2 leading-relaxed">
                  WARP 是第三方网络服务，不保证每个来源可用，也不会绕过账号或付费权限。
                  由你主动启用；退出 HGhub 时停止本地组件。
                </p>
              </div>
            )}
            <div className="px-3.5 pt-3 pb-3 border-b border-s3">
              <div className="text-[12.5px] font-medium text-on">网络访问方式</div>
              <div className="text-[10.5px] text-mut mt-1 leading-relaxed">
                某些 Wi-Fi 无法连接来源时，可指定已经启动的可信 HTTP 代理；本功能不提供代理服务器，也不会自动切换未知站点。
              </div>
              <div className="mt-2 rounded-(--r-sm) bg-s3/60 px-3 py-2 text-[10.5px] text-on/80 leading-relaxed">
                免费加密 DNS 已内置：本地 DNS 解析失败时，会自动尝试 Cloudflare / Google DNS，无须购买或手动填端口。
                仅处理 DNS 问题；如果视频源或 IP 连接本身被限制，仍需要其他可信网络通道。
              </div>
              <div className="mt-2 rounded-(--r-sm) bg-s3/60 px-3 py-2 text-[10.5px] text-on/80 leading-relaxed">
                自动恢复已启用：匿名读取及无凭据播放地址查询发生网络或临时网关故障时，尝试另一条可信固定线路。macOS「跟随系统」会自动识别已启用的 HTTP(S) 代理，无需填写端口。
                如果两条线路都受限，仍需要系统网络服务或手动代理。
              </div>
              <div className="grid grid-cols-3 gap-1.5 mt-3">
                {([
                  ["system", "跟随系统"],
                  ["direct", "直接连接"],
                  ["custom", "手动代理"],
                ] as const).map(([mode, label]) => (
                  <button key={mode} type="button" disabled={!proxyReady || proxyBusy}
                    onClick={() => { setProxyMode(mode); setProxyResult(""); }}
                    className={"h-9 rounded-(--r-ctl) text-[11.5px] pressable disabled:opacity-50 " +
                      (proxyMode === mode ? "bg-primary text-on-primary font-semibold" : "bg-s3 text-on/85")}>
                    {label}
                  </button>
                ))}
              </div>
              {proxyMode === "custom" && (
                <div className="mt-3">
                  <label htmlFor="hghub-proxy" className="block text-[11px] text-on mb-1">
                    HTTP(S) 代理地址
                  </label>
                  <input id="hghub-proxy" type="url" inputMode="url"
                    autoComplete="off" autoCapitalize="none" spellCheck={false}
                    placeholder="http://127.0.0.1:7890"
                    value={proxyUrl}
                    onChange={e => { setProxyUrl(e.target.value); setProxyResult(""); }}
                    className="w-full h-10 px-3 rounded-(--r-ctl) bg-s3 text-on text-[12px] outline-none focus:ring-1 focus:ring-primary"
                  />
                  <p className="text-[10px] text-mut leading-relaxed mt-1">
                    请填写代理软件的 HTTP 或 Mixed 端口。当前不接受 SOCKS 或带账号密码的地址。
                    代理可能知道访问的域名，请仅使用可信代理。
                  </p>
                </div>
              )}
              <div className="flex items-center gap-2 mt-3">
                <button type="button" disabled={!proxyReady || proxyBusy}
                  onClick={() => { void saveProxy(); }}
                  className="h-9 px-4 rounded-(--r-ctl) bg-primary text-on-primary text-[11.5px] font-semibold pressable disabled:opacity-50">
                  {proxyBusy ? "保存中…" : "保存网络设置"}
                </button>
                <button type="button" disabled={!proxyReady || proxyBusy}
                  onClick={() => {
                    setProxyBusy(true);
                    void api.netDiag().then(res => {
                      setProxyResult(describeNetworkDiagnostic(res));
                    }).catch(e => setProxyResult((e as Error).message || "检测失败"))
                      .finally(() => setProxyBusy(false));
                  }}
                  className="h-9 px-3 rounded-(--r-ctl) bg-s3 text-on text-[11.5px] pressable disabled:opacity-50">
                  检测连接
                </button>
              </div>
              {proxyResult && <div role="status" className="mt-2 text-[11px] text-mut break-words">{proxyResult}</div>}
            </div>
            <button onClick={() => setLineOpen(true)} className="w-full flex items-center gap-3 px-3.5 py-3 text-left pressable">
              <span className="w-8 h-8 rounded-full bg-s3 flex items-center justify-center text-primary"><IBoltF size={16} /></span>
              <div className="flex-1 min-w-0">
                <div className="text-[12.5px] text-on">线路</div>
                <div className="text-[10.5px] text-mut truncate">{lineBase ? hostOf(lineBase) : "固定线路 1（默认）"}</div>
              </div>
              <IChevR size={15} className="text-mut" />
            </button>
            <div className="flex items-center gap-3 px-3.5 py-3">
              <span className="w-8 h-8 rounded-full bg-s3 flex items-center justify-center text-primary"><ILockF size={15} /></span>
              <div className="flex-1 min-w-0">
                <div className="text-[12.5px] text-on">严格安全模式</div>
                <div className="text-[10.5px] text-mut">
                  {cliRelaxed
                    ? "已由启动参数 --no-strict 切换为兼容模式；可手动重新开启"
                    : "默认开启，防上游投毒与内网探测；若导致正文或特殊代理异常可关闭（或用 --no-strict 启动）"}
                </div>
              </div>
              <Toggle
                on={strictSecurity}
                disabled={!strictSecurityReady || securityPending}
                onChange={(on) => {
                  void setStrictSecurity(on).then(() => {
                    toast(on ? "已开启严格安全模式" : "已切换为兼容模式（关闭激进清洗与守卫）", "info");
                  }).catch(() => toast("安全设置保存失败，已保持原模式", "info"));
                }}
                label="严格安全模式"
              />
            </div>
            {/* 打不开时先点这里：把 DNS / 代理 / HTTP 结果复制给维护者 */}
            <button onClick={async () => {
              try {
                const text = JSON.stringify(await api.netDiag(), null, 2);
                logLine(`[net_diag] ${text}`);
                await navigator.clipboard?.writeText(text);
                toast("网络诊断结果已复制", "ok");
              } catch (e) {
                toast((e as Error).message || "诊断失败", "info");
              }
            }} className="w-full flex items-center gap-3 px-3.5 py-3 text-left pressable">
              <span className="w-8 h-8 rounded-full bg-s3 flex items-center justify-center text-primary"><ILink size={16} /></span>
              <div className="flex-1">
                <div className="text-[12.5px] text-on">网络诊断</div>
                <div className="text-[10.5px] text-mut">检测 DNS / 系统代理 / 官网连通性</div>
              </div>
              <IChevR size={15} className="text-mut" />
            </button>
          </Section>

          <Section title="播放诊断">
            <div className="flex items-center gap-3 px-3.5 py-3">
              <span className="w-8 h-8 rounded-full bg-s3 flex items-center justify-center text-primary"><IInfo size={16} /></span>
              <div className="flex-1 min-w-0">
                <div className="text-[12.5px] text-on">播放诊断日志</div>
                <div className="text-[10.5px] text-mut">记录点击、拖动、切页和缓冲过程；复现问题后可关闭并复制日志</div>
                {playbackLog.error && <div role="alert" className="text-[10.5px] text-danger">{playbackLog.error}</div>}
              </div>
              <Toggle label="播放诊断日志" on={playbackLog.enabled} disabled={!playbackLog.ready || playbackLog.pending}
                onChange={(on) => { void setPlaybackLog(on).then(() => toast(on ? "已开启播放诊断，请复现问题" : "已关闭播放诊断，已有日志保留", "ok")).catch(() => toast("日志设置保存失败，请重试", "info")); }} />
            </div>
            <button disabled={!playbackLog.ready || playbackLog.pending || copyingLog} onClick={async () => {
              setCopyingLog(true);
              try { await copyPlaybackLog(); toast("播放诊断日志已复制", "ok"); }
              catch (e) { toast((e as Error).message || "复制日志失败", "info"); }
              finally { setCopyingLog(false); }
            }} className="w-full flex items-center gap-3 px-3.5 py-3 text-left pressable disabled:opacity-60">
              <span className="w-8 h-8 rounded-full bg-s3 flex items-center justify-center text-primary"><ILink size={16} /></span>
              <div className="flex-1 min-w-0">
                <div className="text-[12.5px] text-on">{copyingLog ? "正在复制…" : "复制播放诊断日志"}</div>
                <div className="text-[10.5px] text-mut break-all">{playbackLog.path || "日志仅保存在本机"}</div>
              </div>
              <IChevR size={15} className="text-mut" />
            </button>
          </Section>

          <Section title="其他">
            <button onClick={checkUpdate} disabled={checking} className="w-full flex items-center gap-3 px-3.5 py-3 text-left pressable disabled:opacity-60">
              <span className="w-8 h-8 rounded-full bg-s3 flex items-center justify-center text-primary"><IRefresh size={16} /></span>
              <div className="flex-1">
                <div className="text-[12.5px] text-on">检查更新</div>
                <div className="text-[10.5px] text-mut">{checking ? "检查中…" : `当前 v${APP_VERSION} · 对比 GitHub Release`}</div>
              </div>
              <IChevR size={15} className="text-mut" />
            </button>
            <button onClick={() => setAbout(true)} className="w-full flex items-center gap-3 px-3.5 py-3 text-left pressable">
              <span className="w-8 h-8 rounded-full bg-s3 flex items-center justify-center text-primary"><IInfo size={16} /></span>
              <div className="flex-1">
                <div className="text-[12.5px] text-on">关于 HGhub</div>
                <div className="text-[10.5px] text-mut">版本 {APP_VERSION} · 仅限非商业使用</div>
              </div>
              <IChevR size={15} className="text-mut" />
            </button>
          </Section>

          <div className="text-center text-[10px] text-dim mt-5">HGhub {APP_VERSION} · {platform === "android" ? "Android" : platform === "ios" ? "iOS" : platform === "macos" ? "macOS" : platform === "linux" ? "Linux" : "Windows"} 客户端</div>
        </div>
      </div>

      <Modal open={warpTermsOpen} onClose={() => setWarpTermsOpen(false)} width={410}>
        <div className="p-5">
          <div className="text-[15px] font-bold text-on">首次启用内置 WARP</div>
          <p className="mt-2 text-[12px] text-mut leading-relaxed">
            HGhub 使用 MIT 许可的开源 usque 客户端，向 Cloudflare 注册仅供此设备使用的 WARP 凭据。
            启用后，HGhub 访问的网站域名和网络流量会经过 Cloudflare；HTTPS 内容仍使用原有证书验证。
            凭据和设备私钥仅保存在本机应用数据目录，不上传到 HGhub 服务器。
          </p>
          <p className="mt-2 text-[11px] text-mut leading-relaxed">
            这是非官方的 WARP 客户端，服务可能限制、不可用或变更。应用不会修改系统代理、安装网络扩展或代你购买服务。
          </p>
          <button type="button" onClick={() => { void api.openUrl("https://www.cloudflare.com/policies/terms/"); }}
            className="mt-3 text-[11.5px] text-primary underline text-left pressable">
            阅读 Cloudflare 服务条款
          </button>
          <label className="mt-3 flex items-start gap-2 text-[11.5px] text-on leading-relaxed cursor-pointer">
            <input type="checkbox" checked={warpAgreed}
              onChange={e => setWarpAgreed(e.target.checked)} className="mt-0.5"/>
            <span>我已了解并同意使用 Cloudflare WARP 服务，将 HGhub 网络请求通过其服务器传输，并接受相关条款。</span>
          </label>
          <div className="flex gap-2 mt-5">
            <button type="button" onClick={() => setWarpTermsOpen(false)}
              className="flex-1 h-10 bg-s3 rounded-(--r-ctl) text-on/80 text-[12px] pressable">取消</button>
            <button type="button" disabled={!warpAgreed || warpBusy}
              onClick={() => { setWarpTermsOpen(false); void enableWarp(true); }}
              className="flex-1 h-10 bg-primary rounded-(--r-ctl) text-on-primary text-[12px] font-semibold pressable disabled:opacity-50">
              同意并启用
            </button>
          </div>
        </div>
      </Modal>

      <LinePicker open={lineOpen} onClose={() => { setLineOpen(false); readLine(); }} />

      <Modal open={about} onClose={() => setAbout(false)} width={392}>
        <div className="p-5">
          <div className="flex items-center gap-3">
            <span className="w-11 h-11 rounded-(--r-sm) brand-grad glow-neon flex items-center justify-center text-on-primary text-[17px] font-black">H</span>
            <div>
              <div className="text-[16px] font-bold text-on leading-tight">HGhub</div>
              <div className="text-[11px] text-mut mt-0.5">黄果短剧 · 第三方客户端 v{APP_VERSION}</div>
            </div>
          </div>

          <p className="text-[12px] text-on/70 leading-relaxed mt-4">
            基于 Rust + Tauri 2 构建的跨平台客户端，支持 Windows、macOS、Android 与 Linux。
            界面遵循 Fluent 设计语言，播放器采用沉浸式竖屏信息流。
          </p>

          <div className="mt-4 rounded-(--r-sm) bg-s3/60 p-3 text-[11px] text-mut leading-relaxed">
            本项目仅供学习与交流，所有内容均来自上游站点，客户端不存储、不分发任何视频内容。
            请于 24 小时内删除相关资源，并支持正版。
          </div>

          <div className="mt-4 divide-y divide-s3">
            <button onClick={() => { navigator.clipboard?.writeText("https://github.com/Minireel/HGhub").then(() => toast("仓库地址已复制", "ok")).catch(() => {}); }}
              className="w-full flex items-center gap-2 py-2.5 text-left pressable">
              <ILink size={15} className="text-mut" />
              <span className="text-[12px] text-on/85">github.com/Minireel/HGhub</span>
              <span className="ml-auto text-[10.5px] text-primary">复制</span>
            </button>
            <button onClick={() => { navigator.clipboard?.writeText("https://huangguoai.com").then(() => toast("站点地址已复制", "ok")).catch(() => {}); }}
              className="w-full flex items-center gap-2 py-2.5 text-left pressable">
              <ILink size={15} className="text-mut" />
              <span className="text-[12px] text-on/85">huangguoai.com</span>
              <span className="ml-auto text-[10.5px] text-primary">复制</span>
            </button>
            <div className="flex items-center gap-2 py-2.5">
              <IInfo size={15} className="text-mut" />
              <span className="text-[12px] text-on/85">PolyForm Noncommercial 1.0.0</span>
              <span className="ml-auto text-[10.5px] text-mut">仅限非商业</span>
            </div>
          </div>

          <button onClick={() => setAbout(false)} className="mt-4 w-full h-9 rounded-(--r-ctl) bg-s3 text-[12.5px] text-on pressable">知道了</button>
        </div>
      </Modal>

      <Modal open={clearOpen} onClose={() => setClearOpen(false)} width={330}>
        <div className="p-5">
          <div className="text-[14.5px] font-bold text-on">清理缓存</div>
          <p className="text-[12px] text-mut leading-relaxed mt-2">
            只清除封面与播放列表缓存（解密图 / m3u8）。收藏、观看记录、搜索历史与登录状态都会保留。
          </p>
          <div className="flex gap-2 mt-5">
            <button onClick={() => setClearOpen(false)} className="flex-1 h-9 rounded-(--r-ctl) bg-s3 text-[12.5px] text-on/80 pressable">取消</button>
            <button disabled={clearing} onClick={async () => {
              setClearing(true);
              try { await api.clearCache(); } catch { /* 忽略 */ }
              setClearing(false);
              setClearOpen(false);
              toast("缓存已清理", "ok");
            }} className="flex-1 h-9 rounded-(--r-ctl) bg-primary text-[12.5px] font-semibold text-on-primary pressable disabled:opacity-60">
              {clearing ? "清理中…" : "确定清理"}
            </button>
          </div>
        </div>
      </Modal>

      <Modal open={!!update?.has_update} onClose={() => setUpdate(null)} width={392}>
        <div className="p-5">
          <div className="text-[15px] font-bold text-on">发现新版本</div>
          <p className="text-[11.5px] text-mut mt-1.5">
            v{update?.current} → <b className="text-on">v{update?.latest}</b>
            {update?.published_at ? ` · ${update.published_at.slice(0, 10)}` : ""}
          </p>
          {!!update?.notes && (
            <div className="mt-3 max-h-[180px] scroll-y rounded-(--r-sm) bg-s2 p-3 text-[11px] text-mut whitespace-pre-wrap leading-relaxed">{update.notes}</div>
          )}
          <div className="flex gap-2 mt-4">
            <button onClick={() => setUpdate(null)} className="flex-1 h-9 rounded-(--r-ctl) bg-s3 text-[12.5px] text-on/80 pressable">稍后</button>
            <button onClick={() => {
              if (!update) return;
              api.openUrl(update.url).catch(() => {
                navigator.clipboard?.writeText(update.url).then(() => toast("下载地址已复制", "ok")).catch(() => {});
              });
            }} className="flex-1 h-9 rounded-(--r-ctl) bg-primary text-[12.5px] font-semibold text-on-primary pressable">前往下载</button>
          </div>
          <div className="text-[10px] text-dim mt-2.5 break-all">{update?.url}</div>
        </div>
      </Modal>
    </div>
  );
}
