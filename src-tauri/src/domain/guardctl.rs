//! 守卫的启停与就绪判定（生命周期所有权的调用方）。铁律：壳不是守卫的所有者 —— 只向所有者
//! （systemd / launchd / schtasks）提出请求，服务管理器不可用时走 spawn 兜底（可用性优先）。
//! 阶段序列 P0 契约 -> P1 对齐 -> P3 定位 -> P4 定义 -> P5 启动 -> P6 就绪（规范 docs/KERNEL-LAUNCH-STANDARD.md），
//! P1 是 P5 的前置：磁盘内核必须等于线上最新，否则拒绝启动。所有等待都有上限，退出也要能在服务管理器无响应时走完。

use std::net::{TcpStream, ToSocketAddrs};
use tauri::{Emitter, Manager};

/// 启动失败的结构化错误（stage 由规范定义，见 KERNEL-LAUNCH-STANDARD.md 节4）。
///  前端据此给可操作结论；日志据此定位到具体阶段 —— 不允许「未知错误」。
#[derive(Debug, Clone)]
pub(crate) struct LaunchError {
    pub code: &'static str,
    pub message: String,
}

impl LaunchError {
    pub(crate) fn new(code: &'static str, message: impl Into<String>) -> Self {
        LaunchError { code, message: message.into() }
    }
}

/// P1 对齐结果。
pub(crate) enum AlignOutcome {
  /// 找到「版本 == 线上最新」的内核。
    Aligned { bin: std::path::PathBuf, version: String },
  /// 线上版本查询失败（离线/源不可达）—— 无法验证对齐，不启动。
    ResolveFailed(String),
  /// 磁盘上没有与线上最新一致的内核 —— 必须先安装（P1）。
    NotAligned { latest: Option<String>, searched: Vec<String> },
}

/// P1+P3：解析「与线上最新一致」的内核（GUI 路径，带 resource_dir）。
pub(crate) fn resolve_aligned(app: &tauri::AppHandle) -> AlignOutcome {
    resolve_aligned_with(app.path().resource_dir().ok())
}

/// 与 [`resolve_aligned`] 同逻辑，但 resource_dir 显式传入（无 AppHandle 的 CLI 路径传 None）。
pub(crate) fn resolve_aligned_with(resource_dir: Option<std::path::PathBuf>) -> AlignOutcome {
    let pkg = match crate::core::package_name() {
        Ok(p) => p,
        Err(e) => return AlignOutcome::ResolveFailed(e),
    };
    let latest = match crate::core::latest_version(&pkg) {
        Ok((v, _o)) => v,
        Err(e) => return AlignOutcome::ResolveFailed(format!("{}：{}", pkg, e)),
    };
    let pkg_opt = Some(pkg.as_str());
  // 1) 位置契约命中且版本一致（最快路径）。契约路径可能是旧版写入的 `.cmd` 垫片
  //  或含 `\\?\` 前缀 —— 先规范化再判可用。
    if let Some(c) = crate::core_contract::read() {
        let bin = crate::domain::coreloc::normalize_guard(c.bin, pkg_opt);
        if bin.is_file() && c.version == latest {
            return AlignOutcome::Aligned { bin, version: latest };
        }
    }
  // 2) 候选扫描：取版本 == latest 者（候选已在 coreloc 内规范化），命中即**前向自愈**写入契约。
    let cands = crate::domain::coreloc::locate_core_candidates(resource_dir);
    for c in &cands {
        if crate::core::installed_version(c).as_deref() == Some(latest.as_str()) {
            let prefix = crate::core::global_prefix_for(c);
            crate::core_contract::write(&crate::core_contract::InstalledCore {
                bin: c.clone(),
                prefix,
                version: latest.clone(),
                source: "local-adopted".into(),
            });
            return AlignOutcome::Aligned { bin: c.clone(), version: latest };
        }
    }
    AlignOutcome::NotAligned {
        latest: Some(latest),
        searched: cands.iter().map(|p| p.display().to_string()).collect(),
    }
}

