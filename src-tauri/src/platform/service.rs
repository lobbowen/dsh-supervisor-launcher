//! 服务控制契约（**定义 + 启停** 同一对象）。
//!
//! 服务**定义**与**启停**是同一个对象：平台实现无法只改一半。
//!
//! == 不变量 ==
//!
//! · **P1 显式性**：不支持的能力返回 `Err(Unsupported …)`，**绝不静默成功**。
//! · **B1 有界性**：所有外部命令经 [`crate::bounded`]（超时即 kill）。
//! · **幂等性**：`ensure_defined` 对已存在的定义直接返回成功。

use std::path::PathBuf;

pub trait ServiceControl: Send + Sync {
    /// 服务管理器种类（`systemd` / `launchagent` / `schtasks` / `none`）。
    fn kind(&self) -> &'static str;

    /// 服务定义文件的规范路径。
    ///
    /// Windows 的计划任务没有文件，返回标识串（`schtasks://DSH-Supervisor`）供日志。
    fn definition_path(&self) -> PathBuf;

    /// 服务定义**当前是否已存在**（跨平台的事实判定）。
    ///
    /// 2026-09-13（P3 修复，失效模式 a+e）：**不能一律用 `definition_path().is_file()`**。
    ///   Windows 的 "路径" 是标识串 `schtasks://DSH-Supervisor`（没有文件），
    ///   `is_file()` **恒为 false** —— 于是 `--service-plan` 自检无论计划任务
    ///   是否真的存在/刚被建立，都报「现存 = 否」，把排障者的方向带偏
    ///   （而这个自检正是本仓「服务定义」能力的官方验证入口）。
    ///
    ///   默认实现 = 文件存在性（Linux systemd unit / macOS LaunchAgent plist 都适用）；
    ///   Windows 覆写为 `schtasks /Query` 的真实判定。
    fn is_defined(&self) -> bool {
        self.definition_path().is_file()
    }

    /// 建立服务定义（幂等）。
    ///
    /// 返回人类可读的状态描述。**注意**：`enable` 失败不应返回 Err ——
    /// 定义已写入时仍可在 `start` 阶段拉起（并另有 `spawn_daemon` 兜底），
    /// 把「可继续」误判为「彻底失败」会让用户卡在引导页。
    fn ensure_defined(&self, spec: &crate::platform::LaunchSpec) -> Result<String, String>;

    /// 请求服务管理器启动（不直接 spawn）。
    fn start(&self) -> Result<(), String>;

    /// 请求服务管理器停止（守卫的所有者动作）。
    fn stop(&self) -> Result<(), String>;

    /// 服务管理器不可用时的**直接 spawn 兜底**：启动稳定入口 `<壳> --run-guard`。
    ///
    /// 设计取舍：项目原约束为「壳绝不直接 spawn 守卫」（避免游离于服务管理器的
    /// 第二实例），但该约束不能凌驾于**可用性**之上 —— 容器、无 user systemd
    /// session、launchctl 被策略拦截、schtasks 被组策略禁止等场景下服务管理器
    /// 根本无法使用，若无兜底用户被永久挡在门外。
    ///
    /// 第二实例风险由调用方规避：spawn 前已确认端口不存活，
    /// 且 spawn 后仍以「端口就绪」为唯一成功判据（而非进程是否存活）。
    ///
    /// **统一实现（三平台一致）**：不再各自拼 node/guard —— 由 `--run-guard` 运行时检测。
    ///
    /// 2026-09-21（B2）：标准流改走 [`crate::platform::guard_stdio`]，不再三条 `null()`。
    /// 兜底路径恰恰是**最需要正文**的路径 —— 服务管理器已经不可用了，若再把子进程的
    /// stderr 丢掉，这次失败就只剩「超时」两个字。
    fn spawn_daemon(&self, spec: &crate::platform::LaunchSpec) -> Result<u32, String> {
        let mut cmd = std::process::Command::new(&spec.shell);
        cmd.arg("--run-guard")
            .env("DSH_SUPERVISOR_HOME", &spec.state_root);
        crate::platform::guard_stdio(&mut cmd);
        // CREATE_NO_WINDOW 的唯一封装点在 infra（GUI 进程拉子进程不闪控制台），
        // 本文件因此不再需要 `#[cfg(windows)]` 块。
        crate::bounded::prepare(&mut cmd);
        let child = cmd.spawn().map_err(|e| {
            format!("直接拉起守卫失败: {}（{} --run-guard）", e, spec.shell.display())
        })?;
        Ok(child.id())
    }
}