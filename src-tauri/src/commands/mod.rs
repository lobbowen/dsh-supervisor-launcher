//! IPC 命令边界层（**只做校验与委托**）—— 2026-09-11 从 main.rs 拆出。
//!
//! 分层职责（硬约束，门禁 G2/G3）：
//!
//! commands -> domain -> platform -> infra
//!
//! · 命令体**只**允许：参数校验 → 调 domain/platform → 组装返回值；
//! · **禁止**平台分支 #[cfg(target_os)]（那属于 platform 层，门禁 G1）；
//!
//! 拆出的直接收益：main.rs 从「定义全部命令」变成「只注册它们」。

use std::sync::Mutex;

// Tauri 的 trait 方法（`app.emit` / `app.state` / `get_webview_window`）
// 需要这些 trait 在作用域内 —— 不是「多余的 import」。
use tauri::{Emitter, Manager};

use crate::error::{ShellError, ShellResult};
use crate::RunState;



const SHELL_CHECK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);
const SHELL_DOWNLOAD_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20 * 60);

/// 环境状态查询（**纯本地、有界、可轮询**）。
///
/// == 架构修复（2026-09-11 二次，根因见 nodeprobe.rs） ==
///
/// 不变量：本命令必须快速返回（探测在分离线程，未完成即返回 probing=true）。
///
/// 不变量：不含网络 I/O（网络侧信息由 node_latest 单独提供）。
#[tauri::command]
pub async fn node_status(app: tauri::AppHandle) -> serde_json::Value {
    // 首次调用会启动探测线程；此后每次调用都复用在飞结果（不会堆积线程）。
    let budget = std::time::Duration::from_millis(900);
    let out = match tauri::async_runtime::spawn_blocking(move || crate::nodeprobe::status(budget)).await {
        Ok(o) => o,
        Err(_) => crate::nodeprobe::partial(),
    };

    let mut o = {
        let state = app.state::<Mutex<RunState>>();
        let st = state.lock().unwrap_or_else(|e| e.into_inner());
        let installed = out.version.clone().or_else(|| st.installed.clone());
        let mut o = crate::log(&st);
        o["installed"] = serde_json::json!(installed);
        // DSH 最低门槛判定（>=22.12）：引导页据此决定是否需装 Node
        o["minOk"] = serde_json::json!(crate::node::meets_minimum(installed.as_deref()));
        o["minRequired"] = serde_json::json!(crate::node::MIN_NODE);
        if let Some(latest) = &st.latest {
            o["outdated"] = serde_json::json!(crate::node::outdated(installed.as_deref(), latest));
        }
        o
    };
    // 契约落盘：只要探测到可用 Node 就写运行期契约（**不论是否由壳安装**）——
    //   否则「用户本机已有 Node」的机器永远没有 runtime.json，拉起守卫时无从绑定 node。
    if let (Some(p), Some(v)) = (out.path.as_ref(), out.version.as_ref()) {
        if let Some(rt) = crate::runtime_contract::derive(p, v) {
            crate::runtime_contract::write(&rt);
        }
    }
    o["probing"] = serde_json::json!(!out.finished);
    // 明确失败原因（到硬上限 / worker 异常）。前端据此立即给出可操作结论，
    // 而非等自己的预算耗尽后只报一句「超时」。
    o["probeError"] = match &out.error {
        Some(e) => serde_json::json!(e),
        None => serde_json::Value::Null,
    };
    o["nodePath"] = serde_json::json!(out.path.as_ref().map(|p| p.display().to_string()));
    o["elapsedMs"] = serde_json::json!(out.elapsed_ms);
    o["candidates"] = serde_json::json!(crate::nodeprobe::candidate_summary());
    // 「当前卡在哪个候选多久」——环境特有问题无法靠读代码确定，必须靠这份追踪。
    o["stuck"] = match crate::nodeprobe::current_stuck() {
        Some((d, ms)) => serde_json::json!({ "on": d, "ms": ms }),
        None => serde_json::Value::Null,
    };
    o["trace"] = serde_json::json!(out
        .trace
        .iter()
        .map(|t| serde_json::json!({
            "source": t.source, "path": t.path, "ms": t.ms, "ok": t.ok, "note": t.note
        }))
        .collect::<Vec<_>>());
    o
}

/// 预热镜像测速（立即返回，结果稍后经 mirror_cached 读取）。
///
/// 为什么单独成命令：镜像信息必须**与「是否需要下载 Node」解耦** ——
/// 否则 Node 已达标的用户（主力用户）永远看不到壳选了哪个源。
#[tauri::command]
pub fn mirror_warmup() -> serde_json::Value {
    crate::mirror::warmup_async();
    serde_json::json!({ "ok": true })
}

