//! G6：跨平台/回退链路的既有护栏回归（本文件保留 g6i/j/k 三条）。
//!
//! 锁定的不变量：
//!   G6-i  镜像探针超时单一事实源（由 PROBE_TIMEOUT 派生，不得硬编码分叉）
//!   G6-j  Node 安装的 SHASUMS 失败不得中断镜像回退
//!   G6-k  退出握手不得在 UI 线程执行（两条路径都必须 offload）
//!
//!  本测试是**静态源码断言**：锁定「结构与接线存在」，不代替真机行为验证。

use std::fs;
use std::path::PathBuf;

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// 读取待检查文件，并**归一化换行为 LF**。
///
/// 2026-09-13：这些门禁大量做**多行源码片段**的文本断言，而 Windows 检出
///   可能是 CRLF（core.autocrlf + 本仓原先无 .gitattributes）→ 内嵌换行的针脚永不匹配：
///     · 正向 find → 退化为 usize::MAX（失败）；
///     · 反向 !contains → 退化为恒真（假绿、门禁空转，比失败更糟）。
///   实测：把工作树整体转成 CRLF 后跑全套，update_guard_test 的 G6-g 立刻失败 ——
///   这正是 Windows leg 在 CI 上红掉的原因（macOS/Linux 是 LF 故全绿）。
///   修法：**在读取处归一化**，使断言在任何平台检查的是同一件事。
fn read(rel: &str) -> String {
    let s = fs::read_to_string(manifest_dir().join(rel))
        .unwrap_or_else(|e| panic!("读取 {} 失败: {}", rel, e));
    if s.contains('\r') { s.replace("\r\n", "\n") } else { s }
}

#[test]
fn g6i_probe_timeout_is_single_source() {
    let m = read("src/mirror.rs");
    assert!(
        !m.contains("\"timeoutMs\": 6000"),
        "G6-i FAIL 契约仍硬编码 6000ms —— 与 PROBE_TIMEOUT(8s) 分叉，选源会不一致"
    );
    assert!(
        m.contains("PROBE_TIMEOUT.as_millis() as u64"),
        "G6-i FAIL timeoutMs 未由 PROBE_TIMEOUT 派生（单一事实源）"
    );
    // 反向：确认壳真的用该常量做探测（否则派生也没意义）
    assert!(
        m.contains(".timeout(PROBE_TIMEOUT)"),
        "G6-i FAIL probe_all 未使用 PROBE_TIMEOUT"
    );
    eprintln!("G6-i PASS 跨仓 probe 超时单一事实源");
}

/// G6-j：Node 安装的镜像回退**不得被 SHASUMS 失败中断**。
///
/// 原实现 `String::from_utf8(http_get_bytes(..SHASUMS256.txt..)?)` —— 外层 `?` 让
/// **网络失败直接 return**，下面的 `continue` 只覆盖非 UTF-8 情形。
/// `SHASUMS256.txt` 瞬时抽风即中断整条回退链，即使后续镜像健康。
/// 同处还有第二例：条目未找到时的 `ok_or_else(..)?` 也直接 return。
#[test]
fn g6j_shasums_failure_does_not_abort_fallback() {
    let n = read("src/node.rs");
    assert!(
        !n.contains("http_get_bytes(&format!(\"{}/{}/SHASUMS256.txt\", base, version))?"),
        "G6-j FAIL SHASUMS 下载仍用 `?` —— 网络失败会中断镜像回退"
    );
    assert!(
        n.contains("SHASUMS 下载失败") && n.contains("SHASUMS256.txt 中未找到条目"),
        "G6-j FAIL SHASUMS 的两条失败路径都应 continue 到下一个源"
    );
    // 反向：确认「换下一个源」语义仍在（不能在失败处直接返回）
    assert!(
        n.contains("SHASUMS") && n.contains("last_err = Some("),
        "G6-j FAIL 失败时应记 last_err 并继续（而非 return）"
    );
    eprintln!("G6-j PASS SHASUMS 失败不中断镜像回退");
}

/// G6-k：**退出握手不得在 UI 线程执行**（两条路径都必须 offload）。
///
/// `guardctl::shutdown_all` 最坏约 70 秒（`/session/stop` 60s + 轮询）。
/// 在 UI 线程做会让窗口假死，用户强杀 → 跳过握手 → 留下未停的 DSH。
///
/// 原实现：托盘 quit 路径正确地 spawn 到线程，而 `closeAction=exit`
/// 的关窗路径**直接在回调里同步执行** —— 同一纪律只覆盖两条路径中的一条。
#[test]
fn g6k_exit_handshake_is_off_ui_thread() {
    let m = read("src/main.rs");
    // 关窗 exit 路径必须 prevent_close + spawn
    assert!(
        m.contains("api.prevent_close();"),
        "G6-k FAIL 关窗 exit 路径缺 prevent_close（最后一个窗口关闭可能让进程先退出、握手被截断）"
    );
    assert!(
        m.contains("let h = window.app_handle().clone();"),
        "G6-k FAIL 关窗 exit 路径未把退出握手 offload 到线程（UI 会假死 ~70s）"
    );
    // 反向：不得再有「在 on_window_event 里同步调 shutdown_all 后 exit」的形态
    assert!(
        !m.contains("domain::guardctl::shutdown_all(port);\n                    app.exit(0);"),
        "G6-k FAIL 仍存在 UI 线程同步退出握手"
    );
    // 托盘路径的既有纪律仍在
    assert!(
        m.contains("let h = app.clone();"),
        "G6-k FAIL 托盘 quit 路径的 offload 丢失"
    );
    eprintln!("G6-k PASS 两条退出路径都把握手 offload 到后台线程");
}