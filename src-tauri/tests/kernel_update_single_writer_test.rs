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
//!   SW-1  桥契约常量：协议版本 = 1、三类消息、代执行命令名、**等待预算的唯一定义处**
//!   SW-2  shell.html 经 shell_bridge_contract 取常量（不硬编码消息类型）
//!   SW-3  shell.html 只接受内容 iframe 且 origin 回环，回复 targetOrigin = ev.origin
//!   SW-4  安装只有一处实现 core_apply_inner；kernel_update_apply 安装后由所有者停+重拉守卫；
//!         总预算不得在命令层再写一份数字（B4b）
//!   SW-5  两个新命令已在 main.rs 注册
//!   SW-6  反向：判据能识别「不校验来源 / 回复 '*'」的桥（门禁非空转）
//!   SW-7  内核安装进度必须**多帧中继**给面板（面板无 IPC），且 kernelKind/maxWaitMs 由契约下发
//!   SW-8  反向：判据能识别「只回一条 stage:'start' 就不管了」的旧形态（B4b 前的真实形态）
//!   SW-9  跨语言预算单源：前端等待上界由后端 maxWaitMs 派生，兜底常量必须等于「预算 + 收尾
//!         余量」—— 否则后端仍在收尾时前端先判超时，把成功的安装说成失败
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

