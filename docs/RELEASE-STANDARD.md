# 壳仓发布与构建标准（SHELL RELEASE-STANDARD）

> **本文件是壳仓发布/构建的唯一事实源（SSOT）。**
> 由 `src-tauri/tests/release_spec_consistency_test.rs` **机器校验**：
> 本文件写的每个入口、矩阵、job、脚本都必须与仓库现实一致 —— 规范**无法漂移**。

> 与内核的关系：**两套独立流程**（内核 → npm 平台子包；壳 → 安装程序 + npm 壳包）。
> 内核流程见内核仓 `RELEASE-STANDARD.md`；壳侧不重复它的内容。

## 0. 硬标准（2026-09-13，不可协商）

> **所有平台构建与发布必须经 GitHub CI 完成。本地不得产生任何发布产物。**
> **测试一律不得在本机执行；验收只能由推送后的 GitHub CI 裁决。**（与内核仓
> `ACCEPTANCE-STANDARD.md` §0 同源，2026-09-20 补齐到壳侧。）

| 要求 | 壳仓实现 | 门禁 |
|---|---|---|
| 四平台构建只在 CI 内发生 | `build` job 的 4 runner 矩阵，各 runner 只构建自己平台 | R-3 |
| 本地无全平台构建脚本 | 壳仓**本就没有**本地构建/发布脚本（仅 `bump-shell.sh` + `verify-shell-versions.js`）| R-9（新增）|
| 发布只在 CI 内 | `publish` job（tag 触发）| R-4 |
| 无本地发布产物入口 | 壳仓根目录**不存在 `package.json`**，故不可能有本地 release/publish script | R-9 |
| 测试只在 CI 内 | `cargo test` / `cargo build` / 无头冒烟全部是 `build` job 的步骤；本机上限是**纯静态**检查（读文件、`bash -n`、`node --check`）| 第 1 节 H2–H7 的「位置」列 |

**为什么**：本地构建让「产物从哪来」不可复现、不可审计；统一到 CI 后产物可追溯、四平台同构、发布单一入口。
本机跑测试则是另一类代价：它只在**单一平台、单一 glibc、单一 HOME** 下成立，且会在工作区留下
`src-tauri/target/` 与临时产物 —— 结论既不可移植，也不可复现。

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

> **每一行的「位置」就是它唯一合法的发生地。** 除 H0/H1 外全部在 CI 内 —— 与 §0 硬标准同源，
> 不存在「本地也可以跑一遍」的余地：本机的 `cargo build` 只用于复现已知问题，**不构成验收**。

| 阶段 | 名称 | 命令 | 位置 | 必须绿 |
|---|---|---|---|---|
| H0 | 版本提升（三处同步）| `bash scripts/bump-shell.sh <ver>` | 本地（只改三个文件，不产产物）| 是 |
| H1 | 版本一致性 | `node scripts/verify-shell-versions.js` | 本地或 CI（纯静态读文件）| 是（CI 的 `version` job 亦强制）|
| H2 | 门禁测试 | `cargo test --bins` + **自动枚举** `tests/*.rs`（每个文件一个 `--test`；`updater_artifacts` 除外，见 H7）| **仅 CI**（`build` job）| 是 |
| H3 | 无头冒烟 | `cargo build` → `--node-plan`（三平台，必判退出码与结论行）；Linux 追加 `--watchdog` 端到端（伪内核 → 拉起 → `/healthz` 判就绪）；Windows 追加 `--service-plan --service-apply`（真 `schtasks /Create` + `/Query` 回读，随后清理计划任务）| **仅 CI**（`build` job）| 是 |
| H4 | 构建 + 打包 | `npx --yes @tauri-apps/cli@2 build --bundles "<matrix.bundles>"` | **仅 CI**（各 runner 只构建自己平台）| 是 |
| H5 | glibc 基座门禁（仅 Linux）| `bash ci/check-glibc.sh <bin> 2.35` | **仅 CI**（ubuntu-22.04 runner）| 是 |
| H6 | 组装 npm 壳包 | `node shell-release/assemble-shell-pkg.js --platform <p> [--require-sig]`（缺 `.sig` 仅在带 `--require-sig` 时判红；CI 只对 tag 构建传该开关）| **仅 CI** | 是 |
| H7 | 产物验收（同源验签 + 清单契约）| `cargo test --test updater_artifacts` | **仅 CI**，且只在 `refs/tags/v*` 执行（无密钥构建本就产不出 `.sig`）| 是 |
| H8 | 发布（tag `v*`）| `publish` job：归拢产物 → `node shell-release/make-manifest.js` → `npm publish` | **仅 CI** | 是 |
| H9 | 发布后验证 | 见第 5 节 | CI 结论 + registry 查询 | 是 |
| H10 | **安装冒烟**（四平台）| `bash ci/install-smoke.sh <A> <Aver> <B> <Bver> <工作目录>`（Linux/macOS）/ `pwsh ci/install-smoke-win.ps1`（Windows）：装 A -> 读结论 -> 覆盖装 B -> 校验字节真的换了 -> 跑装好的那份二进制 | **仅 CI**（`install-smoke` job，四平台矩阵）| 是（**挡住 `publish`**）|
| H11 | 发布通道冒烟 | `node shell-release/verify-channel.js --ver <ver> [--artifact-dir <dir>]`：从 `tauri.conf.json` 的端点取清单、逐平台验签、比对产物字节 | **仅 CI**（`published-channel-smoke` job，tag 发布成功后 / `workflow_dispatch`）| 是 |