/// 读镜像预热缓存（**无网络 I/O**，任何时刻可安全轮询）。
#[tauri::command]
pub fn mirror_cached() -> serde_json::Value {
    match crate::mirror::cached() {
        Some(s) => serde_json::json!({
            "ready": true,
            "nodeBest": s.node_best,
            "nodeLatencyMs": s.node_latency_ms,
            "npmBest": s.npm_best,
            "npmLatencyMs": s.npm_latency_ms,
            "npmProbes": s.npm_probes.iter().map(|(src, ok, ms)| serde_json::json!({
                "source": src, "ok": ok, "latencyMs": ms
            })).collect::<Vec<_>>(),
            "at": s.at,
        }),
        None => serde_json::json!({ "ready": false }),
    }
}

/// 网络侧元数据：最新 LTS + **镜像选择结果**（与本地环境检测彻底分离）。
#[tauri::command]
pub async fn node_latest() -> serde_json::Value {
    match tauri::async_runtime::spawn_blocking(crate::node::latest_lts).await {
        Ok(Ok(c)) => serde_json::json!({
            "ok": true,
            "version": c.version,
            "file": c.file,
            "mirror": c.source,
            "latencyMs": c.latency_ms,
            "probes": c.probes.iter().map(|(s, ok, ms)| serde_json::json!({
                "source": s, "ok": ok, "latencyMs": ms
            })).collect::<Vec<_>>(),
        }),
        Ok(Err(e)) => serde_json::json!({ "ok": false, "error": e }),
        Err(e) => serde_json::json!({ "ok": false, "error": e.to_string() }),
    }
}

/// 引导页走完所有检测步骤后调用：进入面板（go_panel —— emit 守卫 URL，壳框架 iframe 切换）。
/// 由引导页 JS 在展示完整启动过程后触发，避免步骤一闪而过无感知。
#[tauri::command]
pub fn finish_boot(app: tauri::AppHandle) -> ShellResult<()> {
    crate::domain::windowing::go_panel(&app, false); // 引导完成：URL 变化即导航
    Ok(())
}

/// 互斥锁中毒恢复的**统一约定**（2026-09-11 架构修复）：
///
/// 全部 `RunState` 加锁点使用 `.unwrap_or_else(|e| e.into_inner())` 而非 `.unwrap()`。
/// 原实现一旦有任何线程在持锁期间 panic，该锁**永久中毒**，此后**所有**命令
/// 都在加锁处 panic —— 用户看到的是「重启也没用、功能永久失效」。
/// 锁内是普通状态快照（不承载跨字段不变式），中毒后仍可用，故取回内部值继续。
///
/// 原则：**一次 panic 不应让整个应用的功能不可恢复地失效。**
#[tauri::command]
pub fn start_node_install(state: tauri::State<Mutex<RunState>>, app: tauri::AppHandle) -> ShellResult<()> {
    let mut st = state.lock().unwrap_or_else(|e| e.into_inner());
    if st.busy { return Ok(()); }
    st.busy = true;
    st.error = None;
    st.progress = 0.0;
    st.status = "准备安装 Node.js LTS…".into();
    st.logs.clear();
    let handle = app.clone();
    std::thread::spawn(move || {
        let out = crate::run_install(&handle);
        let state = handle.state::<Mutex<RunState>>();
        let mut s = state.lock().unwrap_or_else(|e| e.into_inner());
        s.busy = false;
        match out {
            Ok((node_path, version)) => {
                s.installed = Some(version.clone());
                s.progress = 1.0;
                s.status = format!("Node.js {} 就绪，正在启动守卫…", version);
                s.logs.push(format!("安装完成: {} @ {}", version, node_path));
                crate::node::record_runtime_meta(&node_path, &version);
                // 探测缓存必须失效：新装的 Node 只有重新探测才会被发现
                // （否则引导页会在「已装好」之后仍报未检测到）。
                crate::nodeprobe::invalidate();
                let _ = handle.emit("env_done", serde_json::json!({ "version": version }));
            }
            Err(e) => {
                s.error = Some(e.clone());
                s.status = "环境就绪前置失败".into();
                s.logs.push(format!("失败: {}", e));
                let _ = handle.emit("env_error", serde_json::json!({ "error": e }));
            }
        }
        let _ = handle.emit("env_progress", crate::log(&s));
    });
    drop(st);
    Ok(())
}

