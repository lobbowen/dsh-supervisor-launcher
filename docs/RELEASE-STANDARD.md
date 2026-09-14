# 壳仓发布与构建标准（SHELL RELEASE-STANDARD）

> **本文件是壳仓发布/构建的唯一事实源（SSOT）。**
> 由 `src-tauri/tests/release_spec_consistency_test.rs` **机器校验**：
> 本文件写的每个入口、矩阵、job、脚本都必须与仓库现实一致 —— 规范**无法漂移**。

> 与内核的关系：**两套独立流程**（内核 → npm 平台子包；壳 → 安装程序 + npm 壳包）。
> 内核流程见内核仓 `RELEASE-STANDARD.md`；壳侧不重复它的内容。

## 0. 硬标准（2026-09-13，不可协商）

> **所有平台构建与发布必须经 GitHub CI 完成。本地不得产生任何发布产物。**

| 要求 | 壳仓实现 | 门禁 |
|---|---|---|
| 四平台构建只在 CI 内发生 | `build` job 的 4 runner 矩阵，各 runner 只构建自己平台 | R-3 |
| 本地无全平台构建脚本 | 壳仓**本就没有**本地构建/发布脚本（仅 `bump-shell.sh` + `verify-shell-versions.js`）| R-9（新增）|
| 发布只在 CI 内 | `publish` job（tag 触发）| R-4 |
| 无本地发布产物入口 | `package.json` 不存在本地 release/publish script | R-9 |

**为什么**：本地构建让「产物从哪来」不可复现、不可审计；统一到 CI 后产物可追溯、四平台同构、发布单一入口。

---
## 为什么需要这份文件（问题的实质）

壳仓此前**没有单一权威流程文档**，且存在一个**幽灵产线文件**：

`src-tauri/launcher-build.yml`（290 行）—— 它**不在 `.github/workflows/` 下**，
GitHub **永远不会执行**它；但 `docs/RELEASE-AND-BUILD-DECISION.md` 与 `scripts/bump-shell.sh` 都
**曾声称它是产线**（「tag 触发 launcher-build.yml：四平台完整构建」）。
真实产线是 `.github/workflows/build.yml` —— 两份定义已漂移（触发策略、步骤数均不同）。

已处置：**删除幽灵文件**，引用改指真实产线，并由本标准的门禁**禁止再出现**
（`.github/workflows/` 之外不得有 workflow YAML）。

## 各文档的分工（不再重复，只指向）

| 文档 | 讲什么 |
|---|---|
| **本文件** | **流程**（阶段/入口/矩阵/门禁/放行/验证/回滚）|
| `docs/RELEASE-AND-BUILD-DECISION.md` | 决策**背景与理由** |
| `docs/DESIGN-SHELL-ARCHITECTURE.md` / `DESIGN-BOUNDARY.md` / `DESIGN-COMPLETE.md` | 设计（架构/边界/完成态）|
| `docs/SHELL-UPDATE-CHANNEL-VERIFICATION.md` | 专项验证记录 |

---

## 1. 全流程（阶段化）

| 阶段 | 名称 | 命令 / 位置 | 必须绿 |
|---|---|---|---|
| H0 | 版本提升（三处同步）| `bash scripts/bump-shell.sh <ver>` | 是 |
| H1 | 版本一致性 | `node scripts/verify-shell-versions.js` | 是（CI 的 version job 亦强制）|
| H2 | 门禁测试 | `cargo test --bins <tests>` | 是 |
| H3 | 无头冒烟 | `cargo run -- --node-plan` | 是 |
| H4 | 构建 + 打包 | `cargo tauri build`（bundles 见第 2 节）| 是 |
| H5 | glibc 基座门禁（仅 Linux）| `bash ci/check-glibc.sh <bin> 2.35` | 是 |
| H6 | 组装 npm 壳包 | `node shell-release/assemble-shell-pkg.js --platform <p>` | 是 |
| H7 | 产物验收（同源验签 + 清单契约）| `cargo test --test updater_artifacts` | 是 |
| H8 | 发布（tag `v*`）| `publish` job：归拢产物 → `node shell-release/make-manifest.js` → `npm publish` | 是 |
| H9 | 发布后验证 | 见第 5 节 | 是 |

