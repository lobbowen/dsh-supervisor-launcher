//! 环境工具链「检测 / 安装 / 下载」门禁（2026-09-16）。
//!
//! 事实源：docs/ENV-TOOLCHAIN-INSTALL-STANDARD.md（SSOT）。
//! 核心不变量：node 与 npm **并行同权** —— 只装 node 就判「环境就绪」，会在干净 Windows 上
//!   留下「node 在、npm 缺」，随后用不存在的 npm 去装内核，必失败。
//! 行为面（probe_npm 命中/兜底、版本仲裁）由各模块的内置单元测试驱动；
//!   本文件守**跨模块结构不变量**：契约字段、run_install 校验、旧 UI/事件残留。
//!
//! 注意：G-1/G-2/G-4 依赖改造落地，落地前**如实失败**（不得为转绿而放宽断言）。

use std::fs;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(rel: &str) -> String {
    fs::read_to_string(root().join(rel)).unwrap_or_else(|e| panic!("读取 {} 失败: {}", rel, e))
}

/// 递归收集目录下所有常规文件。
/// 「为什么」：引导页拆分为多文件后，样式/进度条残留可能落在 html 或尚未拆分的 js ——
///   只扫固定几个 js 就会漏网，门禁必须按目录整体收口。
fn walk_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(dir).unwrap_or_else(|e| panic!("读取目录 {} 失败: {}", dir.display(), e));
    for ent in entries {
        let p = ent.expect("目录项读取失败").path();
        if p.is_dir() {
            walk_files(&p, out);
        } else if p.is_file() {
            out.push(p);
        }
    }
}

fn walk(rel: &str) -> Vec<(PathBuf, String)> {
    let mut files = Vec::new();
    walk_files(&root().join(rel), &mut files);
    files
        .into_iter()
        .filter_map(|p| fs::read_to_string(&p).ok().map(|s| (p, s)))
        .collect()
}

/// 返回 sig 所指函数的函数体（花括号配平）。
/// 「为什么」：G-2 要求断言「run_install **函数体内**」有 npm 校验，
///   纯 src.contains("...") 会被文件里任意位置的同名字符串骗过。
fn fn_body(src: &str, sig: &str) -> String {
    let start = src.find(sig).unwrap_or_else(|| panic!("未找到函数签名 {}", sig));
    let open = src[start..].find('{').map(|i| start + i).expect("函数体缺左花括号");
    let mut depth = 0i32;
    let mut end = open;
    for (i, b) in src.as_bytes().iter().enumerate().skip(open) {
        match b {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    end = i;
                    break;
                }
            }
            _ => {}
        }
    }
    src[open..=end].to_string()
}

/// G-1：node_status 必须回传 npm 两字段，且来源是**真实探测**（probe_npm）。
/// 不变量 T-1：npmOk 不得伪造 —— 字段存在还不够，必须能追溯到 probe_npm。
#[test]
fn g1_node_status_exposes_real_npm_probe() {
    let cmd = read("src/commands/mod.rs");
    assert!(cmd.contains("npmOk"), "node_status 未回传 npmOk（npm 与 node 同权，缺失即漏判）");
    assert!(cmd.contains("npmPath"), "node_status 未回传 npmPath（UI/排障无法定位 npm）");
    assert!(cmd.contains("probe_npm"), "node_status 的 npmOk 未追溯 probe_npm（禁止伪造 npm 存在）");
    let rt = read("src/runtime_contract.rs");
    assert!(rt.contains("fn probe_npm"), "runtime_contract 缺 probe_npm（npm 真实探测的唯一实现）");
}

/// G-2：run_install 函数体内必须校验 npm —— 否则「安装成功」只保证 node，
/// 干净机器上 npm 仍缺失却 emit done（SSOT §1 的根因正是它）。
#[test]
fn g2_run_install_validates_npm_inside_body() {
    let main = read("src/main.rs");
    let body = fn_body(&main, "fn run_install");
    let has_npm = body.contains("probe_npm") || body.contains("npmOk") || body.contains("npm_exe_name");
    assert!(
        has_npm,
        "run_install 函数体内未出现任何 npm 校验（probe_npm/npmOk/npm_exe_name）—— \
         装完 node 即报成功，npm 缺失会被误判为环境就绪"
    );
}

