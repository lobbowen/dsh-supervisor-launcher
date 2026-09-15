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
//!   L-1  三平台服务定义/包装脚本**必须显式绑定 Node**（node 路径或注入 PATH）
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

// ── L-1：三平台服务定义必须显式绑定 Node ──
#[test]
fn l1_linux_unit_binds_node() {
    let src = read("src/platform/linux.rs");
    // 允许两种等价实现：显式 node 可执行 + Environment PATH。要求两者都在。
    let missing = has_all(&src, &["spec.node.display()", "spec.guard.display()", "Environment=", "@PATH@", "daemon"]);
    assert!(missing.is_empty(), "L-1 失败：Linux unit 未显式绑定 Node/PATH，缺 {:?}", missing);
}

#[test]
fn l1_macos_plist_binds_node() {
    let src = read("src/platform/macos.rs");
    let missing = has_all(&src, &["@NODE@", "@BIN@", "@PATH@", "EnvironmentVariables", "ProgramArguments"]);
    assert!(missing.is_empty(), "L-1 失败：macOS plist 未显式绑定 Node/PATH，缺 {:?}", missing);
}

#[test]
fn l1_windows_wrapper_binds_node() {
    let src = read("src/platform/windows.rs");
    let missing = has_all(&src, &["%PATH%", "node_dir", "spec.node", "spec.guard"]);
    assert!(missing.is_empty(), "L-1 失败：Windows 包装脚本未注入 Node PATH，缺 {:?}", missing);
}

/// Windows 必须**显式用 node 执行守卫**（无扩展名 Node 脚本，cmd 不能执行）。
#[test]
fn l1_windows_executes_guard_via_node() {
    let src = read("src/platform/windows.rs");
    assert!(src.contains("fn guard_argv"), "L-1 失败：缺 guard_argv（Windows 执行器）");
    assert!(windows_guard_argv_uses_node(&src),
        "L-1 失败：Windows 未显式用 node 执行守卫 —— schtasks /Run 会成功但守卫永不起来");
    // 反向：旧形态（guard_argv 只用 guard，不含 node）必须被判为不合格
    let old = "fn guard_argv(spec: &LaunchSpec) -> String { spec.guard only }";
    assert!(!windows_guard_argv_uses_node(old), "L-1 反向失败：裸守卫形态被判为合格（门禁空转）");
}

fn windows_guard_argv_uses_node(src: &str) -> bool {
    let f = match src.find("fn guard_argv") { Some(i) => &src[i..], None => return false };
    let body = match f.find("\n}") { Some(i) => &f[..i], None => f };
    body.contains("spec.node")
}

#[test]
fn l1_spawn_daemon_binds_node() {
    // 三平台 spawn 兜底都必须用契约里的 node/PATH，而不是 ambient PATH。
    for f in ["linux.rs", "macos.rs"] {
        let src = read(&format!("src/platform/{}", f));
        let missing = has_all(&src, &["Command::new(&spec.node)", ".env(\"PATH\", &spec.env_path)"]);
        assert!(missing.is_empty(), "L-1 失败：{} spawn_daemon 未绑定 node/PATH，缺 {:?}", f, missing);
    }
    let w = read("src/platform/windows.rs");
    let missing = has_all(&w, &[".env(\"PATH\", &spec.env_path)"]);
    assert!(missing.is_empty(), "L-1 失败：windows spawn_daemon 未注入 PATH，缺 {:?}", missing);
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
        gc.contains("current_api_port") && gc.contains("fn wait_alive(ticks"),
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