> 本地可做 H0–H5 / H7；H6 / H8 走 CI（四平台产物必须来自各自 runner）。

## 2. 平台矩阵（4 平台，唯一来源 = CI 矩阵）

| runner | artifact | bundles | glibc 上限 |
|---|---|---|---|
| `ubuntu-22.04` | `linux-x64` | `deb,rpm` | `2.35` |
| `windows-latest` | `win-x64` | `nsis,msi` | — |
| `macos-latest` | `darwin-arm64` | `app,dmg` | — |
| `macos-15-intel` | `darwin-x64` | `app,dmg` | — |

约束：

- Linux **必须** ubuntu-22.04 基座（glibc 2.35）—— 在 24.04 构建的产物无法在 22.04 / Debian 12 运行；
- Linux 已**废弃 AppImage**，改用标准 `deb` / `rpm`；
- darwin-x64 用 **`macos-15-intel`**（原生 Intel；`macos-13` 已弃用）；
- 壳**必须四平台** —— 缺任一平台则桌面安装程序不全。

## 3. 版本单一事实源（三处互锁）

| 处 | 字段 |
|---|---|
| `src-tauri/Cargo.toml` | `[package].version` |
| `src-tauri/tauri.conf.json` | `version`（Tauri 打包与更新清单读它）|
| `src-tauri/Cargo.lock` | `[[package]] name = "dsh-supervisor-gui"` 的 `version` |

三处由 `scripts/bump-shell.sh` 同步写入，由 `scripts/verify-shell-versions.js` 校验，
并由 CI 的 `version` job **强制**（2026-09-13 补：此前该脚本**从未被 CI 调用**）。

> 壳版本与内核版本**相互独立**，不需要也不得保持一致。

## 4. CI 与放行条件

触发：`push`（`main` 与 `v*` tag）、`pull_request`（`main`）、`workflow_dispatch`。

| job | 何时跑 | 作用 |
|---|---|---|
| `version` | 总是 | 读版本 + **校验三处一致**（`verify-shell-versions.js`）|
| `build` | 总是（4 平台矩阵）| 门禁测试 → 无头冒烟 → 构建打包 → glibc 门禁 → 组装壳包 → 产物验收 → 上传；tag 时挂 Release |
| `publish` | tag `v*` | 归拢四平台产物 → 生成并校验 `shell-manifest.json` → 发布 npm 壳包 |

> `pull_request` 触发器于 2026-09-13 补入：此前 PR **完全不跑 CI**，
> 若直接设 required status check 会让 PR **永久等待一个永不出现的状态**。

分支保护（服务器端放行条件）：**`version` + 4 条 `build (...)`**，strict + enforce_admins。

> 注意 required 的 context **内嵌矩阵参数**（如 `build (ubuntu-22.04, linux-x64, deb,rpm, 2.35)`）。
> **改平台矩阵时必须同步更新分支保护**，否则旧语境永不出现 → 所有 PR 阻塞。

## 5. 发布后验证（H9）

| 项 | 位置 | 期望 |
|---|---|---|
| npm 壳包四平台 | `npm view @dsh-sup/shell-<platform>@<ver> version` ×4 | 四者皆等于目标版本 |
| 清单包 | `npm view @dsh-sup/shell-release dist-tags` | 指向新版本 |
| GitHub Release | tag `v<ver>` | 四平台安装包附件齐备 |
| CI 结论 | tag run | version + 四平台 build + publish 全绿 |
| 安装冒烟 | 各平台安装包 | 能装、能起、能更新 |

> **注意：查询 npm 必须容忍传播延迟**（2026-09-14 实测）：刚发布后立即查询可能返回 E404 或旧版本列表 ——
> 这是 **registry / CDN 传播延迟**，不代表发布失败。判据顺序：**先看 CI 的 publish 日志**
> （`+ @dsh-sup/<pkg>@<ver>` 是 npm 的确认），再重试查询（建议 45s 间隔、最多 4 次）。
> 本次发布即因此出现过一次假警报（两个包被误判为漏发，实为传播延迟）。

