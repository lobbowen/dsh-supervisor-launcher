# 壳仓本地克隆分叉分析（S1 · 只读分析 + 对齐方案）

> 执行者：子代理（**只读 git**，未做任何 git 写、未跑测试/构建）。
> 仓库：壳仓本地克隆（remote=`wasi7mglns/dsh-supervisor-launcher`）。
> 备份（未改动）：主控指定目录下的 `worktree-unstaged.patch` / `0001-chore-shell.patch` / `status.txt` / `classify.txt`。

## 0. 结论（TL;DR）

1. **分叉根因**：本地提交 `00f872c`（「接收内核仓移交的壳资产文档并登记索引」）**从未 push**，且它之上还有
   **27 个已跟踪文件改动 + 6 个未跟踪新文件**未提交。
2. **分类结论：这 33 项 + `00f872c` 的 3 个文件，全部属于「② main 没有的新工作」**；
   **无 ①（已并入 main 的重复）、无 ③（同区域冲突/被 main 部分覆盖）**。
3. 决定性证据：**`origin/main` 的 tree 与 merge-base `1982239` 的 tree 逐字节相同**
   （`git rev-parse 1982239^{tree}` == `git rev-parse origin/main^{tree}` == `d04328d628a356a54136f9ca7001af189febffae`；
   `git diff --stat 1982239 origin/main` 为空）⇒ PR #19/#20/#21 是**空合并**，main 未引入任何内容。
   因此本地已包含 windows 包装/1.1.6 合并（作为 `1982239` 的祖先），不存在「缺少 main 合并」的叠加。
4. **对齐已被主控执行**（本报告写作期间实测）：本地新分支 `release/shell-1.1.8` =
   `5df08aa`(origin/main) + `e41c1d8`(cherry-pick `00f872c`) + `0930884`(27 改 + 6 新，33 文件)。
   工作树已 clean。
5. **仍待主控**：版本提升 1.1.7→1.1.8（三处互锁 + CHANGELOG）、push 新分支并 PR、合并后打 tag `v1.1.8` 触发发布。

## 1. 分叉几何（只读证据）

```
* 0930884 (HEAD -> release/shell-1.1.8) feat(shell): 内核选版契约落地（release_channel）+ npm 复探与工具链加固
* e41c1d8 chore(shell): 接收内核仓移交的壳资产文档并登记索引
*   5df08aa (tag: v1.1.7, origin/main) Merge pull request #21 from wasi7mglns/fix/toolchain-npm-1.1.7
|\
| * 1982239 (origin/fix/toolchain-npm-1.1.7) fix(shell): 工具链契约补齐 npm …
|/
*   c3e878d (tag: v1.1.6) Merge pull request #20 …
...
```

| 项 | 值 |
|---|---|
| 分叉前本地 HEAD | `00f872c`（基于 `1982239`） |
| merge-base(HEAD, origin/main) | `1982239` |
| origin/main | `5df08aa`（tag `v1.1.7`） |
| 本地未推送提交 | 仅 `00f872c`（`git log --oneline origin/main..HEAD`） |
| main 多出的提交 | 3 个**空合并**（`95d161a`/`c3e878d`/`5df08aa`），tree 与 `1982239` 相同 |
| `00f872c` 是否推送 | 否（`git branch -r --contains 00f872c` 为空） |

## 2. 逐文件结论表

判定记号：T=工作树、H=本地 HEAD(`00f872c`)、W=`origin/main`(`5df08aa`)。
**25 个已跟踪文件满足 `H==W`（本地提交与 main 内容一致）→ 未提交改动是在 main 之上的纯新工作**。