> **H10 为什么独立成 job 而不是 build 里的一步**：build 的四条腿跑的是 `./target/debug/` 下的构建产物，
> 它证明不了「用户装进系统里的那份字节能不能起」。判据必须**拿不到**构建树才算数，
> 所以安装冒烟只能消费 `actions/download-artifact` 与 GitHub Release 里的包（由 `installer_smoke_coverage_test.rs` 的 I-c 钉住）。
>
> **H10 的 A/B 语义**：A = 上一个**已发布**版本的安装包（`gh release download`），B = 本次构建产物。
> 先装 A 再覆盖装 B 就是用户的升级路径；只装 B 只能证明「装得上」，证明不了「升得上」。
> 版本未提升时 A 与 B 同号，此时产物可逐字节相同，故「覆盖后字节必须变」只在两版号不同时判。
>
> **H10 不覆盖**：应用内更新器的下载与应用动作（H7 判签名与清单契约、H11 判通道字节、H10 判装与起）。

## 2. 平台矩阵（4 平台，唯一来源 = CI 矩阵）

| runner | artifact | bundles | glibc 上限 |
|---|---|---|---|
| `ubuntu-22.04` | `linux-x64` | `deb` | `2.35` |
| `windows-latest` | `win-x64` | `nsis,msi` | — |
| `macos-latest` | `darwin-arm64` | `app,dmg` | — |
| `macos-15-intel` | `darwin-x64` | `app,dmg` | — |

约束：

- Linux **必须** ubuntu-22.04 基座（glibc 2.35）—— 在 24.04 构建的产物无法在 22.04 运行；
- **Linux 支持面 = Ubuntu + `deb` 一种形态**：`rpm`（Fedora / openSUSE / RHEL 系）与 AppImage
  都**不在支持面内** —— 不产、不测、不承诺。此前矩阵同时产 `deb,rpm`，但更新清单每平台只有一个
  槽位、放的是 deb，于是 rpm 客户端的自动更新会拿到 deb 包（缺口已登记过）；收口方式就是**不产 rpm**。
  谁要扩支持面（rpm 或 apt/yum 仓库），先改本节矩阵与本条，再按 §4 同步 required context；
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
| `install-smoke` | 总是（4 平台矩阵）| **H10 安装冒烟**：取上一已发布版本的包（A）+ 本次产物（B），装 A -> 覆盖装 B -> 只跑装进系统里的那份字节 |
| `publish` | tag `v*` | 归拢四平台产物 → 生成并校验 `shell-manifest.json` → 发布 npm 壳包（**`needs: install-smoke`**）|
| `published-channel-smoke` | tag 发布成功后 / `workflow_dispatch(ver=…)` | **H11 通道冒烟**：按客户端端点取清单、逐平台验签、比对字节 |

> `pull_request` 触发器于 2026-09-13 补入：此前 PR **完全不跑 CI**，
> 若直接设 required status check 会让 PR **永久等待一个永不出现的状态**。

分支保护（服务器端放行条件）**已启用**（2026-09-21 `PUT` 后 `GET` 读回）：required =
**`version` + 4 条 `build (...)`**，`strict` + `enforce_admins` + `required_conversation_resolution`，
必须走 PR 且**审批数 0**（单人仓设 ≥1 会让「CI 绿后合入」死锁；这条的作用是关掉直推主干），
禁止 force push 与删除分支。`publish` 在 PR 事件上 `skipped`，**不在** required 里。

> **服务器端配置不随仓迁移**：2026-09-19 迁到 `lobbowen` 后本仓 `main` 一度 `404 Branch not protected`
> （2026-09-20 实测），合入约束退化为本地纪律，直到 09-21 重新写入。**换账号 / 迁仓后必须重设**，
> 且任何文档里的保护描述都不算依据 —— 以 `GET /repos/lobbowen/dsh-supervisor-launcher/branches/main/protection` 为准。
> 精确 payload 与逐字 contexts、PUT/GET 的字段结构差异、误删后的恢复调用：
> 见 `RELEASE-AND-BUILD-DECISION.md` 的「把 CI 设为合并门禁」附录。