## 6. 失败处置与回滚
| 情形 | 处置 |
|---|---|
| 构建 / 门禁失败 | 修代码；不需回滚（未发布）|
| 某平台打包失败 | 修该平台；**不要**只发其余平台（安装程序会不全）|
| npm 已发布但需修 | npm 同版本不可重发 → 提 patch 版本 |
| 安装包不可用 | 提新版本；旧安装包不覆盖 |
| 更新通道异常 | 见 `docs/SHELL-UPDATE-CHANNEL-VERIFICATION.md` |

## 7. 禁止事项（红线）

1. 不得在 `.github/workflows/` **之外**放置 workflow YAML（幽灵产线就是这么产生的）；
2. 不得手工改三处版本中的任一处（必须经 `bump-shell.sh`）；
3. 不得缺平台发布（四平台是壳的完整定义）；
4. 不得在非 22.04 基座构建 Linux 产物；
5. 不得绕过 `make-manifest.js` 手写 `shell-manifest.json`；
6. 不得把壳的版本 / 资产逻辑放回内核仓（双仓隔离）。

## 8. 规范自校验（防漂移）

`src-tauri/tests/release_spec_consistency_test.rs` 校验：

| 组 | 校验 |
|---|---|
| R-1 | 规范里的每个入口 / 脚本文件存在 |
| R-2 | `.github/workflows/` 之外**无 workflow YAML**（禁幽灵）|
| R-3 | 规范第 2 节的四平台与 workflow 矩阵**逐项一致**（runner + artifact + bundles + glibc）|
| R-4 | job 名与触发（含 tag 模式）与 workflow 一致 |
| R-5 | 版本三处互锁一致（Cargo.toml = tauri.conf.json = Cargo.lock）|
| R-6 | `verify-shell-versions.js` 确实被 CI 调用 |
| R-7 | 必需章节标题齐备 |
| R-8 | 反向：判据能识别幽灵文件 / 缺失入口（门禁非空转）|
| R-9 | `scripts/` 下无本地构建/发布脚本（硬标准：仅 CI 构建与发布）|

```json shell-release-pipeline
{
  "version": 1,
  "entries": [
    "scripts/bump-shell.sh",
    "scripts/verify-shell-versions.js",
    "shell-release/assemble-shell-pkg.js",
    "shell-release/make-manifest.js",
    "shell-release/version-vectors.json",
    "ci/check-glibc.sh",
    ".github/workflows/build.yml"
  ],
  "ciWorkflow": ".github/workflows/build.yml",
  "ciJobs": [
    "version",
    "build",
    "publish"
  ],
  "tagPattern": "v*",
  "matrix": [
    {
      "os": "ubuntu-22.04",
      "artifact": "linux-x64",
      "bundles": "deb,rpm",
      "glibcMax": "2.35"
    },
    {
      "os": "windows-latest",
      "artifact": "win-x64",
      "bundles": "nsis,msi",
      "glibcMax": ""
    },
    {
      "os": "macos-latest",
      "artifact": "darwin-arm64",
      "bundles": "app,dmg",
      "glibcMax": ""
    },
    {
      "os": "macos-15-intel",
      "artifact": "darwin-x64",
      "bundles": "app,dmg",
      "glibcMax": ""
    }
  ],
  "versionSources": [
    "src-tauri/Cargo.toml",
    "src-tauri/tauri.conf.json",
    "src-tauri/Cargo.lock"
  ],
  "requiredSections": [
    "## 1. 全流程（阶段化）",
    "## 2. 平台矩阵（4 平台，唯一来源 = CI 矩阵）",
    "## 3. 版本单一事实源（三处互锁）",
    "## 4. CI 与放行条件",
    "## 5. 发布后验证（H9）",
    "## 6. 失败处置与回滚",
    "## 7. 禁止事项（红线）",
    "## 8. 规范自校验（防漂移）"
  ],
  "versionGuardScript": "scripts/verify-shell-versions.js",
  "hardStandard": "所有平台构建与发布必须经 GitHub CI 完成；本地不得产生发布产物"
}
```