/// `--run-guard` 的本地检测（不触网）：解析可运行的 node + 守卫（取本地最高版本）。
/// 服务定义只指向稳定入口，故每次启动重新检测，node 迁移（nvm/volta/fnm）与内核升级后自动适配。
/// 线上对齐（P1）仍由壳在创建/启动服务前把关；此处只做本地解析，离线也能启动。
pub fn resolve_local(resource_dir: Option<std::path::PathBuf>) -> Option<(crate::runtime_contract::NodeRuntime, std::path::PathBuf)> {
    let rt = crate::runtime_contract::ensure()?;
    let guard = crate::domain::coreloc::pick_highest(
        crate::domain::coreloc::locate_core_candidates(resource_dir),
    )?;
    Some((rt, guard))
}

pub(crate) fn shutdown_all(port: u16) {
  // 契约 节4.1 退出握手（阶段 3 增强）：
  //  1) 带超时请求内核停全部被管对象，并等待回执（防止守卫挂起时壳无限阻塞）；
  //  2) 轮询 sessionState 直到 stopped（确认内核确实停好；守卫已不可达同样视为完成）；
  //  3) 由所有者停止守卫进程——守卫自身从不停止自己（阶段 1 所有权归一）。
    let _ = crate::domain::localhttp::post_local_timeout(port, "/session/stop", std::time::Duration::from_secs(60));
    for _ in 0..40 {
        match crate::domain::localhttp::get_session_state(port) {
  Some(s) if s == "stopped" => break, // 内核已确认停链完成
  None => break,  // 守卫已不可达 = 已退出
            _ => std::thread::sleep(std::time::Duration::from_millis(250)),
        }
    }
    if let Err(e) = crate::platform::service().stop() {
  // ：除 stderr（GUI 下常丢）外**必须落盘** —— 退出未真正停掉守卫是用户
  //  可感知的严重缺陷（"程序关不掉"），必须留下可诊断痕迹。
        eprintln!("[shell] 停止守卫失败: {}（可手动 systemctl --user stop dsh-supervisor）", e);
        crate::update::log(&format!("[shell] 停止守卫失败: {}", e));
    }
}

/// 服务管理器路径（P4 建立定义 -> P5 请求启动）的阶段产物。不变量由结构强制而非注释：
/// `started` 只可能在 `defined` 为 `Ok` 时才是 `Some` —— 定义失败时那条出边是关闭的。
/// 把 `ensure_defined` 的错误写进日志就丢掉再无条件 `start()`，报错就只剩「/Run 失败（退出码 1）」，
/// 而真实失败点可能在更早的 P4；对不存在的任务发起 /Run 不产生命中信息，只产出误导性退出码。
pub(crate) struct ServiceLaunch {
  /// P4：服务定义的结果（成功时带平台给出的状态描述）。
    defined: Result<String, String>,
  /// P5：仅在 P4 成功时才有值；`None` = **未向服务管理器发起请求**（出边已关）。
    started: Option<Result<(), String>>,
}

impl ServiceLaunch {
  /// 跑 P4 -> P5（不跑 P6：等待由调用方决定预算，见 `await_ready`）。
  ///
  /// `report` 用于把阶段名上报给引导页（静默等待与卡死必须可区分）。
    fn define_and_start(spec: &crate::platform::LaunchSpec, report: &dyn Fn(&str)) -> Self {
        let defined = match crate::platform::service().ensure_defined(spec) {
            Ok(desc) => {
                crate::update::log(&format!("守卫服务定义: {}", desc));
                Ok(desc)
            }
  // 关键的一步：**记下原因并关掉出边**，让调用方走 spawn 兜底。
            Err(e) => {
                crate::update::log(&format!("守卫服务定义失败（跳过启动请求，改走直接拉起）: {}", e));
                Err(e)
            }
        };
        let started = match &defined {
            Ok(_) => {
                report("正在请求服务管理器启动守卫…");
                let r = crate::platform::service().start();
                if let Err(e) = &r {
                    crate::update::log(&format!("服务管理器启动失败: {}", e));
                }
                Some(r)
            }
            Err(_) => None,
        };
        ServiceLaunch { defined, started }
    }

