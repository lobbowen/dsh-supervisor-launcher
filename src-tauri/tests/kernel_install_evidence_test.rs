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
//! ## 规则住在哪儿（2026-09-21 B2）
//!   剥 verbatim 前缀的实现全仓只有一份：\`src-tauri/src/platform/mod.rs\` 的
//!   \`external_path\` —— 路径形态属平台事实而非业务选择，且**不带 cfg**，
//!   所以三条 CI 腿都跑到同一份规则与同一组回归测试。
//!   本文件只判**安装侧后果**（ npm 拿到的路径必须已归一）；
//!   「不许再长出第二份」由 kernel_launch_standard_test.rs 的 K-11 守住。
//!
//! ## 不变量 K-1..K-10

use std::fs;
use std::path::PathBuf;

/// 取 `start` 与 `end` 两个签名之间的文本（本文件的函数体切法，K-7 同形）。
fn between(src: &str, start: &str, end: &str) -> String {
    let i = src.find(start).unwrap_or_else(|| panic!("K FAIL 未找到起点 {}", start));
    let j = src[i + start.len()..]
        .find(end)
        .map(|x| i + start.len() + x)
        .unwrap_or(src.len());
    src[i..j].to_string()
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

fn read(rel: &str) -> String {
    let s = fs::read_to_string(root().join(rel)).unwrap_or_else(|e| panic!("read {} failed: {}", rel, e));
    if s.contains('\r') { s.replace("\r\n", "\n") } else { s }
}

#[test]
fn k1_failure_reports_command_prefix_and_registry() {
    // install_version 只做版本校验后转交；cmd/prefix/registry 三项证据由 install_spec 组织。
    //   旧判据从 install_version 一路切到**文件末尾**：文件里任何位置出现这些串都算通过，
    //   锚点形同虚设（切片到 EOF = 判据落在整个文件上）。故逐函数切定，并反向证明切片不越界。
    let core = read("src-tauri/src/core.rs");
    let vi = core.find("pub fn install_version").expect("K-1 FAIL no install_version");
    let sj = vi + core[vi..].find("fn install_spec(").expect("K-1 FAIL install_spec 缺失");
    let lj = sj + core[sj..].find("pub fn install_local(").expect("K-1 FAIL install_local 缺失");
    let entry = &core[vi..sj];
    let seg = &core[sj..lj];
    assert!(entry.contains("install_spec("), "K-1 FAIL install_version 未转交 install_spec");
    assert!(seg.contains("cmd: ") && seg.contains("install -g --no-audit --no-fund"), "K-1 FAIL no cmd");
    assert!(seg.contains("--prefix {}"), "K-1 FAIL no prefix");
    assert!(seg.contains("[registry {}]"), "K-1 FAIL no registry");
    // 反空转：切片内不得出现后续函数签名（出现即说明又切到了 EOF，断言会恒真）。
    assert!(!seg.contains("pub fn build_plan") && !seg.contains("fn fresh_cache_dir"), "K-1 FAIL 切片越出 install_spec");
    eprintln!("K-1 PASS evidence has cmd/prefix/registry (anchored on install_spec body)");
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
    // 2026-09-21（B2）：剥前缀的**规则**收口到 `platform::external_path`（全仓唯一实现；
    //   单点性由 kernel_launch_standard_test.rs 的 K-11 钉死，旧的两份实现
    //   `strip_verbatim` / `simplify` 已连同其测试一并删除）。
    //   本判据只管安装侧的**后果**：交给 npm 的每个路径都必须先过这道归一。
    let core = read("src-tauri/src/core.rs");
    let gi = core.find("pub fn global_prefix_for").expect("K-7 FAIL no global_prefix_for");
    let gj = core[gi..].find("fn tail(").map(|x| gi + x).unwrap_or(core.len());
    let body = &core[gi..gj];
    assert!(body.contains("Some(crate::platform::external_path(&p))"), "K-7 FAIL node_modules branch not normalized");
    assert!(body.contains("Some(crate::platform::external_path(dir))"), "K-7 FAIL shim branch not normalized");
    assert!(core.contains("cmd.arg(\"--prefix\").arg(crate::platform::external_path(p))"), "K-7 FAIL npm prefix not normalized");
    // 规则必须**在 CI 上真的被跑到**：实现不带 cfg，故 POSIX/Windows/macOS 三条腿执行同一份
    //   规则与同一组回归测试 —— 旧实现藏在 Windows 路径里，POSIX CI 既编译不到也测不到，
    //   那正是两份规则不同的实现能长期存活的原因。
    let pm = read("src-tauri/src/platform/mod.rs");
    assert!(pm.contains("pub fn external_path"), "K-7 FAIL external_path 缺失");
    assert!(
        pm.contains("fn external_path_strips_verbatim_drive_and_device_prefix")
            && pm.contains("fn external_path_leaves_clean_and_unix_paths_untouched"),
        "K-7 FAIL verbatim 归一的回归测试被删"
    );
    eprintln!("K-7 PASS verbatim stripped before npm");
}

#[test]
fn k8_reverse_detects_unstripped_verbatim() {
    // 反向：判据必须能识别旧形态，否则 K-7 是空转的门禁。
    let bad = "cmd.arg(\"--prefix\").arg(p);";
    assert!(!bad.contains("external_path"), "K-8 reverse self-check");
    let core = read("src-tauri/src/core.rs");
    assert!(!core.contains(bad), "K-8 FAIL unstripped prefix injection present");
    // 旧形态 2：在本文件里再长出一份剥前缀规则（曾真实存在两份、规则还不同）。
    assert!(
        !core.contains("fn strip_verbatim") && !core.contains("fn simplify("),
        "K-8 FAIL core.rs 又出现第二份剥前缀实现"
    );
    // 旧形态 3：取不到自身路径时静默用空串 —— 会写坏服务定义且事后无法解释。
    assert!(
        !core.contains("current_exe().unwrap_or_default()") && !core.contains("current_exe()"),
        "K-8 FAIL core.rs 仍自行取壳自身路径"
    );
    eprintln!("K-8 PASS reverse judgement valid");
}

/// K-9（2026-09-22，T-7 真分母）：内核包必须由壳按该源自己的 dist 元数据**先下载**再装本地包。
/// 为什么这是安装侧不变量而不是 UI 细节：`npm install -g pkg@ver` 根本不吐取件进度，
/// 只要写入路径还挂在 npm 身上，「内核下载到哪了」就永远是黑的 —— 用户从 1.2.1 起报的就是这件事。
#[test]
fn k9_kernel_package_is_downloaded_with_a_real_denominator() {
    let core = read("src-tauri/src/core.rs");
    for needle in ["pub fn dist_from", "pub fn fetch_dist", "pub fn install_local", "fn dist_slug", "fn file_spec"] {
        assert!(core.contains(needle), "K-9 FAIL core.rs 缺 {}", needle);
    }
    // 分母与摘要都必须真的参与判定：只下不核，等于用「装不上」换掉了「可能装错」。
    let fd = between(&core, "pub fn fetch_dist(", "\n/// ");
    assert!(fd.contains("http_get_bytes_progress(&dist.tarball, dist.size"),
        "K-9 FAIL 分母没交给唯一的带进度客户端：\n{}", fd);
    assert!(fd.contains("Sha512::digest") && fd.contains("拒绝安装"), "K-9 FAIL fetch_dist 未做摘要核对");
    assert!(fd.contains("取回不完整"), "K-9 FAIL fetch_dist 未核对声明的字节数（截断将无声通过）");
    // 落点名由 registry 给的字符串拼成，必须过归一才准进路径。
    assert!(core.contains("dist_slug(pkg), dist_slug(version)"), "K-9 FAIL 落点名未经 dist_slug");
    // 本地包 spec 必须是 file: 形态（裸绝对路径在 npm 的 spec 解析里不保证算文件）。
    assert!(core.contains("format!(\"file:{}\", tgz.display())"), "K-9 FAIL 本地包没用 file: 协议");

    let cmd = read("src-tauri/src/commands/mod.rs");
    let body = between(&cmd, "async fn core_apply_inner", "pub async fn kernel_update_apply");
    assert!(body.contains("fetch_kernel_tgz(&app2, &pkg2, &target2, o)"), "K-9 FAIL 取件步骤没进安装路径");
    assert!(body.contains("crate::core::install_local("), "K-9 FAIL 没装已下载的本地包");
    assert!(body.contains("crate::core::install_version("), "K-9 FAIL 丢了 registry 直装回退（降级不得砍能力）");
    assert!(body.contains("install::kernel_direct("), "K-9 FAIL 降级没有可见文案（用户会看到进度凭空消失）");
    // 取件文件没有复用方（每次换源重取并覆盖），装完必须删：留着就是状态根里按版本逐份累积的垃圾。
    assert!(body.contains("std::fs::remove_file(tgz)"), "K-9 FAIL 取件文件装完不清理");

    let inst = read("src-tauri/src/domain/install.rs");
    assert!(inst.contains("pub(crate) fn kernel_fetch") && inst.contains("pub(crate) fn kernel_fetched")
        && inst.contains("pub(crate) fn kernel_direct"), "K-9 FAIL install.rs 缺内核取件三行");
    assert!(inst.contains("（{}%）"), "K-9 FAIL 下载行没带百分比");
    eprintln!("K-9 PASS kernel package downloaded with a real denominator");
}

/// K-10 反向：判据必须认得「无分母直装」的旧形态，也要认得「把 null 当 0」的假条形态。
#[test]
fn k10_reverse_detects_denominator_free_kernel_install() {
    let old = "match crate::core::install_version(&pkg2, &target2, pref.as_deref(), Some(o.as_str()), beat) {";
    assert!(!old.contains("fetch_kernel_tgz") && !old.contains("install_local"), "K-10 reverse self-check");
    let cmd = read("src-tauri/src/commands/mod.rs");
    assert!(cmd.contains("fetch_kernel_tgz"), "K-10 FAIL 命令层回到了无分母直装");
    let ui = read("src-tauri/bootstrap/js/10-ui.js").replace(' ', "");
    assert!(!ui.contains("ratio||0") && !ui.contains("ratio??0"), "K-10 FAIL 无分母被当成 0（假条会长回来）");
    eprintln!("K-10 PASS reverse judgement valid");
}
