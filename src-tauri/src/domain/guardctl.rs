//! 守卫的启停与就绪判定（**生命周期所有权的调用方**）。
//!
//! 铁律：壳**不是**守卫的所有者 —— 它只向所有者（systemd / launchd / schtasks）
//! 提出请求，并在服务管理器不可用时走 spawn 兜底（可用性优先）。
//!
//! 退出时序（契约 §4.1）：① 带超时请求内核停全部被管对象并等回执；
//! ② 由所有者停止守卫；③ 守卫**不自停**。
//!
//! 不变量 B1：所有等待都有上限 —— 退出流程也要能在服务管理器无响应时走完，
//!   否则用户会觉得「程序关不掉」。

use std::net::{TcpStream, ToSocketAddrs};
use tauri::Emitter;

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

/// 拉起守卫（定位已安装内核 + 请求所有者启动），等面板就绪。
/// 端口从用户 config.apiPort 解析（非硬编码 3100）。
pub(crate) fn ensure_guard(app: &tauri::AppHandle) -> Result<(), String> {
    let port = crate::env::current_api_port();
// 本函数最长约 2 分钟：每个阶段都上报（静默等待与卡死无法区分）。
    let step = |s: &str| {
        let _ = app.emit("guard_progress", serde_json::json!({ "status": s }));
        crate::update::log(s);
    };
    if is_alive(port) { step("守卫已在运行"); return Ok(()); }

    // 运行期契约：Node/npm 的**单一事实源**（缺失则先解析并原子落盘）。
    //   守卫是 env-node 脚本；服务管理器/spawn 的 ambient PATH 经常不含壳解析出的
    //   Node —— 这是「装了却起不来」的根因，故拉起前必须先有契约。
    let rt = crate::runtime_contract::ensure()
        .ok_or_else(|| "Node 运行环境未就绪：无法解析 node/npm（请先完成环境准备）".to_string())?;
    let guard = crate::domain::coreloc::locate_core(app).ok_or_else(|| match crate::core::package_name() {
        Ok(p) => format!("未检测到内核。请先安装：npm i -g {}", p),
        Err(e) => format!("未检测到内核：{}", e),
    })?;
    let spec = crate::platform::LaunchSpec::from_runtime(&rt, guard);

    // ① 建立服务定义（**首次安装的关键一步**）。
// 必须先建立服务定义（幂等）；否则首启 start 必失败。
    step("正在建立守卫服务定义…");
    match crate::platform::service().ensure_defined(&spec) {
        Ok(desc) => crate::update::log(&format!("守卫服务定义: {}", desc)),
        Err(e) => crate::update::log(&format!("守卫服务定义失败（稍后走 spawn 兜底）: {}", e)),
    }

    // ② 请求服务管理器启动（正常路径：由 systemd/launchd/schtasks 托管，具备开机自启与崩溃自拉）
    step("正在请求服务管理器启动守卫…");
    let started = crate::platform::service().start();
    if let Err(e) = &started {
        crate::update::log(&format!("服务管理器启动失败: {}", e));
    }
    step("等待守卫就绪（服务管理器路径）…");
    if wait_alive(60) { return Ok(()); }

    // ③ 兜底：直接拉起守护进程。
    //    服务管理器不可用的场景真实存在（容器/无 user session/策略拦截），
    //    此时若不给兜底，用户将被永久挡在门外。
    step("服务管理器未能在 30s 内拉起守卫 · 改用直接启动兜底…");
    match crate::platform::service().spawn_daemon(&spec) {
        Ok(pid) => crate::update::log(&format!("兜底 spawn 守卫 pid={}", pid)),
        Err(e) => {
            return Err(format!(
                "守卫启动失败：服务管理器错误({}) 且直接拉起也失败({})",
                started.err().unwrap_or_else(|| "无".into()),
                e
            ));
        }
    }
    if wait_alive(120) { return Ok(()); }

    Err(format!(
        "守卫启动超时（服务管理器与直接拉起均未就绪）。服务管理器错误：{}",
        started.err().unwrap_or_else(|| "无".into())
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