  /// 是否真的向服务管理器发过**并被接受**的启动请求 —— 请求被拒（定义走的是兜底通道、
  /// 任务不存在、策略拦截）就没必要为它等 30 秒：那段时间里没有任何东西会去拉起守卫。
    fn start_requested(&self) -> bool {
        matches!(self.started, Some(Ok(())))
    }

  /// 面向人的**一句话阶段证据**：说清走到了哪一步、为什么停在那一步。
  ///
  /// 进 `READY_TIMEOUT` / `SERVICE_START_FAILED` 的正文（规范 H8：报错必须可定位到阶段）。
    fn evidence(&self) -> String {
        match (&self.defined, &self.started) {
            (Err(d), None) => format!("服务定义未建立（因此未请求服务管理器启动）：{}", d),
            (Err(d), Some(_)) => format!("服务定义未建立：{}", d),
            (Ok(_), Some(Err(e))) => format!("服务管理器错误：{}", e),
            (Ok(_), Some(Ok(()))) => "服务管理器已接受启动请求，但守卫未在其间就绪".to_string(),
            (Ok(d), None) => format!("服务定义已建立（{}），未发起启动请求", d),
        }
    }
}

/// 拉起守卫（P0 契约 -> P1 对齐 -> P3 定位 -> P4 定义 -> P5 启动 -> P6 就绪）。
/// 端口从用户 config.apiPort 解析（非硬编码 3100）。
pub(crate) fn ensure_guard(app: &tauri::AppHandle) -> Result<(), LaunchError> {
    let port = crate::env::current_api_port();
  // 本函数最长约 2 分钟：每个阶段都上报（静默等待与卡死无法区分）。
    let step = |s: &str| {
        let _ = app.emit("guard_progress", serde_json::json!({ "status": s }));
        crate::update::log(s);
    };
    if port_open(port) {
        step("守卫端口已开 · 跳过启动");
        // 守卫已活时也确保一次服务定义：退出时 Windows stop() 会 /Delete 看护任务，而登录任务可能已先拉起
        // 守卫使本函数提前返回，那样看护任务永不重建、崩溃自愈在整个会话内失效。ensure_defined 三平台幂等且自愈。
        // 不复用 spec 变量名以免 K-3 的顺序判据锚到本提前返回分支；不得在此引入平台条件编译（platform/ 之外禁止平台分支）。
        if let Some((rt_wd, guard_wd)) = resolve_local(None) {
            match crate::platform::LaunchSpec::from_runtime(&rt_wd, guard_wd) {
                Ok(spec_wd) => {
                    if let Err(e) = crate::platform::service().ensure_defined(&spec_wd) {
                        crate::update::log(&format!("守卫已在运行，但服务定义确保失败: {}", e));
                    }
                }
                Err(e) => crate::update::log(&format!("守卫已在运行，但启动规格组装失败: {}", e)),
            }
        }
        return Ok(());
    }

  // P0 运行期契约：Node/npm 的**单一事实源**（缺失则先解析并原子落盘）。
    let rt = crate::runtime_contract::ensure().ok_or_else(|| {
        LaunchError::new("RUNTIME_MISSING", "Node 运行环境未就绪：无法解析 node/npm（请先完成环境准备）")
    })?;

  // P1 对齐 + P3 定位（**前置**）：磁盘内核必须等于线上最新。
    step("正在校验内核与线上版本对齐…");
    let (guard, version) = match resolve_aligned(app) {
        AlignOutcome::Aligned { bin, version } => (bin, version),
        AlignOutcome::ResolveFailed(e) => {
            return Err(LaunchError::new("ALIGN_RESOLVE_FAILED", format!("内核版本对齐失败（线上不可达）：{}", e)));
        }
        AlignOutcome::NotAligned { latest, searched } => {
            let l = latest.unwrap_or_else(|| "?".into());
            return Err(LaunchError::new(
                "KERNEL_NOT_ALIGNED",
                format!(
                    "磁盘内核与线上最新（v{}）不一致，已拒绝启动旧内核；请先安装/更新内核。已搜索：{}",
                    l,
                    if searched.is_empty() { "（无候选）".into() } else { searched.join("、") }
                ),
            ));
        }
    };
    step(&format!("内核已对齐 v{}", version));
    let spec = crate::platform::LaunchSpec::from_runtime(&rt, guard)
        .map_err(|e| LaunchError::new("LAUNCH_SPEC_FAILED", e))?;
    ensure_started(&spec, &step)
}