| 文件 | T vs W | T vs H | H vs W | 工作树相对 main | 结论 |
|---|---|---|---|---|---|
| `.github/workflows/build.yml` | DIFF | DIFF | SAME | +15 −2 | ② 新工作 |
| `README.md` | DIFF | DIFF | SAME | +12 −4 | ② |
| `docs/DESIGN-BOUNDARY.md` | DIFF | DIFF | SAME | +2 −2 | ② |
| `docs/DESIGN-COMPLETE.md` | DIFF | DIFF | SAME | +6 −5 | ② |
| `docs/DESIGN-SHELL-ARCHITECTURE.md` | DIFF | DIFF | SAME | +9 −8 | ② |
| `docs/DESKTOP-ACCEPTANCE.md` | DIFF | DIFF | **DIFF** | +50 −0 | ②（`00f872c` 的 +48 不在 main，另 +2 未提交） |
| `docs/KERNEL-LAUNCH-STANDARD.md` | DIFF | DIFF | SAME | +11 −11 | ② |
| `docs/README.md` | DIFF | DIFF | **DIFF** | +12 −2 | ②（`00f872c` 的 +2 索引不在 main，另 +10 −2） |
| `docs/RELEASE-STANDARD.md` | DIFF | DIFF | SAME | +1 −1 | ② |
| `src-tauri/bootstrap/bootstrap.html` | DIFF | DIFF | SAME | +3 −7 | ② |
| `src-tauri/bootstrap/js/10-ui.js` | DIFF | DIFF | SAME | +34 −6 | ② |
| `src-tauri/bootstrap/js/20-env.js` | DIFF | DIFF | SAME | +42 −12 | ② |
| `src-tauri/bootstrap/js/40-shell-update.js` | DIFF | DIFF | SAME | +4 −4 | ② |
| `src-tauri/bootstrap/js/50-kernel.js` | DIFF | DIFF | SAME | +4 −3 | ② |
| `src-tauri/bootstrap/js/80-init.js` | DIFF | DIFF | SAME | +11 −39 | ② |
| `src-tauri/src/commands/mod.rs` | DIFF | DIFF | SAME | +54 −16 | ② |
| `src-tauri/src/core.rs` | DIFF | DIFF | SAME | +386 −40 | ② |
| `src-tauri/src/core_contract.rs` | DIFF | DIFF | SAME | +1 −1 | ② |
| `src-tauri/src/env.rs` | DIFF | DIFF | SAME | +41 −6 | ② |
| `src-tauri/src/main.rs` | DIFF | DIFF | SAME | +96 −21 | ② |
| `src-tauri/src/mirror.rs` | DIFF | DIFF | SAME | +1 −1 | ② |
| `src-tauri/src/node.rs` | DIFF | DIFF | SAME | +36 −0 | ② |
| `src-tauri/src/runtime_contract.rs` | DIFF | DIFF | SAME | +2 −2 | ② |
| `src-tauri/src/update_plan.rs` | DIFF | DIFF | SAME | +64 −1 | ② |
| `src-tauri/tests/bootstrap_flow.rs` | DIFF | DIFF | SAME | +17 −7 | ② |
| `src-tauri/tests/mirror_env_wiring_test.rs` | DIFF | DIFF | SAME | +22 −18 | ② |
| `src-tauri/tests/round13_p3_batch_test.rs` | DIFF | DIFF | SAME | +7 −5 | ② |

**6 个未跟踪新文件**（`git cat-file -e origin/main:<f>` 均不存在）→ 全部 ②：
`docs/DEVELOPMENT-TRACK.md`、`docs/ENV-TOOLCHAIN-INSTALL-STANDARD.md`、
`src-tauri/src/release_channel.rs`、`src-tauri/tests/dev_runtime_safety_test.rs`、
`src-tauri/tests/env_toolchain_standard_test.rs`、`src-tauri/tests/no_console_window_test.rs`。

**`00f872c` 提交本身**（3 文件，+146；main 均不含）→ ②：
`docs/DESKTOP-ACCEPTANCE.md`(+48)、`docs/README.md`(+2 索引)、`docs/UPDATER-SIGNING-KEY.md`(+96，main 无此文件)。

> **①/③ 计数 = 0**。原因见 §0.3：main 的 tree == merge-base，无 main 侧改动可与本地重叠。

## 3. 主题归纳