/// 引导页查询用：内核是否已安装 + 当前版本 + 真实包名提示（不再硬编码平台字符串）。
///
/// 内核状态查询。
///
/// == 架构修复（2026-09-11）==
///
/// 必须 async：工作放阻塞线程池，主线程立即返回。
///
/// 不重复执行：locate_core 已取过版本，直接复用其返回。
#[tauri::command]
pub async fn core_status(app: tauri::AppHandle) -> serde_json::Value {
    let pkg = crate::core::package_name().unwrap_or_else(|_| "@dsh-sup/dsh-core-<platform>".into());
    let located = tauri::async_runtime::spawn_blocking(move || {
        let a = app.clone();
        crate::domain::coreloc::locate_core_with_version(&a)
    })
    .await
    .ok()
    .flatten();
    let (installed, version, path) = match located {
        Some((p, v)) => (true, Some(v), Some(p.display().to_string())),
        None => (false, None, None),
    };
    serde_json::json!({
        "installed": installed,
        "version": version,
        "path": path,
        "package": pkg,
        "hint": format!("npm i -g {}", pkg),
    })
}

/// 内核版本规划（引导页决策输入）：本地已装版本 + 远端最高版本 + 动作(install/upgrade/none)。
///
/// 两段都必须 spawn_blocking：locate_core 是同步阻塞调用，放主线程会拖慢整个 IPC。
#[tauri::command]
pub async fn core_plan(app: tauri::AppHandle) -> ShellResult<serde_json::Value> {
    // ① 本地定位 + 版本探测（阻塞，逐候选执行二进制）
    let installed = tauri::async_runtime::spawn_blocking(move || {
        let a = app.clone();
        crate::domain::coreloc::locate_core(&a).and_then(|b| crate::core::installed_version(&b))
    })
    .await
    .ok()
    .flatten();
    // ② 远端最高版本（网络，同样经线程池）
    let pkg = crate::core::package_name()?;
    let latest = tauri::async_runtime::spawn_blocking(move || crate::core::latest_version(&pkg))
        .await.map_err(|e| ShellError::ipc(e.to_string()))?;
    Ok(crate::core::build_plan(installed, latest))
}

/// 安装/升级内核到**最新版**（强制更新，只升不降，无回退）。
///   - 显式 `--prefix`（从现有内核路径反推）确保装回同一前缀（跨平台布局差异见 core.rs）；
///   - 逐个镜像回退（源回退，与版本回退无关）；
///   - 如实回传成败（含退出码/stderr），绝不吞错。
#[tauri::command]
pub async fn core_apply(app: tauri::AppHandle) -> ShellResult<serde_json::Value> {
    core_apply_inner(app).await
}