/// 启动序列本体（P4 定义 -> P5 服务管理器 -> P6 就绪 -> P5 兜底 spawn）。
/// 从 ensure_guard 拆出的唯一原因：无头 `--watchdog` 没有 AppHandle、也不做版本对齐，
/// 但它必须走与 GUI 启动逐字相同的拉起序列，否则「怎么把守卫拉起来」又要写第二遍。
pub(crate) fn ensure_started(
    spec: &crate::platform::LaunchSpec,
    step: &dyn Fn(&str),
) -> Result<(), LaunchError> {
  // P4 建立服务定义 + P5 请求服务管理器启动（**定义失败则不出边**，见 ServiceLaunch）。
    step("正在建立守卫服务定义…");
    let launch = ServiceLaunch::define_and_start(spec, step);

  // P6 就绪（只在真的向服务管理器发过请求时等；否则这 30s 是纯粹地卡住用户）。
    if launch.start_requested() {
        step("等待守卫就绪（服务管理器路径）…");
        if await_ready(SERVICE_READY_BUDGET) == Readiness::Ready {
            return Ok(());
        }
    }

  // P5 兜底：直接拉起守护进程（容器/无 user session/策略拦截等场景）。
    step("服务管理器未能拉起守卫 · 改用直接启动兜底…");
  // 服务管理器那一段的**阶段证据**必须在最终报错里出现（规范 H8）：真机上它才是根因所在。
    let evidence = launch.evidence();
    match crate::platform::service().spawn_daemon(spec) {
        Ok(mut child) => {
            let pid = child.id();
            crate::update::log(&format!("兜底 spawn 守卫 pid={}", pid));
            let (verdict, exited) = await_daemon(DAEMON_READY_BUDGET, &mut child);
            if verdict == Readiness::Ready {
                return Ok(());
            }
            Err(LaunchError::new(
                "READY_TIMEOUT",
                format!(
                    "守卫启动失败（{}）。{}；直接拉起进程 pid={}{}，其输出见 {}\n守卫输出末段：{}",
                    verdict.describe(),
                    evidence,
                    pid,
                    match exited {
                        Some(code) => format!("已退出（退出码 {}）", code),
                        None => "仍在运行".to_string(),
                    },
                    crate::update::guard_log_path().display(),
                    guard_log_tail()
                ),
            ))
        }
        Err(e) => Err(LaunchError::new(
            "SERVICE_START_FAILED",
            format!("守卫启动失败：{}；直接拉起也失败：{}", evidence, e),
        )),
    }
}

