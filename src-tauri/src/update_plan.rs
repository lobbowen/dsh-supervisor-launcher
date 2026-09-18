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
///
/// **这是"版本号自己叫什么"，不是"它是被哪个 tag 选中的"** —— 两者必须分清：
///   · 本函数只看**版本字面量**的命名习惯（`-BETA.` / `-RC.`）；
///   · "从哪个通道选的"由 `release_channel::Selected::via` 回答。
/// 例：紧急回退的目标 `0.1.5-BETA.6` —— 版本名是 `beta`，但**通道是 `rollback`**。
///   把二者混为一谈会让面板把"回退中"显示成"测试版"（可观测性失真，契约 §4）。
///
/// 词表（契约 §2 的五个 tag）：`canary` / `beta` / `rc` / `latest` / `rollback`。
///   后两个是**通道身份**、不出现在版本号里（npm 的 dist-tag 与 semver 是两套命名空间），
///   故只能由 `selected_channel_of` 判定。
pub fn channel_of(version: Option<&str>) -> &'static str {
    match version {
        Some(v) if v.contains("-CANARY.") => "canary",
        Some(v) if v.contains("-BETA.") => "beta",
        Some(v) if v.contains("-RC.") => "rc",
        _ => "latest",
    }
}

/// 选版**依据** → 通道词（契约 §2 五通道；`via` 取自 `release_channel::Selected`）。
///
/// 与 `channel_of` 的分工：本函数回答"这一版是怎么被选出来的"，
///   因此 `rollback` 只在**显式回退 tag 生效**时为真 —— 正是契约 §2 第 3 条
///   「不可观测性」要求的能力：一眼看出"当前是否有回退在生效"。
pub fn selected_channel_of(via: &str) -> &'static str {
    match via {
        "rollback" => "rollback",
        "canary" => "canary",
        "latest" => "latest",
        // versions 兜底不是"通道"，但作为**降级信号**必须可见（正常路径不该出现）。
        "versions" => "fallback",
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
    // 通道的判定（2026-09-16 收口，消除 selected_channel_of 的死代码）：
    //   · 若调用方给了**选版依据**（extra.latestVia，由 latest_pick 带出）→ 用它。
    //     这才是"当前是否回退中"的**唯一可靠依据** —— 版本号字面量回答不了这个问题：
    //     回退目标 `0.1.5-BETA.6` 的名字是 beta，但通道是 rollback（契约 §2 第 3 条考察的正是此点）。
    //   · 否则退回"看版本名字"（桌面自更新等没有选版依据的场景）。
    let via = extra.get("latestVia").and_then(|v| v.as_str());
    let channel = match via {
        Some(v) => selected_channel_of(v),
        None => channel_of(latest.as_deref().or(current.as_deref())),
    };
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
        assert_eq!(channel_of(Some("0.1.6-CANARY.1")), "canary");
        assert_eq!(channel_of(Some("1.1.0")), "latest");
        assert_eq!(channel_of(None), "latest");
        // rollback 是**通道身份**而非版本命名：版本字面量里不会出现它
        //   （见 selected_channel_of —— 那才是"回退是否生效"的判据）。
        assert_eq!(channel_of(Some("0.1.5-BETA.6")), "beta");
    }

    /// 契约 §2 五通道词表：`selected_channel_of` 必须覆盖全部选版依据。
    #[test]
    fn selected_channel_covers_all_five_tags() {
        assert_eq!(selected_channel_of("rollback"), "rollback");
        assert_eq!(selected_channel_of("canary"), "canary");
        assert_eq!(selected_channel_of("latest"), "latest");
        // versions 兜底必须区别于 latest —— 否则面板会把"降级路径"显示成正常发布。
        assert_eq!(selected_channel_of("versions"), "fallback");
        assert_ne!(selected_channel_of("versions"), selected_channel_of("latest"));
        // 未知依据不得恐慌，回退到一个确定值
        assert_eq!(selected_channel_of("???"), "latest");
    }

    /// 反向（RC-G5 同族）：**回退中的版本**其 `channel_of` 仍是 beta，
    ///   但 `selected_channel_of` 必须是 rollback —— 二者不可互相替代。
    #[test]
    fn rollback_is_observable_even_when_version_name_looks_like_beta() {
        let v = Some("0.1.5-BETA.6");
        assert_eq!(channel_of(v), "beta");
        assert_eq!(selected_channel_of("rollback"), "rollback");
        assert_ne!(channel_of(v), selected_channel_of("rollback"));
    }
}
