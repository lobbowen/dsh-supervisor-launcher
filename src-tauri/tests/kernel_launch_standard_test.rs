//! 内核启动规范门禁（K-1..K-6；docs/KERNEL-LAUNCH-STANDARD.md）—— 2026-09-15。
//!
//! 锁定 P0–P6 的**规范骨架**，防止回退成「按 PATH 猜位置 / 不对齐就启动 / 平台各自为政」：
//!   K-1  core.json 位置契约存在、schema=1、原子写、字段齐全
//!   K-2  locate 先读 core.json，再由 runtime.json 派生 nodeBinDir（反向判据非空转）
//!   K-3  guardctl 有对齐解析；ensure_guard **先对齐后启动**（顺序断言）
//!   K-4  四平台服务实现同构（node+PATH+daemon）；trait 有 prefix 候选且 Windows 覆写
//!   K-5  就绪只认契约端口（current_api_port = ports.json 实际值优先）
//!   K-6  平台分支只在 platform/（core.json 读取不得引入平台分支）
//!
//! 静态源码断言（与 platform_launch_contract / bootstrap_flow 同一惯例）。

use std::fs;
use std::path::PathBuf;

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(rel: &str) -> String {
    let s = fs::read_to_string(manifest_dir().join(rel))
        .unwrap_or_else(|e| panic!("读取 {} 失败: {}", rel, e));
    if s.contains('\r') { s.replace("\r\n", "\n") } else { s }
}

fn has_all(src: &str, needles: &[&str]) -> Vec<String> {
    needles.iter().filter(|n| !src.contains(**n)).map(|n| n.to_string()).collect()
}

// ── K-1：位置契约 core.json ──
#[test]
fn k1_core_contract() {
    let src = read("src/core_contract.rs");
    let missing = has_all(&src, &[
        "pub const SCHEMA: u32 = 1",
        "pub struct InstalledCore",
        "pub bin: PathBuf",
        "pub prefix: Option<PathBuf>",
        "pub version: String",
        "pub source: String",
        "pub fn read()",
        "with_extension(\"json.tmp\")", // 原子写
    ]);
    assert!(missing.is_empty(), "K-1 失败：core.json 契约缺失 {:?}", missing);
    // 与 runtime.json 同域、同写者语义
    assert!(src.contains("supervisor_dir"), "K-1 失败：契约未落在 supervisor 域");
    let main = read("src/main.rs");
    assert!(main.contains("mod core_contract;"), "K-1 失败：main.rs 未注册 core_contract");
}

// ── K-2：locate 先契约、再运行期派生 ──
#[test]
fn k2_locate_prefers_contract() {
    let src = read("src/domain/coreloc.rs");
    assert!(src.contains("crate::core_contract::read()"), "K-2 失败：locate 未读 core.json 契约");
    assert!(src.contains("crate::runtime_contract::read_node()"), "K-2 失败：locate 未读 runtime.json");
    assert!(src.contains("node_bin_dir.join(name)"), "K-2 失败：未由 nodeBinDir 派生候选");
    assert!(covers_contract(&src), "K-2 失败：判据不完整");
}

/// 判据谓词（K-2 反向自检用）。
fn covers_contract(src: &str) -> bool {
    has_all(src, &["crate::core_contract::read()", "crate::runtime_contract::read_node()", "node_bin_dir.join(name)"]).is_empty()
}

// ── K-3：先对齐后启动 ──
#[test]
fn k3_align_before_start() {
    let src = read("src/domain/guardctl.rs");
    let missing = has_all(&src, &[
        "enum AlignOutcome",
        "fn resolve_aligned",
        "KERNEL_NOT_ALIGNED",
        "ALIGN_RESOLVE_FAILED",
    ]);
    assert!(missing.is_empty(), "K-3 失败：对齐解析缺失 {:?}", missing);
    let align = src.find("resolve_aligned(app)").expect("K-3 未调用 resolve_aligned");
    let define = src.find("ensure_defined(&spec)").expect("K-3 未建服务定义");
    let start = src.find("service().start()").expect("K-3 未启动");
    assert!(align < define && define < start, "K-3 失败：ensure_guard 未按「对齐 → 定义 → 启动」顺序");
}