/// G-3：递归扫描 bootstrap（含 .html/.js）不得残留进度条实现。
/// 不变量 T-7：进度条与 showProgress/hideProgress 全部移除，安装/下载只留文字。
#[test]
fn g3_bootstrap_has_no_progress_bar_leftovers() {
    let banned = ["showProgress", "hideProgress", "progBar", "id=\"prog\""];
    let mut hits: Vec<String> = Vec::new();
    for (p, text) in walk("bootstrap") {
        for (n, line) in text.lines().enumerate() {
            if banned.iter().any(|b| line.contains(b)) {
                hits.push(format!("{}:{} {}", p.display(), n + 1, line.trim()));
            }
        }
    }
    assert!(
        hits.is_empty(),
        "bootstrap 仍残留进度条实现（SSOT §3.2 不变量 T-7）：\n{}",
        hits.join("\n")
    );
}

/// G-4：扫描 src 不得出现旧事件名（SSOT §2.4：改统一 install_* 且**无兼容层**）。
#[test]
fn g4_src_has_no_legacy_install_events() {
    let banned = ["env_progress", "env_done", "env_error", "shell_update_progress"];
    let mut hits: Vec<String> = Vec::new();
    for (p, text) in walk("src") {
        for (n, line) in text.lines().enumerate() {
            if banned.iter().any(|b| line.contains(b)) {
                hits.push(format!("{}:{} {}", p.display(), n + 1, line.trim()));
            }
        }
    }
    assert!(
        hits.is_empty(),
        "src 仍出现旧事件名（应统一为 install_progress/install_done/install_error）：\n{}",
        hits.join("\n")
    );
}

/// G-5：20-env.js 必须有**独立的** npm 分支，且文案自带 npm 字样。
/// 不变量 T-5：npm 缺失不得复用 node 分支（复用即「重装 node 补 npm」的死循环文案）。
#[test]
fn g5_env_js_has_standalone_npm_branch() {
    let js = read("bootstrap/js/20-env.js");
    // 2026-09-18：npmOk 改为**三态**（true/false/null=未知），只有 npmOk === true 才放行。
    //   故独立分支判据由 "npmOk === false" 收紧为 "npmOk !== true"（覆盖「缺」与「未知」）。
    let at = js
        .find("npmOk !== true")
        .expect("20-env.js 缺 npmOk !== true 独立分支（npm 缺失/未知不会被修复）");
    assert!(
        js.contains("npmOk === true"),
        "就绪判定必须要求 npmOk === true（文件存在 != 可用，不变量 T-1b）"
    );
    // 取分支起点后的窗口：足够覆盖分支体（probeMirrorThen + status 文案 + invoke）。
    let window: String = js[at..].chars().take(700).collect();
    assert!(
        window.contains("start_node_install") || window.contains("start_npm_install"),
        "npm 分支未触发任何安装/修复调用：\n{}",
        window
    );
    // 独立文案：分支窗口内必须有一句含 npm 的 status/文字，而不是照抄 node 分支。
    let mentions_npm = window
        .lines()
        .any(|l| (l.contains("status(") || l.contains("install.")) && l.to_lowercase().contains("npm"));
    assert!(
        mentions_npm,
        "npm 分支未使用独立文案（必须显式提到 npm，不得与 node 分支共用）：\n{}",
        window
    );
}

/// G-6：10-ui.js 导出统一安装入口 NS.install，且含 begin/text/done/fail 四项。
/// 不变量 T-6：任何模块不得自行拼装下载/安装样式，一律经 NS.install.*。
#[test]
fn g6_ui_js_exports_unified_install_api() {
    let js = read("bootstrap/js/10-ui.js");
    let at = js
        .find("NS.install")
        .expect("10-ui.js 未导出 NS.install（安装/下载样式无唯一实现）");
    let region: String = js[at..].chars().take(600).collect();
    for key in ["begin", "text", "done", "fail"] {
        assert!(
            region.contains(key),
            "NS.install 缺 {} 方法（统一入口须含 begin/text/done/fail 四项）：\n{}",
            key,
            region
        );
    }
}