> 注意 required 的 context **内嵌矩阵参数**（Linux 腿 = `build (ubuntu-22.04, linux-x64, deb, 2.35)`）。
> **改平台矩阵（含 bundles）时必须同步更新分支保护**，否则旧语境永不出现 → 所有 PR 阻塞。

> `install-smoke` 的四条腿语境（`install-smoke (<os>, <artifact>, <pkg_glob>)`）**目前不在** required 列表里：
> 它已在产线上挡住 `publish`（装不上的包发不出去），但「PR 未过安装冒烟不得合入」要等服务端
> `PUT /branches/main/protection` 写入并以 `GET` 读回才算成立。**本文档不声称已启用**，
> 以服务器端读回为准（同上一段「服务器端配置不随仓迁移」的教训）。

## 5. 发布后验证（H9）

| 项 | 位置 | 期望 |
|---|---|---|
| npm 壳包四平台 | `npm view @dsh-sup/shell-<platform>@<ver> version` ×4 | 四者皆等于目标版本 |
| 清单包 | `npm view @dsh-sup/shell-release dist-tags` | 指向新版本 |
| 清单签名钥匙 | 取清单，逐条把 `platforms.*.signature` base64 解出文本，再解其中**第二行**（签名 blob）的第 2..10 字节按小端转 hex | 四条**全等于** `tauri.conf.json` 内置公钥的 key id（H11 已按同一口径自动判，此处是人工复核口径）|
| GitHub Release | tag `v<ver>` | 四平台安装包附件齐备 |
| CI 结论 | tag run | version + 四平台 build + 四平台 install-smoke + publish + published-channel-smoke 全绿 |
| 安装冒烟 | `install-smoke` job（H10，四平台）| 已由 CI 执法：装得上、装好的二进制自报版本、覆盖升级换了字节、Linux 腿起得到 `/healthz` 就绪。**不再是人工项** |
| 更新通道 | `published-channel-smoke` job（H11）| 主端点清单可取、四平台 url+signature 齐、签名对得上配置公钥、（tag 构建）字节与本次产物一致 |

> **清单是嵌套两层编码**：`signature` 字段本身是 base64，解出来是 minisign 的**四行文本**
> （untrusted comment / 签名 blob / trusted comment / global signature），钥匙 id 只在**第二行**那个 blob 里。
> 直接对外层 base64 取第 2..10 字节会解出 `"trusted "` 之类的 ASCII，看着像 hex 却不是钥匙 id；
> 取最后一行则拿到 global signature 的字节，每平台都不同 —— 两种错法都会**假绿**（判不出钥匙错配）。

> **执行位置与判据顺序**（与内核仓 `RELEASE-STANDARD.md` §5 同规）：本机没有 `gh` / `curl`，
> 且 registry 与 CDN 查询都有**传播延迟** —— 刚发布就 `npm view` 可能返回 E404 或旧版本，unpkg /
> jsdelivr 也可能仍在服务旧的文件列表，这**不是发布失败**。判据顺序：**先看 CI 的 `publish` 日志**
> （`+ @dsh-sup/<pkg>@<ver>` 是 npm 自己的确认行），再重试查询（建议 45s 间隔、最多 4 次；
> 2026-09-21 实测 unpkg 侧滞后约 3 分钟，比 registry 更久）。查询需要凭据时在**进程内**读取凭据库里的
> `github-pat`（库在内核仓 `release/scripts/cred.sh path github-pat` 所指位置，本仓不携带凭据工具），
> 不把令牌拼进命令行参数。
>
> **H6-④ 只判客户端真正走的 URL**，别判那些没人走的路径：`tauri.conf.json` 的两条 `endpoints` 都写
> `@latest`，清单里的安装包是 `@<ver>/artifact/<文件>`。（曾按 `@<ver>/shell-manifest.json` 这一格取证，
> 它在发布后约 3 分钟内返回 404、之后自愈 —— unpkg 的同步滞后，不是产物缺失；而这一格**没有任何客户端会取**，
> 所以拿它当判据只会诱导「发布失败」的误判。判发布是否成立按上面的顺序：CI 日志 → registry → 真实 URL。）
>
> **GitHub Release 一行的判据来源**：tag 构建在无密钥时会被 workflow 主动判红
> （`.github/workflows/build.yml` 的 `::error::TAURI_SIGNING_PRIVATE_KEY 未配置`），所以
> 「tag run 绿但 Release 为空」是不可能状态；反过来，**Release 为空即等于密钥丢失**，
> 按 `docs/UPDATER-SIGNING-KEY.md` §〇 处置，不要当成打包失败。

