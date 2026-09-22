//! 运行期启动契约门禁（2026-09-15）—— Phase 1 不变量的可执行面。
//!
//! ## 为什么需要（真实根因）
//!
//! 内核守卫是 `#!/usr/bin/env node` 脚本。旧实现里 Node/npm 有**四处独立推导**
//! （nodeprobe / npm install / systemd ExecStart / spawn_daemon），且服务定义不绑定 node。
//! 实测（nvm 用户）：systemd --user 的 PATH 不含 nvm 的 node 目录 →
//! `ExecStart` 以 127 失败 → 内核「装上了却永远拉不起来」。
//!
//! 本门禁把 Phase 1 的修复钉死：
//!   L-1  三平台服务定义统一指向稳定入口 `<壳> --run-guard`（运行时检测 node/guard，定义内无路径）
//!   L-2  运行期契约 runtime.json 含 schema 与 node/npm 键，且保留内核已读的旧键
//!   L-3  端口发现读 ports.json 的 supervisor-api 实际值；等待循环每 tick 重读
//!   L-4  反向：判据能识别「未绑定 Node」的旧形态（门禁非空转）
//!
//! 说明：这些是**静态源码断言**（CI 三平台工具链无法同时编译三份 cfg 分支，
//!   与 platform_shared_items / platform_unsupported_structure 同一惯例）。

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

// ── L-1：三平台服务定义统一指向**稳定入口**（运行时检测 node/guard） ──
//
// 架构（2026-09-15 二次修正）：服务定义**不得**再固化 node/guard 路径。
//   三平台定义只写 `<壳> --run-guard`（由 service_command/service_exec_line 单源组装）；
//   `--run-guard` 每次启动重新检测 node/guard 后 exec（platform::exec_guard）。
#[test]
fn l1_three_platforms_share_stable_entry() {
    for f in ["src/platform/linux.rs", "src/platform/macos.rs", "src/platform/windows.rs"] {
        let src = read(f);
        let missing = has_all(&src, &["service_command()"]);
        assert!(missing.is_empty(), "L-1 失败：{} 未用统一稳定入口，缺 {:?}", f, missing);
    }
}

/// 稳定入口的定义本身：service_command = 壳自身 + --run-guard（platform/mod.rs 单源）。
#[test]
fn l1_service_command_is_shell_run_guard() {
    let m = read("src/platform/mod.rs");
    let missing = has_all(&m, &["pub shell:", "fn service_command", "\"--run-guard\"", "pub fn service_exec_line"]);
    assert!(missing.is_empty(), "L-1 失败：稳定入口定义缺失 {:?}", missing);
}

/// 执行侧：--run-guard 解析出的 node/guard 由 platform::exec_guard 执行（唯一执行点）。
#[test]
fn l1_run_guard_executes_node_guard() {
    let m = read("src/platform/mod.rs");
    assert!(m.contains("pub fn exec_guard"), "L-1 失败：缺 exec_guard（--run-guard 执行点）");
    let body = match m.find("pub fn exec_guard") { Some(i) => &m[i..], None => "" };
    assert!(body.contains("spec.node") && body.contains("spec.guard"),
        "L-1 失败：exec_guard 未用契约 node/guard 执行");
}

/// spawn 兜底统一（同一 trait 默认实现）：启动稳定入口，而非各自拼 node/guard。
#[test]
fn l1_spawn_daemon_is_uniform_stable_entry() {
    let s = read("src/platform/service.rs");
    let missing = has_all(&s, &["fn spawn_daemon", "spec.shell", "\"--run-guard\""]);
    assert!(missing.is_empty(), "L-1 失败：spawn 兜底未统一到稳定入口，缺 {:?}", missing);
    // 反向：平台文件不得再有各自的 spawn_daemon（统一实现的意义）。
    for f in ["linux.rs", "macos.rs", "windows.rs"] {
        let src = read(&format!("src/platform/{}", f));
        assert!(!src.contains("fn spawn_daemon"), "L-1 失败：{} 仍有平台专属 spawn_daemon（应统一）", f);
    }
}

// ── L-2：运行期契约内容 ──
#[test]
fn l2_runtime_contract_schema_and_keys() {
    let src = read("src/runtime_contract.rs");
    let missing = has_all(
        &src,
        &[
            "pub const SCHEMA",
            "\"schema\"",
            "\"nodePath\"",
            "\"nodeVersion\"",
            "\"nodeBinDir\"",
            "\"npmPath\"",
            "\"minNode\"", // 内核 env-catalog 已读的旧键，必须保留
        ],
    );
    assert!(missing.is_empty(), "L-2 失败：runtime.json 契约缺键 {:?}", missing);
}

#[test]
fn l2_contract_owner_is_shell_and_consumed_by_install_and_start() {
    // 单一事实源：安装（core.rs）与拉起（guardctl.rs）都必须读契约。
    let core = read("src/core.rs");
    let gc = read("src/domain/guardctl.rs");
    let cmds = read("src/commands/mod.rs");
    for (name, src, needle) in [
        ("core.rs", core.as_str(), "runtime_contract::read_node"),
        ("guardctl.rs", gc.as_str(), "runtime_contract::ensure"),
        ("commands/mod.rs", cmds.as_str(), "runtime_contract"),
    ] {
        assert!(src.contains(needle), "L-2 失败：{} 未接入运行期契约（缺 {}）", name, needle);
    }
}

// ── L-3：端口发现 ──
#[test]
fn l3_port_discovery_from_ports_json_and_reread_in_wait() {
    let env = read("src/env.rs");
    let missing = has_all(&env, &["discovered_api_port", "supervisor-api", "ports.json", "current_api_port"]);
    assert!(missing.is_empty(), "L-3 失败：env.rs 未从 ports.json 发现实际端口，缺 {:?}", missing);
    let gc = read("src/domain/guardctl.rs");
    assert!(
        gc.contains("current_api_port") && gc.contains("fn await_ready(budget"),
        "L-3 失败：guardctl 未在等待循环每 tick 重读实际端口"
    );
}

// ── L-5：契约版本握手（两侧各自断言 schema=2；不跨仓读源码）──
#[test]
fn l5_contract_schema_version_is_2() {
    let src = read("src/runtime_contract.rs");
    assert!(
        src.contains("pub const SCHEMA: u32 = 2"),
        "L-5 失败：runtime.json 契约 schema 版本不是 2（与内核 runtime-contract.js 的 SUPPORTED_SCHEMA 失配）"
    );
}

// ── L-4：反向自检（判据必须能识别旧形态）──
#[test]
fn l4_reverse_judgement_is_not_vacuous() {
    // 旧 Linux unit 形态：只有 guard、无 node、无 PATH → 必须被判为缺项。
    let old = "[Service]\nExecStart=\"@BIN@\" daemon\n";
    let missing = has_all(old, &["spec.node.display()", "Environment=", "@PATH@"]);
    assert!(!missing.is_empty(), "L-4 失败：判据无法识别未绑定 Node 的旧 unit（门禁空转）");
    // 新形态样例必须通过。
    let new = "Environment=\"PATH=@PATH@\"\nExecStart=@EXEC@\nspec.node.display()";
    assert!(has_all(new, &["spec.node.display()", "Environment=", "@PATH@"]).is_empty());
}
