//! 探测代际与共享进度的写入隔离门禁。
//!
//! 缺陷：`nodeprobe.rs` 的代际校验只挡**最终回写**（Outcome），于是被作废的旧 worker 在
//! `detect()` 里照常 `stage/finish/set_summary` 写共享的 `live()`。新一代面板读到的是
//! 上一轮留下的「正在做什么」与记录 —— 「卡住时唯一线索」被串扰成假现场，且无从辨别是哪一轮。
//!
//! 修法：worker 线程登记自己所属代际，四个共享写入点先判「是否已作废」再写；
//! 读侧（`current_stuck` / `candidate_summary` / `snapshot`）**不受代际影响**，
//! 否则进展读不到，等于把线索抹掉。
//!
//! 锁定不变量：
//!   G-p1 四个写入点全部经代际判定（函数体级判据，不是全文出现一次就算）
//!   G-p2 worker 线程登记代际
//!   G-p3 读侧不许被挡
//!   G-p4 反向：旧形态（写侧无挡）喂进同一判据必须判红

use std::fs;
use std::path::PathBuf;

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// 读取并把换行归一为 LF（Windows 检出可能是 CRLF，多行针脚会永不匹配）。
fn read(rel: &str) -> String {
    let s = fs::read_to_string(manifest_dir().join(rel))
        .unwrap_or_else(|e| panic!("读取 {} 失败: {}", rel, e));
    if s.contains('\r') { s.replace("\r\n", "\n") } else { s }
}

/// 去掉整行注释：本仓多次被自己的说明文字喂饱断言。
fn code_only(src: &str) -> String {
    src.lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// 函数体（从头签名到第一个「行首的 `}`」）；找不到签名返回 None。
fn body_of(src: &str, head: &str) -> Option<String> {
    let i = src.find(head)?;
    let rest = &src[i..];
    Some(match rest.find("\n}") {
        Some(e) => rest[..e].to_string(),
        None => rest.to_string(),
    })
}

const WRITERS: [&str; 4] = [
    "fn live_reset()",
    "fn stage(",
    "fn finish(",
    "fn set_summary(",
];

/// 判据本体：返回「定位到的写入点数」与「未做代际判定的写入点」。
fn writer_gating(code: &str) -> (usize, Vec<&'static str>) {
    let mut found = 0usize;
    let mut ungated: Vec<&'static str> = Vec::new();
    for h in WRITERS.iter().copied() {
        if let Some(b) = body_of(code, h) {
            found += 1;
            if !b.contains("stale_writer()") {
                ungated.push(h);
            }
        }
    }
    (found, ungated)
}

#[test]
fn gp_live_writes_are_generation_scoped() {
    let code = code_only(&read("src/nodeprobe.rs"));
    let (found, ungated) = writer_gating(&code);
    assert_eq!(
        found, 4,
        "G-p1 失败：只定位到 {} 个共享进度写入函数（判据可能已空转）",
        found
    );
    assert!(
        ungated.is_empty(),
        "G-p1 失败：作废代际仍在写共享进度：{:?}（旧 worker 会串扰新面板的卡住线索）",
        ungated
    );
    assert!(
        code.contains("WORKER_GEN.with(|g| g.set(Some(gen)))"),
        "G-p2 失败：worker 线程未登记自己所属的代际（判定无从生效）"
    );
    for h in [
        "fn snapshot()",
        "pub fn current_stuck()",
        "pub fn candidate_summary()",
    ] {
        if let Some(b) = body_of(&code, h) {
            assert!(
                !b.contains("stale_writer()"),
                "G-p3 失败：读侧 {} 被代际挡住（进展就读不到了）",
                h
            );
        }
    }
}

/// 反向夹具：旧形态必须被同一判据认出，否则 G-p1 只是装饰。
#[test]
fn gp_reverse_old_form_is_caught() {
    let old = "fn live_reset() {\n    let mut l = live_lock();\n    l.current = None;\n}\n\
               fn stage(desc: &str) {\n    live_lock().current = Some((desc.to_string(), Instant::now()));\n}\n\
               fn finish(entry: Record) {\n    let mut l = live_lock();\n    l.done.push(entry);\n}\n\
               fn set_summary(s: String) {\n    let mut l = live_lock();\n    l.summary = s;\n}\n";
    let (found, ungated) = writer_gating(old);
    assert_eq!(found, 4, "G-p4 反向夹具不完整：只定位到 {} 个写入函数", found);
    assert_eq!(
        ungated.len(),
        4,
        "G-p4 反向失败：判据认不出「写侧无代际判定」的旧形态：{:?}",
        ungated
    );
}
