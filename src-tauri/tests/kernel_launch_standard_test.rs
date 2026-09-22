//! 内核启动规范门禁（K-1..K-13；docs/KERNEL-LAUNCH-STANDARD.md §6）—— 2026-09-15 起累积。
//!
//! 锁定 P0–P6 的**规范骨架**，防止回退成「按 PATH 猜位置 / 不对齐就启动 / 平台各自为政」：
//!   K-1  core.json 位置契约存在、schema=1、原子写、字段齐全
//!   K-2  locate 先读 core.json，再由 runtime.json 派生 nodeBinDir（反向判据非空转）
//!   K-3  ensure_guard **先对齐后启动**；且「服务定义未建立 ⇒ 不请求服务管理器启动」
//!        在结构上成立（出边只在建定义成功的分支里）
//!   K-4  四平台服务实现同构（node+PATH+daemon）；trait 有 prefix 候选且 Windows 覆写
//!   K-5  就绪只认契约端口（current_api_port = ports.json 实际值优先）
//!   K-6  平台分支只在 platform/（core.json 读取不得引入平台分支）
//!   K-7  Windows 看护任务的所有者 = 壳（只锁任务，形态判据在 K-13）
//!   K-8  产品状态根独立于 DSH（XDG），且与内核 state-root.js 握手
//!   K-9  状态根随启动注入（LaunchSpec 单一事实源，防壳/内核各自推导分叉）
//!   K-10 Windows 稳定入口 + 按命令行精确杀守卫
//!   K-11 交给外部工具的路径**只有一处**规范化实现、壳自身路径**只有一处**入口（2026-09-21 B2）
//!   K-12 守卫子进程的输出**永不丢弃**（统一落 guard.log）
//!   K-13 看护（watchdog）的存活判据与拉起序列**各只有一处实现**：判据 = `guardctl::ready`、
//!        动作 = `guardctl::ensure_started`；看护脚本不以内嵌 PowerShell 存在（2026-09-21 B3b）
//!
//! 注意命名：`kernel_install_evidence_test.rs` 另有一套**同号不同义**的 K-1..K-8
//!   （安装证据侧）；跨文件引用本套判据时必须带文件名。
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

