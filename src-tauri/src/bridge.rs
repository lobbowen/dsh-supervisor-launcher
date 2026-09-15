//! 面板→壳 消息桥契约（内核更新单写入者）。
//!
//! 背景：面板由**内核**托管，运行在壳主帧 \`shell.html\` 的内容 iframe 内。
//! Tauri 的 IPC 初始化脚本被标记为「仅主帧」（见 bootstrap_flow 的 B54/B55），
//! 故 iframe 内**不能**直接 \`invoke\`。而内核包的唯一写入者必须是壳 —— 于是需要一条
//! 受校验的 \`postMessage\` 通道把「更新内核」从面板转交壳主帧。
//!
//! 本模块是**唯一事实源**：版本与消息类型都在此定义，由 \`shell_bridge_contract\` 命令
//! 下发给 \`shell.html\` 使用（\`shell.html\` 不得硬编码这些字面量）。
//! 详见 docs/DESIGN-SHELL-ARCHITECTURE.md §3.2c。

/// 协议版本。任何语义变更都必须递增；内核侧 \`ui\` 桥各有同名常量并由门禁锁定。
pub const KERNEL_UPDATE_PROTOCOL_VERSION: u32 = 1;

/// 面板 → 壳：请求更新内核（壳是唯一写入者）。
pub const MSG_KERNEL_UPDATE_REQUEST: &str = "dsh:kernel-update-request";

/// 壳 → 面板：更新结果（终结消息）。
pub const MSG_KERNEL_UPDATE_RESULT: &str = "dsh:kernel-update-result";

/// 壳 → 面板：进度（非终结，可多次）。
pub const MSG_KERNEL_UPDATE_PROGRESS: &str = "dsh:kernel-update-progress";

/// 壳主帧必须执行的 Tauri 命令名（安装内核 → 重启守卫）。
pub const CMD_KERNEL_UPDATE_APPLY: &str = "kernel_update_apply";
