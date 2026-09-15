//! 内核更新单写入者门禁（2026-09-15）—— A 方案「收敛为单写入者 = 壳」的可执行面。
//!
//! ## 为什么需要（真实根因）
//!
//! 内核 npm 包曾有两个写入者：内核自更新（\`POST /self-update/apply\` → \`runNpmInstall\`）
//! 与壳（启动门 2 的 \`core_apply\`）。两套版本判定、两种源策略（内核官方 registry vs 壳镜像），
//! 同一个全局 npm 包被两个进程写 —— 这就是「更新逻辑分裂」的根。
//!
//! 现契约：**内核包安装/升级只有壳一个写入者**；守卫只提供只读状态，从不安装/重启自己。
//! 面板由内核托管、运行在壳的内容 iframe 内、**没有 Tauri IPC**，故经 postMessage 请求壳主帧
//! 代执行。本门禁把该契约钉死：
//!   SW-1  桥契约常量：协议版本 = 1、三类消息、代执行命令名
//!   SW-2  shell.html 经 shell_bridge_contract 取常量（不硬编码消息类型）
//!   SW-3  shell.html 只接受内容 iframe 且 origin 回环，回复 targetOrigin = ev.origin
//!   SW-4  安装只有一处实现 core_apply_inner；kernel_update_apply 安装后由所有者停+重拉守卫
//!   SW-5  两个新命令已在 main.rs 注册
//!   SW-6  反向：判据能识别「不校验来源 / 回复 '*'」的桥（门禁非空转）
//!
//! 说明：这是**静态源码断言**（与 platform_launch_contract / bootstrap_flow 同一惯例）。

use std::fs;
use std::path::PathBuf;

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// 读源码并归一化换行为 LF（Windows 检出为 CRLF，多行断言会失配）。
fn read(rel: &str) -> String {
    let s = fs::read_to_string(manifest_dir().join(rel))
        .unwrap_or_else(|e| panic!("读取 {} 失败: {}", rel, e));
    if s.contains('\r') { s.replace("\r\n", "\n") } else { s }
}

fn has_all(src: &str, needles: &[&str]) -> Vec<String> {
    needles.iter().filter(|n| !src.contains(**n)).map(|n| n.to_string()).collect()
}

/// 桥是否合格的可判定谓词（SW-6 用它做反向自检，证明判据非空转）。
fn bridge_is_valid(src: &str) -> bool {
    has_all(src, &[
        "shell_bridge_contract",
        "BRIDGE.types.request",
        "BRIDGE.types.result",
        "BRIDGE.types.progress",
        "BRIDGE.cmd",
        "ev.source !== frame.contentWindow",
        "isLoopbackOrigin(ev.origin)",
        "replyToPanel(src, origin,",
    ]).is_empty()
}

// ── SW-1：桥契约常量（协议版本/消息类型/命令名）──
#[test]
fn sw1_bridge_contract_constants() {
    let src = read("src/bridge.rs");
    let missing = has_all(&src, &[
        "KERNEL_UPDATE_PROTOCOL_VERSION: u32 = 1",
        "\"dsh:kernel-update-request\"",
        "\"dsh:kernel-update-result\"",
        "\"dsh:kernel-update-progress\"",
        "\"kernel_update_apply\"",
    ]);
    assert!(missing.is_empty(), "SW-1 失败：桥契约常量缺失 {:?}", missing);
}

// ── SW-2：shell.html 经后端取常量，不硬编码消息类型 ──
#[test]
fn sw2_shell_html_consumes_contract() {
    let src = read("bootstrap/shell.html");
    assert!(src.contains("shell_bridge_contract"), "SW-2 失败：shell.html 未经 shell_bridge_contract 取契约");
    assert!(!src.contains("dsh:kernel-update-request"), "SW-2 失败：shell.html 硬编码了消息类型（应为单一事实源）");
    assert!(bridge_is_valid(&src), "SW-2 失败：shell.html 桥接线不完整（缺 {:?}）",
        has_all(&src, &["shell_bridge_contract", "BRIDGE.types.request", "BRIDGE.types.result", "BRIDGE.types.progress", "BRIDGE.cmd"]));
}

// ── SW-3：来源校验与回复目标（K2/K3）──
#[test]
fn sw3_source_and_origin_validated() {
    let src = read("bootstrap/shell.html");
    assert!(src.contains("ev.source !== frame.contentWindow"), "SW-3 失败：未限定消息来源为内容 iframe");
    assert!(src.contains("isLoopbackOrigin"), "SW-3 失败：未限制 origin 为回环");
    assert!(src.contains("h === '127.0.0.1'") && src.contains("h === 'localhost'"),
        "SW-3 失败：回环判定缺 127.0.0.1/localhost");
    assert!(src.contains("src.postMessage(msg, origin)"), "SW-3 失败：回复未指定 targetOrigin");
    assert!(!src.contains("postMessage(msg, '*')"), "SW-3 失败：回复使用了 '*'");
}

// ── SW-4：安装单一实现 + 重启由所有者完成 ──
#[test]
fn sw4_single_install_and_owner_restart() {
    let src = read("src/commands/mod.rs");
    let missing = has_all(&src, &[
        "async fn core_apply_inner",
        "core_apply_inner(app).await",
        "core_apply_inner(app.clone()).await",
        "pub async fn kernel_update_apply",
        "crate::platform::service().stop()",
        "guardctl::is_alive(port)",
        "guardctl::ensure_guard(&app)",
    ]);
    assert!(missing.is_empty(), "SW-4 失败：安装/重启语义缺失 {:?}", missing);
    let start = src.find("pub async fn kernel_update_apply").expect("kernel_update_apply");
    let body = &src[start..];
    let end = body.find("\n}").map(|i| start + i).unwrap_or(src.len());
    let body = &src[start..end];
    assert!(!body.contains("install_version(") && !body.contains("run_npm_install"),
        "SW-4 失败：kernel_update_apply 自行安装（应复用 core_apply_inner）");
}

// ── SW-5：命令已注册 ──
#[test]
fn sw5_commands_registered() {
    let src = read("src/main.rs");
    let missing = has_all(&src, &["commands::kernel_update_apply", "commands::shell_bridge_contract", "mod bridge;"]);
    assert!(missing.is_empty(), "SW-5 失败：main.rs 未注册 {:?}", missing);
}

// ── SW-6：反向自检（判据必须能识别坏桥）──
#[test]
fn sw6_reverse_judgement_is_not_vacuous() {
    let bad = "window.addEventListener('message', function (ev) { ev.source.postMessage(ev.data, '*'); });";
    assert!(!bridge_is_valid(bad), "SW-6 失败：判据把不校验来源、回复 '*' 的坏桥当成合格（门禁空转）");
    let good = read("bootstrap/shell.html");
    assert!(bridge_is_valid(&good), "SW-6 失败：判据无法识别合格桥");
}