// ── K-4：四平台服务同构 + prefix 候选 trait ──
#[test]
fn k4_platforms_uniform() {
    for (f, node, path) in [
        ("src/platform/linux.rs", "spec.node", "@PATH@"),
        ("src/platform/macos.rs", "@NODE@", "@PATH@"),
        ("src/platform/windows.rs", "node_dir", "%PATH%"),
    ] {
        let s = read(f);
        let missing = has_all(&s, &[node, path, "daemon"]);
        assert!(missing.is_empty(), "K-4 失败：{} 未同构绑定 node/PATH/daemon，缺 {:?}", f, missing);
    }
    let m = read("src/platform/mod.rs");
    assert!(m.contains("fn core_bin_candidates_in_prefix"), "K-4 失败：trait 缺 prefix 候选（P2 记录位置）");
    let w = read("src/platform/windows.rs");
    assert!(w.contains("fn core_bin_candidates_in_prefix"), "K-4 失败：Windows 未覆写 prefix 候选（.cmd + 包内脚本）");
}

// ── K-5：就绪只认契约端口 ──
#[test]
fn k5_readiness_uses_contract_port() {
    let env = read("src/env.rs");
    assert!(env.contains("pub fn current_api_port") && env.contains("discovered_api_port().unwrap_or_else(api_port)"),
        "K-5 失败：current_api_port 未以 ports.json 实际值优先");
    let cmd = read("src/commands/mod.rs");
    let gr = cmd.find("pub async fn guard_ready").expect("K-5 未找到 guard_ready");
    let body = &cmd[gr..];
    let end = body.find("\n}").map(|i| gr + i).unwrap_or(cmd.len());
    let body = &cmd[gr..end];
    assert!(body.contains("current_api_port()"), "K-5 失败：guard_ready 未用 current_api_port");
    assert!(!body.contains("36360") && !body.contains("3100"), "K-5 失败：guard_ready 硬编码端口");
}

// ── K-6：平台分支只在 platform/ ──
#[test]
fn k6_no_platform_branch_in_contract_path() {
    for f in ["src/core_contract.rs", "src/domain/coreloc.rs", "src/domain/guardctl.rs"] {
        let s = read(f);
        assert!(!s.contains("cfg(target_os") && !s.contains("std::env::consts::OS"),
            "K-6 失败：{} 出现平台分支（应只在 platform/）", f);
    }
}

// ── K-2 反向：判据必须能识别「不读契约」的旧形态 ──
#[test]
fn k2_reverse_not_vacuous() {
    let old = "for name in core_exe_names() { if let Some(p) = find_in_path(name) { out.push(p); } }";
    assert!(!covers_contract(old), "K-2 反向失败：旧「只按 PATH 猜」形态被判为合格（门禁空转）");
    assert!(covers_contract(&read("src/domain/coreloc.rs")), "K-2 反向失败：当前实现未被判为合格");
}

// ── K-7：Windows 看护任务的所有者 = 壳（G3/C2）──
#[test]
fn k7_windows_watchdog_owned_by_shell() {
    let src = read("src/platform/windows.rs");
    let missing = has_all(&src, &[
        "fn ensure_watchdog",
        "WATCHDOG_TASK",
        "fn watchdog_script",
        "\"/TN\", WATCHDOG_TASK",
        "\"MINUTE\"",
    ]);
    assert!(missing.is_empty(), "K-7 失败：壳未建立 Windows 看护，缺 {:?}", missing);
    assert!(shell_creates_watchdog(&src), "K-7 失败：判据不完整");
}