| 主题 | 文件 |
|---|---|
| **A. 发布通道选版（对齐内核 `RELEASE-CHANNEL-CONTRACT`）** | 新增 `src-tauri/src/release_channel.rs`（§3 冻结算法的唯一实现）；`src-tauri/src/core.rs`/`update_plan.rs` 接入；`.github/workflows/build.yml` 修正 dist-tag（`-RC.n → latest` + 补 `rc` 别名） |
| **B. 工具链契约：npm 与 node 同等必需** | 新增 `docs/ENV-TOOLCHAIN-INSTALL-STANDARD.md`；`src-tauri/src/env.rs`/`node.rs`/`main.rs`/`commands/mod.rs`；`bootstrap/js/20-env.js` |
| **C. 无控制台窗口（对齐内核 `NO-CONSOLE-WINDOW-STANDARD`）** | `src-tauri/src/env.rs`（`bounded::prepare` 唯一封装点）；新增 `tests/no_console_window_test.rs` |
| **D. 开发轨道 / 运行时禁区** | 新增 `docs/DEVELOPMENT-TRACK.md`；新增 `tests/dev_runtime_safety_test.rs` |
| **E. 内核↔壳契约 / 运行时契约** | `core_contract.rs`/`runtime_contract.rs`/`mirror.rs`/`core.rs`/`update_plan.rs`；新增 `tests/env_toolchain_standard_test.rs` |
| **F. bootstrap 启动统一** | `bootstrap.html`、`bootstrap/js/{10-ui,20-env,40-shell-update,50-kernel,80-init}.js` |
| **G. 文档移交与索引** | `00f872c`：`DESKTOP-ACCEPTANCE.md`/`README.md`/`UPDATER-SIGNING-KEY.md`；另 `DESIGN-*.md`/`KERNEL-LAUNCH-STANDARD.md`/`RELEASE-STANDARD.md` 更新 |
| **H. 门禁/测试** | 更新 `bootstrap_flow.rs`/`mirror_env_wiring_test.rs`/`round13_p3_batch_test.rs`；新增 3 个门禁测试（见 C/D/E） |

## 4. 安全对齐方案

### 4.1 已被主控执行（实测结果）
```
git checkout -b release/shell-1.1.8 5df08aa          # 以已发布 main 为基
git cherry-pick 00f872c                              # -> e41c1d8（3 文档，+146）
git add -A && git commit -m "feat(shell): …"          # -> 0930884（33 文件，+2476/−228）
```
结果：`release/shell-1.1.8` = `5df08aa` + `e41c1d8` + `0930884`；工作树 clean；**无内容冲突**。

### 4.2 仍待主控执行（我禁 git 写，仅列出）
```
# 1) 版本提升（三处互锁；用壳仓自带单入口）
bash scripts/bump-shell.sh 1.1.8          # Cargo.toml / tauri.conf.json / lock 同步（以仓内实际脚本名为准）
# 2) CHANGELOG：[未发布] 整理为 [1.1.8]，新开 [未发布]
# 3) 推送新分支并开 PR（不覆盖 main、不 force-push 旧分支）
git push -u origin release/shell-1.1.8
# 4) CI 绿 + review 合并到 main 后，打 tag 触发发布（安装程序 + npm 壳包 + GitHub Release）
git tag v1.1.8 && git push origin v1.1.8
```
> 若暂不发布，可只推送 `release/shell-1.1.8` 作为 WIP/归档分支（或另建 `archive/shell-local-work`），
> **不要** force-push 覆盖 `fix/toolchain-npm-1.1.7`(=1982239) 或 main。

## 5. 风险点

1. **未编译/未跑测试**：33 文件含 Rust 改动 + 3 个新门禁测试；本机禁构建/测试，必须由壳仓 CI（`push`/PR 触发，4 平台矩阵）裁决。
2. **版本未提升**：分支名 `release/shell-1.1.8` 但 `Cargo.toml`/`tauri.conf.json` 仍为 `1.1.7`；不提升就发会与已发布 1.1.7 撞版（npm 同版本不可重发）。
3. **文档索引**：新增的 `DEVELOPMENT-TRACK.md`/`ENV-TOOLCHAIN-INSTALL-STANDARD.md` 已由 `0930884` 写入 `docs/README.md`（实测在册）；仍需确认壳仓是否有「文档清单一致性」门禁。
4. **备份不含未跟踪文件的风险已消除**：6 个未跟踪新文件现已进入 `0930884`（本地 git 对象），不再只存在于工作树。
5. **旧本地分支**：`fix/toolchain-npm-1.1.7@00f872c` 已被 `release/shell-1.1.8` 取代；保留为本地备份分支即可，勿再其上继续开发。
6. **发布通道语义**：`.github/workflows/build.yml` 把 `-RC.n` 改为写 `latest`（与内核一致）——这是**行为变更**，需在壳仓 CHANGELOG/契约同步说明，避免与旧 `rc` tag 预期冲突。
