//! 壳侧反回归：不得重新引入「回退 / 拉黑 / 冷却 / 跳过」机构（2026-09-14）。
//!
//! ## 归属为什么在壳仓
//!
//! 这条约束曾由**内核仓**的跨仓门禁承担 —— 内核测试去读壳仓源码、断言壳源码里没有
//! `attempt` / `pendingVersion` 等字段。那违反了双仓隔离（内核 <-> 壳是两个账号、两个仓库，
//! 运行期只经「已发布产物 / identity.json / registry.json」交互），并导致内核 CI 读壳仓
//! 默认分支的**浮动版本**、本地绿而 CI 红。
//!
//! 壳内部实现的约束，应由**壳仓自身测试**负责。本门禁扫描壳仓源码树（`src/` 与
//! `bootstrap/`），防止已废除的回退/拉黑机构复活。
//!
//! 判据用 `concat!` 拼接关键词，避免本文件自匹配（与 `update.rs::t5` 同一手法）。
//!
//! 边界（2026-09-21 澄清）：这四个 token 禁的是**标识符**，即那套按版本记账并抑制重试的
//! 机构本身，不是这些英文词的日常用法。安装包下载换源（`mirror::artifact_candidates` 的
//! 候选源循环）不属于被禁机构：它不落账本、不拉黑版本、失败时把每个源的原因一起报出，
//! 判据见 `docs/SHELL-UPDATE-CHANNEL-VERIFICATION.md` §九。但该循环里的变量**不得取这些
//! token 名**，否则本门禁按子串命中而红 —— 用 `candidate` 一类的名字。

use std::fs;
use std::path::{Path, PathBuf};

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let ents = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for e in ents.flatten() {
        let p = e.path();
        if p.is_dir() {
            let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if name == "target" || name == "node_modules" {
                continue;
            }
            collect(&p, out);
        } else if let Some(ext) = p.extension().and_then(|s| s.to_str()) {
            if matches!(ext, "rs" | "js" | "ts" | "html") {
                out.push(p);
            }
        }
    }
}

#[test]
fn no_suppression_machinery_in_shell_sources() {
    let root = manifest_dir();
    let tokens: [&str; 4] = [
        concat!("at", "tempt"),
        concat!("pending", "Version"),
        concat!("pinned", "Versions"),
        concat!("update", "-journal"),
    ];
    let mut files: Vec<PathBuf> = Vec::new();
    for sub in ["src", "bootstrap"] {
        collect(&root.join(sub), &mut files);
    }
    // 反向：必须真的扫到源码，否则该门禁恒真（假门禁）。
    assert!(
        files.len() > 5,
        "未扫描到足够的壳源码文件（门禁可能空转）：{}",
        files.len()
    );

    let mut offenders: Vec<String> = Vec::new();
    for f in &files {
        let text = fs::read_to_string(f).unwrap_or_default();
        for t in tokens {
            if text.contains(t) {
                offenders.push(format!("{} :: {}", f.display(), t));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "壳源码中不得出现回退/拉黑机构：{:?}",
        offenders
    );
}