// ── K-3：先对齐后启动 + **定义失败关闭 P5 出边**（阶段产物化）──
//
// 2026-09-21 改：顺序断言从「全文件字符串位置」改为**函数体切片**，因为
//   `ensure_defined` 与 `start` 现已收进 `ServiceLaunch::define_and_start`（同一职责
//   只有一个所有者），文件级位置比较会因函数摆放顺序而误判。更关键的是新增断言：
//   **start 只能出现在定义成功的分支里** —— 旧实现无论定义成败都照样 `/Run`，于是真机
//   报错只剩「退出码 1」，而「定义环节到底有没有成」在报错里完全看不见（H8 的反例）。
#[test]
fn k3_align_before_start_and_define_failure_closes_start_edge() {
    let src = read("src/domain/guardctl.rs");
    let missing = has_all(&src, &[
        "enum AlignOutcome",
        "fn resolve_aligned",
        "KERNEL_NOT_ALIGNED",
        "ALIGN_RESOLVE_FAILED",
        // P4/P5 的阶段产物（H8 的载体）
        "struct ServiceLaunch",
        "fn define_and_start",
        "fn evidence",
    ]);
    assert!(missing.is_empty(), "K-3 失败：对齐解析/阶段产物缺失 {:?}", missing);

    // ensure_guard 体内：对齐 → 建规格 → 进启动序列（顺序）。
    // 主路径的建规格点取**最后一次**出现：「守卫已活」的提前返回分支里也有一次
    // `LaunchSpec::from_runtime(&rt_wd, ..)`，它按设计排在对齐之前（不参与启动）。
    let body = fn_slice(&src, "pub(crate) fn ensure_guard", "pub(crate) fn ensure_started");
    let align = body.find("resolve_aligned(app)").expect("K-3 失败：ensure_guard 未做版本对齐");
    let spec = body.rfind("LaunchSpec::from_runtime").expect("K-3 失败：ensure_guard 未建启动规格");
    let started = body.find("ensure_started(&spec").expect("K-3 失败：ensure_guard 未进启动序列");
    assert!(align < spec && spec < started, "K-3 失败：ensure_guard 未按「对齐 → 建规格 → 启动序列」顺序");
    // 反空转：切片必须真的停在启动序列 **之前**（越界则顺序判据漂到别的函数里凑符号）。
    assert!(!body.contains("ServiceLaunch::define_and_start("), "K-3：ensure_guard 切片越界，已进入启动序列本体");
    // 序列本体（GUI 与无头看护共用这一份）：定义 → 就绪 → 兜底，且证据进 LaunchError。
    let seq = fn_slice(&src, "pub(crate) fn ensure_started", "const SERVICE_READY_BUDGET");
    let define = seq
        .find("ServiceLaunch::define_and_start(spec")
        .expect("K-3 失败：启动序列未走服务管理器路径");
    let evidence = seq.find("launch.evidence()").expect("K-3 失败：阶段证据未进入 LaunchError");
    assert!(define < evidence, "K-3 失败：证据取自尚未产生它的阶段");
    assert!(seq.contains("spawn_daemon") && seq.contains("READY_TIMEOUT"),
        "K-3 失败：启动序列缺兜底或缺兜底后的就绪判定");

    // 阶段产物体内：start **只能**出现在 defined 为 Ok 的分支里。
    let run = fn_slice(&src, "fn define_and_start", "fn start_requested");
    let start_at = run.find("service().start()").expect("K-3 失败：阶段产物内未请求启动");
    let before = &run[..start_at];
    assert!(
        before.contains("Ok(_) => {") && run.contains("Err(_) => None"),
        "K-3 失败：start 未收在「定义成功」分支内（定义失败仍可能请求启动）"
    );
    // 反向：切片必须真的落在 define_and_start 内（否则会漂到 evidence() 里去凑符号）。
    assert!(!before.contains("fn evidence("), "K-3：切片越界，判据恒真");
}

/// 取 `[head, next)` 这段源码（`next` 是**下一个**函数签名，用于切边）。
///
/// 为什么按「下一个函数签名」而不是大括号配对：Rust 源码的字符串与注释里会出现 `}`，
/// 朴素配对在中文文本上极易失配（本仓踩过一次）。切边由调用方显式给出，
/// 因此每个调用点都能另外断言「切片不得越界」，避免判据漂到相邻函数里凑符号。
fn fn_slice<'a>(src: &'a str, head: &str, next: &str) -> &'a str {
    let a = src.find(head).unwrap_or_else(|| panic!("未找到 {}", head));
    let b = src[a + head.len()..]
        .find(next)
        .map(|i| a + head.len() + i)
        .unwrap_or(src.len());
    &src[a..b]
}

