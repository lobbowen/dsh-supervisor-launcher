//! 统一更新决策门禁（2026-09-15）—— 问题 1「桌面一套、内核一套」的机制层收口。
//!
//! 产品规则：壳与内核是同一套升级逻辑。本门禁锁定两侧的"检查"返回**同一形状**；
//! 执行器可按产物分派（壳=Tauri updater，内核=npm），但决策模型必须一套。
//!
//! 锁定不变量：
//!   U-1  内核 core_plan 与桌面 shell_update_check 都经 update_plan::unified
//!   U-2  统一形状包含公共键（artifact/current/latest/available/channel/source/error）
//!   U-3  反向：判据能识别旧的两套形态（门禁非空转）

use std::fs;
use std::path::PathBuf;

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// 读源码并归一化换行为 LF（Windows 检出为 CRLF，断言会失配）。
fn read(rel: &str) -> String {
    let s = fs::read_to_string(manifest_dir().join(rel))
        .unwrap_or_else(|e| panic!("读取 {} 失败: {}", rel, e));
    if s.contains('\r') { s.replace("\r\n", "\n") } else { s }
}

#[test]
fn u1_kernel_plan_uses_unified_shape() {
    let src = read("src/core.rs");
    assert!(
        src.contains("crate::update_plan::unified"),
        "U-1 失败：core.rs 的 build_plan 未走统一更新形状"
    );
    assert!(src.contains("\"kernel\""), "U-1 失败：内核计划未标注 artifact=kernel");
}

#[test]
fn u1_shell_check_uses_unified_shape() {
    let src = read("src/commands/mod.rs");
    assert!(
        src.contains("crate::update_plan::unified(\"shell\""),
        "U-1 失败：shell_update_check 未走统一更新形状（artifact=shell）"
    );
}

#[test]
fn u2_shared_keys_declared() {
    let src = read("src/update_plan.rs");
    for k in ["artifact", "current", "latest", "available", "channel", "source", "error"] {
        assert!(src.contains(&format!("\"{}\"", k)), "U-2 失败：统一形状缺键 {}", k);
    }
}

#[test]
fn u3_reverse_not_vacuous() {
    // 旧的两套形态：桌面用 ok/available，内核用 installed/action —— 不含 update_plan。
    let old_shell = "{\"ok\": true, \"available\": true, \"current\": cur}";
    let old_kernel = "{\"installed\": installed, \"action\": action}";
    assert!(!old_shell.contains("update_plan") && !old_kernel.contains("update_plan"),
        "U-3 反向判据自检失败");
}