/// `core_apply` 的实现体（安装/升级到最新版，强制更新，只升不降）。
/// 抽成独立函数：启动门 2（`core_apply`）与面板请求（`kernel_update_apply`）**共用同一实现** ——
/// 内核包只能经这一处写入，符合「单写入者」契约（docs/DESIGN-SHELL-ARCHITECTURE.md §3.2c）。
async fn core_apply_inner(app: tauri::AppHandle) -> ShellResult<serde_json::Value> {
    // 契约先行：安装内核需要 npm，而 npm 的单一来源是运行期契约（缺失则解析并落盘）。
    let _ = crate::runtime_contract::ensure();
    let pkg = crate::core::package_name()?;
    let prefix = crate::domain::coreloc::locate_core(&app).and_then(|b| crate::core::global_prefix_for(&b));
    let origins = crate::core::registry_origins();
    // 强制更新：唯一目标是「当前最新」——不接受调用方指定版本，内核不存在回退路径。
    let target = {
        let p = pkg.clone();
        let r = tauri::async_runtime::spawn_blocking(move || crate::core::latest_version(&p))
            .await.map_err(|e| ShellError::ipc(e.to_string()))?;
        match r {
            Ok((v, _)) => v,
            Err(e) => return Ok(serde_json::json!({"ok": false, "stage": "resolve", "error": e})),
        }
    };
    // 失败回传用（闭包会 move origins）
    let origins_for_report = origins.clone();
    let pkg2 = pkg.clone();
    let target2 = target.clone();
    let pref = prefix.clone();
    let res = tauri::async_runtime::spawn_blocking(move || {
        let mut last = String::from("无可用镜像");
        for o in &origins {
            match crate::core::install_version(&pkg2, &target2, pref.as_deref(), Some(o.as_str())) {
                Ok(out) => return Ok::<(String, String), String>((o.clone(), out)),
                Err(e) => last = e,
            }
        }
        Err(last)
    }).await.map_err(|e| ShellError::ipc(e.to_string()))?;
    Ok(match res {
        // P2（安装成功后）：回读**确切位置**并写 core.json —— 位置单一事实源。
        //   必须先回读再报成功：否则「装上了却定位不到」（nvm/自定义 prefix）会被静默跳过，
        //   后续 guard_start 的对齐门会以 KERNEL_NOT_ALIGNED 拒绝，用户只看到「起不来」。
        Ok((origin, out)) => {
            let prefix_used = prefix.clone().or_else(crate::core::npm_global_prefix);
            match crate::domain::coreloc::locate_core_at_version(&app, &target, prefix_used.as_deref()) {
                Some(bin) => {
                    crate::core_contract::write(&crate::core_contract::InstalledCore {
                        bin: bin.clone(),
                        prefix: prefix_used.clone(),
                        version: target.clone(),
                        source: origin.clone(),
                    });
                    serde_json::json!({
                        "ok": true, "version": target, "origin": origin, "output": out,
                        "coreBin": bin.display().to_string(),
                        "prefix": prefix_used.map(|p| p.display().to_string()),
                    })
                }
                None => serde_json::json!({
                    "ok": false, "stage": "record",
                    "version": target, "origin": origin,
                    "error": "内核已安装但定位不到目标版本（安装前缀不一致）——已拒绝继续，请反馈此诊断",
                    "prefix": prefix_used.as_ref().map(|p| p.display().to_string()),
                }),
            }
        }
        // 失败分支必须回传 prefix 与尝试过的源（否则现场无法定位）。
        Err(e) => serde_json::json!({
            "ok": false, "version": target, "error": e,
            "prefix": prefix.as_ref().map(|p| p.display().to_string()),

            "originsTried": origins_for_report,
            "prefixIsNodeDir": prefix.as_ref().map(|p| crate::core::is_node_install_prefix(p)),
        }),
    })
}