## 6. 失败处置与回滚
| 情形 | 处置 |
|---|---|
| 构建 / 门禁失败 | 修代码；不需回滚（未发布）|
| 某平台打包失败 | 修该平台；**不要**只发其余平台（安装程序会不全）|
| macOS leg 只有 `bundle_dmg.sh` 那一步退出 1（`.app` 已 `Bundling` 成功、其余三平台全绿、日志收尾出现 `Terminate orphan process (diskimages-help)`）| 按**runner 抖动**判：先 `rerun-failed-jobs` 复判同一 commit，再谈改代码。实测 run 35588464417 首跑即此形态，重跑四平台全绿；同一份产线配置在 tag run 35581948978 一次通过并产出 `_x64.dmg`；取样口径 `GET /repos/…/actions/runs?event=push&per_page=20`：最近 20 次 push 运行里**只有本次需要重跑**（`attempt=2` 收口为绿）。**不得**为此给产线加重试循环，也**不得**改成只产 `.app` —— 那是砍能力迁就缺陷；重跑仍红才算确定性缺陷，按根因查 `hdiutil` 侧 |
| npm 已发布但需修 | npm 同版本不可重发 → 提 patch 版本 |
| 安装包不可用 | 提新版本；旧安装包不覆盖 |
| 更新通道异常 | 见 `docs/SHELL-UPDATE-CHANNEL-VERIFICATION.md` |

## 7. 禁止事项（红线）

1. 不得在 `.github/workflows/` **之外**放置 workflow YAML（幽灵产线就是这么产生的）；
2. 不得手工改三处版本中的任一处（必须经 `bump-shell.sh`）；
3. 不得缺平台发布（四平台是壳的完整定义）；
4. 不得在非 22.04 基座构建 Linux 产物；
5. 不得绕过 `make-manifest.js` 手写 `shell-manifest.json`；
6. 不得把壳的版本 / 资产逻辑放回内核仓（双仓隔离）；
7. 不得在本机跑 `cargo build` / `cargo test` / `npx @tauri-apps/cli build` 来「代替」或「抢跑」CI 的结论 ——
   本机跑绿不算绿，本机跑红也不算红（单平台单 glibc 下两类都是假信号，且会留下 `src-tauri/target/`）。

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
| R-10 | CI 矩阵的每个 artifact 都在 `assemble-shell-pkg.js` 的 `PLATFORMS` 里（防「能构建但组装不了」）|

`src-tauri/tests/installer_smoke_coverage_test.rs` 校验安装/通道冒烟**真的存在且真的安装**：

| 组 | 校验 |
|---|---|
| I-a | `install-smoke` job 存在、四平台矩阵齐、`needs: [version, build]`、既有取 A（Release）也有取 B（artifact）的步骤 |
| I-b | 每条腿真的执行平台安装动词（`sudo dpkg -i` / `hdiutil attach` + `cp -R` / NSIS `/S`），并从包管理器侧定位实际落盘二进制 |
| I-c | 该 job 结构上拿不到构建产物（出现 `target/debug` / `cargo build` 即判失败）|
| I-d | `publish` 必须 `needs` 安装冒烟 |
| I-e | `published-channel-smoke` 只在 tag 发布成功或手工触发跑，且校验脚本真的从 `tauri.conf.json` 端点取清单、验 key id、验签、比字节 |
| I-f | 装机判据读的是**结论**：自报版本 + 包管理器版本 + identity.json/shell.log 落盘 + 覆盖后字节变化（按版本分流）|
| I-g | 伪内核夹具单源（workflow 内不得再内联第二份）|
| I-h | 反向：以上判据能识别「只解包不安装」「只下载不验签」的假冒烟形态 |

```json shell-release-pipeline
{
  "version": 1,
  "entries": [
    "scripts/bump-shell.sh",
    "scripts/verify-shell-versions.js",
    "shell-release/assemble-shell-pkg.js",
    "shell-release/make-manifest.js",
    "shell-release/verify-channel.js",
    "shell-release/version-vectors.json",
    "ci/check-glibc.sh",
    "ci/fake-core.js",
    "ci/install-smoke.sh",
    "ci/install-smoke-win.ps1",
    ".github/workflows/build.yml"
  ],
  "ciWorkflow": ".github/workflows/build.yml",
  "ciJobs": [
    "version",
    "build",
    "install-smoke",
    "publish",
    "published-channel-smoke"
  ],
  "tagPattern": "v*",
  "matrix": [
    {
      "os": "ubuntu-22.04",
      "artifact": "linux-x64",
      "bundles": "deb",
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
  "hardStandard": "所有平台构建与发布必须经 GitHub CI 完成；本地不得产生发布产物；测试一律不得在本机执行，验收由 CI 裁决"
}
```