/// 兜底直拉后的等待：就绪判据与 [`await_ready`] 同源，但子进程非零退出即早停 —— 一个已经
/// 退出的进程不会再变绿，让它跑满 60 秒就是把「拉起即失败」说成「启动超时」。退出码 0 不算
/// 失败：Windows 上稳定入口 `<壳> --run-guard` 是先 detach 出 node 再退出的，它退了不代表守卫退了。
fn await_daemon(
    budget: std::time::Duration,
    child: &mut std::process::Child,
) -> (Readiness, Option<i32>) {
    let started = std::time::Instant::now();
    let mut last = Readiness::PortClosed;
    loop {
        last = ready(crate::env::current_api_port(), READINESS_HTTP_TIMEOUT);
        if last == Readiness::Ready {
            return (last, None);
        }
        if let Ok(Some(st)) = child.try_wait() {
            if !st.success() {
                return (last, st.code());
            }
        }
        if started.elapsed() >= budget {
            return (last, None);
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
}

/// 守卫输出日志的末段（最多 6 行 / 2000 字节）。报错要能自己带证据：真机上
/// 「已有守卫实例在运行，本进程退出」这类关键一行就写在这里，而用户不该为此去开文件。
fn guard_log_tail() -> String {
    let path = crate::update::guard_log_path();
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(_) => return "(读不到)".to_string(),
    };
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    let tail = lines[lines.len().saturating_sub(6)..].join("\n");
    let chars: Vec<char> = tail.chars().collect();
    let s: String = if chars.len() > 2000 {
        chars[chars.len() - 2000..].iter().collect()
    } else {
        tail.clone()
    };
    if s.trim().is_empty() { "(日志为空)".to_string() } else { s }
}

/// 服务管理器路径的就绪等待预算（原「60 tick x 500ms」；含每 tick 的探针成本）。
const SERVICE_READY_BUDGET: std::time::Duration = std::time::Duration::from_secs(30);
/// 兜底直接拉起后的就绪等待预算（守卫是刚 fork 的 node，冷启动比服务管理器路径慢）。
const DAEMON_READY_BUDGET: std::time::Duration = std::time::Duration::from_secs(60);

/// 守卫就绪判据（规范 H6 的**唯一**实现产物）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Readiness {
  /// `GET /healthz` 返回 2xx —— 唯一算「就绪」的形态。
    Ready,
  /// 端口 TCP 都连不上：守卫进程多半还没起来（或在顺延后的另一个端口）。
    PortClosed,
  /// 端口通了，但 `/healthz` 回了非 2xx —— 进程在、服务没好（或病了）。
    Http(u16),
  /// 端口通了，但 HTTP 请求没走完（超时 / 连接被 reset / 响应无法解析）。
    NoHttpResponse,
}

impl Readiness {
  /// 面向人的判定说明（进报错正文与就绪探针的 `reason`）。
    pub(crate) fn describe(self) -> String {
        match self {
            Readiness::Ready => "就绪".to_string(),
            Readiness::PortClosed => "端口不可达".to_string(),
            Readiness::Http(code) => format!("/healthz 返回 {}", code),
            Readiness::NoHttpResponse => "/healthz 无响应".to_string(),
        }
    }
}

/// 裸 TCP 可达判定 —— **只**用于「不再可达 = 进程已停」这类否定问题（重启前的等待）。
///
/// 就绪与否**不得**用它回答（那是 [`ready`] 的活）：端口能连只说明有人在听，
/// 守卫在绑定端口与真正可服务之间还有一大段启动过程。
pub(crate) fn port_open(port: u16) -> bool {
    let addr = format!("127.0.0.1:{}", port);
    if let Ok(mut it) = addr.to_socket_addrs() {
        if let Some(sa) = it.next() {
            return TcpStream::connect_timeout(&sa, std::time::Duration::from_millis(400)).is_ok();
        }
    }
    false
}

/// 契约判据：TCP 可达 **且** `GET /healthz` 2xx。
pub(crate) fn ready(port: u16, http_timeout: std::time::Duration) -> Readiness {
    if !port_open(port) {
        return Readiness::PortClosed;
    }
    match crate::domain::localhttp::http_get_local(port, "/healthz", http_timeout) {
        Some((code, _)) if (200..300).contains(&code) => Readiness::Ready,
        Some((code, _)) => Readiness::Http(code),
        None => Readiness::NoHttpResponse,
    }
}