/// 只留代码（剥掉整行注释）后的文本。
///
/// 「旧形态不许回来」这类反向判据必须在剥注释之后跑：解释**为什么**删掉它的注释里
///   合法地出现旧形态的名字，否则门禁会逼着注释回避历史教训（B63/B65 同一惯例）。
fn code_only(src: &str) -> String {
    src.lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

// ── K-4：三平台服务定义同构（统一稳定入口）+ prefix 候选 trait ──
#[test]
fn k4_platforms_uniform() {
    // 架构（2026-09-15 二次修正）：服务定义只指向统一稳定入口 `<壳> --run-guard`；
    //   定义中不得出现 node/guard 路径。三平台必须**同一入口**（防「只修 Windows」）。
    for f in ["src/platform/linux.rs", "src/platform/macos.rs", "src/platform/windows.rs"] {
        let s = read(f);
        let missing = has_all(&s, &["service_command()"]);
        assert!(missing.is_empty(), "K-4 失败：{} 未用统一稳定入口，缺 {:?}", f, missing);
    }
    // systemd/schtasks 需要单行命令（service_exec_line）；launchd 用数组，天然不需要。
    for f in ["src/platform/linux.rs", "src/platform/windows.rs"] {
        let s = read(f);
        assert!(s.contains("service_exec_line"), "K-4 失败：{} 未用 service_exec_line 组装命令", f);
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
    // 构造即规范化（2026-09-21 B2）：状态根也要过外部路径规范化，否则 Windows 上
    //   注入给内核的 DSH_SUPERVISOR_HOME 可能带 verbatim 前缀，内核读不到契约 → 永远拉不起来。
    assert!(m.contains("state_root: external_path(&crate::env::state_root())"), "K-9 失败：未从壳解析并规范化状态根");
    // 注入点：exec_guard（--run-guard）与 spawn_daemon（trait 默认）都必须携带 DSH_SUPERVISOR_HOME。
    assert!(m.contains("DSH_SUPERVISOR_HOME"), "K-9 失败：exec_guard 未注入状态根");
    let svc = read("src/platform/service.rs");
    assert!(svc.contains("DSH_SUPERVISOR_HOME") && svc.contains("spec.state_root"), "K-9 失败：spawn 兜底未注入状态根");
    // 服务定义侧：systemd/launchd 仍显式声明；Windows 计划任务无法设环境变量，由 --run-guard 注入。
    for f in ["src/platform/linux.rs", "src/platform/macos.rs"] {
        let s = read(f);
        assert!(s.contains("DSH_SUPERVISOR_HOME"), "K-9 失败：{} 未注入 DSH_SUPERVISOR_HOME", f);
        assert!(s.contains("spec.state_root"), "K-9 失败：{} 未使用壳解析的状态根", f);
    }
    assert!(read("src/platform/windows.rs").contains("service_command()"), "K-9 失败：Windows 未用稳定入口（状态根由 --run-guard 注入）");
}
// ── K-10：Windows 稳定入口 + 按命令行杀守卫（G3/C2 收尾）──
#[test]
fn k10_windows_stable_entry_and_kill_correct() {
    let src = read("src/platform/windows.rs");
    let missing = has_all(&src, &[
        "service_command()",  // 定义 = 稳定入口（不再写 node/guard）
        "service_exec_line",  // 与另两平台同一组装
        "guard-task.action",  // 动作记录（计划任务无法回读动作串 → 本地留档自愈）
        "Win32_Process",      // 看护/停止按命令行识别 node 守卫（进程名不是 dsh-supervisor）
        "Stop-Process",       // 精确杀守卫 node
    ]);
    assert!(missing.is_empty(), "K-10 失败：Windows 稳定入口/诊断缺失 {:?}", missing);
    // 反向：不得回归到「现场生成包装脚本 / 把 node·guard 写进定义」。
    for bad in ["guard-task.ps1", "guard-task.cmd", "guard_argv", "win_path_expr"] {
        assert!(!src.contains(bad), "K-10 失败：windows.rs 仍含旧包装脚本痕迹 {}", bad);
    }
    assert!(
        !src.contains("spec.node") && !src.contains("spec.guard"),
        "K-10 失败：windows.rs 服务定义仍引用 node/guard 绝对路径"
    );
    assert!(!src.contains("\"/IM\", \"dsh-supervisor.exe\""), "K-10 失败：仍按 dsh-supervisor.exe 杀进程（守卫是 node.exe，杀不掉）");
}

// ── K-11：交给外部工具的路径**只有一处规范化**，壳自身路径**只有一处入口 ──
//
// 2026-09-21（B2）。真机链条：`LaunchSpec` 的 shell 字段曾是
//   `std::env::current_exe().unwrap_or_default()` —— 取不到时得到**空路径**并原样写进
//   服务定义；而 Windows 取到时又带 `\\?\` verbatim 前缀。同一时期仓里有**两份规则不同**
//   的剥前缀实现（core.rs 与 coreloc.rs），于是「同一条路径」在两条代码路径上形态不同。
//   本门禁钉的正是这两件事：**只允许一个实现、只允许一个入口**。
#[test]
fn k11_path_normalization_and_self_exe_have_single_home() {
    let m = read("src/platform/mod.rs");
    assert!(m.contains("pub fn external_path"), "K-11 失败：缺唯一的路径规范化实现");
    assert!(m.contains("pub fn self_exe"), "K-11 失败：缺唯一的壳自身路径入口");
    assert!(m.contains("shell: self_exe()?"), "K-11 失败：LaunchSpec 未走 self_exe（可能退回静默空路径）");
    // 构造即规范化：四个路径字段全部过 external_path（漏一个 = 该平台上悄悄拉不起来）。
    let from = fn_slice(&m, "pub fn from_runtime", "fn service_command");
    for field in ["node: external_path(", "guard: external_path(", "state_root: external_path("] {
        assert!(from.contains(field), "K-11 失败：from_runtime 未规范化 {}", field);
    }
    // 反向（旧形态必须被判违规）：第二份剥前缀实现、以及 `unwrap_or_default()` 取壳路径。
    let old = "fn simplify(p: PathBuf) -> PathBuf { ... }\nshell: std::env::current_exe().unwrap_or_default(),";
    assert!(
        old.contains("unwrap_or_default") && !old.contains("self_exe"),
        "K-11 反向失败：判据无法识别旧的静默空路径形态"
    );
    for f in ["src/core.rs", "src/domain/coreloc.rs", "src/platform/windows.rs", "src/platform/linux.rs", "src/platform/macos.rs"] {
        let s = read(f);
        for banned in ["fn strip_verbatim", "fn simplify(", "current_exe("] {
            assert!(!s.contains(banned), "K-11 失败：{} 仍有第二份实现/绕过入口（{}）", f, banned);
        }
    }
}

// ── K-12：守卫子进程的输出不得丢弃（证据通道单一实现）──
//
// 与 B1（`ExecRecord` 收口）同一类缺陷的另一半：B1 管「等得到的输出」，
//   分离拉起的子进程没有「等待」，它的 stderr 只有落盘才存在。
//   旧实现在 3 处写 `Stdio::null()`，于是 READY_TIMEOUT 永远只能报「超时」两个字。
#[test]
fn k12_guard_child_output_is_never_discarded() {
    let m = read("src/platform/mod.rs");
    let svc = read("src/platform/service.rs");
    assert!(m.contains("pub fn guard_stdio"), "K-12 失败：缺统一的守卫标准流装配点");
    assert!(m.contains("fn guard_stdio_streams"), "K-12 失败：日志→流的映射未独立成可测函数");
    assert!(svc.contains("guard_stdio(&mut cmd)"), "K-12 失败：spawn 兜底未走 guard_stdio");
    // 两条 exec_guard 分支（unix/windows）都要接管输出。
    let unix_body = fn_slice(&m, "#[cfg(unix)]\npub fn exec_guard", "#[cfg(windows)]");
    let win_body = fn_slice(&m, "#[cfg(windows)]\npub fn exec_guard", "pub trait Platform");
    for (name, body) in [("exec_guard(unix)", unix_body), ("exec_guard(windows)", win_body)] {
        assert!(body.contains("guard_stdio(&mut cmd)"), "K-12 失败：{} 未接管守卫标准流", name);
        assert!(!body.contains("Stdio::null()"), "K-12 失败：{} 仍直接丢弃守卫输出", name);
    }
    // 反向：旧形态（三条 null）必须被判违规。
    let old = ".stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null())";
    assert!(old.contains("Stdio::null()") && !old.contains("guard_stdio"), "K-12 反向失败：判据空转");
    // 落点单一：日志文件路径只在 update.rs 定义一次，且报错能指名它。
    assert!(read("src/update.rs").contains("pub fn guard_log_path"), "K-12 失败：守卫日志落点未单一定义");
    assert!(
        read("src/domain/guardctl.rs").contains("guard_log_path()"),
        "K-12 失败：READY_TIMEOUT 未把证据落点写进报错"
    );
}

// ── K-13：看护的存活判据与拉起序列各只有一处实现（2026-09-21 B3b）──
//
// 旧形态把检测写成 windows.rs 里内嵌的 PowerShell：`Test-NetConnection` 只看 TCP 端口
//   （端口被占但服务没起 = 判为活着），再自己 `Start-Process` 补拉 —— 与 GUI 启动路径并列的
//   第二套判据、第二套动作。现在：判据 = `guardctl::ready()`（H6），动作 = `guardctl::ensure_started()`
//   （与 GUI 启动逐字同一段 P4→P6）。GUI 自愈不在本仓：所有者是守卫（内核 domains/shell），
//   壳监督不了自己 —— 它的监督者会随它一起死。
#[test]
fn k13_watchdog_shares_readiness_and_launch_sequence() {
    let win = read("src/platform/windows.rs");
    // 任务动作指向稳定入口的无头模式，且 /TR 引号仍走单源装配。
    assert!(
        win.contains("pub const WATCHDOG_ARGS: &[&str] = &[\"--watchdog\"]"),
        "K-13 失败：看护任务未指向壳的无头入口"
    );
    let wd = fn_slice(&win, "fn ensure_watchdog", "impl ServiceControl for Impl");
    assert!(wd.contains("service_exec_line(shell, args)"), "K-13 失败：看护任务另写了一份 /TR 引号规则");
    // 升级后旧脚本必须就地清掉（否则留下无人维护的第二实现残骸）。
    assert!(wd.contains("remove_file"), "K-13 失败：未清理历史 watchdog.ps1");
    let wd_code = code_only(&wd);
    for banned in ["powershell", "Start-Process", "Test-NetConnection"] {
        assert!(!wd_code.contains(banned), "K-13 失败：看护任务代码里仍有 {}", banned);
    }
    // 反向判据在**整个文件**的代码上跑（Get-CimInstance 例外：stop() 按命令行精确杀守卫是 K-10 的合法能力）。
    let win_code = code_only(&win);
    assert!(!win_code.contains("fn watchdog_script"), "K-13 失败：内嵌看护脚本未删除");
    assert!(!win_code.contains("Test-NetConnection"), "K-13 失败：Windows 自带第二套端口判据");

    // 无头入口：判据与序列都取自 guardctl，且不碰版本对齐（对齐属 GUI 引导页）。
    let cli = read("src/domain/cli.rs");
    let body = fn_slice(&cli, "pub(crate) fn cli_watchdog", "const WATCHDOG_PROBE_TIMEOUT");
    assert!(
        has_all(body, &[
            "guardctl::ready(",
            "Readiness::Ready",
            "guardctl::resolve_local(None)",
            "LaunchSpec::from_runtime",
            "guardctl::ensure_started(&spec",
        ]).is_empty(),
        "K-13 失败：看护入口未复用单一判据/单一启动序列"
    );
    for banned in ["TcpStream", "ensure_guard(", "resolve_aligned", "Stdio::", "Command::new"] {
        assert!(!body.contains(banned), "K-13 失败：看护入口自己实现了 {}", banned);
    }
    let main = read("src/main.rs");
    assert!(main.contains("a == \"--watchdog\"") && main.contains("domain::cli::cli_watchdog()"),
        "K-13 失败：--watchdog 未在 Tauri 初始化之前分派");
    // 启动序列只有一份：ensure_guard 委托，兜底动作不在它体内重复出现。
    let g = read("src/domain/guardctl.rs");
    let guard_body = fn_slice(&g, "pub(crate) fn ensure_guard", "pub(crate) fn ensure_started");
    assert!(guard_body.contains("ensure_started(&spec, &step)"), "K-13 失败：GUI 启动未委托共享序列");
    assert!(!guard_body.contains("spawn_daemon"), "K-13 失败：拉起动作在 ensure_guard 里重复了一份");
    assert_eq!(g.matches(".spawn_daemon(").count(), 1, "K-13 失败：兜底 spawn 的调用点不止一处");

    // 反向：旧脚本形态必须判为违规（证明上面的 banned 不是空转）。
    let old = "fn watchdog_script(spec: &LaunchSpec) -> String {\n  \"$up = Test-NetConnection -Port $port; Start-Process $shell\"\n}";
    assert!(code_only(old).contains("Test-NetConnection") && !code_only(old).contains("service_exec_line"),
        "K-13 反向失败：判据空转");
}

// ── K-15：Windows 定义链的权限边界与通道可观测 ──
// 真机（用户 Windows 桌面）实测过的一条死路：`/RL HIGHEST` 在非提权进程里必被拒，
// 「降级重试」又只是把同一条命令少写一个参数再跑一遍，两次都拒绝访问，而报错里
// 只有两句一模一样的「拒绝访问」。这里把三件事钉住：不请求最高权限、只有权限类失败
// 才换通道、换到的通道必须是可观测的（start 按通道分派，不假装服务管理器接受了请求）。
#[test]
fn k15_windows_definition_permission_boundary() {
    let w = read("src/platform/windows.rs");
    let code = code_only(&w);
    let missing = has_all(&code, &[
        "fn create_guard_task",
        "fn ensure_run_key",
        "fn is_access_denied",
        "fn read_action_record",
        "fn write_action_record",
        r#"HKCU\Software\Microsoft\Windows\CurrentVersion\Run"#,
        "if !is_access_denied(&why)",
    ]);
    assert!(missing.is_empty(), "K-15 失败：Windows 定义链缺要素 {:?}", missing);
    // 不再请求最高权限（守卫是用户态进程，提权要求只会把可用通道换成必失败）。
    assert!(!code.contains("HIGHEST"), "K-15 失败：代码里仍出现 HIGHEST 权限请求");
    // 守卫任务的 /Create 只有一处装配，且不再出现「同一条命令换个参数再跑一遍」的降级重试。
    assert_eq!(
        code.matches("\"/Create\", \"/TN\", GUARD_TASK").count(),
        1,
        "K-15 失败：守卫任务的 /Create 参数表不止一处"
    );
    let create = fn_slice(&w, "fn create_guard_task", "fn ensure_run_key");
    assert!(!create.contains("降级重试"), "K-15 失败：仍是无差别重试同一条命令");
    // 通道进记录、start 按通道分派：Run 键不支持即时启动，就必须如实报 Err，
    // 否则调用方会为一个没人被请求过的启动等满就绪预算。
    let start = fn_slice(&w, "fn start(&self) -> Result<(), String>", "fn stop");
    assert!(start.contains("Channel::RunKey"), "K-15 失败：start 未按定义通道分派");
    assert!(
        fn_slice(&w, "fn ensure_defined", "fn create_guard_task").contains("write_action_record"),
        "K-15 失败：动作记录未在定义成功后写入（失败也留档会把坏定义判成已是最新）"
    );
    // 请求被拒不等于已发出请求：等待判据必须只认 Some(Ok(()))。
    let g = read("src/domain/guardctl.rs");
    let req = fn_slice(&g, "fn start_requested", "fn evidence");
    assert!(
        req.contains("matches!(self.started, Some(Ok(())))"),
        "K-15 失败：start_requested 仍把被拒的请求算作已发起（用户白等 30 秒）"
    );
    // 兜底直拉必须看得见子进程死活：非零退出即早停，且把守卫输出末段带进报错。
    let daemon = fn_slice(&g, "fn await_daemon", "fn guard_log_tail");
    assert!(
        daemon.contains("child.try_wait()") && daemon.contains("!st.success()"),
        "K-15 失败：兜底拉起仍不等子进程退出，只等端口超时"
    );
    let fallback = fn_slice(&g, "pub(crate) fn ensure_started", "fn await_daemon");
    assert!(
        fallback.contains("Ok(mut child)") && fallback.contains("guard_log_tail()"),
        "K-15 失败：兜底路径未持有子进程或未带上守卫输出末段"
    );
    assert_eq!(
        code.matches("spawn_daemon").count(),
        0,
        "K-15 失败：平台实现里又各自写了一份 spawn 兜底"
    );
    // 反向：旧形态（请求最高权限 + 无差别重试）必须被上面的判据抓到，证明非空转。
    let old = "let r = crate::bounded::run(&mut base(Some(\"HIGHEST\")), SVC_NORMAL)?;\n\
               let second = crate::bounded::run(&mut base(None), SVC_NORMAL)?; // 降级重试";
    assert!(
        old.contains("HIGHEST")
            && old.matches("\"/Create\", \"/TN\", GUARD_TASK").count() == 0
            && code_only(old).contains("降级重试"),
        "K-15 反向失败：判据空转（旧形态未被识别）"
    );
}
