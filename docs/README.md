# 壳仓文档索引（docs/README.md）

> **先读哪个？** 要发布/构建 → 读 `RELEASE-STANDARD.md`（唯一流程事实源）。
> 要理解设计 → 读设计类；要看历史决策过程 → 读时间点记录类。

## 角色表（**规范性 vs 时间点记录**）

| 文档 | 角色 | 讲什么 |
|---|---|---|
| `RELEASE-STANDARD.md` | **规范（现行）** | **发布/构建流程的唯一事实源**（阶段/矩阵/CI/验证/回滚/红线）|
| `RELEASE-AND-BUILD-DECISION.md` | 决策依据（现行）| 为什么这样发布/构建（背景与理由）|
| `DESIGN-SHELL-ARCHITECTURE.md` | 设计（现行）| 壳的架构 |
| `DESIGN-BOUNDARY.md` | 设计（现行）| 壳与内核的边界契约 |
| `DESIGN-COMPLETE.md` | 设计（现行，**汇总**）| 完整设计（体量最大，含历年增补）|
| `SHELL-NATIVE-STABILITY-DECISION.md` | 决策（现行）| 原生稳定性（三平台差异的规范答案）|
| `SHELL-UPDATE-CHANNEL-VERIFICATION.md` | 验证记录（现行）| 更新通道验证 |
| `AUDIT-SHELL-BOOTSTRAP.md` | **时间点记录** | 引导流程审计（已完成）|
| `SHELL-STABILITY-AUDIT.md` | **时间点记录** | 稳定性审计（已完成）|
| `SHELL-EXECUTION-PLAN.md` | **时间点记录** | 执行计划（已执行）|
| `SHELL-BOOTSTRAP-REMEDIATION.md` | **时间点记录** | 引导修复过程 |
| `SHELL-UPDATE-TRIGGER-CORRECTION.md` | **时间点记录** | 更新触发修正过程 |

> 时间点记录类**不再作为现行规范**；其结论已并入上表的规范/设计文档。
> 保留仅为可追溯。若某条与现行文档冲突，**以现行文档为准**。

## 相关但不在本目录

| 文件 | 内容 |
|---|---|
| `../README.md` | 壳仓总览 |
| `../CHANGELOG.md` | 变更记录 |
| 内核仓 `RELEASE-STANDARD.md` | **内核**发布流程（与壳独立）|
| 内核仓 `CREDENTIALS-STANDARD.md` | 凭据管理（两仓共用同一套凭据库）|
| 内核仓 `DEVELOPMENT-TRACK.md` | 改代码的规则（分层/测试/注入验证）|

## 一致性由门禁保证

`src-tauri/tests/release_spec_consistency_test.rs`（8 条）校验 `RELEASE-STANDARD.md` **与仓库现实一致**，含：
入口存在、`.github/workflows/` 外无幽灵 workflow、四平台矩阵一致、job 名一致、
版本三处互锁、版本校验已接 CI、章节齐备、反向判据。

> 本仓历史上存在过 `src-tauri/launcher-build.yml`（**幽灵产线**：不在 `.github/workflows/` 下，
> GitHub 永不执行，却被打成「产线」写进文档）—— 现由 R-2 永久禁止该类文件。