/// 等待预算内轮询就绪（每 tick 500ms），返回最后一次判定。每 tick 重读实际端口：
/// 内核可能因端口占用而顺延并持久化（见 env::current_api_port），只盯固定端口会永远等不到已健康的守卫。
/// 用时长而非 tick 数计预算：每 tick 成本不是常数（多了 /healthz 一次往返），按 tick 计数会让
/// 总上界随网络状态漂移，而外层 guard_start 的预算是固定的。
pub(crate) fn await_ready(budget: std::time::Duration) -> Readiness {
    let started = std::time::Instant::now();
    let mut last = Readiness::PortClosed;
    loop {
        last = ready(crate::env::current_api_port(), READINESS_HTTP_TIMEOUT);
        if last == Readiness::Ready || started.elapsed() >= budget {
            return last;
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::time::Duration;

  /// 起一个假守卫：持续接管连接，每条按 `reply` 应答（`None` = 连上但不回一个字节）。
  /// 必须持续 accept 而非一次一答：ready() 先做裸 TCP 探测再看 /healthz，同一次判定会打开
  /// 两个连接。只 accept 一次的夹具会把真正的 HTTP 连接留在队列里无人接管，
  /// 「200 = 就绪」那条用例必然以无响应收场（假阴性）。
    fn fake_guard(reply: Option<&'static [u8]>) -> u16 {
        let l = TcpListener::bind("127.0.0.1:0").expect("假守卫应能绑定回环端口");
        let port = l.local_addr().expect("回环监听必有地址").port();
        let bytes = reply.map(|b| b.to_vec());
        std::thread::spawn(move || {
            for stream in l.incoming().flatten() {
                let bytes = bytes.clone();
                std::thread::spawn(move || {
                    let mut stream = stream;
                    let mut buf = [0u8; 256];
                    let _ = stream.read(&mut buf);
                    match bytes {
                        Some(b) => {
                            let _ = stream.write_all(&b);
                            let _ = stream.flush();
                        }
  // 端口通着、进程却不回话：占住连接不放，让探针自己撞到读超时。
                        None => std::thread::sleep(std::time::Duration::from_secs(3)),
                    }
                });
            }
        });
        port
    }

  /// 一个**无人监听**的端口（绑定后立即释放）。
    fn closed_port() -> u16 {
        let l = TcpListener::bind("127.0.0.1:0").expect("应能绑定回环端口");
        l.local_addr().expect("回环监听必有地址").port()
    }

  /// 就绪判据必须**分辨得出**四种现场 —— 它们的可操作结论完全不同：
  /// 没起来（继续等）/ 起了但病了（看日志）/ 应答 5xx（真失败）/ 健康（放行）。
  /// 旧实现只回 `bool`，于是「healthz 回 500」与「端口都没开」在报错里是同一句话。
    #[test]
    fn readiness_distinguishes_closed_healthz_and_sick() {
  // 2xx = 唯一算就绪的形态
        assert_eq!(
            ready(fake_guard(Some(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok")), Duration::from_millis(600)),
            Readiness::Ready
        );
  // 非 2xx = 进程在、服务没好，必须带上状态码（报错正文要能指到它）
        assert_eq!(
            ready(
                fake_guard(Some(b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\n\r\n")),
                Duration::from_millis(600)
            ),
            Readiness::Http(503)
        );
  // 连得上但不吐字 节= 无响应，**不得**混成 Http(0)（那等于把「没回话」说成「回了 0」）
        assert_eq!(
            ready(fake_guard(None), Duration::from_millis(300)),
            Readiness::NoHttpResponse
        );
  // 没人监听 = 端口不可达（回环上立刻 ECONNREFUSED，不会等满预算）
        assert_eq!(ready(closed_port(), Duration::from_millis(300)), Readiness::PortClosed);
    }

  /// 四种判定各自给出**不同**的人话：报错正文靠它区分「再等等」与「去看日志」。
    #[test]
    fn readiness_describe_is_not_a_single_string() {
        let all = [
            Readiness::Ready.describe(),
            Readiness::PortClosed.describe(),
            Readiness::Http(500).describe(),
            Readiness::NoHttpResponse.describe(),
        ];
        assert!(all[2].contains("500"), "HTTP 判定必须带状态码: {}", all[2]);
        let uniq: std::collections::HashSet<_> = all.iter().collect();
        assert_eq!(uniq.len(), 4, "四种判定文案不得撞车: {:?}", all);
    }
}


/// 等待期间单次 `/healthz` 的超时：必须**远小于** tick，否则预算会被探针自己吃满。
const READINESS_HTTP_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(800);
