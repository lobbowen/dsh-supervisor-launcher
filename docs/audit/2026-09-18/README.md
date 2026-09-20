# 壳仓审计台账 2026-09-18（1.1.8 候选 / HEAD 0930884）

> 对 `release/shell-1.1.8` 候选提交 `0930884` 的只读审计（= 已发布 1.1.7 内容 + 文档移交 + 27 改 + 6 新文件）。
> 5 个执行者（1 分叉对齐 + 4 分片），**全程只读**：未做 git 写、未跑测试/构建。报告在本目录。

## 1. 基线

- 远端已发布：`v1.1.7`（`5df08aa`，npm `shell-*` + GitHub Release 12 资产）。
- 本地分叉：`00f872c`（未 push）+ 27 改 + 6 新；相对 `origin/main` **无重复、无冲突**（main tree == merge-base `1982239` tree，PR #19/#20/#21 是空合并）。
- 已对齐：`5df08aa → e41c1d8`(文档移交) `→ 0930884`(新工作)。

## 2. 结论分级

| 级别 | 数量 | 代表 |
|---|---|---|
| P0 | 2 | ①`dev_runtime_safety_test.rs` 扫描 0 文件（门禁空转/假绿）；②设计文档状态根仍写 `~/.dsh`（与 K-8 门禁冲突） |
| P1 | 8 | 模块图非 DAG（6 组双向循环）；命令层调 crate 根自由函数（越层）；`core_apply` 无总预算；`mirror::probe_all` 的 DNS 不覆盖 `PROBE_TIMEOUT`；canary 名单串行；`cannotSelfUpdate` 死分支（deb/rpm 无提权时强更必失败）；内核 RC-G1/G2 交叉门禁读错文件；壳侧无发布通道契约门禁 |
| P2/P3 | ~30 | withTimeout 不取消后端；spawn_daemon 未 detach/reap；下载/响应体无体积上限；契约序列化失败仍写盘；locate_core 无预算；bootstrap 死导出；文档树/阈值漂移等 |

## 3. 本版已修（随 1.1.8）

1. **P0-① 门禁空转**：`dev_runtime_safety_test.rs` 改以**仓根**为基准（新增 `repo_root()`），并加 `assert!(scanned > 0)`（R-G1/R-G2）。
2. **P1 死分支**：`bootstrap/js/40-shell-update.js` 由只认 `cannotSelfUpdate` 改为并认 `NS.shellId.selfUpdateCapable === false`。
3. **P1 core_apply 总预算**：`commands/mod.rs` 源循环前取 17min deadline（与前端 `CORE_APPLY_BUDGET_MS` 对齐），耗尽即停并如实回报已尝试源数。
4. **D-5 签名闸**：tag 构建缺 `TAURI_SIGNING_PRIVATE_KEY` 由 `::warning::` 升为 `::error::` + `exit 1`。
5. **操作者绝对路径**：`docs/DEVELOPMENT-TRACK.md` 去掉硬编码本机路径。

## 4. 已登记债（本版不修，理由见各报告）

- **结构**：模块图 6 组双向循环（建议抽真叶子 infra + 加无环门禁）；命令层越层调 `crate::log/run_install/shell_updater`（建议下沉 domain）。**属重构，改动面大，单独周期。**
- **热路径 P1**：`mirror::probe_all` 的 `thread::scope` 必须 join 全部线程且 ureq 超时不含 DNS（需换可取消探测）；canary 名单逐源串行。
- **P0-②**：设计文档状态根 `~/.dsh` 全量更到产品状态根（纯文档，多文件）。
- **门禁**：`no_console_window`/`env_toolchain_standard` 判据弱于内核 SSOT；壳侧发布通道契约门禁缺失。
- **跨仓**：内核 `release-channel-gate-test.js` 的 RC-G1/G2 应改读 `release_channel.rs`（算法已迁）；内核 `journal.js` 兜底写 `identity.json` 与「单写入方」冲突。

## 5. 分片报告索引

| 文件 | 内容 |
|---|---|
| `_s1-align.md` | 本地分叉逐文件分类 + 对齐方案 |
| `_s2-a-structure.md` | 模块划分/依赖方向/bootstrap/边界/门禁缺口 |
| `_s2-b-hotpath.md` / `_s2-b-release-probe.md` | 启动/工具链/更新计划/稳定性 + 下级分片 |
| `_s2-c-contract.md` | release_channel vs 内核契约/启动/单写入/跨仓文件/签名 |
| `_s2-d-deadcode-gates.md` / `_s2-d-docs.md` / `_s2-d-gates.md` | 死代码/文档漂移/门禁覆盖 |
| `_s3-exit-relaunch.md` | **退出管家后桌面壳自启**：跨仓完整排查 + 修复（PR #23 / 内核 PR #20） |
