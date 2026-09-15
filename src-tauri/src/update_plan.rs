//! 统一更新决策模型（桌面壳与内核**同一形状**）。
//!
//! ## 为什么需要（问题 1 的根因）
//!
//! 产品规则：壳与内核是**同一套升级逻辑**（有新版必须强制更新，不得回退/跳过）。
//! 但旧实现两侧返回不同 JSON：
//!   · 内核 core_plan：installed / action / registry / error；
//!   · 桌面上更新：ok / available / current / latest / notes / date。
//! 前端因此表现为"两套流程"（用户明确反馈的感受）。
//!
//! 现统一为同一组键：artifact / current / latest / available / channel / source / error，
//! 各侧特有键以 extra 附加（向后兼容既有前端字段）。
//! 执行器按产物类型分派（壳=Tauri updater，内核=npm），但**决策模型是一套**。

use serde_json::{json, Map, Value};

/// 版本 → 发布通道（两侧同一词表）。
pub fn channel_of(version: Option<&str>) -> &'static str {
    match version {
        Some(v) if v.contains("-BETA.") => "beta",
        Some(v) if v.contains("-RC.") => "rc",
        _ => "latest",
    }
}

/// 统一更新计划形状：公共键 + 各侧特有键（extra）。
pub fn unified(
    artifact: &str,
    current: Option<String>,
    latest: Option<String>,
    available: bool,
    source: Option<String>,
    error: Option<String>,
    extra: Map<String, Value>,
) -> Value {
    let channel = channel_of(latest.as_deref().or(current.as_deref()));
    let mut m = Map::new();
    m.insert("artifact".into(), json!(artifact));
    m.insert("current".into(), json!(current));
    m.insert("latest".into(), json!(latest));
    m.insert("available".into(), json!(available));
    m.insert("channel".into(), json!(channel));
    m.insert("source".into(), json!(source));
    m.insert("error".into(), json!(error));
    for (k, v) in extra {
        m.insert(k, v);
    }
    Value::Object(m)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unified_shape_is_stable() {
        let mut extra = Map::new();
        extra.insert("action".into(), json!("upgrade"));
        let v = unified(
            "kernel",
            Some("0.1.5-BETA.3".into()),
            Some("0.1.5-BETA.4".into()),
            true,
            Some("npm".into()),
            None,
            extra,
        );
        for k in ["artifact", "current", "latest", "available", "channel", "source", "error", "action"] {
            assert!(v.get(k).is_some(), "缺键 {}", k);
        }
        assert_eq!(v["artifact"], json!("kernel"));
        assert_eq!(v["channel"], json!("beta"));
        assert_eq!(v["available"], json!(true));
    }

    #[test]
    fn channel_of_is_single_vocabulary() {
        assert_eq!(channel_of(Some("0.1.5-BETA.4")), "beta");
        assert_eq!(channel_of(Some("1.1.0-RC.1")), "rc");
        assert_eq!(channel_of(Some("1.1.0")), "latest");
        assert_eq!(channel_of(None), "latest");
    }
}
