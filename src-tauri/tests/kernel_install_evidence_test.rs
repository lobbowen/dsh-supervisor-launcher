//! 内核安装门禁：失败可定位 + verbatim 前缀剥除（2026-09-14）。
//!
//! ## 起源（附 npm debug log 证据）
//!   Windows 升级内核报 Maximum call stack size exceeded。
//!   debug log 的 argv 显示 --prefix 带了 Windows verbatim 前缀
//!   （两反斜杠+问号+反斜杠开头）；而 npm prefix -g 给的是干净形式。
//!   栈指向 @npmcli/arborist 的 realpathCached 无限递归。
//!   来源：Rust canonicalize() 在 Windows 上总返回 verbatim 形式。
//!
//!   难查的原因：core_apply 的失败分支丢了 prefix。
//!   纪律：失败路径的信息量必须 >= 成功路径。
//!
//! ## 不变量 K-1..K-8

use std::fs;
use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

fn read(rel: &str) -> String {
    let s = fs::read_to_string(root().join(rel)).unwrap_or_else(|e| panic!("read {} failed: {}", rel, e));
    if s.contains('\r') { s.replace("\r\n", "\n") } else { s }
}

#[test]
fn k1_failure_reports_command_prefix_and_registry() {
    let core = read("src-tauri/src/core.rs");
    let i = core.find("pub fn install_version").expect("K-1 FAIL no install_version");
    let seg = &core[i..];
    assert!(seg.contains("cmd: ") && seg.contains("install -g --no-audit --no-fund"), "K-1 FAIL no cmd");
    assert!(seg.contains("--prefix {}"), "K-1 FAIL no prefix");
    assert!(seg.contains("[registry {}]"), "K-1 FAIL no registry");
    eprintln!("K-1 PASS evidence has cmd/prefix/registry");
}

#[test]
fn k2_cache_isolation_retry_present() {
    let core = read("src-tauri/src/core.rs");
    assert!(core.contains("npm_config_cache"), "K-2 FAIL no cache isolation");
    assert!(core.contains("fresh_cache_dir"), "K-2 FAIL no fresh cache dir");
    eprintln!("K-2 PASS cache isolation retry present");
}

#[test]
fn k3_retry_bounded_by_fast_fail_window() {
    let core = read("src-tauri/src/core.rs");
    assert!(core.contains("FAST_FAIL_RETRY"), "K-3 FAIL no window");
    assert!(core.contains("t0.elapsed() <= FAST_FAIL_RETRY"), "K-3 FAIL retry unbounded");
    assert!(core.contains("FAST_SKIP_NOTE"), "K-3 FAIL no skip note");
    eprintln!("K-3 PASS retry bounded");
}

#[test]
fn k4_core_apply_failure_branch_carries_prefix_and_origins() {
    let cmd = read("src-tauri/src/commands/mod.rs");
    let i = cmd.find("pub async fn core_apply").expect("K-4 FAIL no core_apply");
    let rest = &cmd[i..];
    let end = rest.find("#[tauri::command]").unwrap_or(rest.len());
    let body = &rest[..end];
    // 锚定 Ok(match res { 之后的那个 Err —— 该函数体内有更早的 Err（resolve 守卫），
    //   若取第一个 Err，err_branch 会一直延伸到函数末尾，
    //   把**成功分支的 prefix** 也算进来 → 断言恒真。
    //   （首版就是这样，被注入验证抓到。）
    let mi = body.find("Ok(match res {").expect("K-4 FAIL no Ok(match res");
    let ei = body[mi..].find("Err(e) =>").expect("K-4 FAIL no Err in Ok(match res") + mi;
    let err_branch = &body[ei..];
    assert!(err_branch.contains("\"prefix\""), "K-4 FAIL failure branch has no prefix");
    assert!(err_branch.contains("originsTried"), "K-4 FAIL no originsTried");
    // 反空转：失败分支切片**不得**包含成功分支（否则断言会恒真）
    assert!(!err_branch.contains("Ok((origin, out))"), "K-4 FAIL err_branch includes success branch (vacuous)");
    eprintln!("K-4 PASS failure branch carries prefix + originsTried (anchored)");
}
#[test]
fn k5_node_install_prefix_has_call_site() {
    let core = read("src-tauri/src/core.rs");
    let cmd = read("src-tauri/src/commands/mod.rs");
    assert!(core.contains("pub fn is_node_install_prefix"), "K-5 FAIL not defined");
    assert!(cmd.contains("is_node_install_prefix"), "K-5 FAIL no call site");
    eprintln!("K-5 PASS has call site");
}

#[test]
fn k6_reverse_judgement_detects_old_deficient_form() {
    let old = "Err(e) => serde_json::json!({ok:false}),";
    assert!(!old.contains("prefix"), "K-6 reverse self-check");
    let old2 = "let out = run_command_bounded(cmd, NPM_INSTALL_TIMEOUT)?;";
    assert!(!old2.contains("npm_config_cache"), "K-6 reverse self-check 2");
    eprintln!("K-6 PASS reverse judgement valid");
}

#[test]
fn k7_verbatim_prefix_stripped_before_npm() {
    let core = read("src-tauri/src/core.rs");
    assert!(core.contains("pub fn strip_verbatim"), "K-7 FAIL no strip_verbatim");
    assert!(core.contains("fn simplify("), "K-7 FAIL no simplify");
    let gi = core.find("pub fn global_prefix_for").expect("K-7 FAIL no global_prefix_for");
    let gj = core[gi..].find("fn tail(").map(|x| gi + x).unwrap_or(core.len());
    let body = &core[gi..gj];
    assert!(body.contains("return Some(simplify(p));"), "K-7 FAIL node_modules branch not simplified");
    assert!(body.contains("return Some(simplify(dir.to_path_buf()));"), "K-7 FAIL shim branch not simplified");
    assert!(core.contains("cmd.arg(\"--prefix\").arg(simplify("), "K-7 FAIL npm prefix not simplified");
    assert!(core.contains("strips_verbatim_drive_prefix") && core.contains("strips_device_prefix") && core.contains("leaves_clean_paths_untouched"), "K-7 FAIL regressions removed");
    eprintln!("K-7 PASS verbatim stripped before npm");
}

#[test]
fn k8_reverse_detects_unstripped_verbatim() {
    let bad = "cmd.arg(\"--prefix\").arg(p);";
    assert!(!bad.contains("simplify"), "K-8 reverse self-check");
    let core = read("src-tauri/src/core.rs");
    assert!(!core.contains(bad), "K-8 FAIL unstripped prefix injection present");
    eprintln!("K-8 PASS reverse judgement valid");
}