// ── SW-1：桥契约常量（协议版本/消息类型/命令名/预算）──
#[test]
fn sw1_bridge_contract_constants() {
    let src = read("src/bridge.rs");
    let missing = has_all(&src, &[
        "KERNEL_UPDATE_PROTOCOL_VERSION: u32 = 1",
        "\"dsh:kernel-update-request\"",
        "\"dsh:kernel-update-result\"",
        "\"dsh:kernel-update-progress\"",
        "\"kernel_update_apply\"",
        // B4b：等待预算是**后端事实**，必须与消息类型同处一个单一事实源。
        "KERNEL_UPDATE_BUDGET_MS: u64 = 17 * 60 * 1000",
        "pub const KERNEL_UPDATE_MAX_WAIT_MS: u64 = KERNEL_UPDATE_BUDGET_MS + KERNEL_UPDATE_GRACE_MS",
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

// ── SW-4：安装单一实现 + 重启由所有者完成 + 预算只有一个定义处 ──
#[test]
fn sw4_single_install_and_owner_restart() {
    let src = read("src/commands/mod.rs");
    let missing = has_all(&src, &[
        "async fn core_apply_inner",
        "core_apply_inner(app).await",
        "core_apply_inner(app.clone()).await",
        "pub async fn kernel_update_apply",
        "crate::platform::service().stop()",
        "guardctl::port_open(port)",
        "guardctl::ensure_guard(&app)",
        // B4b：deadline 必须**取**自 bridge.rs 的那个常量。原写法是 `from_secs(17 * 60)`，
        //   于是同一事实有 Rust 与 JS 两份账，改一处就静默失配。
        "Duration::from_millis(crate::bridge::KERNEL_UPDATE_BUDGET_MS)",
    ]);
    assert!(missing.is_empty(), "SW-4 失败：安装/重启语义缺失 {:?}", missing);
    assert!(
        !src.contains("from_secs(17 * 60)"),
        "SW-4 失败：命令层又写了一份 17 分钟总预算（唯一来源是 bridge::KERNEL_UPDATE_BUDGET_MS）"
    );
    let start = src.find("pub async fn kernel_update_apply").expect("kernel_update_apply");
    let body = &src[start..];
    let end = body.find("\n}").map(|i| start + i).unwrap_or(src.len());
    let body = &src[start..end];
    assert!(
        !body.contains("install_version(") && !body.contains("run_npm_install")
            // 下载/本地装同样是「写内核包」，绕开 core_apply_inner 就会出现第二写入者（T-7 之后新增两条口子）
            && !body.contains("install_local(") && !body.contains("fetch_kernel_tgz(")
            && !body.contains("fetch_dist("),
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

/// 进度中继是否**成形**的可判定谓词（SW-7 及其反向自检共用）。
///
/// 每一项都对应一个真实失效模式：
///   · 没有 install_progress 监听 —— 面板全程只有一句「start」，17 分钟零反馈（B4b 的根因）；
///   · 按 `BRIDGE.kernelKind` 筛 —— 在 JS 里写死 `"kernel"` 就是给 kind 开第二份账（G-12B 同源）；
///   · 第一帧带 `maxWaitMs` —— 面板的等待上界必须由后端事实给出，而不是自己编一个 6 分钟；
///   · 终结路径清 `kernelPending` 且拒第二个在途请求 —— 否则两次 npm install 并发写同一前缀。
fn relay_is_wired(src: &str) -> Vec<String> {
    has_all(src, &[
        "listen('install_progress'",
        "BRIDGE.kernelKind",
        "kernelPending",
        "maxWaitMs",
        "type: BRIDGE.types.progress, requestId: kernelPending.id",
    ])
}

// ── SW-7：内核安装进度必须中继给面板，且面板的等待上界来自后端（B4b）──
#[test]
fn sw7_kernel_progress_relayed_to_panel() {
    let html = read("bootstrap/shell.html");
    let missing = relay_is_wired(&html);
    assert!(missing.is_empty(), "SW-7 失败：进度中继接线不完整（缺 {:?}）", missing);
    // 面板只应收到**内核**的事件：桌面壳自更新（kind=shell）走的是另一条 UI 路径。
    assert!(
        html.contains("p.kind !== BRIDGE.kernelKind"),
        "SW-7 失败：中继未按契约下发的 kernelKind 过滤（会把别的 kind 念给面板）"
    );
    // 契约必须真的把这两件事实下发（否则 shell.html 只能硬编码）。
    let cmd = read("src/commands/mod.rs");
    let missing2 = has_all(&cmd, &["\"kernelKind\": InstallKind::Kernel.as_str()", "\"maxWaitMs\": crate::bridge::KERNEL_UPDATE_MAX_WAIT_MS"]);
    assert!(missing2.is_empty(), "SW-7 失败：桥契约未下发 {:?}", missing2);
}

// ── SW-8：反向：旧形态（只回一条 start、无在途去重）必须被判为不合格 ──
#[test]
fn sw8_relay_reverse_detects_old_form() {
    let old = "replyToPanel(src, origin, { v: BRIDGE.v, type: BRIDGE.types.progress, requestId: rid, stage: 'start' });";
    assert!(
        !relay_is_wired(old).is_empty(),
        "SW-8 失败：判据认不出「只回一条 start 就不管了」的旧形态（门禁空转）"
    );
    assert!(relay_is_wired(&read("bootstrap/shell.html")).is_empty(), "SW-8 失败：当前实现被误判");
}

/// 取 `marker` 之后到 `;` 之间的常量表达式并求值（只支持十进制与 `*`，够本仓两处定义用）。
fn const_expr_ms(src: &str, marker: &str) -> Option<u64> {
    let at = src.find(marker)? + marker.len();
    let rest = &src[at..];
    let end = rest.find(';')?;
    let mut acc: Option<u64> = None;
    for tok in rest[..end].split('*') {
        let digits: String = tok.chars().filter(|c| c.is_ascii_digit()).collect();
        let v: u64 = digits.parse().ok()?;
        acc = Some(match acc {
            None => v,
            Some(a) => a * v,
        });
    }
    acc
}

/// 判据本体：前端兜底预算必须等于后端「预算 + 收尾余量」，且前端余量非零。
fn bound_ok(fallback: u64, margin: u64, max_wait: u64) -> bool {
    margin > 0 && fallback == max_wait
}

#[test]
fn sw9_frontend_wait_budget_derives_from_backend() {
    let br = read("src/bridge.rs");
    let js = read("bootstrap/js/00-runtime.js");
    let k50 = read("bootstrap/js/50-kernel.js");
    let budget = const_expr_ms(&br, "pub const KERNEL_UPDATE_BUDGET_MS: u64 =")
        .expect("SW-9 FAIL bridge.rs 缺 KERNEL_UPDATE_BUDGET_MS 的数字定义");
    let grace = const_expr_ms(&br, "pub const KERNEL_UPDATE_GRACE_MS: u64 =")
        .expect("SW-9 FAIL bridge.rs 缺 KERNEL_UPDATE_GRACE_MS 的数字定义");
    let max_wait = budget + grace;
    assert!(
        br.contains(
            "pub const KERNEL_UPDATE_MAX_WAIT_MS: u64 = KERNEL_UPDATE_BUDGET_MS + KERNEL_UPDATE_GRACE_MS;"
        ),
        "SW-9 FAIL MAX_WAIT 不是由预算与余量相加而来（该处又写了一份数字，两处会漂移）"
    );
    let fallback = const_expr_ms(&js, "NS.CORE_APPLY_BUDGET_MS =")
        .expect("SW-9 FAIL 前端兜底预算缺失");
    let margin = const_expr_ms(&js, "NS.CORE_APPLY_MARGIN_MS =")
        .expect("SW-9 FAIL 前端余量缺失");
    assert!(
        bound_ok(fallback, margin, max_wait),
        "SW-9 FAIL 前端上界与后端不符：兜底 {} 应等于 maxWait {}，余量 {}",
        fallback,
        max_wait,
        margin
    );
    // 上界必须由契约派生（后端改预算时前端跟着走），调用点不得再直接用兜底常量
    assert!(
        js.contains("NS.bridge.maxWaitMs") && js.contains("NS.coreApplyBudgetMs"),
        "SW-9 FAIL 前端未从契约取 maxWaitMs 派生上界"
    );
    assert!(
        k50.contains("NS.coreApplyBudgetMs()"),
        "SW-9 FAIL 内核安装调用点未走派生函数"
    );
    assert!(
        !k50.contains("NS.CORE_APPLY_BUDGET_MS"),
        "SW-9 FAIL 内核安装调用点绕过派生函数直取兜底常量"
    );
    // 反向：旧形态（前端 = 后端「预算」而非「预算 + 余量」）必须被同一判据拒掉
    assert!(
        !bound_ok(budget, margin, max_wait),
        "SW-9 FAIL 判据恒真（旧形态 1020000 也放行）"
    );
    assert!(!bound_ok(max_wait, 0, max_wait), "SW-9 FAIL 零余量被放行");
    eprintln!("SW-9 PASS frontend wait budget derives from backend");
}
