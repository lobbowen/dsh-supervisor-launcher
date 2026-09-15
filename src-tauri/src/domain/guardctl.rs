//! 守卫的启停与就绪判定（**生命周期所有权的调用方**）。
//!
//! 铁律：壳**不是**守卫的所有者 —— 它只向所有者（systemd / launchd / schtasks）
//! 提出请求，并在服务管理器不可用时走 spawn 兜底（可用性优先）。
//!
//! 退出时序（契约 §4.1）：① 带超时请求内核停全部被管对象并等回执；
//! ② 由所有者停止守卫；③ 守卫**不自停**。
//!
//! 启动规范见 docs/KERNEL-LAUNCH-STANDARD.md：P0 契约 → P1 对齐 → P3 定位 →
//!   P4 定义 → P5 启动 → P6 就绪。**P1 对齐是 P5 的前置**：磁盘内核必须等于线上最新，
//!   否则拒绝启动（绝不拉起磁盘上的旧内核）。
//!
//! 不变量 B1：所有等待都有上限 —— 退出流程也要能在服务管理器无响应时走完，
//!   否则用户会觉得「程序关不掉」。

use std::net::{TcpStream, ToSocketAddrs};
use tauri::{Emitter, Manager};

/// 启动失败的结构化错误（stage 由规范定义，见 KERNEL-LAUNCH-STANDARD.md §4）。
///   前端据此给可操作结论；日志据此定位到具体阶段 —— 不允许「未知错误」。
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

/// P1+P3：解析「与线上最新一致」的内核。
///
/// 顺序（规范 §3）：① 位置契约 core.json（版本须一致）→ ② 候选扫描中取版本一致者
///   （命中即**前向自愈**写入契约）→ ③ 都没有 = NotAligned。
/// **绝不**退回磁盘上的旧内核（这是「内核只有最新版本」的落地）。
pub(crate) fn resolve_aligned(app: &tauri::AppHandle) -> AlignOutcome {
    let pkg = match crate::core::package_name() {
        Ok(p) => p,
        Err(e) => return AlignOutcome::ResolveFailed(e),
    };
    let latest = match crate::core::latest_version(&pkg) {
        Ok((v, _o)) => v,
        Err(e) => return AlignOutcome::ResolveFailed(format!("{}：{}", pkg, e)),
    };
    // ① 位置契约命中且版本一致（最快路径）
    if let Some(c) = crate::core_contract::read() {
        if c.bin.is_file() && c.version == latest {
            return AlignOutcome::Aligned { bin: c.bin, version: latest };
        }
    }
    // ② 候选扫描：取版本 == latest 者，并记入契约（前向自愈）
    let cands = crate::domain::coreloc::locate_core_candidates(app.path().resource_dir().ok());
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

pub(crate) fn shutdown_all(port: u16) {
    // 契约 §4.1 退出握手（阶段 3 增强）：
    //   1) 带超时请求内核停全部被管对象，并等待回执（防止守卫挂起时壳无限阻塞）；
    //   2) 轮询 sessionState 直到 stopped（确认内核确实停好；守卫已不可达同样视为完成）；
    //   3) 由所有者停止守卫进程——守卫自身从不停止自己（阶段 1 所有权归一）。
    let _ = crate::domain::localhttp::post_local_timeout(port, "/session/stop", std::time::Duration::from_secs(60));
    for _ in 0..40 {
        match crate::domain::localhttp::get_session_state(port) {
            Some(s) if s == "stopped" => break, // 内核已确认停链完成
            None => break,                      // 守卫已不可达 = 已退出
            _ => std::thread::sleep(std::time::Duration::from_millis(250)),
        }
    }
    if let Err(e) = crate::platform::service().stop() {
        eprintln!("[shell] 停止守卫失败: {}（可手动 systemctl --user stop dsh-supervisor）", e);
    }
}

/// 拉起守卫（P0 契约 → P1 对齐 → P3 定位 → P4 定义 → P5 启动 → P6 就绪）。
/// 端口从用户 config.apiPort 解析（非硬编码 3100）。
pub(crate) fn ensure_guard(app: &tauri::AppHandle) -> Result<(), LaunchError> {
    let port = crate::env::current_api_port();
    // 本函数最长约 2 分钟：每个阶段都上报（静默等待与卡死无法区分）。
    let step = |s: &str| {
        let _ = app.emit("guard_progress", serde_json::json!({ "status": s }));
        crate::update::log(s);
    };
    if is_alive(port) { step("守卫已在运行"); return Ok(()); }

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
    let spec = crate::platform::LaunchSpec::from_runtime(&rt, guard);

    // P4 建立服务定义（幂等；否则首启 start 必失败）。
    step("正在建立守卫服务定义…");
    match crate::platform::service().ensure_defined(&spec) {
        Ok(desc) => crate::update::log(&format!("守卫服务定义: {}", desc)),
        Err(e) => crate::update::log(&format!("守卫服务定义失败（稍后走 spawn 兜底）: {}", e)),
    }

    // P5 请求服务管理器启动（正常路径：由 systemd/launchd/schtasks 托管，具备开机自启与崩溃自拉）
    step("正在请求服务管理器启动守卫…");
    let started = crate::platform::service().start();
    if let Err(e) = &started {
        crate::update::log(&format!("服务管理器启动失败: {}", e));
    }
    step("等待守卫就绪（服务管理器路径）…");
    if wait_alive(60) { return Ok(()); }

    // P5 兜底：直接拉起守护进程（容器/无 user session/策略拦截等场景）。
    step("服务管理器未能在 30s 内拉起守卫 · 改用直接启动兜底…");
    match crate::platform::service().spawn_daemon(&spec) {
        Ok(pid) => crate::update::log(&format!("兜底 spawn 守卫 pid={}", pid)),
        Err(e) => {
            return Err(LaunchError::new(
                "SERVICE_START_FAILED",
                format!(
                    "守卫启动失败：服务管理器错误({}) 且直接拉起也失败({})",
                    started.err().unwrap_or_else(|| "无".into()),
                    e
                ),
            ));
        }
    }
    if wait_alive(120) { return Ok(()); }

    Err(LaunchError::new(
        "READY_TIMEOUT",
        format!(
            "守卫启动超时（服务管理器与直接拉起均未就绪）。服务管理器错误：{}",
            started.err().unwrap_or_else(|| "无".into())
        ),
    ))
}

/// 轮询等待端口存活（每 tick 500ms）。
///
/// **每 tick 重读实际端口**：内核可能在启动时因端口占用而顺延并持久化（见
///   env::current_api_port）；只盯固定端口会永远等不到已健康的守卫。
pub(crate) fn wait_alive(ticks: u32) -> bool {
    for _ in 0..ticks {
        if is_alive(crate::env::current_api_port()) { return true; }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    false
}

pub(crate) fn is_alive(port: u16) -> bool {
    let addr = format!("127.0.0.1:{}", port);
    if let Ok(mut it) = addr.to_socket_addrs() {
        if let Some(sa) = it.next() {
            return TcpStream::connect_timeout(&sa, std::time::Duration::from_millis(400)).is_ok();
        }
    }
    false
}