/// 内核更新（**唯一写入者 = 壳**）：安装最新内核 → 由所有者停守卫 → 等端口释放 → 重新拉起。
///
/// 面板由内核托管、运行在壳主帧的内容 iframe 内，**没有 Tauri IPC**（IPC 仅主帧）；
/// 它经 postMessage 请求本命令（见 crate::bridge）。启动门 2 走 \`core_apply\`。
/// 两者共用同一个安装实现 \`core_apply_inner\` —— 内核包只有这一处写入。
///
/// 重启必须由**所有者（服务管理器）**完成：守卫从不重启自己（所有权契约 §2.2 V2/V3）。
#[tauri::command]
pub async fn kernel_update_apply(app: tauri::AppHandle) -> ShellResult<serde_json::Value> {
    // ① 安装/升级到最新（与启动门 2 完全同一实现）
    let install = core_apply_inner(app.clone()).await?;
    if install.get("ok").and_then(|v| v.as_bool()) != Some(true) {
        return Ok(serde_json::json!({ "ok": false, "stage": "install", "detail": install }));
    }
    // ②+③ 停守卫 → 等端口释放 → 重新拉起，**全部放线程池**（含阻塞 sleep，绝不占 async 运行时）。
    //    等端口释放是必须的：否则 ensure_guard 会误判「守卫已在运行」而直接返回，重启被静默跳过。
    let restart = tauri::async_runtime::spawn_blocking(move || -> Result<(bool, Option<String>), crate::domain::guardctl::LaunchError> {
        let port = crate::env::current_api_port();
        let stop_error = crate::platform::service().stop().err();
        let mut stopped = false;
        for _ in 0..60 {
            if !crate::domain::guardctl::is_alive(port) { stopped = true; break; }
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
        // ensure_guard：P1 对齐 → 建定义 → 请求所有者启动（不可用则 spawn 兜底）。
        crate::domain::guardctl::ensure_guard(&app)?;
        Ok((stopped, stop_error))
    }).await;
    match restart {
        Ok(Ok((stopped, stop_error))) => Ok(serde_json::json!({
            "ok": true, "stage": "done",
            "version": install.get("version").cloned().unwrap_or(serde_json::Value::Null),
            "origin": install.get("origin").cloned().unwrap_or(serde_json::Value::Null),
            "stopped": stopped,
            // 服务管理器不可用（spawn 兜底）时端口可能未由我们释放：如实标注，不假装已重启。
            "restartUncertain": !stopped,
            "stopError": stop_error,
        })),
        Ok(Err(e)) => Ok(serde_json::json!({ "ok": false, "stage": "restart", "code": e.code, "error": e.message, "detail": install })),
        Err(e) => Ok(serde_json::json!({ "ok": false, "stage": "restart", "error": e.to_string(), "detail": install })),
    }
}

/// 面板→壳 消息桥契约（单一事实源在 crate::bridge）：把协议版本、命令名与消息类型下发给 shell.html，
/// 使 shell.html **不硬编码**这些字面量（门禁 SW-3 锁定接线）。
#[tauri::command]
pub fn shell_bridge_contract() -> serde_json::Value {
    serde_json::json!({
        "v": crate::bridge::KERNEL_UPDATE_PROTOCOL_VERSION,
        "cmd": crate::bridge::CMD_KERNEL_UPDATE_APPLY,
        "types": {
            "request": crate::bridge::MSG_KERNEL_UPDATE_REQUEST,
            "result": crate::bridge::MSG_KERNEL_UPDATE_RESULT,
            "progress": crate::bridge::MSG_KERNEL_UPDATE_PROGRESS,
        }
    })
}

/// 产品状态根（诊断/支持用）：schema + 实际路径。**独立于 DSH 的 ~/.dsh**。
/// 同时使 \`STATE_ROOT_SCHEMA\` 成为可观测契约（与内核 state-root.js 的 SCHEMA 握手）。
#[tauri::command]
pub fn shell_state_root() -> serde_json::Value {
    serde_json::json!({
        "schema": crate::env::STATE_ROOT_SCHEMA,
        "root": crate::env::state_root().display().to_string(),
        "supervisor": crate::env::supervisor_dir().display().to_string(),
        "shell": crate::env::shell_dir().display().to_string(),
    })
}

/// 引导页驱动：申请所有者启动守卫（唯一启停权威，见 crate::platform::service::start）。阻塞放线程池。
#[tauri::command]
pub async fn guard_start(app: tauri::AppHandle) -> ShellResult<serde_json::Value> {
    // 必须有界：ensure_guard 内部可能长时间阻塞（服务管理器 + wait_alive + spawn 兜底）。
    // 外层超时保证命令仍会返回，前端据此给出重试/诊断入口。
    const GUARD_TOTAL_BUDGET: std::time::Duration = std::time::Duration::from_secs(180);
    let a = app.clone();
    let task = tauri::async_runtime::spawn_blocking(move || crate::domain::guardctl::ensure_guard(&a));
    let r = match tokio::time::timeout(GUARD_TOTAL_BUDGET, task).await {
        Ok(Ok(inner)) => inner,
        Ok(Err(e)) => Err(crate::domain::guardctl::LaunchError::new(
            "JOIN_ERROR",
            format!("守卫启动任务异常: {}", e),
        )),
        Err(_) => Err(crate::domain::guardctl::LaunchError::new(
            "READY_TIMEOUT",
            format!(
                "守卫启动超时（{} 秒未完成）。可能原因：服务管理器无响应，或守卫进程无法启动。请用 dsh-supervisor-gui --service-plan 查看服务定义状态。",
                GUARD_TOTAL_BUDGET.as_secs()
            ),
        )),
    };
    Ok(match r {
        Ok(()) => serde_json::json!({"ok": true}),
        // code：结构化阶段（规范 §4）；error 保持**字符串**——前端多处做 `'...' + e` 拼接。
        Err(e) => serde_json::json!({"ok": false, "code": e.code, "error": e.message}),
    })
}

/// 守卫就绪探针（TCP + HTTP /healthz 双确认）：引导页据此决定进面板，替代固定延时（K6）。
///
/// 必须 async：探测最长数秒且被高频轮询，必须放阻塞线程池，主线程立即返回。
#[tauri::command]
pub async fn guard_ready() -> serde_json::Value {
    tauri::async_runtime::spawn_blocking(|| {
        let port = crate::env::current_api_port();
        if !crate::domain::guardctl::is_alive(port) { return serde_json::json!({"ready": false, "reason": "tcp", "port": port}); }
        match crate::domain::localhttp::http_get_local(port, "/healthz", std::time::Duration::from_secs(3)) {
            Some((code, _)) if (200..300).contains(&code) => serde_json::json!({"ready": true, "port": port}),
            Some((code, _)) => serde_json::json!({"ready": false, "reason": "http", "status": code, "port": port}),
            None => serde_json::json!({"ready": false, "reason": "http", "port": port}),
        }
    })
    .await
    .unwrap_or_else(|_| serde_json::json!({"ready": false, "reason": "probe-panic"}))
}

/// 桌面自定义窗口控制（Phase 3b：无边框窗口 + 自绘标题栏）。
/// 前端 WindowTitlebar 按钮 → invoke("win_ctl", {action})：
///   "minimize" / "toggle-maximize" / "hide"（关闭按钮 = 隐藏到托盘，与 CloseRequested 语义一致）。
#[tauri::command]
pub fn win_ctl(app: tauri::AppHandle, action: String) -> ShellResult<()> {
    let win = app.get_webview_window("main").ok_or("主窗口不存在")?;
    match action.as_str() {
        "minimize" => win.minimize().map_err(|e| ShellError::ipc(e.to_string())),
        "toggle-maximize" => {
            if win.is_maximized().unwrap_or(false) {
                win.unmaximize().map_err(|e| ShellError::ipc(e.to_string()))
            } else {
                win.maximize().map_err(|e| ShellError::ipc(e.to_string()))
            }
        }
        "maximize" => win.maximize().map_err(|e| ShellError::ipc(e.to_string())),
        "unmaximize" => win.unmaximize().map_err(|e| ShellError::ipc(e.to_string())),
        "hide" => {
            let _ = win.hide();
            Ok(())
        }
        "drag" => {
            // 显式窗口拖动（Linux WebKitGTK drag-region 属性常不生效的可靠替代）：
            // 前端标题栏拖动区 mousedown → invoke win_ctl drag → 走 Rust start_dragging
            win.start_dragging().map_err(|e| ShellError::ipc(e.to_string()))
        }
        _ => Err(format!("不支持的窗口动作: {}（minimize/toggle-maximize/maximize/unmaximize/hide/drag）", action).into()),
    }
}

/// 退出管家（契约 §4.1 冻结时序）：请求内核停全部被管对象（同步等待回执）→ 由所有者停止守卫。
/// 内核在回执前**不会**停止自己（阶段 1 已移除内核自停）。
/// 无头自检：**环境探测**（架构修复后的可诊断入口）。
///
/// 在有界预算内跑完探测并打印逐候选追踪（卡住时可见「卡在谁、多久」）。
/// 用法：dsh-supervisor-gui --env-plan
/// 无头自检：**镜像测速与选择**。
///
/// 打印「测了哪些源、各自延迟、最终选了谁」，使镜像选择可核对。
/// 用法：dsh-supervisor-gui --mirror-plan
/// 打开面板（分体架构 2026-09-07 定稿）：
/// 壳 = 自绘窗口容器(shell.html 唯一窗口栏 + 内容 iframe)；面板由守卫内核 HTTP 托管（同源）。
/// 切面板 = emit 守卫实际 API 基址(读 config.apiPort, 动态端口不硬编码) → 壳 iframe 导航该 URL，
/// 页面与守卫 API 同源直连（无跨源/CORS 透传）。
/// 返回控制面板 URL（供壳框架在导航后自行取得面板地址）。
///
/// 由壳框架主动索取面板 URL，避免 shell:goto-panel 事件早于 listener 注册而丢失。
#[tauri::command]
pub fn shell_panel_url() -> serde_json::Value {
    let url = crate::env::api_base_url();
    // 落盘一行：**证明主帧导航确实完成**（引导页 → 壳框架）。
    // 这条日志也是可观测性的关键一环：从 shell.log 就能看出卡在引导页还是壳框架。
    crate::update::log(&format!("壳框架就绪（主帧导航完成），面板 URL: {}", url));
    serde_json::json!({ "url": url })
}

/// 壳身份快照（版本/安装形态/自更新能力），供引导页与诊断。
#[tauri::command]
pub fn shell_identity(app: tauri::AppHandle) -> serde_json::Value {
    let v = app.package_info().version.to_string();
    let mut id = crate::update::identity_snapshot();
    if id.get("version").is_none() {
        id = crate::update::init_identity(&v);
    }
    id
}

#[tauri::command]
pub fn shell_set_phase(phase: String) {
    crate::update::set_phase(&phase);
}

/// 读取当前镜像配置 + 并行探测延迟（供引导页展示与选择）。
#[tauri::command]
pub async fn mirror_status() -> ShellResult<serde_json::Value> {
    let m = crate::mirror::load();
    let node_probes = tauri::async_runtime::spawn_blocking(|| {
        let m = crate::mirror::load();
        crate::mirror::probe_all(&m.node, "index.json")
    })
    .await
    .map_err(|e| ShellError::ipc(e.to_string()))?;
    let npm_probes = tauri::async_runtime::spawn_blocking(|| {
        let m = crate::mirror::load();
        crate::mirror::probe_all(&m.npm, "")
    })
    .await
    .map_err(|e| ShellError::ipc(e.to_string()))?;
    let fmt = |v: Vec<crate::mirror::Probe>| {
        v.into_iter()
            .map(|p| serde_json::json!({ "source": p.source, "ok": p.ok, "latencyMs": p.latency_ms }))
            .collect::<Vec<_>>()
    };
    Ok(serde_json::json!({
        "ok": true,
        "node": m.node,
        "npm": m.npm,
        "shell": m.shell,
        "selectedNode": m.selected_node,
        "selectedNpm": m.selected_npm,
        "nodeProbes": fmt(node_probes),
        "npmProbes": fmt(npm_probes),
    }))
}

/// 保存用户自定义镜像（引导页失败时的自助出口）。
/// 入参为 URL 列表；保存后使缓存失效，并在 npm 类型时立即导出给内核（若已安装）。
#[tauri::command]
pub fn mirror_set(kind: String, urls: Vec<String>) -> ShellResult<serde_json::Value> {
    let mut m = crate::mirror::load();
    let list: Vec<String> = urls
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if list.is_empty() {
        return Err("镜像列表不能为空".into());
    }
    for u in &list {
        if !(u.starts_with("http://") || u.starts_with("https://")) {
            return Err(format!("镜像地址必须以 http(s):// 开头：{}", u).into());
        }
    }
    match kind.as_str() {
        "node" => m.node = list.clone(),
        "npm" => m.npm = list.clone(),
        "shell" => m.shell = list.clone(),
        _ => return Err(format!("未知镜像类型: {}（支持 node / npm / shell）", kind).into()),
    }
    m.checked_at = None; // 使缓存失效，下次重新测速
    if kind == "npm" {
        m.selected_npm = None; // 用户改了候选集 → 旧的选择结果失效
    }
    crate::mirror::save(&m)?;
    if kind == "npm" {
        // 导出**完整契约**（schema/catalog/selected/probe），而非仅 origins。
        let _ = crate::mirror::export_to_kernel(&m);
    }
    crate::update::log(&format!("镜像配置已更新 {}: {}", kind, list.join(", ")));
    Ok(serde_json::json!({ "ok": true, "kind": kind, "urls": list }))
}

/// 检查是否有壳更新。
/// 返回 { ok, available, current, latest, notes, skipped?, reason? }
/// 语义：跳过的原因一律**不阻断启动**（有界失败即放行）。
/// 桌面自更新的统一决策形状（与内核 core_plan 同一组键：artifact/current/latest/available/channel/source/error）。
///
/// 问题 1 的机制层统一：两侧用同一形状，前端不再是"两套流程"。执行器按产物分派
/// （壳=Tauri updater，内核=npm），但**决策模型是一套**。
fn shell_plan(
    cur: &str,
    latest: Option<String>,
    available: bool,
    err: Option<String>,
    notes: String,
    date: String,
) -> serde_json::Value {
    let mut extra = serde_json::Map::new();
    extra.insert("ok".into(), serde_json::json!(err.is_none()));
    extra.insert("current".into(), serde_json::json!(cur));
    extra.insert("notes".into(), serde_json::json!(notes));
    extra.insert("date".into(), serde_json::json!(date));
    crate::update_plan::unified("shell", Some(cur.to_string()), latest, available, None, err, extra)
}

#[tauri::command]
pub async fn shell_update_check(app: tauri::AppHandle) -> ShellResult<serde_json::Value> {
    let cur = app.package_info().version.to_string();
    crate::update::set_phase("shell-update-check");
    let updater = match crate::shell_updater(&app, SHELL_CHECK_TIMEOUT) {
        Ok(u) => u,
        Err(msg) => {
            crate::update::log(&format!("桌面更新检查失败：{}", msg));
            return Ok(shell_plan(&cur, None, false, Some(msg), String::new(), String::new()));
        }
    };
    // 双保险：reqwest 的 request timeout 不保证覆盖所有阶段（如 DNS），外层再包一层 tokio 超时。
    let checked = tokio::time::timeout(
        SHELL_CHECK_TIMEOUT + std::time::Duration::from_secs(5),
        updater.check(),
    )
    .await;
    match checked {
        Err(_) => {
            let msg = format!("检查超时（{} 秒无响应，可能网络不可达）", SHELL_CHECK_TIMEOUT.as_secs());
            crate::update::log("桌面更新检查超时（网络不可达？）");
            Ok(shell_plan(&cur, None, false, Some(msg), String::new(), String::new()))
        }
        Ok(Ok(Some(u))) => {
            let latest = u.version.clone();
            crate::update::log(&format!("桌面更新发现新版本 {}（当前 {}）", latest, cur));
            Ok(shell_plan(
                &cur,
                Some(latest),
                true,
                None,
                u.body.clone().unwrap_or_default(),
                u.date.map(|d| d.to_string()).unwrap_or_default(),
            ))
        }
        Ok(Ok(None)) => {
            crate::update::log(&format!("桌面更新已是最新（{}）", cur));
            Ok(shell_plan(&cur, None, false, None, String::new(), String::new()))
        }
        Ok(Err(e)) => {
            // 网络失败/清单不可达/验签失败 → 一律「失败放行」，由引导页决定是否重试
            let msg = format!("{}", e);
            crate::update::log(&format!("桌面更新检查失败：{}", msg));
            Ok(shell_plan(&cur, None, false, Some(msg), String::new(), String::new()))
        }
    }
}

/// 下载并安装更新（minisign 验签在插件内强制执行）。
/// 成功后**不自动重启**——由引导页统一调用 shell_restart（便于先告知用户）。
#[tauri::command]
pub async fn shell_update_apply(app: tauri::AppHandle) -> ShellResult<serde_json::Value> {
    crate::update::set_phase("shell-update-download");
    // 下载需要长超时；但 check 不能等那么久 → check 单独用 tokio 包短超时。
    let updater = crate::shell_updater(&app, SHELL_DOWNLOAD_TIMEOUT)?;
    let found = match tokio::time::timeout(SHELL_CHECK_TIMEOUT, updater.check()).await {
        Err(_) => {
            return Ok(serde_json::json!({ "ok": false, "error": "检查超时（可能网络不可达）" }));
        }
        Ok(Err(e)) => {
            return Ok(serde_json::json!({ "ok": false, "error": format!("检查失败: {}", e) }));
        }
        Ok(Ok(f)) => f,
    };
    let Some(u) = found else {
        return Ok(serde_json::json!({ "ok": true, "upToDate": true }));
    };
    let target = u.version.clone();

    crate::update::log(&format!("桌面更新开始下载 {}", target));

    // 用进度事件驱动前端进度条；on_chunk 给的是本块大小（增量），此处自行累加。
    let mut got: u64 = 0;
    let dl = tokio::time::timeout(
        SHELL_DOWNLOAD_TIMEOUT,
        u.download(
            |chunk, total| {
                got += chunk as u64;
                let _ = app.emit(
                    "shell_update_progress",
                    serde_json::json!({ "downloaded": got, "total": total }),
                );
            },
            || {},
        ),
    )
    .await;
    let bytes = match dl {
        Err(_) => {
            let msg = format!("下载超时（{} 分钟未完成）", SHELL_DOWNLOAD_TIMEOUT.as_secs() / 60);
            crate::update::log(&format!("桌面更新{}", msg));
            return Ok(serde_json::json!({ "ok": false, "error": msg }));
        }
        Ok(Err(e)) => {
            let msg = format!("下载失败: {}", e);
            crate::update::log(&format!("桌面更新{}", msg));
            return Ok(serde_json::json!({ "ok": false, "error": msg }));
        }
        Ok(Ok(b)) => b,
    };

    let total = bytes.len() as u64;
    let _ = app.emit(
        "shell_update_progress",
        serde_json::json!({ "downloaded": total, "total": total, "installing": true }),
    );
    crate::update::log(&format!("桌面更新下载完成（{} 字节），开始安装 {}", total, target));

    // Windows：install 启动安装程序后 std::process::exit(0)，**不会返回**；
    //   Linux/macOS：返回后由引导页调用 shell_restart 重启进入新版本。
    if let Err(e) = u.install(bytes) {
        let msg = format!("安装失败: {}", e);
        crate::update::log(&format!("桌面更新{}", msg));
        return Ok(serde_json::json!({ "ok": false, "error": msg }));
    }
    crate::update::log(&format!("桌面更新安装完成 {}", target));
    Ok(serde_json::json!({ "ok": true, "installed": target }))
}

/// 重启进入新版本（旧进程装、新进程跑）。
#[tauri::command]
pub fn shell_restart(app: tauri::AppHandle) {
    crate::update::set_phase("restarting");
    crate::update::log("桌面更新重启以应用新版本");
    app.restart();
}
