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

/// 壳 → 面板：进度（非终结，**可多次**）。
///
/// 「可多次」一直是本消息的契约语义（否则它无需标为非终结），此前壳只回一条 `stage:'start'`
///   属于**未按契约实现**，故 2026-09-22（B4b）补齐多帧时**不**递增协议版本：
///   新帧只是在原有 `{stage}` 之上增加 `status` / `progress`，而内核侧的旧面板本就丢弃
///   progress 消息（`kernelUpdateBridge.ts` 的 `if (d.type === PROGRESS) return`）。
///   反之若递增版本，K1 的 `ev.data.v !== BRIDGE.v` 会让「新壳 + 尚未升级的旧内核面板」
///   直接拒收更新请求 —— 那才是真的把兼容打断。
pub const MSG_KERNEL_UPDATE_PROGRESS: &str = "dsh:kernel-update-progress";

/// 壳主帧必须执行的 Tauri 命令名（安装内核 → 重启守卫）。
pub const CMD_KERNEL_UPDATE_APPLY: &str = "kernel_update_apply";

/// 内核安装（逐源尝试）的**总时间预算**。
///
/// 为什么是这里而不是两处：同一条预算此前有**三个**数字 —— Rust 的 `17 * 60`、引导页的
///   `CORE_APPLY_BUDGET_MS = 1020000`、面板桥的默认 `6 * 60 * 1000`。前两个恰好相等纯属巧合，
///   第三个则**必然**误报：壳按 17 分钟正常逐源安装，面板 6 分钟就判「桌面壳无响应」，
///   用户据此重试 → 两个进程并发写同一个 npm 全局前缀。
///   现由 `core_apply_inner` 的 deadline 与本常量共用一个定义，其余消费者经 `maxWaitMs` 取。
pub const KERNEL_UPDATE_BUDGET_MS: u64 = 17 * 60 * 1000;

/// 预算之外允许的**收尾余量**：安装完成后还要定位内核、写 core.json、停+重拉守卫。
///
/// 为什么单列：这些步骤不在逐源 deadline 内，但同样会推迟终结消息的送达。面板的等待上界
///   必须是「预算 + 余量」这一个数，而不是让面板自己猜一个加法。
pub const KERNEL_UPDATE_GRACE_MS: u64 = 60 * 1000;

/// 面板可等待终结消息的上界（预算 + 收尾余量）。
pub const KERNEL_UPDATE_MAX_WAIT_MS: u64 = KERNEL_UPDATE_BUDGET_MS + KERNEL_UPDATE_GRACE_MS;