/// 判据谓词（K-7 反向自检用）。
fn shell_creates_watchdog(src: &str) -> bool {
    has_all(src, &["fn ensure_watchdog", "WATCHDOG_TASK", "\"MINUTE\""]).is_empty()
}

#[test]
fn k7_reverse_not_vacuous() {
    // 旧形态：只 stop（/End）不 create（/Create）→ 必须判为未落实。
    let old = "schtasks /End /TN DSH-Supervisor-Watchdog";
    assert!(!shell_creates_watchdog(old), "K-7 反向失败：只停不建的形态被判为合格");
    assert!(shell_creates_watchdog(&read("src/platform/windows.rs")), "K-7 反向失败：当前实现未判合格");
}

// ── K-8：产品状态根独立于 DSH（XDG；与内核 state-root.js 握手）──
#[test]
fn k8_state_root_independent_of_dsh() {
    let env = read("src/env.rs");
    let missing = has_all(&env, &[
        "STATE_ROOT_SCHEMA: u32 = 1",
        "DSH_SUPERVISOR_HOME",
        "pub fn state_root",
        "pub fn supervisor_dir",
        "pub fn shell_dir",
        "pub fn migrate_legacy",
    ]);
    assert!(missing.is_empty(), "K-8 失败：env.rs 状态根契约缺失 {:?}", missing);
    // 负向：supervisor_dir 不得再拼 .dsh
    assert!(state_root_is_independent(&env), "K-8 失败：env.rs 仍拼 ~/.dsh");
    // 平台默认值在 platform/（G1），三平台同构
    let m = read("src/platform/mod.rs");
    assert!(m.contains("fn state_root_default"), "K-8 失败：trait 缺 state_root_default");
    assert!(read("src/platform/macos.rs").contains("fn state_root_default"), "K-8 失败：macOS 未覆写状态根");
    assert!(read("src/platform/windows.rs").contains("fn state_root_default"), "K-8 失败：Windows 未覆写状态根");
    // 可观测：诊断命令已注册
    assert!(read("src/commands/mod.rs").contains("pub fn shell_state_root"), "K-8 失败：缺 shell_state_root 命令");
    assert!(read("src/main.rs").contains("commands::shell_state_root"), "K-8 失败：命令未注册");
}

fn state_root_is_independent(src: &str) -> bool {
    // 允许 migrate_legacy 出现旧路径；只要求 supervisor_dir 的**函数体**不拼 .dsh。
    let f = match src.find("pub fn supervisor_dir") { Some(i) => &src[i..], None => return false };
    let body = match f.find("\n}") { Some(i) => &f[..i], None => f };
    !body.contains(".dsh")
}

#[test]
fn k8_reverse_not_vacuous() {
    let old = "pub fn supervisor_dir() -> PathBuf { PathBuf::from(home).join(\".dsh\").join(\"supervisor\") }";
    assert!(!state_root_is_independent(old), "K-8 反向失败：旧 ~/.dsh 形态被判为独立");
    assert!(state_root_is_independent(&read("src/env.rs")), "K-8 反向失败：当前实现未判合格");
}
// ── K-9：状态根随启动注入（单一事实源，防壳/内核各自推导分叉）──
#[test]
fn k9_state_root_injected_into_launch() {
    let m = read("src/platform/mod.rs");
    assert!(m.contains("pub state_root: std::path::PathBuf"), "K-9 失败：LaunchSpec 缺 state_root");
    assert!(m.contains("state_root: crate::env::state_root()"), "K-9 失败：未从壳解析状态根");
    for f in ["src/platform/linux.rs", "src/platform/macos.rs", "src/platform/windows.rs"] {
        let s = read(f);
        assert!(s.contains("DSH_SUPERVISOR_HOME"), "K-9 失败：{} 未注入 DSH_SUPERVISOR_HOME", f);
        assert!(s.contains("spec.state_root"), "K-9 失败：{} 未使用壳解析的状态根", f);
    }
}
