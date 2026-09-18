//! 无控制台窗口门禁（壳侧）—— SSOT: NO-CONSOLE-WINDOW-STANDARD §4。
//!
//! ## 背景（真机取证）
//!   壳 Rust 侧 bounded::prepare() 已加 CREATE_NO_WINDOW，platform/mod.rs 与
//!   service.rs 也各自加；但 env.rs::node_version() 直接 spawn node，未经统一
//!   执行器 —— 每次探测 Node 都会闪黑框。窗口来自**子进程未隐藏**（壳自身是
//!   windows 子系统，见 SSOT W4），故约束点是 spawn 调用。
//!
//! ## 断言（SSOT §4）
//!   S-W1 src/** 下 .spawn() 只出现在白名单文件（其余文件若有 spawn，必须先经
//!        bounded::prepare —— 即其函数体内出现 prepare(）。
//!   S-W2 env.rs 的 node_version 函数体内出现 prepare，且不得出现裸
//!        creation_flags（平台分支不得泄漏到 env.rs，门禁 G1）。
//!
//! 说明：本文件只**读**源码做静态分析，不执行被测代码。

use std::fs;
use std::path::{Path, PathBuf};

fn read(rel: &str) -> String {
    fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel))
        .unwrap_or_else(|e| panic!("读取 {} 失败: {}", rel, e))
}

/// 递归收集 src/ 下的 .rs 文件，返回相对 src/ 的路径（统一使用正斜杠）。
fn src_rs_files() -> Vec<String> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut out = Vec::new();
    fn walk(dir: &Path, base: &Path, out: &mut Vec<String>) {
        let rd = fs::read_dir(dir).unwrap_or_else(|e| panic!("读取目录 {:?} 失败: {}", dir, e));
        for e in rd {
            let e = e.unwrap_or_else(|err| panic!("目录项读取失败: {}", err));
            let p = e.path();
            if p.is_dir() {
                walk(&p, base, out);
            } else if p.extension().and_then(|x| x.to_str()) == Some("rs") {
                let rel = p.strip_prefix(base).unwrap_or(&p);
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    walk(&root, &root, &mut out);
    out.sort();
    out
}

/// 该文件中出现 .spawn() 的文件名集合（SSOT §4 简化实现）。
fn files_with_spawn() -> Vec<String> {
    let mut hits = Vec::new();
    for f in src_rs_files() {
        let src = read(&format!("src/{}", f));
        if src.contains(".spawn()") {
            hits.push(f);
        }
    }
    hits
}

/// 剥离 Rust 行注释（//）与块注释（/* */），保留字符串字面量内的内容。
/// 本仓曾两次被自己写的说明文字骗过（见 env.rs 顶部注释）—— 判据必须先剥离注释，
/// 否则文档里提到 prepare( 就会让门禁空转。
fn strip_comments(src: &str) -> String {
    let b = src.as_bytes();
    let mut out = String::with_capacity(src.len());
    let mut i = 0usize;
    let mut in_str = false;
    while i < b.len() {
        let c = b[i];
        if in_str {
            out.push(c as char);
            if c == b'\\' {
                if i + 1 < b.len() {
                    out.push(b[i + 1] as char);
                    i += 2;
                    continue;
                }
            } else if c == b'"' {
                in_str = false;
            }
            i += 1;
            continue;
        }
        if c == b'"' {
            in_str = true;
            out.push('"');
            i += 1;
            continue;
        }
        if c == b'/' && i + 1 < b.len() && b[i + 1] == b'/' {
            while i < b.len() && b[i] != b'\n' {
                out.push(' ');
                i += 1;
            }
            continue;
        }
        if c == b'/' && i + 1 < b.len() && b[i + 1] == b'*' {
            out.push_str("  ");
            i += 2;
            while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                out.push(if b[i] == b'\n' { '\n' } else { ' ' });
                i += 1;
            }
            out.push_str("  ");
            i += 2;
            continue;
        }
        out.push(c as char);
        i += 1;
    }
    out
}

/// 以 name 定义处为起点，括号配对提取函数体（只按 ASCII 括号扫描）。
fn fn_body(src: &str, name: &str) -> Option<String> {
    let at = src.find(name)?;
    let rest = &src[at..];
    let open = rest.find('{')?;
    let bytes = rest.as_bytes();
    let mut depth = 0i32;
    for (i, &b) in bytes.iter().enumerate().skip(open) {
        match b {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(rest[..=i].to_string());
                }
            }
            _ => {}
        }
    }
    Some(rest[open..].to_string())
}

/// S-W1 白名单：平台分支内部（及统一执行器 bounded.rs）允许直接 spawn。
fn is_allowed_spawn_file(rel: &str) -> bool {
    if rel == "bounded.rs" {
        return true;
    }
    if let Some(rest) = rel.strip_prefix("platform/") {
        return matches!(
            rest,
            "mod.rs" | "service.rs" | "unsupported.rs" | "linux.rs" | "macos.rs" | "windows.rs"
        );
    }
    false
}

const SPAWN_WHITELIST: &[&str] = &[
    "bounded.rs",
    "platform/mod.rs",
    "platform/service.rs",
    "platform/unsupported.rs",
    "platform/linux.rs",
    "platform/macos.rs",
    "platform/windows.rs",
];

#[test]
fn s_w1_spawn_sites_are_bounded() {
    let hits = files_with_spawn();
    assert!(
        !hits.is_empty(),
        "S-W1 FAIL 在 src/ 下未找到任何 .spawn() —— 判据可能已失效（门禁空转）"
    );

    // 白名单必须与仓库现实一致（防白名单腐化为死引用）。
    let all = src_rs_files();
    for w in SPAWN_WHITELIST {
        assert!(
            all.iter().any(|f| f == w),
            "S-W1 FAIL 白名单条目 {} 不存在（白名单已腐化）",
            w
        );
    }

    let mut offenders = Vec::new();
    for f in &hits {
        if is_allowed_spawn_file(f) {
            continue;
        }
        // 非白名单文件：spawn 必须已先经统一加标志（prepare）。
        // 先剥离注释，只认真正的调用（防文档文字让门禁空转）。
        let src = strip_comments(&read(&format!("src/{}", f)));
        if !src.contains("prepare(") {
            offenders.push(f.clone());
        }
    }
    assert!(
        offenders.is_empty(),
        "S-W1 FAIL 白名单外的 .spawn() 且未经 prepare: {:?}（应为 {:?} 的子集）",
        offenders,
        SPAWN_WHITELIST
    );
}

#[test]
fn s_w2_env_node_version_goes_through_prepare() {
    // 先剥离注释再提取函数体：只认真实调用，不认说明文字
    //（本仓曾被自己的注释骗过，且注释里的花括号会干扰括号配对）。
    let src = strip_comments(&read("src/env.rs"));
    let body = fn_body(&src, "pub fn node_version")
        .expect("S-W2 FAIL 未在 env.rs 找到 node_version 函数");

    assert!(
        body.contains("prepare"),
        "S-W2 FAIL node_version 未经统一执行器 prepare()（Windows 上会闪黑框）"
    );
    // 字符串拼接，避免本测试源码自身出现该字面量。
    let needle = String::from("creation_") + "flags";
    assert!(
        !body.contains(&needle),
        "S-W2 FAIL node_version 泄漏平台分支 {}（应留在 platform/ 与 bounded.rs）",
        needle
    );
}
