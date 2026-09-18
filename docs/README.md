# 壳仓文档索引（docs/README.md）

> 先读哪个？要发布/构建 → `RELEASE-STANDARD.md`（唯一流程事实源）。
> 要理解设计 → `DESIGN-SHELL-ARCHITECTURE.md` / `DESIGN-BOUNDARY.md`。

## 产品硬规则（不可协商）

- 壳的职责只有三件：**下载内核 → 安装内核 → 拉起服务**。
- 壳与内核是**同一套升级逻辑**：检测到新版本即**强制更新**；
  **不得跳过、不得按版本拉黑、不得冷却抑制**（不得有「隐式回退」）。
- 升级方向**不得靠版本比较推断回退**（「latest 低于全量最高即回退」已被证伪，见内核
  `RELEASE-CHANNEL-CONTRACT.md` §2 与 RC-1..RC-3）；**紧急回退是契约内的显式通道**：
  运维给内核包打 `rollback` dist-tag，壳 `src-tauri/src/release_channel.rs` 按 §3 第 ① 步执行（RC-2）。
  DSH 自身升级的回滚属于内核仓，与本仓无关。

## 文档角色表

| 文档 | 角色 | 讲什么 |
|---|---|---|
| `RELEASE-STANDARD.md` | **规范（现行）** | **发布/构建流程的唯一事实源**（阶段/矩阵/CI/验证/回滚红线）|
| `RELEASE-AND-BUILD-DECISION.md` | 决策依据（现行）| 为什么这样发布/构建（背景与理由）|
| `KERNEL-LAUNCH-STANDARD.md` | **规范（现行）** | **内核启动的唯一事实源**（跨平台 P0–P6 流水线 / 对齐前置 / 平台矩阵 / core.json）|
| `ENV-TOOLCHAIN-INSTALL-STANDARD.md` | **规范（现行）** | **环境工具链检测/安装/下载的唯一事实源**（node 与 npm 并行同权；统一安装事件与 UI 规范；禁进度条）|
| `DESIGN-SHELL-ARCHITECTURE.md` | 设计（现行）| 壳的工程架构与门禁 |
| `DESIGN-BOUNDARY.md` | 设计（现行）| 壳与内核的边界契约 |
| `DESIGN-COMPLETE.md` | 设计（现行，**汇总**）| 完整设计（体量最大）|
| `SHELL-UPDATE-CHANNEL-VERIFICATION.md` | 验证记录（现行）| 更新通道验证 |
| `UPDATER-SIGNING-KEY.md` | 运维手册（现行）| minisign 自更新签名密钥的保管/备份/验证/轮换（**不含私钥**）|
| `DESKTOP-ACCEPTANCE.md` | 验收清单（现行）| 桌面壳真机验收（引导页/服务定义/自更新，GUI 场景）|
| `DEVELOPMENT-TRACK.md` | **规范（唯一事实源）** | **壳仓改代码规则**：运行时禁区（R-1/R-2/R-3，源码开发绝不触碰系统安装版）|

## 相关但不在本目录

| 文件 | 内容 |
|---|---|
| `../README.md` | 壳仓总览 |
| `../CHANGELOG.md` | 变更记录 |
| 内核仓 `RELEASE-STANDARD.md` | **内核**发布流程（与壳独立）|
| 内核仓 `CREDENTIALS-STANDARD.md` | 凭据管理（两仓共用同一套凭据库）|
| 内核仓 `DEVELOPMENT-TRACK.md` | 改代码的规则（分层/测试/注入验证）|
| 内核仓 `RELEASE-CHANNEL-CONTRACT.md` | **发布通道/选版唯一事实源**（canary/beta/rc/latest/rollback；壳侧由 `src-tauri/src/release_channel.rs` 实现、`core.rs` 消费）|
| 内核仓 `DSH-TOKEN-CONTRACT.md` | 令牌分类与铁律（壳不持有 DSH 令牌，仅经配置读 `remoteToken`）|
| 内核仓 `NO-CONSOLE-WINDOW-STANDARD.md` | **两仓共遵**的无控制台窗口规范（壳侧由 `src-tauri/tests/no_console_window_test.rs` 校验）|

## 一致性由门禁保证

`src-tauri/tests/release_spec_consistency_test.rs` 校验 `RELEASE-STANDARD.md` **与仓库现实一致**，含：
入口存在、`.github/workflows/` 外无幽灵 workflow、四平台矩阵一致、job 名一致、
版本三处互锁、版本校验已接 CI、章节齐备、反向判据。

> 本仓历史上存在过 `src-tauri/launcher-build.yml`（**幽灵产线**：不在 `.github/workflows/` 下，
> GitHub 永不执行，却被打成「产线」写进文档）—— 现由 R-2 永久禁止该类文件。
