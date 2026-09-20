# Changelog（桌面壳）

本文件记录桌面壳（`dsh-supervisor-gui`，公开仓 `lobbowen/dsh-supervisor-launcher`）的重要变更。

## [未发布]

## [1.2.0]（2026-09-21）

本版做一件事：**把自更新签名链换到新一把 minisign 钥匙上**，并据此承担一次强制重装。
（旧钥不可得的取证与「恢复 vs 轮换」的裁决过程见下面第一条历史记录，它当时的结论是冻结待议，
本版是该定案的落地。）

### 签名密钥轮换落地：新公钥内置，≤1.1.11 需手动重装一次

- `tauri.conf.json` 的 `plugins.updater.pubkey` 换为新钥（minisign key id `54A15461E39C8AEF`，
  生成于 2026-09-21，口令加密，本机与 `~/.tauri/backup/` 各留一份，布局见 `docs/UPDATER-SIGNING-KEY.md` §四），
  CI 侧 `TAURI_SIGNING_PRIVATE_KEY` / `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` 两枚 secret 已配置并读回确认。
- **承担掉的代价（不是待议风险）**：≤1.1.11 的客户端内置旧公钥，Tauri 强制验签且无降级路径 ⇒
  对这些版本自动更新永久失效，**必须手动重装 1.2.0 一次**；1.2.0 起恢复正常自动更新。
  两套钥匙不能混用，也不存在「先兼容旧客户端」的过渡形态。下载点与重装口径写进 `README.md`。
- **内核不受牵制**：壳装内核走 `npm install -g @dsh-sup/dsh-core-<platform>-<arch>`，完整性由 registry 的
  `integrity` 保障，不经 minisign。
- 旧钥 `96DE3EF26F389F70` 自此不再使用；`@latest` 上若再出现旧钥签名的清单，1.2.0 客户端会验签失败。
- 产线证据（本 PR 的 CI，非本机）：四平台各产出**成对**的 `<安装程序>` + `<安装程序>.sig`，
  且组装 npm 包步骤无 `TAURI_SIGNING_PRIVATE_KEY 未配置` 告警；tag 轮另加 H7 同源验签。

### 发布后验证（H9）实测：1.2.0 确已按新钥出厂

判据全部取自线上（本机不构建、不发布），口径见 `docs/RELEASE-STANDARD.md` §5。

- tag run `35533127291`：`version` + 四平台 `build` + `publish` 六 job 全绿；`publish` 日志内
  npm 自认的确认行齐五条 —— `+ @dsh-sup/shell-{darwin-arm64,darwin-x64,linux-x64,win-x64,release}@1.2.0`。
- H7 同源验签（`产物验收（Tauri 同源验签 + 清单契约）`，即 `cargo test --test updater_artifacts`）
  在**四个平台的 tag 构建里都绿** —— 它用 updater 插件内部同一个 minisign 实现按 `tauri.conf.json`
  的新 pubkey 验，过则证明客户端会接受这份产物（换钥的决定性证据，非本机可得）。
- registry：五个包的 `1.2.0` 均在，`dist-tags.latest` 全部指到 `1.2.0`（含清单包 `shell-release`）。
- 清单（`unpkg` 与 `jsdelivr`、`@1.2.0` 与 `@latest` 四种组合一致）：`version=1.2.0`，
  四条 `platforms.*.signature` 解出的 key id **全为 `54A15461E39C8AEF`**（新钥）；
  同期取 `@1.1.11` 清单复算得 `96DE3EF26F389F70`（旧钥）→ 换钥在产物侧成立，且解法本身可信。
- GitHub Release `v1.2.0`：12 项资产 = 7 个安装形态 + 5 个 `.sig`（deb / rpm / nsis exe / msi /
  mac app.tar.gz 各一份签名），两个 `.dmg` 无 `.sig` —— Tauri 只给它认的更新产物签名，与清单无关。
- **≤1.1.11 客户端的实测预期**：`@latest` 已是新钥清单，旧客户端拉到后验签报错而**不会**装上半个更新，
  这正是换钥公告里承诺的行为；恢复自动更新只能手动装 1.2.0（`README.md` 下载点）。

### 已知缺口（本版未修，如实登记）：Linux 更新清单只有 deb 一个槽位

Linux job 的 `bundles` 是 `deb,rpm` —— 两种形态一起产，但清单每平台只有一个槽位，放的是 **deb**。
后果：用 rpm 装上的客户端在自动更新时拿到的是 deb 包。当前选择是**文档化而非静默**：
`README.md` 明确「Linux 请装 .deb」。真正的修法（rpm 独立槽位 / 停发 rpm / 按包形态分流）属更新通道设计，
待定案，不在本版顺手改。

### 第二个已知缺口（本版未修，如实登记）：jsdelivr 屏蔽 `.exe`，Windows 只剩单一 CDN

1.2.0 出厂后按端点分平台复测（`tauri.conf.json` 的 `plugins.updater.endpoints` 是 unpkg → jsdelivr 两条）：

| 端点 | 清单 | deb | app.tar.gz | win `.exe` |
|---|---|---|---|---|
| unpkg | 200 | 200 | 200 | 200 |
| jsdelivr | 206 | 206 | 206 | **403 Forbidden** |

`.exe` 的 403 在 `1.1.11` 上同样复现，故与本版无关，是 jsdelivr 侧对可执行文件的处置；
同包的 `manifest-entry.json` 与 `.exe.sig` 在 jsdelivr 都取到 200，说明不是整包缺失。

后果：**Windows 客户端实际只有 unpkg 一条路** —— 主端点排在前面所以正常更新不受影响，
但清单里承诺的「多 CDN 回退」对 win 不成立，unpkg 故障时 win 用户会停在旧版。
修法（加第三方镜像 / 自建镜像 / 换 win 产物形态）属更新通道设计，与上一条同批待定案，
本版只把「两 CDN 皆可直取」的错误概括改掉，不动 `endpoints`。

### 签名密钥的现状纠正：已签名产物一直在产线，换钥不是无害

`docs/UPDATER-SIGNING-KEY.md` §〇 原写「壳仓从未产出过签名产物，也从未发布过 GitHub Release」。
后半句真、前半句**假**，而且危害直接：读的人会判定「既然从没签过，换把钥匙无所谓」，而 §四/§六
自己写的正是「换钥 = 已安装用户永久收不到更新」。实测（2026-09-21，取 npm 上的线上清单逐条解码）：

- 更新通道是 **npm + CDN**，不是 GitHub Release：清单 `@dsh-sup/shell-release@latest` 该轮实测指
  `1.1.11`（`pub_date` 2026-09-18；现值以 `npm view @dsh-sup/shell-release dist-tags` 为准），
  四平台各带一份 minisign 签名；安装程序本身在 `@dsh-sup/shell-<platform>@<ver>/artifact/…`，
  **unpkg 可直取**（win setup 3.27 MB、darwin app.tar.gz 3.93 MB 均 HTTP 200）。故 `README.md`
  原写「安装包只在 CI artifacts 里，没有面向用户的下载点」也一并纠正。
  > 该轮把结论写成「unpkg / jsdelivr 均可直取」，是**没逐端点分平台取证**的过度概括：1.2.0 出厂后
  > 复测，jsdelivr 对 `.exe` 一律 403（`1.1.11` 与 `1.2.0` 同），见下方「第二个已知缺口」。
- 抽验 `1.0.1 / 1.0.5 / 1.1.0 / 1.1.5 / 1.1.9 / 1.1.10 / 1.1.11` 七个版本共 28 条签名，
  key id **全部** 是 `96DE3EF26F389F70`，与 `tauri.conf.json` 内置公钥一致 → 全体存量客户端只认这一把钥匙。
- **旧**私钥四处不可得（该轮取证的时点结论；本机现有 `~/.tauri/` 是新钥，见上方轮换条目）：
  当时本机无 `~/.tauri/`、`find -maxdepth 6` 无 `*.key`、git 历史从未入库
  （`-S"minisign secret key"` 无命中）、`lobbowen` 两仓 secrets 只有 `NPM_TOKEN`。
  唯一可能残存处是迁仓前的 `wasi7mglns/dsh-supervisor-launcher` Actions secret（1.1.11 在那里签出），
  而现 PAT 取其 `/actions/secrets` 返回 403（无 admin），连存在性都无法由 agent 核实。

据此把 §〇 重写为「事实 + 证据」两列，并登记**当时的**冻结规则与恢复动作排序：找回旧私钥前不得生成新钥、
不得改 `pubkey`、不得向 `@dsh-sup/shell-release` 发新版本（新钥清单会让存量客户端验签报错，
不是「拿不到更新」这么轻）。同时明确一条边界以免误判影响面：**内核自更新不经 minisign**
（`src-tauri/src/core.rs` 走 `npm install -g @dsh-sup/dsh-core-*`，完整性由 registry `integrity` 保障），
壳冻结期间内核照常滚动。该冻结已由用户 2026-09-21 定案解除：选轮换、发 1.2.0、承担存量手动重装，
见本节开头的「签名密钥轮换落地」条目。

### 分支保护落成服务端事实（2026-09-21）

用户定案「分支保护必须做」。此前 `main` 自 2026-09-19 迁仓后一直是 `404 Branch not protected`
（服务端配置不随仓迁移），合入约束只剩本地纪律。现已 `PUT` 写入并 `GET` 读回：

- required = `version` + 4 条 `build (...)`（逐字含矩阵参数）；`publish` 在 PR 上 `skipped`，故**不在**
  required 里 —— 一旦写入永不出现的语境，所有 PR 会永久阻塞。
- `strict` / `enforce_admins` / `required_conversation_resolution` 开启，必须走 PR 且**审批数 0**
  （作用是关掉不经 PR 的直推；单人仓设 ≥1 会让「CI 绿后合入」死锁），禁 force push 与删除分支。
- 本文档侧纠正：`docs/RELEASE-STANDARD.md` §4 由「设计为 / 当前未生效」改为现值；
  `docs/RELEASE-AND-BUILD-DECISION.md` 附录的 payload 把 `required_pull_request_reviews: null`
  （＝允许直推主干，与「CI 是唯一放行裁决者」矛盾）改为存在但审批数 0，并补 PUT 后必须 GET 读回的
  两处字段结构差异；`README.md` 的「Releases 为空」原因收敛为**唯一阻塞项 = 签名私钥未配置**
  （已核 `build.yml`：tag 构建缺 `TAURI_SIGNING_PRIVATE_KEY` 即报错退出，非 tag 不阻断）。
- 维护规则登记：**改 `build.yml` 平台矩阵必须在同一次变更里同步 required contexts**。

### 文档纠正：以现在时态写着的假现状（第 3 轮残留清扫）

等 CI 的窗口里把两仓又扫了一遍。判据只有一条：**这句话会不会让人去做一件仓库里不存在的事**。
下列每一条都先用代码/grep 核实过，再改；未核实的外报一律不采纳。

**权限模型的残留（危害最大）**

- `docs/DESIGN-BOUNDARY.md` D2 把「壳装内核**需提权**（`pkexec`/`osascript`/`msiexec`），
  提权需要人在场所以内核永远做不到」当作两条安装链不可合并的**理由**。2026-09-18 的权限模型重写
  之后这条理由已不存在：全仓的提权消费者只剩**壳自更新**（`has_privilege_channel` 只喂
  `privilege_channel` 一个状态字段，Node 安装由门禁 `a2_user_scope_install_needs_no_privilege`
  反向钉住不含 `pkexec`/`sudo`，内核 npm 安装落在只读前缀时只回传证据不擅自换前缀）。
  理由改为真正的两条：**装的对象不同** + **内核必须无人值守**（R2）。
- `src-tauri/src/platform/linux.rs` 的 `PRIVILEGE_COMMANDS` 头注仍写着「`install_node()` 用它选
  **实际执行**的命令」—— 与它自己文件里的门禁直接矛盾；同文件把安装超时注解为「下载 + 解包 + 系统授权」。
  两处都改为只描述壳自更新通道。`main.rs` 的 shell.log 注释也还写着旧前缀 `~/.dsh/shell/`。

**「本机可以跑」的错误引导**

- `docs/RELEASE-STANDARD.md` §0 硬标准只覆盖了**构建与发布**，没覆盖**测试**，于是别处继续出现
  本地跑测试的指令。现补入「测试一律不得在本机执行；验收只能由 CI 裁决」（与内核仓
  `ACCEPTANCE-STANDARD.md` §0 同源），§1 全流程表加「位置」列（H2–H7 全部标 **仅 CI**），
  §7 红线加第 7 条。原先那行「本地可做 H0–H5 / H7」正是与 §0 互相打脸的入口，已删。
- 三处具体的本地执行指令按新口径收口：`RELEASE-AND-BUILD-DECISION.md` §2.2 的 `cargo test` +
  `cargo check --all-targets` → 上限为 `bash -n` / `node --check` / `cargo fmt --check`；
  `DEVELOPMENT-TRACK.md` 铁律 R-1 的「允许：跑本仓测试」→ 移到禁止列；
  `UPDATER-SIGNING-KEY.md` §五 用 `export` 私钥 + 本机 `npx @tauri-apps/cli@2 build` 验证密钥
  → 私钥只进 CI secret、由**非 tag** 的 CI 构建产出成对 `.sig` 来证明，并明确这不等于 H7 同源验签。
- `DESKTOP-ACCEPTANCE.md` §1 不再给任何本机产包命令（含此前留的 `cargo build --release`），
  §5 构建矩阵改为只指向 `RELEASE-STANDARD.md` §2 的唯一事实源。

### 文档纠正：README 仍在本机跑构建与测试（第 4 轮）

第 3 轮把「本机可以跑」这一类收口时漏了根 `README.md` —— 它不在任何门禁的读取范围内
（壳侧门禁读 `docs/RELEASE-STANDARD.md`、`docs/DEVELOPMENT-TRACK.md` 等，不读 README），
于是它继续以命令块教操作者本机执行：

- 「想从源码跑起来」下的 `cd src-tauri && cargo build --release`；
- 「开发 / 贡献」下的 `cargo build` + `./target/debug/dsh-supervisor-gui --node-plan`
  （H3 无头冒烟在 §1 全流程表里明确标「仅 CI」，本机跑它没有验收意义）；
- 版本校验代码块里的 `cargo test`。

三处删净，改为说明构建 / 无头冒烟 / 全部测试都是 CI `build` job 的步骤、本机上限只有
`bash -n` / `node --check` / `cargo fmt --check`，安装包取自 CI run 的 artifacts。
`bump-shell.sh` 与 `verify-shell-versions.js` 留在本机（只改文件 / 只读比对，不构建）。
`DEVELOPMENT-TRACK.md` §2 的「壳改动必须过 `cargo test --bins --tests`」补明由 CI 执行。

**登记缺口**：`README.md` 没有任何门禁读取，这类漂移只能靠人工清扫；本轮不新增文本匹配门禁
（对「提到 `cargo test`」的正文做正则区分「禁止语境 / 指令语境」必然误伤），
待有真实判据再收口。

**契约路径与不存在的机制**

- 产品状态根迁移后，`~/.dsh/supervisor/*` 与 `~/.dsh/shell/*` 作为**现行读写路径**残留在
  `KERNEL-LAUNCH-STANDARD.md`（P0/runtime.json、§3/core.json）、`DESIGN-BOUNDARY.md`（D1、§4.1、§4.3）、
  `DESIGN-SHELL-ARCHITECTURE.md`（§3.2 契约层）、`DESIGN-COMPLETE.md`（§6 契约面、§18 契约层）
  —— 共 10+ 处。统一改为 `<状态根>/…`，并在 `DESIGN-BOUNDARY.md` §四 与 `DESIGN-COMPLETE.md` §6
  各立一处**路径记法**定义（含 `DSH_SUPERVISOR_HOME` 与三平台默认、`migrate_legacy` 只是迁移源）。
- `KERNEL-LAUNCH-STANDARD.md` §4 的失败分类表列了 `INSTALL_FAILED`（P1）与 `SERVICE_DEFINE_FAILED`（P4）
  两个**代码里从未产出过**的 code（`domain/guardctl.rs` 实际只有 5 个）。删掉，并写明这两个位置
  **刻意没有独立 code**：P1 失败由 `core_apply` 回传 npm 原始输出；P4 定义失败**不阻断启动**，
  只记一条日志继续走 P5，真起不来时由 `SERVICE_START_FAILED` 连服务管理器错误原文一起报出。
- `DESIGN-SHELL-ARCHITECTURE.md` §3.2 的契约层树画的是 `domain/contract/{schema.rs,identity.rs}` ——
  该目录与这两个文件都不存在（`identity.rs` 尤其误导：壳身份写在 `update.rs`）。改为四个真实模块，
  并补上树里完全缺失的 `core_contract.rs`。§2.2 的 `trait Platform` 代码块按 `// platform/mod.rs`
  的名义展示了 `path_dirs` / `known_node_locations` / `InstallReport` / `PlatformCapabilities`
  —— 四个名字全仓 0 命中，且 `install_node` 的返回类型写错。改为当前 17 个方法的真实面。
  C4 不变量仍写「现仅在 `latest_lts()` 成功时导出部分内容」，该缺陷早已由 `mirror::export_on_boot`
  无条件导出修掉，按现状改写。
- `DESIGN-COMPLETE.md` §27.2 给出的内核侧落地代码（`dist/index.js::_registryOrigins` 调
  `_catalogFromContract`）**从未存在**，`dist/index.js` 也没有这个文件。改为指向真实落点
  （`platform/contract/registry.js` 校验 + `platform/distribution/registry.js` 逐级回退 +
  `policies.js::FALLBACK_REGISTRIES` 两条兜底），并确认 `REGISTRY_PRESETS` 确已全仓 0 引用。

**数字漂移**

- `DESIGN-COMPLETE.md` §7 的 IPC 表标题写着「全部经 `main.rs` 的 20 个 `#[tauri::command]`」——
  门禁 G3 恰恰要求 `main.rs` 里命令数为 **0**（实测 0），23 个命令全在 `commands/mod.rs`。
  表的「行 / 体量」两列同批漂移（`shell_identity` 记 960 行，实际该文件共 813 行）。
  整表改为按职责分组、不记行号，并指出权威清单 = `#[tauri::command]` 与 `generate_handler!` 两处。
- 同文 §8「工程现状」的七行审计快照里有四条判断已被后续改造**反转**（壳零平台层 / `main.rs` 1487 行
  单体 / Error 枚举 0 / 前端 760 行单块）。改为按当前代码复测 + 标出各自由哪条门禁锁定，
  并留下两条**仍然真实**的缺口：`Result<_, String>` 51 处残留、`main.rs` 538 行未达 <150 目标。
  `DESIGN-BOUNDARY.md` §二 标题与 D6 的「内核 80 文件 / 19354 行」「待统一 420 行副本」标注为审计时快照，
  D6 那行按已落地形态（契约优先 + 最小兜底）改写。
- 无头入口数「4 个」其实列了 5 个，实际 `main.rs` 分派 8 个自检/计划入口 + `--run-guard`；
  同时如实记下 **CI 目前只冒烟 `--node-plan`（且 `|| true`）**，其余入口有产出无断言。
- `RELEASE-STANDARD.md` §8 门禁表停在 R-8/R-9，而 `release_spec_consistency_test.rs` 已有 **R-10**
  （CI 矩阵 artifact 必须被 assembler 的 `PLATFORMS` 覆盖）。补登记。
- `SHELL-UPDATE-CHANNEL-VERIFICATION.md` 的 §六/§八 是**建议**，被后人读成**已具备**：N3 的
  「内核预取 + 缓存到 `~/.dsh/shell/cache/`」从未落地，且与当前「无预取、无缓存、无隐式回退，
  账本只有 `pending -> confirmed`」的实况相反。加读前必看的三项裁决表（N1/N2 已采纳、N3 未采纳），
  并在 §六 那段就地标注。§一 的网速实测标注为当时出口取样、非当前结论。
- `DESKTOP-ACCEPTANCE.md` §3 与 §6 对同一件事**各写了一套矛盾说法**：§6 声明「已实现：引导页显示
  『升级到官方最新 LTS』+『跳过并使用现有版本』」，§3 却说这是已知缺口。前端 grep 不到这两个文案，
  也没有对应分支 —— 引导页判定只看 `minNode`（v22.12，经 `commands/mod.rs` 的 `minOk`/`minRequired`
  回传），达标就不弹。**「已实现」那一侧是假的**，删除并把两节统一到同一个事实：
  升级入口只在「缺失或低于 `minNode`」时出现，达标但低于最新 LTS 没有入口。

**本批不含发布动作**：不打 tag、不 `npm publish`；合入条件仍是 CI 全绿。

### 文档纠正（续）：把「做不到 / 没在做」写成「已具备」的残留

- `docs/RELEASE-AND-BUILD-DECISION.md` 把分支保护未设的原因写成「自动化令牌属另一账号、无该仓
  admin」。该前提已不成立：现用 PAT 属 `lobbowen` 本人且对两仓带 `Administration: Read and write`，
  **API 已具备设置能力**；保持未设是因为恢复保护属共享状态变更、待单独定案。执行片段同时改为
  在进程内读凭据（不把令牌拼进命令行参数），并注明无命令行时的等价 UI 路径。
- 同文件「与内核仓的差异」称内核 `build` 是条件 job 故不可设 required —— 内核仓自 2026-09-14 起
  已移除 job 级 `if:`，两仓的 `build` 现在都是无条件矩阵，口径按此更正。
- `docs/DESIGN-SHELL-ARCHITECTURE.md` §八 / `docs/DESIGN-COMPLETE.md` §22 的「验收标准」在两处
  被 README 与 docs 索引列为**现行权威**，却含三条与仓库现实不符的条目：CI **从未**调用
  `--platform-matrix`（四平台 job 里都没有该步骤）、`main.rs` 实测 538 行（两份文档分别写 ≤200 与
  ≤150，且**无任何门禁执行这两个数**）、前端为 `bootstrap/js/00..80` **九个**模块而非八个。
  现逐条标注达成度并声明其为未完成清单。
- `docs/DESKTOP-ACCEPTANCE.md` 前置行称「Linux 可本机验证；mac/win 需对应平台构建」，与 §0 硬标准
  （所有平台构建经 CI）冲突；改为验收对象是 CI 产出的安装包，本机 `cargo build` 只用于复现问题。
- `docs/RELEASE-AND-BUILD-DECISION.md` §2.2「本地自证（发布前）」降级为**可选自查、不构成验收依据**，
  并登记共享工作树场景下不得在本机跑测试套件的口径（上限为静态检查）。

### 文档纠正：删掉会被当成当前能力的历史状态

两处文档在用现在时态描述已不存在的东西，读的人据此安排工作就会跑偏：

- `docs/RELEASE-STANDARD.md` §4 把「`version` + 4 条 `build (...)`，strict + enforce_admins」
  写成服务器端既有事实。实测 `/branches/main/protection` 返回 404 `Branch not protected`
  —— 迁仓后从未恢复，PR 目前可不经 CI 直接合入。改为「设计为 …」并显式标注当前未生效。
- `docs/UPDATER-SIGNING-KEY.md` §四/§五 仍把 `~/.tauri/` 下的私钥、备份与「已完成的恢复演练」
  列为在用状态。本机该目录已不存在（见 §〇）。改为设计布局 + 记录口径，验证命令加前置条件。

### 修复：装了 npm 却看不见 npm —— 工具链版本贯穿契约 / 事件 / 文案

用户实测：环境检测与安装全流程走通后，界面上没有任何 npm 的痕迹。npm 确实被安装并校验过
（1.1.10 起的工具链契约），但它的**版本**在源头就没有落脚点：`runtime.json` 只记 npm 路径，
安装管线返回 `(node_path, version)` 元组，于是完成事件 `install_done { kind: "npm", version }`
只能填 node 的版本号 —— 引导页念出的「npm 已就绪（v22.x）」从来不是 npm 的版本。

- 运行期契约（schema 2）新增 `npm.version`：只在真实执行过 `npm --version` 时写入，未执行为
  null，`derive`（只解析路径，服务启动路径）与 `derive_usable`（执行过）因此可区分。
  写与读改为共用同一处键映射（`meta` / `from_meta`），加字段不再会漏一侧。
- 安装管线（`run_install` / `node::finalize_install` / `reinstall_for_npm`）返回**契约本身**，
  不再返回字段子集。顺带修三处同源缺陷：重装后仍上报重装前的 node 路径、node 版本可能以空串
  写进契约、一次收尾把 npm 探测执行两遍。
- `install_done` 按 kind 各发一条，version 各归各的；npm 版本未回读时发 null，怎么念由 UI 决定。
- 引导层把 `NS.nodeVer` / `NS.npmVer` 两个散装字段收成 `NS.toolchain` 快照：唯一写入点
  `applyToolchain`、唯一读取口 `readEnv`（三个轮询点各写一遍的 15s 超时预算与文案一并收口）。
  「环境就绪 · Node x · npm y」与诊断串同时含 node 与 npm，缺失如实说「版本未回读」。
- 门禁：新增 G-7（按 `handle.emit(...)` 调用切块对账 kind 与 version 归属）、G-8（快照只有一个
  所有者与一个读取口），各带旧形态反向夹具；G-2 改为只按**函数体的代码行**取证，注释不再能
  让门禁转绿。SSOT `docs/ENV-TOOLCHAIN-INSTALL-STANDARD.md` §1/§2.1/§2.2/§2.4/§3.1/§3.2/§5 同步
  （新增不变量 T-1b/T-1c/T-7b/T-8/T-9），并纠正两处错误引导：`npmOk === false` 应为 `!== true`、
  `node_status` 契约漏记 `npmVersion`。
- 删除 `10-ui.js` 中重复声明的 `wait` / `hideFail`（合并残留；行为不变，但会让人读错出口）。

### 修复：产线承诺与执行不符 —— 无签名密钥的构建被误判为代码红

`build.yml` 的打包步骤注释写着「未配置 secret 时为空，不阻断」，但这句话没有实现：secret 缺失时
Actions 把变量展开成空字符串继续导出，Tauri v2 CLI 拿空串去解码并报
`failed to decode secret key: ... Missing comment in secret key`；紧接着组装步骤对缺 `.sig` 无条件
`exit 1`，验签验收步骤也无条件跑。结果是**任何没有签名密钥的构建必然全红**（mac/win/linux 四平台
一起红），而这红与当次代码改动无关，等价于完整构建矩阵不可用。

- 打包步骤：key 缺失分支内 `unset TAURI_SIGNING_PRIVATE_KEY TAURI_SIGNING_PRIVATE_KEY_PASSWORD`，
  并在同一分支关掉 updater 产物（`--config` 覆盖 `bundle.createUpdaterArtifacts=false`）。
  两步都是必需的：配置里内置了 pubkey，Tauri 见「有公钥无私钥」会直接失败
  （`A public key has been found, but no private key`），只撤变量仍红。
  tag 构建仍先拦后报（缺密钥不可发布）。
- 组装步骤：`--require-sig` 仅在 `refs/tags/v*` 上传入；`assemble-shell-pkg.js` 的缺 `.sig` 早失败
  挂到该开关上（发布路径的强校验一字不放宽），并修 `parseArgs` 使末尾布尔开关不被当成取值
  （旧实现会让 `--require-sig` 得到 `undefined` 且其后参数整体错位）。
  macOS 在关闭 updater 产物时不产 `.app.tar.gz`，组装器加「主形态零命中才用 `.dmg`」的备用形态；
  tag 构建主形态必命中，产物集合与既往一致。
- 验签验收步骤加步骤级 tag `if:`；**不给** `build` job 加 job 级 `if:`（那会违反 C-c/C-d）。
- 门禁：新增 C-f（无密钥构建不阻断的三处语义）、C-g（组装器保留发布强校验 + 布尔开关解析）、
  C-h（三条判据的旧形态反向夹具），全部为纯函数判据。
- 文档：`docs/UPDATER-SIGNING-KEY.md` 纠正把同一报错归因成「漏填密码」的错误引导，并如实登记
  **该轮时点**的状态（当时无私钥、无 GitHub Release、公钥指纹可核而私钥指纹不可核）。

## [1.1.11]（2026-09-18）

### 修复：跨平台权限模型重写 —— Node 用户级安装（零权限）

用户实测 Windows：msiexec 退出码 1619（安装包无法打开）。这不是 UAC 取消，而是系统级
安装本身的缺陷：UAC 提升到管理员账户后常读不到当前用户 profile 下的 .msi；且
canonicalize() 在 Windows 返回 \\?\ 前缀路径，msiexec 不认。macOS .pkg 需要系统授权且无
arm64 pkg；Linux /usr/local 需要 pkexec/sudo（容器/WSL/SSH 常不可用）且 tar -xJf 依赖 xz。

- **三平台统一为用户级安装（零权限）**：制品 Windows win-{arch}.zip / macOS
  darwin-{arch}.tar.gz / Linux linux-{arch}.tar.gz，解包到 <状态根>/node，经
  platform::commit_user_node 原子落定；node_bin_after_install 与 node_candidate_paths
  指向该落点；运行期契约记录绝对路径，服务管理器无需 PATH 里有 node。
- 提权从此只与「壳自更新（替换安装程序）」有关，由各平台自身通道完成。

## [1.1.10]（2026-09-18）

### 修复：环境工具链标准化（Node 安装 / npm 可用性 / 镜像网络）

- **全部 Node 镜像均不可用（用户实测）**：ureq 默认既不读环境变量也不读系统代理
  → 只有代理的机器上全部镜像直连失败。新增进程级 mirror::agent：环境变量
  （ALL_PROXY/HTTPS_PROXY/HTTP_PROXY）优先，缺失时回退**系统代理**
  （Windows WinINET 注册表 / macOS scutil --proxy）；镜像探测与 Node 下载共用同一 agent。
- probe_all 过去把失败原因丢弃（只报「不可达」）→ 新增 Probe.error，逐源带出
  HTTP/DNS/TLS/代理/读体原因；PROBE_TIMEOUT 8s→20s（index.json 单个 1.5~2MB）。
- **npm 只查文件存在、从不执行**：新增 runtime_contract::probe_npm_usable，真实执行
  npm --version（Windows .cmd 经 cmd /C）；node_status.npmOk 改三态
  （true/false/null=未知），新增 npmVersion；前端只有 npmOk === true 才放行。
- 删除 record_runtime_meta：与 runtime_contract::write 双写同一 runtime.json，
  覆盖掉 npmPath/npmArgs/schema。
- run_install：安装后先作废探测缓存，并校验**安装器返回的路径**（而非可能记录旧 Node 的
  PATH）→ 修「安装后版本不一致」永不收敛；断言最低门槛。Windows install_node 对 /i
  路径加引号、用 -PassThru 取真实 ExitCode、核对 node.exe 是否就位；macOS 同补结果核对。

## [1.1.9]（2026-09-18）

### 修复

- **退出管家后桌面壳被自动重新拉起（严重，Windows 主根因）**：`platform/windows.rs stop()`
  对 `DSH-Supervisor-Watchdog` 计划任务改用 `schtasks /Delete /F` —— 原 `/End` 只结束本次运行实例、
  不禁用 `/SC MINUTE /MO 5` 计划，导致 ≤5 分钟后 `watchdog.ps1` 把守卫与桌面壳拉回；
  下次 `ensure_defined` 幂等重建看护任务。
- `domain/guardctl.rs ensure_guard`：在「守卫已在运行」的提前返回分支补一次 Windows-only 幂等
  `ensure_defined`（防登录任务先拉起守卫 → 看护任务永不重建 → 本会话 GUI 崩溃自愈失效）。
- `guardctl::shutdown_all`：停止守卫失败除 stderr 外**落盘 `shell.log`**（GUI 下 stderr 常丢）。

详见 `docs/audit/2026-09-18/_s3-exit-relaunch.md`（含跨仓完整清单）。

## [1.1.8]（2026-09-18）

### 新增：内核选版遵循发布通道契约（RELEASE-CHANNEL-CONTRACT §3）

- 新增 `src-tauri/src/release_channel.rs`：契约 §3 冻结算法的唯一实现 ——
  `rollback` > （灰度机）`canary` > `latest` > `versions` 兜底 > `Err`；§5 灰度名单只认
  `schema:1 + entries[].installId`（主）/ `hostnames[]`（兜底），`note` 不参与匹配；旧 7 形状显式拒绝。
- `core.rs` 的 `latest_pick`/`LatestPick`/`build_plan` 接入：**回退（rollback）目标低于当前版本时仍判为需动手**（RC-2），
  升级与回退共用同一写入路径（执行体不做版本比较）。
- **行为变更**：`.github/workflows/build.yml` 发布 tag 由 `-RC.n` 改指 `latest`（与内核选版契约一致）。

### 修复：工具链 npm 与 node 同权

- `node.rs`/`main.rs`：node 安装成功后**必须校验 npm**；两条通道都无则幂等重装再复探；
  仍失败带手动安装指引，绝不静默（对齐 `docs/ENV-TOOLCHAIN-INSTALL-STANDARD.md`）。
- 新增 `docs/ENV-TOOLCHAIN-INSTALL-STANDARD.md`、`src-tauri/tests/env_toolchain_standard_test.rs`。

### 修复：门禁与稳定性（2026-09-18 审计）

- `tests/dev_runtime_safety_test.rs`：修扫描根（此前以 `src-tauri/` 为基准 → 扫描 0 文件、门禁空转），
  并加 `assert!(scanned > 0)`。
- `bootstrap/js/40-shell-update.js`：修「不支持自更新」死分支（后端真值为 `selfUpdateCapable`，
  deb/rpm 无提权通道时此前会强更失败）。
- `commands/mod.rs`：`core_apply` 加 **17 分钟总预算**，耗尽即停并如实回报已尝试源数（与前端预算对齐）。
- CI：**tag 构建缺签名密钥由 warning 升为 error**（Tauri 自动更新强制验签）。

### 文档

- 内核仓移交：`DESKTOP-ACCEPTANCE.md`、`UPDATER-SIGNING-KEY.md`；新增 `DEVELOPMENT-TRACK.md`。
- 新增审计台账 `docs/audit/2026-09-18/`（5 分片报告 + 索引：P0/P1/P2 分级与本版「已修/已登记」去向）。

## [1.1.7]（2026-09-16）

### 修复：工具链契约补齐 npm（干净 Windows 上「node 在、npm 缺」）

真机：干净 Windows 上壳判「环境就绪」，却用**不存在的 npm** 去安装内核，必失败。

根因：环境契约是**单工具且可伪造**的 —— `runtime_contract::derive()` 恒拼
`nodeBinDir/npm[.cmd]`（不查存在性），`ensure()` 只校验 `node.is_file()`。
「环境就绪」＝「node.exe 存在」，npm 完全不在判据内。

修法（**工具链契约**：必需工具集必须逐项真实存在，缺则修复）：

- `derive()` → `probe_npm()`：按 `npm`/`npm.cmd`/`npm.exe` → 包内
  `node_modules/npm/bin/npm-cli.js` 顺序解析；找不到返回 None（**绝不伪造**）；
- npm 仅包内 JS 时以 `(node, [npm-cli.js])` 表达：契约新增 `npmArgs`，`core.rs` 调用带前缀；
- `ensure()` 要求 **node 与 npm 都存在**才算就绪；
- `node_status` 回传 `npmOk`/`npmPath`；引导页（`20-env.js`）在 `npmOk === false` 时
  自动重跑官方 Node 安装（官方分发包自带 npm）补全工具链；
- 内核侧 `env-catalog` 的 npm 探测改为**契约优先**（Windows 裸 `npm` 是 ENOENT）。
- 门禁：`runtime_contract::toolchain_tests`（缺失即 None / 包内 JS / 垫片优先）+
  `guard_resolution_test::toolchain_gate_requires_npm`。

## [1.1.6]（2026-09-15）

### 架构修正：服务定义只指向**稳定入口** `<壳> --run-guard`，node/守卫改为运行时检测

真机证据（1.1.5）：PowerShell 包装脚本确实执行了，但 `node` 拿到的是 npm 的 `.cmd` 垫片
（且带 `\\?\` verbatim 前缀）→ `Error: EISDIR: lstat 'C:'`。日志还混了三段编码（GBK/UTF-8/UTF-16）。

根因不是「哪个路径写错」，而是**把运行时检测结果固化进每台机器现场生成的脚本**：
node 一迁移（nvm/fnm/volta）即失效；Windows 还要现场拼 `.cmd/.ps1`，被码页/前缀/垫片轮番咬。
门禁此前全是 `str::contains` 源码文本断言，观测不到运行时行为，故 CI 全绿而真机失败。

修法（对齐内核看护脚本，但更进一步 —— 连脚本也不再需要）：

- 新增无头入口 `dsh-supervisor-gui --run-guard`：每次启动重新检测 node（运行期契约/探针）
  与守卫（core.json + 候选扫描，取本地最高版本），随即 `exec`。**不触网**，离线可启动；
- 三平台服务定义统一为 `<壳> --run-guard`（`LaunchSpec::service_command()` +
  `service_exec_line()` 单源组装）：Systemd `ExecStart`、launchd `ProgramArguments`、
  schtasks `/TR`。**定义中不再出现任何 node/guard 路径**；
- **删除** Windows 的 `guard-task.cmd/.ps1`、`win_path_expr`、`guard_argv` 及整个包装脚本机制；
  看护任务也改为 `Start-Process <壳> --run-guard`；
- `spawn_daemon` 收敛为 `ServiceControl` 的**同一默认实现**（三平台不再各写一份）；
- `domain/coreloc.rs` 单点归一化：剥 `\\?\` verbatim 前缀、`.cmd` 垫片 → 包内真实 JS 入口；
- 门禁升级为**行为/结构**：`coreloc` 内置单测直接驱动路径规范化与版本仲裁（假 npm prefix、
  假 node/guard）；L-1/K-4/K-10/B56 改为「定义不含 node/guard 路径、只指向 --run-guard」；
  新增 `guard_resolution_test.rs` 结构不变量。

## [1.1.5]（2026-09-15）

### 修复：Windows 守卫包装脚本弃用 `.cmd`，改用 PowerShell（UTF-8 BOM）

真机证据（1.1.4）：包装脚本**确实被执行**（`guard-task.log` 有 `start`），但紧接着
`系统找不到指定的路径。` 且 `exit 3` —— `cmd` 只在「命令行里某个**目录**不存在」时给 3
（找不到命令是 9009）。也就是说：日志能写，但 node/guard 命令行的路径被判为不存在。

两类根因同源（用 `cmd` 读文件 + 依赖任务环境变量）：

1. **码页**：`.cmd` **正文**由 `cmd` 按 OEM 码页解码，Rust 写出的 UTF-8 中若含非 ASCII
   用户名（中文），路径即乱码；
2. **任务环境**：正文里的 `%APPDATA%`/`%LOCALAPPDATA%` 在计划任务环境可能未展开，留下
   `%APPDATA%\...` 字面量 → 目录不存在。

修法（对照内核看护脚本 `watchdog.ps1`，同一稳定形态）：

- 包装脚本改为 **`guard-task.ps1`**，`schtasks /TR` = `powershell -NoProfile -NonInteractive
  -ExecutionPolicy Bypass -File "<ps1>"`（`-File` 参数自带引号，用户名含空格也不截断）；
- 脚本以 **UTF-8 BOM** 落盘：Windows PowerShell 5.1 对无 BOM 的脚本按 ANSI 解码，中文路径会再次乱码；
- 所有路径经 `ps_quote` 以**单引号字面量**写入，不插值、不依赖任何环境变量；
- 脚本内 `Test-Path` 分别记录 `NODE MISSING` / `GUARD MISSING`，并回显实际 node/guard 路径与
  `$LASTEXITCODE` —— 再失败也能一眼定位到是哪条路径；
- 自动清理 1.1.4 遗留的 `guard-task.cmd`；
- 门禁：K-10（PS/BOM/可诊断）、L-1（`$env:PATH` 绑定）、B56（`-File` 引号）同步。

## [1.1.4]（2026-09-15）

### 修复：Windows 守卫「从未被执行」的三处加固

真机现象：状态目录里 `config.json`/`ports.json`/`guard.log` **一个都没建** —— 而 `cmdDaemon`
第一行就是 `mkdirSync(SUPERVISOR_DIR)`，说明守卫进程**从未被启动**（不是启动后崩）。

- 包装脚本正文改 **ASCII**：绝对路径经 `%LOCALAPPDATA%`/`%APPDATA%`/`%ProgramFiles%` 引用。
  非 ASCII 用户名（中文）写进 `.cmd` 会被 `cmd` 按 OEM 码页误解码 → node/guard 找不到；
- 包装脚本与兜底 spawn **全程落日志**（`guard-task.log` / `guard-spawn.log`，含 exit code）——
  下次失败必有证据，不再「什么都没有」；
- 修 `stop()`：守卫是 `node.exe`，旧的 `taskkill /IM dsh-supervisor.exe` 根本杀不掉 →
  残留进程/锁使后续启动被 `guard.lock` 拒绝；改为按命令行精确杀 dsh-supervisor 的 node；
- 门禁：K-10（ASCII/日志/杀进程）、L-1 判据同步。

## [1.1.3]（2026-09-15）

### 修复：Windows 上守卫从未被执行 —— 内核永远拉不起来

根因：守卫是 npm 包内**无扩展名的 Node 脚本**，而壳的包装脚本与 spawn_daemon 都是
`"<guard>" daemon` —— Windows `cmd` 不能执行无扩展名文件、也不认 shebang。
`schtasks /Run` 只报「任务已触发」，因此表现为「服务管理器错误：无 但守卫永不就绪」。

- `platform/windows.rs`：新增 `guard_argv()` —— 无扩展名守卫**显式用契约 node 执行**（仅 `.cmd`/`.bat` 直接执行）；
- `platform/mod.rs`：`LaunchSpec` 增加 `state_root`（壳解析的单一事实源）；
- 三平台服务定义与 spawn 均注入 `DSH_SUPERVISOR_HOME`，杜绝壳/内核各自推导状态根分叉；
- 门禁：L-1 Windows 执行器（含反向）、K-9 状态根注入。

## [1.1.2]（2026-09-15）

### 产品状态根独立于 DSH（XDG，2026-09-15）

本产品**管控 DSH**，状态不得寄在被管控对象的 `~/.dsh` 下。状态根改为自有目录：

- 覆盖 `DSH_SUPERVISOR_HOME`；默认 Linux `$XDG_STATE_HOME/dsh-supervisor`（`~/.local/state/dsh-supervisor`）、macOS `~/Library/Application Support/dsh-supervisor`、Windows `%LOCALAPPDATA%\dsh-supervisor`；
- `env.rs`：`state_root()/supervisor_dir()/shell_dir()` + `STATE_ROOT_SCHEMA=1`；`update.rs`/`mirror.rs`/`platform/{macos,windows}.rs` 全部经此；
- 平台默认值下沉 `platform::state_root_default`（G1）；新增诊断命令 `shell_state_root`；
- 启动早期 `env::migrate_legacy()`：旧 `~/.dsh/{supervisor,shell}` **按条目**前向迁移（不覆盖新文件）；
- 门禁：`kernel_launch_standard_test` K-8（含反向）。

### Windows 看护任务收归壳（2026-09-15）

按 KERNEL-LAUNCH-STANDARD H5 / KERNEL-DAEMON-CONTRACT D6：

- `platform/windows.rs::ensure_defined` 同时建立 `DSH-Supervisor-Watchdog`
  （写 `watchdog.ps1` + `schtasks /SC MINUTE /MO 5`）；保活语义与内核旧实现一致
  （守卫与 GUI 壳两个判断**相互独立**，否则「壳崩、守卫活」时壳永不回来）；
- 幂等：即使守卫任务已是最新，也确保看护任务存在；
- 门禁：`kernel_launch_standard_test` 增 K-7（含反向）。

### 内核启动规范落地：位置契约 + 对齐前置（2026-09-15）

按 `docs/KERNEL-LAUNCH-STANDARD.md`（跨平台 P0–P6）落地：

- 新增 **core.json 位置契约**（`src-tauri/src/core_contract.rs`，schema 1，壳唯一写入）：安装成功后记录 `bin/prefix/version/source`；
- `locate_core` 候选顺序改为 **core.json → runtime.json 的 nodeBinDir → PATH/固定目录**（nvm/volta/fnm/自定义 prefix 不再「装了找不到」）；
- `guard_start` 增加**对齐前置**：磁盘内核必须等于线上最新，否则返回 `KERNEL_NOT_ALIGNED` 且**不触服务管理器**；引导页收到后自动对齐一次再重试（`60-guard.js`）；
- `core_apply` 成功后**回读确切位置**并写 `core.json`；回读不到 → `stage=record` 如实失败（不假装成功）；
- 平台层新增 `core_bin_candidates_in_prefix`（Windows 覆写 `.cmd` + 包内真实脚本）；
- 门禁：`src-tauri/tests/kernel_launch_standard_test.rs`（K-1..K-6，含反向）。

## [1.1.1]（2026-09-15）

### 运行期启动契约：修「内核装上却永远拉不起来」（Phase 1）

- 新增 runtime.json（schema 2）由壳写、内核读；node/nodeBinDir/npmPath + 保留旧键（nodePath/nodeVersion/minNode）；
- 服务定义三平台显式绑定 Node + PATH（systemd ExecStart=node+guard / launchd ProgramArguments / Windows 包装脚本注入 PATH）；
- 内核安装改用契约里的绝对 npm + PATH；spawn 兜底用 node+guard+env PATH；
- 端点从 ports.json 的 supervisor-api **实际**值发现，等待循环每 tick 重读；
- 门禁：tests/platform_launch_contract_test.rs（L-1..L-5，含反向判据）。

### 统一更新决策模型（Phase 2）

问题 1：桌面自更新与内核更新是两套检测/形状，前端表现为「两套流程」。
- 新增 src/update_plan.rs：统一形状 artifact/current/latest/available/channel/source/error + channel 词表；
- core.rs::build_plan 与 shell_update_check 都经 update_plan::unified（保留旧字段向后兼容前端）；
- 内核侧新增 platform/runtime-contract.js（壳写内核读）：runNpmInstall 用契约的绝对 npm + PATH，
  env-catalog 读同一契约 —— 内核自身执行 npm 与壳**同源**；
- 门禁：update_pipeline_test（U-1..U-3）、runtime-contract-test（R-1..R-6）。

### 契约 schema 握手门禁（Phase 3）

两仓各自断言 schema 版本，**互不读源码**；任一侧改 schema 必须同时改两侧，否则各自 CI 变红。

- 壳 `runtime_contract.rs` 的 `SCHEMA` 恒为 2，门禁 L-5 锁定；
- 内核 `platform/runtime-contract.js` 的 `SUPPORTED_SCHEMA` 恒为 2，门禁 R-6 锁定；
- 门禁：`platform_launch_contract_test`（L-1..L-5，共 9 断言）。

### 内核更新收敛为单写入者 = 壳（2026-09-15）

问题 1 的根：内核 npm 包有**两个写入者**（内核自更新 `POST /self-update/apply` 与壳 `core_apply`），
两套版本判定、两种源策略（内核官方 registry vs 壳镜像）。现收敛为**唯一写入者 = 壳**。

- 新增 `src/bridge.rs`：面板→壳 消息桥契约（协议 v1、请求/结果/进度、代执行命令名）—— 唯一事实源；
- 新增命令 `kernel_update_apply`：安装复用 `core_apply_inner`（与启动门 2 同一实现）→
  经服务管理器停守卫 → 等端口释放 → `ensure_guard` 重拉（守卫从不重启自己）；
- `shell.html` 增加受校验的消息桥：只接受 `ev.source === 内容 iframe` 且 origin 回环，
  回复 `targetOrigin = ev.origin`（不回 `*`），重启成功后重载面板；
- `commands/mod.rs` 抽出 `core_apply_inner`，`core_apply` 与新命令共用；
- 门禁：`src-tauri/tests/kernel_update_single_writer_test.rs`（SW-1..SW-6，含反向判据）；
- SSOT：`docs/DESIGN-SHELL-ARCHITECTURE.md` §3.2c。

## [1.1.0]（2026-09-14）

### 内核零回退（产品硬规则落地）

内核更新强制且唯一：检测到新版本即安装最新，**不得回退旧内核**（唯一保留回退的是 DSH 自身升级）。

| 位置 | 内容 |
|---|---|
| `bootstrap/js/60-guard.js` | 删除「守卫未就绪 → 回退内核 vX」整段；失败即失败 |
| `bootstrap/js/{00-runtime,50-kernel,70-boot,10-ui}.js` | 删除 `coreFrom/coreTo` 状态，改为单一 `coreVersion` |
| `src/commands/mod.rs` | `core_apply` 删除 `version` 参数，永远解析并安装最新 |
| `src/core.rs` | 移除「失败回退/方案 B」注释 |

### 全面审计与清理

- 删除过时文档（时间点记录/归档）与未使用图标、重复副本（LAUNCHER_README/LICENSE/app-icon/.gitignore.shell）；
- emoji 全仓清零；自我论证式长注释统一收敛为约束/不变量；
- `bump-shell.sh` 的 GNU `sed -i` 改为可移植写法；`make-manifest.js` 删除 no-op 与失效 `--base`；
- 文档修正：RELEASE-STANDARD / README / 跨仓契约表等与现实现对齐。

### 跨仓解耦与平台规范收口（2026-09-14 追加）

- `src-tauri/src/platform/mod.rs`：`Platform` trait 新增 `core_platform_tag()`；`core.rs::package_name()`
  只负责拼包名，平台分支彻底收拢到平台层；
- `tests/bootstrap_flow.rs`：G1/B59 门禁扩展覆盖 `std::env::consts::OS/ARCH` 的**分支**形态；
- 新增 `tests/no_suppression_machinery_test.rs`（回退/拉黑机构反回归，迁自内核跨仓门禁）；
- 新增 R-10 门禁：CI 矩阵的每个 artifact 必须被 `shell-release/assemble-shell-pkg.js` 覆盖；
- `shell-release/version-vectors.json` 契约说明更新（不再要求两仓逐字节相同）。

### 验证

`cargo test` 全绿（CI 四平台：linux-x64 / win-x64 / darwin-arm64 / darwin-x64）。

## [1.0.10]（2026-09-14）

> 修复 **Windows 上内核升级必然失败** 的致命缺陷。根因来自 npm debug log，已确证。

### 修复：--prefix 带 Windows verbatim 前缀 -> npm arborist 无限递归爆栈

现场：Windows 用户升级内核报 Maximum call stack size exceeded（退出码 1）。

决定性证据（npm debug log 的 argv）：我们传给 npm 的 --prefix 带着 Windows
verbatim 前缀（两反斜杠+问号+反斜杠开头）。而 npm prefix -g 对同一目录返回干净形式。
堆栈指向 @npmcli/arborist 的 realpathCached **无限递归** —— 它在 verbatim 前缀上不收敛。

根因：Rust 的 std::fs::canonicalize() 在 Windows 上总是返回 verbatim 形式；
locate_core -> global_prefix_for 按组件重建前缀时把它原样带上，最终交给 npm。

修法：新增 strip_verbatim（字符串级，跨平台可测）+ 单一 simplify，在三处调用点生效：

- global_prefix_for 的两个返回分支（node_modules 布局 / Windows 垫片布局）；
- run_npm_install 构造 --prefix 处（单一实现、两处调用，不分叉）。

### 修复：失败路径的证据缺失（本次事故难查的直接原因）

core_apply 的成功分支一直回传 prefix，失败分支却丢了；install_version 的错误
也只有一句「npm 退出码 1」，没有命令、没有 prefix、没有试过哪些源。
现失败时回传：命令 + prefix + registry + 每次尝试的输出 + originsTried + prefixIsNodeDir。

> 纪律：失败路径的信息量必须 >= 成功路径 —— 否则最需要诊断的时刻恰恰没有证据。

### 加固：缓存隔离重试（有界）

该类错误的次要成因是 npm 缓存损坏。首次失败且失败得很快（<=120s）时，用全新临时缓存重试一次；
窗口外不重试（否则两次 15 分钟会突破引导页 17 分钟预算），并说明为何未重试。

### 新增门禁 kernel_install_evidence_test.rs（8 断言）+ 4 条单测

K-1 失败证据含命令/prefix/源；K-2 缓存隔离存在；K-3 重试有界；K-4 失败分支带 prefix 与来源；
K-5 无「声明无调用点」；K-6 反向判据；K-7 verbatim 前缀在交给 npm 前被剥除（3 处调用点 + 回归单测）；
K-8 反向识别未剥前缀形态。

注入验证（三次，均按「注入 -> 确认门禁红 -> 还原 -> 确认绿」）：

| 注入 | 结果 |
|---|---|
| global_prefix_for 不剥前缀 | K-7 抓到 |
| npm --prefix 去掉 simplify | K-7 + K-8 抓到 |
| 失败分支去掉 prefix | K-4 抓到 |

> 第三次注入在第一版门禁下未被抓到 —— 因为 K-4 取到的是函数体内更早的 resolve 守卫分支，
> 而切片一直延伸到函数末尾，把成功分支的 prefix 也算进来 -> 断言恒真。
> 现锚定 Ok(match res 之后的那个 Err，并加反空转断言。这正是注入验证不可省的原因。

### 计数

壳仓 cargo test：137 -> **149 断言 / 0 失败**（+4 单测 +8 门禁）；cargo check：0 告警。

## [1.0.9]（2026-09-14）

> 本轮为**底座审计与硬标准落地**：壳仓同样必须有唯一规范、同样必须走 GitHub 完整四平台构建。

### 硬标准（不可协商）

**所有平台构建与发布必须经 GitHub CI 完成；本地不得产生任何发布产物。**

写入 `docs/RELEASE-STANDARD.md` §0，并加入机器可读块 `hardStandard` 字段。
壳仓本就无本地构建/发布脚本（仅 `bump-shell.sh` / `verify-shell-versions.js`），与硬标准一致。

### 修复：**幽灵产线文件**（最严重）

`src-tauri/launcher-build.yml`（**290 行**）**不在 `.github/workflows/` 下**，
GitHub **永远不会执行**它；但 `docs/RELEASE-AND-BUILD-DECISION.md:50` 与 `scripts/bump-shell.sh:30` 都**声称它是产线**：

    → tag 触发 launcher-build.yml：四平台完整构建        （文档）

它还有自己的 git 历史（3 个提交都是 CI 修复）—— **过去的 CI 修复曾提交到这个永不执行的文件上**；
两份定义已漂移（触发策略与步骤数均不同）。

**处置**：删除幽灵 + 两处引用改指真实产线 `.github/workflows/build.yml`。
已核实**真产线是幽灵的超集**（多一道门禁测试步骤，关键修复全在且实例更多）→ 无功能损失。

### 修复：版本三处互锁**从未被 CI 校验**

`scripts/verify-shell-versions.js` 存在且正确（校验 `Cargo.toml` / `tauri.conf.json` / `Cargo.lock`），
但 `version` job **从未调用它** → 三处可静默失配（锁文件失配会被 cargo 当作依赖变更）。
**处置**：在 `version` job 增加校验步骤，并由 R-6 门禁强制。

### 新增门禁 R-9（防旁路复活）

`src-tauri/tests/release_spec_consistency_test.rs` 由 8 条增至 9 条：

- `scripts/` 下不得出现含 build/publish/release 的脚本（只允许版本提升与校验）；
- 不得存在 `shell-release/{build,publish,release}.sh` 之类本地发布脚本；
- 含反向判据。

将来若有人为「方便」加回本地构建/发布脚本，门禁会立刻红。

### 文档：唯一事实源与角色表

- 新增 `docs/RELEASE-STANDARD.md`（**壳仓发布唯一事实源**：H0–H9 全流程、四平台矩阵、版本互锁、CI 放行、验证、回滚、红线）；
- 新增 `docs/README.md`：12 份文档**角色表**（规范 / 设计 / 决策 / **时间点记录**），并声明冲突时以现行文档为准；
- `docs/RELEASE-AND-BUILD-DECISION.md` 加横幅：流程以 `RELEASE-STANDARD.md` 为准，本文件只讲决策背景。

### CI

- `pull_request` 触发器补入（此前 PR **完全不跑 CI**；若直接设 required status check 会让 PR 永久等待）；
- 分支保护 required = `version` + 4 条 `build (...)`（strict + enforce_admins）。

## [1.0.8]（2026-09-11）

### 修复：**架构级根因** —— 需要 IPC 的页面被放进了 iframe（所有 invoke 永久挂起）

这是「卡在检测环境、不报错、诊断全 none」的**真正根因**。此前三轮修复都未触及。

#### 一、根因（Tauri 源码级证据）

`tauri/src/manager/webview.rs`：

```rust
fn main_frame_script(script: String) -> InitializationScript {
    InitializationScript { script, for_main_frame_only: true }   // ← 仅主帧
}
all_initialization_scripts.push(main_frame_script(self.invoke_initialization_script.clone()));
```

**IPC 的初始化脚本全部标记为「仅主帧」**，其中包括 `window.__TAURI_INTERNALS__`
（`invoke` / `ipc` 的实现）。而 `__TAURI__.core.invoke` 内部正是：

```js
async function invoke(cmd, payload) { return window.__TAURI_INTERNALS__.invoke(cmd, payload); }
```

⇒ **在 iframe 中 `__TAURI_INTERNALS__` 不存在 → invoke 立即抛错 → Promise 永不 settle。**

#### 二、症状完全吻合

旧架构：窗口加载 `shell.html`，引导页放在其中的 `<iframe src="bootstrap.html">`。
于是 iframe 中的引导页：

- `node_status` / `shell_identity` / `mirror_cached` **全部挂起**；
- 我们的诊断代码大量使用 `.catch(function(){})`，**错误被静默吞掉**；
- 只有**由前端主动发起**的阶段上报（`shell_set_phase`）会缺失 →
  日志停在「壳启动」，`phase` 停在 `boot`；
- 那些**不需要 invoke** 的展示位回落到初值 → 诊断串每一项都是 `none`。

这与用户报告**逐字吻合**：

```
node=unknown | core_from=none | ... | env_candidates=none | env_probe_error=none
env_stuck=none | env_trace=none | mirror=none（预热未启动或全部不可达）
mirror_node_best=none | mirror_npm_best=none | mirror_probes=none
error=环境检测超时（探针无响应，可能有异常的可执行文件占位）
```

**「探针无响应」是误判** —— 探针根本没被调用成功，因为 IPC 不可达。

#### 三、为什么此前三轮都没找到

| 轮次 | 我修的东西 | 为何无效 |
|---|---|---|
| 第 1 轮 | 探测移出主线程 | 探测**根本没被调用** |
| 第 2 轮 | 枚举纳入进度上报 | 同上 |
| 第 3 轮 | 硬死线 + 交错探测 | 同上 |

我一直在修「探测慢 / 探测卡住」，而真问题是**前端根本无法与 Rust 通信**。
具体原因：

1. **该错误在两个平台表现不同**：Linux 上 iframe 的 `window.__TAURI__` 因某种原因
   路径尚可解析（故我本机测试通过），Windows WebView2 上则彻底不可用 ——
   而我在 Linux 上验证，**从未在 Windows 真机上验证过**；
2. **错误被 `.catch(function(){})` 静默吞掉**，没有任何日志暴露它；
3. 我**只读代码、静态断言**，没有真实运行 GUI 观察运行时行为。

#### 四、修复：需要 IPC 的页面必须是主帧

**不采用「换一种取 IPC 的方式」**（那只是绕过症状），而是修正架构：

```json
// tauri.conf.json —— 窗口直接加载引导页
"url": "bootstrap.html"        // 原为 shell.html（内含 iframe）
```

```
引导页（主帧）── 完成引导 ──▶ 导航到 shell.html（壳框架，主帧）
                                    └── iframe：守卫托管的面板（HTTP 页面，不需 IPC）
```

关键变化：

1. **引导页成为主帧** —— IPC 必然可用；
2. **`tauri.windows.conf.json` 同步修改** —— 该文件是**数组整体替换**而非字段合并，
   若不同步，**修复在 Windows 上完全失效**（门禁 B11 抓到了这一点）；
3. **引导页自带窗口栏** —— 窗口是 `decorations:false`，故引导页必须有拖动区与
   最小化/最大化/关闭按钮（此前由 shell.html 提供）；
4. **引导完成 → 主帧导航**到 shell.html；
5. **壳框架主动索取面板 URL**（新增命令 `shell_panel_url`）—— 不再依赖
   `shell:goto-panel` 事件（它在首帧可能早于 listener 注册而被丢弃）；
6. **`boot()` 不再静默返回** —— `core` 缺失时明确报错「Tauri IPC 不可用」，
   而不是让页面停在静态文案上（旧代码 `if (!core) return;` 正是「不报错」的来源）。

#### 五、真实 GUI 验证（端到端，非静态断言）

Xvfb + 全新 dbus session 跑真实二进制：

```
[boot] main enter → building app → setup enter → init_identity done → building tray
壳启动 v1.0.8
阶段 → env                ← 检测环境
阶段 → shell-update       ← 桌面版本
阶段 → kernel             ← 内核版本
阶段 → guard              ← 守卫
守卫服务定义: 已存在 ~/.config/systemd/user/dsh-supervisor.service
服务管理器未能在 30s 内拉起守卫 · 改用直接启动兜底
兜底 spawn 守卫 pid=3059670
阶段 → ready              ← 守卫就绪
壳框架就绪（主帧导航完成），面板 URL: http://127.0.0.1:36360/
```

**「壳框架就绪」这行证明主帧导航真的发生** —— 这是我第一次观察到 GUI 走完全链路。

#### 六、新增门禁

- **B54** 引导页必须是主帧（窗口 URL = `bootstrap.html`；`shell.html` 不得再把
  引导页放进 iframe；引导页不得依赖 iframe 的 IPC 回退；壳框架必须主动索取面板 URL）；
- **B55** IPC 不可用时 `boot()` 必须**明确报错**，不得静默返回。

#### 验证

- 壳测试 **68 项全通过**（bootstrap_flow 54 + updater_artifacts 6 + 单元 8）；
- 真实 GUI：引导链路完整推进 `env → shell-update → kernel → guard → ready`，
  并**成功导航**到壳框架（`壳框架就绪（主帧导航完成）`）。

## [1.0.7]（2026-09-11）

### 修复：引导页 JS 语法错误导致引导完全静默（本轮真正的根因）+ 把真机验证变成常规手段

#### 一、决定性发现：一个逗号让整个引导页失效

在 diagText() 里新增数组元素时**漏了行尾逗号**。下面是修复前后的对照（
**修复前那段是错误代码**，仅为说明；当前源码已是「修复后」形态）：

```js
// ❌ 修复前（错误）：第 310 行末尾没有逗号，导致整个 <script> 块语法错误
//     ... : (warmTimer ? 'A' : 'B')))      <-- 此处应有逗号
//     'mirror_node_best=' + ...
//
// ✅ 修复后（当前源码，见 src-tauri/bootstrap/bootstrap.html 第 310 行）：
//     ... : (warmTimer ? 'A' : 'B'))),     <-- 行尾逗号
//     'mirror_node_best=' + ...
```

**后果是整段 script 块语法错误，导致所有 JS 都不执行、boot() 从不运行**，
于是页面永远停在 HTML 里的静态文案「正在检测系统环境…」：

- **不报错**（没有任何 JS 在执行，也就没人报错）；
- **不推进**（boot() 未被调用）；
- **诊断全 none**（diagText() 也没执行）；
- Rust 侧日志只有「壳启动」一行（前端从未调用 shell_set_phase）。

这个现象与「探测卡住」**表面完全一致**，但病因截然相反 —— 一个在前端一行 JS，
一个在 Rust 探测逻辑。我因此在 Rust 侧来回排查了三轮。

#### 二、更该反省的：我提交前明明跑了检查，却没看结果

我在提交前执行了 node --check，它**失败了**，但命令用了 && 串联，
失败导致后续「成功提示」未打印 —— 而**我没有核对输出就继续往下走**。

教训：**「跑过检查」不等于「检查通过」。必须核对结果，且最好是自动化的。**
故本次把 JS 语法检查固化为门禁 B53（见下）。

#### 三、方法论的突破：用真实 GUI 验证，而不是只查源码

此前我一直在「读代码 + 静态断言」的层面验证，而本轮改用**真实运行**：

```bash
Xvfb :95 -screen 0 1400x900x24 &
dbus-run-session -- bash -c "HOME=$WORK DSH_BOOT_TRACE=1 ./dsh-supervisor-gui"
# 然后读 $WORK/.dsh/shell/shell.log 看 phase 推进
```

修复前 shell.log 只有 1 行；修复后：

```
[boot] main enter
[boot] building app
[boot] setup enter
[boot] init_identity done
[boot] setup: building tray
壳启动 v1.0.7
阶段 -> env                <- 检测环境 OK
阶段 -> shell-update       <- 桌面版本 OK
阶段 -> kernel             <- 内核版本 OK
阶段 -> guard              <- 守卫就绪 OK
守卫服务定义: 已建立并启用 ~/.config/systemd/user/dsh-supervisor.service
```

**这才是「链路是否真的通」的可核对证据**，而不是我的判断。

#### 四、把启动里程碑日志固化为常开能力（而非临时脚手架）

shell.log 若只有「壳启动」一行，**无法区分**两种截然不同的病因：

| 现象 | 病因 | 修复方向 |
|---|---|---|
| setup 从未执行 | Rust 侧插件/DBus 层失败 | 查插件初始化 |
| setup 正常但前端无日志 | 前端 JS 未执行 / IPC 失败 | 查前端 |

两者方向相反，而我为此来回三轮。现改为**常开**：
main enter -> building app -> setup enter -> init_identity done -> building tray，
每次启动 6 行（shell.log 超 1MB 自动滚动），打开日志一眼即可定位到**哪一层**。

> 成本极低、收益极高：这是用一次真实事故换来的诊断能力。

#### 五、新增门禁 B53：前端 JS 必须语法正确

提取 HTML 全部内联 script，逐个跑 node --check；
node 缺失时**明确 SKIP**（而非静默通过 —— 否则门禁形同虚设）。
失败信息直接打印语法错误与行号，**不依赖人的注意力**。

#### 六、版本号合并（用户要求）

原计划分两次发布（1.0.7 / 1.0.8），但两者**均未发布**。
按用户要求「不要为每个修复递增版本号」，已**合并为单一 1.0.7**：
tauri.conf.json / Cargo.toml / Cargo.lock 三处一致，CHANGELOG 两段合并为一段。

#### 验证（本轮全部用真实运行）

- **真实 GUI（Xvfb + 全新 dbus session）**：引导链路完整推进至 guard，
  **P0 服务定义真实建立**（systemctl --user unit 落盘），镜像选出 mirrors.huaweicloud.com；
- 壳测试 **66 项全通过**（bootstrap_flow 52 + updater_artifacts 6 + 单元 8，含新增 B53）。


# 本版合并了原计划分两次发布的修复（1.0.7 / 1.0.8）——
# 两者均未发布，故合并为单一版本，避免无意义的版本号膨胀。

### 修复：镜像「一等公民」化 + 数据流审计（用户质疑驱动：能力没丢，可见性丢了）

用户指出：「**你连镜像源都看不到**，根本就不会去选择镜像源…我严重怀疑你的探针和镜像
配置没放在壳里，是丢失状态」。

#### 一、先用实物证据回答「能力是否丢失」：**没有丢失**

我下载了**用户手上那个已发布 1.0.6 的安装包**、解包、与本地从同一源码构建的 release
二进制对照：

| | 本地 release | 已发布 1.0.6 |
|---|---|---|
| 二进制大小 | 10,497,408 | 10,500,608（差 3KB，构建环境差异）|
| 探针/镜像/引导页标记 | 有 | 有 |

壳内实际代码（全部编译进二进制）：

```
nodeprobe.rs  638 行   环境探针        mirror.rs  282 行  18 个镜像源
node.rs       425 行   Node 下载+安装   core.rs    465 行  内核 npm 安装
service.rs    257 行   三平台服务定义   bounded.rs 190 行  有界执行
bootstrap.html 815 行  引导页 UI（压缩嵌入二进制）
```

`tauri.conf` 的 `frontendDist=bootstrap`、窗口 `url=shell.html`、
`shell.html` 内 `iframe src="bootstrap.html"` —— **引导页是本地资源，不依赖内核**。

按引导页调用顺序，八个能力**全部由壳提供**：探针 / 镜像测速 / Node 下载校验 / Node 安装 /
内核 npm 安装 / 服务定义 / 守卫启动 / 引导 UI。**内核是被安装的对象，不是能力提供者。**

> 附：我最初用 `strings` 查二进制，结果"找不到"标记 —— 那是**我的测量方法错了**
> （Tauri 压缩嵌入资源，明文不可见）。改用本地/已发布对照后结论反转。

#### 二、但用户的观察是对的：**镜像可见性确实丢失**

代码路径上的必然结果：

```js
// bootstrap.html afterEnv
nodeVer = st.installed;
return stepNodeDone();   // ← 直接放行，不调 probeMirrorThen
```

**只要 Node 已达标（主力用户就是这样），镜像探测根本不执行** → `lastMirror` 恒为 null
→ 诊断串必然 `mirror=none | mirror_probes=none`。

而 `core_plan` **早已回传 `registry`（命中的镜像）**，前端**从未使用** —— 数据链路是断的。

#### 三、全字段数据流审计：**9 个字段后端产出、前端从未使用**

```
registry  updateAvailable  nodeBest  npmBest  path
package   hint            selectedNode  selectedNpm
```

这说明问题不是「能力放在内核里」，而是「**壳有能力却完全不展示**」。
外壳做了大量工作，用户什么都看不到 —— 这会让人合理地怀疑能力根本不存在。

#### 四、修复：把镜像提升为一等公民

镜像不是「下载 Node 的辅助」，而是**壳所有网络动作的基础设施**（内核安装同样依赖它）。

1. **预热**：引导开始即后台并行测速（`mirror_warmup`，**立即返回不阻塞**）；
2. **全程可读**：`mirror_cached` 纯读缓存（**无网络 I/O**，可安全高频轮询）；
3. **全步骤可见**：
   - 环境步骤：显示「镜像已就绪：npmmirror.com（185ms）」；
   - 内核步骤：显示「内核已是最新（v0.1.5）· 源 mirrors.huaweicloud.com」（**消费 `p.registry`**）；
   - 安装步骤：「未安装内核 · 正在安装标准产品包…（源 xxx）」；
4. **诊断串始终带镜像**：
   `mirror_npm_best=... | mirror_probes=源1(88ms) > 源2(x) > ...`，
   并将「预热中」与「全部不可达」**区分开**（而非一律 none）。

#### 五、顺带查出一个真实缺陷：npm 探测用了根路径

`probe_all(&npm, "")` 实际请求 `https://<源>/`，而多数 registry 根路径返回 **404** ——
**健康的源被判「不可达」**。

实测：腾讯云 npm 镜像连测 3 次均 HTTP 200、能正确返回我们的包，
却因根路径 404 被显示为「不可达」并排除在选择之外。**探测方法错误让壳无谓地少一个镜像。**

已改用真实包名做探针。修复后 npm 侧 **6/6 全部可达**（修复前 5/6）。

#### 六、新增无头自检 + 门禁

**`--mirror-plan`**（无 GUI 验证镜像，任何平台可用）：

```
=== 镜像测速自检 ===
Node 候选 10 个 / npm 候选 6 个
https://mirror.nju.edu.cn/nodejs-release          可达    108 ms
https://registry.npmmirror.com                    可达    290 ms
...
Node 选中: https://mirror.nju.edu.cn/nodejs-release (108 ms)
npm  选中: https://registry.npmmirror.com (290 ms)
```

新增门禁 B49–B52：
- **B49** 镜像必须与引导并行预热（不得只在下载分支里产生），诊断必须区分「预热中」；
- **B50** `core_plan` 回传的 `registry` 必须被前端消费（数据链路不得断裂）；
- **B51** 内核步骤必须显示当前镜像；
- **B52** npm 探测必须用真实包名（不得用根路径）。

#### 验证

- 壳测试 **65 项全通过**（bootstrap_flow 51 + updater_artifacts 6 + 单元 8）；
- `--mirror-plan`：Node 10 源全可达、npm 6 源全可达，正确选出最快源。

### 修复：卡住的第三层根因 —— 候选枚举落在进度上报之外（诊断三项全空的真正原因）

用户反馈 1.0.6 仍卡在检测环境，诊断串为：

```
env_candidates=none | env_stuck=none | env_trace=none | error=环境检测超时
```

#### 三个 none 由同一件事解释 —— 这是决定性线索

`detect()` 的执行顺序是：

```rust
fn detect() {
    let cands = candidates();          // ← 整个枚举在这里，**在进度上报之外**
    if ... { l.summary = summary; }    // ← 永远到不了
    for (source, cand) in cands {
        live_stage(...)                // ← 永远到不了
```

而 `candidates()` 内部会依次做三件可能阻塞的事：

| 步骤 | 阻塞点 |
|---|---|
| ① 读 `runtime.json` | 漫游配置/网络用户目录上的 `read` |
| ② 枚举已知落点 | `read_dir`（nvm / fnm / volta 目录） |
| ③ 过滤 PATH | **每个 PATH 条目一次 `GetDriveTypeW`**（微软文档明确提示该 API 可能慢） |

**一旦卡在上述任一步（最可能是 ③），summary / stage / trace 三项同时为空。**
这与用户看到的 `env_candidates=none | env_stuck=none | env_trace=none` **完全吻合**。

#### 这是同一类错误的第三次出现（诚实记录）

| 轮次 | 我做的事 | 留下的盲区 |
|---|---|---|
| 1.0.5 | 把探测搬进分离线程 | **候选枚举本身没搬** —— 仍在命令路径上做 I/O |
| 1.0.6 | 把枚举搬进线程 + 摘要改缓存 | **枚举阶段没有 stage** —— 卡住时三项全空 |
| 1.0.7 | 见下 | —— |

规律很清楚：**「让调用不阻塞」不等于「卡住时看得见」**。
两次我都只做了前者。故本次不只修代码，还把它变成**机械化断言**（B47）。

#### 真正的结构性修复：**边枚举边探测，且 PATH 过滤放到最后**

上一版的顺序是「先把**全部**候选枚举完，再逐个探测」。这有一个致命后果：

> **只要枚举阶段慢/卡（PATH 过滤要逐盘符调 `GetDriveTypeW`），
> 就连已经枚举好的廉价候选都永远试不到** ——
> 用户本可瞬间命中「已知安装落点」，却因 PATH 过滤卡住而**完全失败**。

现改为**交错（interleaved）**：

```
① 读 runtime.json 记录路径  → 立即探测
② 已知安装落点（含 nvm/volta/fnm/scoop）→ 列一个、试一个
③ PATH 过滤 → **最后才做**（唯一需要逐盘符系统调用的阶段）
```

于是「Node 装在标准位置」的绝大多数用户（含本项目的目标场景）
**根本不会走到 PATH 过滤** —— 那个可疑的系统调用连一次都不会被调用。

> 这比「把它加进进度上报」更根本：**不上报不如不调用。**

#### 修复（两条硬规则）

**规则一：任何可能阻塞的调用之前，必须先 `stage()`。没有例外。**

现在枚举的每一阶段都有独立阶段标记，且**增量更新**候选摘要：

```
① 读取 runtime.json 记录路径
② 枚举已知安装落点
③ 过滤 PATH（跳过网络盘/UNC）
③ 收集 PATH 候选 N/M
探测候选 <来源>（<路径>）
```

摘要也改为增量写入，故即使卡在阶段 ③，`env_candidates` 也会显示
`候选 N 个（记录 0 / 已知 5 / PATH 0）（进行中：已完成 ①②）` ——
**一眼看出卡在哪一步、已收集多少**，而不是一片空白。

**规则二：Rust 侧必须有硬上限（25 秒），超限即明确失败并给出原因。**

前端预算 45 秒、Rust 硬上限 25 秒 —— 于是**总是 Rust 先给出结论**：

```
环境探测超过 25000 ms 未完成（卡在「③ 过滤 PATH（跳过网络盘/UNC）」已 24800 ms）
```

这消除了「probing 永远为真」：`node_status` 现在有三种确定的返回 ——
完成 / 进行中（附当前阶段）/ **明确失败（附卡住阶段与耗时）**。

#### 附带改进

- **`GetDriveTypeW` 按盘符缓存**：原实现对**每个 PATH 条目**都调一次，
  而 PATH 里数十个条目往往集中在同一两个盘符 —— 一旦该盘有问题就重复付出阻塞代价。
  现每个盘符最多查询一次（≤26 次）。
- **PATH 条目上限 64**：极端长的 PATH 不应把探测拖成分钟级。
- **`catch_unwind` 包裹探测**：worker 内部 panic 也必须产出结论，
  而不是让线程静默死亡（那会表现为「永不结束的 probing」）。
- **`Disconnected` 视为明确失败**：不再静默停在进行中。
- 前端新增 `env_probe_error` 诊断字段，并据此**立即**给出可操作结论，
  不等自己的预算耗尽（那只得到一句没有信息量的「超时」）。

#### 本次最重要的产出：性质测试 + 机械化门禁

新增**性质测试**（`nodeprobe::tests`），用真实注入复刻线上故障：

```rust
// 注入：让枚举阶段永久阻塞；硬上限压到 500ms 便于快速验证
set_hang_in_enumerate(true);
// 断言 1：卡住时**阶段必须可见**（否则无从排障）
assert!(current_stuck().unwrap().0.contains("模拟枚举"));
// 断言 2：必须在硬上限内给出**明确失败**，且原因包含卡住的阶段
assert!(last.finished);
assert!(err.contains("模拟枚举"));
```

> 这个测试是**直接针对线上故障**写的：它证明「卡在枚举时，用户一定能拿到带阶段的原因」，
> 而不再是三项全空的静默卡死。

新增门禁 **B47**（规则一的机械化检查）：
断言 `enumerate_staged()` 中每一处可能阻塞的调用（`recorded_node_path` /
`known_locations_staged` / `path_dirs_staged`）**之前都存在 `stage(`**，
并断言硬上限存在且能产出可读原因。

#### 验证

- 壳测试 **60 项全通过**（bootstrap_flow 46 + updater_artifacts 6 + 单元 8）；
- 含两条新增性质测试：`hard_deadline_yields_actionable_failure_when_enumeration_hangs`、
  `normal_probe_completes`；
- `--env-plan` 正常（14 个候选，50ms）。

## [1.0.6]（2026-09-11）

### 修复：我上一轮引入的致命回归（命令路径做 I/O）+ 三平台适配逐项核对

用户真机反馈：**1.0.5 仍卡在检测环境，而且不报错**。并要求逐平台核对适配是否正确
——「不要拍脑袋，要真实地看代码」。

#### 一、致命：我上一轮修复时自己引入的缺陷

为让诊断串显示候选数量，我在 `node_status`（**命令路径**）里调了 `candidate_summary()`，
而它内部会**枚举候选** —— 那要做 `read_dir`（版本管理器目录）并对每个 PATH 条目
调 `GetDriveTypeW`。

**这正是我声称已经消除的那类无界阻塞 I/O。我把刚搬走的石头又搬了回来。**

而更糟的是第二个缺陷让它表现为「不报错」：

```js
// 旧：轮询只在 invoke 的 .then 里再调度
function poll() { core.invoke('node_status').then(... setTimeout(poll, 400) ...) }
```

`invoke` 一旦不返回，`poll()` 就**再也不会被调度** —— 既不报错、也不推进。
**轮询循环必须有独立于被调方的心跳**，而单次调用（`withTimeout`）与轮询是两回事。
这解释了用户看到的「卡住且不报错」：不是探测慢，是**轮询自己停摆了**。

修法：
- `candidate_summary()` 改为**只读缓存**（由探测线程在开始时写入），命令路径只读一个 `String`；
- 前端**每次**查询都包 `withTimeout`，单次无响应则继续轮询到总预算耗尽再给出口；
- `stepNodeWait` / `stepGuardReady` 的轮询同样加独立心跳。

#### 二、平台适配逐项核对（真实读码 + 实测，非推测）

**① macOS `.pkg` 的判定标签与产物语义不一致（同类缺陷）**

代码用 `osx-arm64-tar` 标签判定，却下载 `.pkg` —— 靠两者恰好都存在而**侥幸可用**。

实测（解包官方 `node-v24.21.0.pkg`）：payload 中同时含 x86_64 与 arm64 两个 Mach-O 切片
（fat 二进制），**确证 .pkg 是通用包**。又逐版本核对官方 `index.json` 的 `files[]`：

```
osx-x64-pkg    —— 所有 LTS 版本都存在（通用 pkg 的标签）
osx-arm64-pkg  —— 从不存在
osx-arm64-tar  —— 存在，但那是 tarball 的标签
```

已改为 `osx-x64-pkg`，使判定与产物一致。

**② Linux 标签硬编码 `linux-x64`（同一类缺陷，审计新发现）**

arm64 上 `platform_file()` 返回 `linux-arm64.tar.xz`，标签却是 `linux-x64`。
它能通过 `has` 检查只因「x64 标签恰好在 files[] 里」—— 若某版本只有 x64 而无 arm64，
代码仍会判定可用，随后去下载不存在的文件（404）。已改为按架构给出。

**③ Windows arm64 无官方 msi（如实记录限制）**

官方 `files[]` **没有** `win-arm64-msi`（只有 `win-arm64-7z` / `win-arm64-zip`）。
故 Windows arm64 只能装 x64 msi（依赖系统模拟执行）—— 已在代码中**明确记录**这是
有意折中，而非静默忽略。

**④ Windows 经 `cmd /C` 启动的引号不足以承受含空格路径**

`%APPDATA%` 含 Windows 用户名，而用户名**可以含空格**（如 "John Smith"）。
旧写法 `.args(["/C", path, "daemon"])` 会让 cmd 拆错 → 守卫启动失败且错误难解读。
已改用 `raw_arg` 给出 cmd 的经典双引号形式 `""<path>" daemon"`。

**⑤ Windows `schtasks /RL HIGHEST` 可能因权限被拒**

创建「以最高权限运行」的计划任务在非提权会话下可能失败。已改为**失败即降级重试**
（去掉 `/RL HIGHEST`）—— 守卫本身不需要管理员权限。
原则：**权限不足时应降级而非彻底失败**。

**⑥ Windows 落点硬编码 `C:\Program Files`**

真实路径随**系统盘符**与**系统语言**变化（中文系统的目录名被本地化），也可能在
`Program Files (x86)`。已改为经 `ProgramFiles` / `ProgramFiles(x86)` 环境变量推导，
与 `nodeprobe::known_locations()` 口径一致；同时补上 macOS Homebrew 落点 `/opt/homebrew/bin`。

**⑦ `guard_ready` 是同步命令且做网络 I/O**

同步命令在**主线程**执行：最多 400ms TCP 探测 + 3 秒 HTTP 往返，而引导页每 500ms
轮询一次、最多 40 次 —— 合计可占住主线程十几秒，界面**无法重绘**。已改为 async + 阻塞线程池。

**⑧ 三处 `TcpStream::connect` 没有连接超时**

`connect` **无超时**：端口被防火墙 DROP（而非 REJECT）时会等到 OS SYN 重试耗尽
（Windows 默认 20+ 秒）。已统一为 `connect_local()`（`connect_timeout` 800ms）。
**原则：不能依赖「回环地址正常时很快」来省略上限。**

**⑨ `--env-plan` 的候选摘要打印顺序错误**

摘要由探测线程写入缓存，在 `status()` **之前**读取必然为空 —— 诊断输出出现空白，
易被误读为「没有候选」。已调整为先探测后打印。

#### 三、门禁（防同类问题再犯）

新增 B40–B46：

| 断言 | 内容 |
|---|---|
| **B40** | **命令路径不得枚举候选**（摘要必须来自缓存）—— 直接针对本次回归 |
| **B41** | **轮询循环必须有独立心跳**（每次查询都要包超时） |
| B42 | macOS 标签必须与 `.pkg` 产物语义一致 |
| B43 | Windows `cmd /C` 引号必须能承受含空格路径 |
| B44 | 本地 TCP 必须用 `connect_timeout`，且 `guard_ready` 必须 async |
| B45 | 平台标签必须**按架构**给出（不得硬编码 x64） |
| B46 | Windows 路径必须来自环境变量（不得硬编码） |

#### 验证

- 壳测试 **57 项全通过**（bootstrap_flow 45 + updater_artifacts 6 + 单元 6）；
- release 形态编译干净；
- `--env-plan` 候选摘要正常显示（14 个候选），连续 3 次稳定 88–89ms。


## [1.0.5]（2026-09-11）

### 修复（架构级）：全壳审计 —— 「无界阻塞 + 被 await」同一模式另有 6 处

用户要求顺着环境探测的根因，深度排查整个桌面壳是否还有同类（逻辑/架构）缺陷。
把根因抽象为可检索的模式 —— **① 无界阻塞调用 ② 被命令 await ③ 失败被静默吞掉** ——
逐类扫描全部源码后，确认同一模式**另有 6 处**，其中 2 处在引导关键路径上。

#### 新增公共设施 `src/bounded.rs`

审计发现「无界执行」是**分散潜伏**的：service.rs / main.rs / node.rs 各写各的
`.output()`。故提取公共有界执行器，**所有**外部命令一律经它执行：

- 输出重定向到**临时文件**而非管道（管道不读取会在填满 64KB 缓冲后死锁）；
- 轮询 `try_wait` + 超时 kill（std 无跨平台 wait-with-timeout）；
- Windows 加 `CREATE_NO_WINDOW`（GUI 调控制台程序不弹黑框）；
- 自带单测：成功路径 / 超时必须被 kill / 不存在的二进制返回 Err 而非 panic。

#### 六处同源缺陷与修复

**① 服务管理器命令全部无界（致命 —— P0 已在关键路径上）**

`service.rs` 的 `systemctl --user daemon-reload/enable`、`loginctl enable-linger`、
`launchctl bootstrap`、`schtasks /Query /Create`，以及 `main.rs` 的
`start_guard_service` / `stop_guard_service` / `taskkill` —— **全部**用 `.output()`。

而它们在**建立服务定义 → 启动守卫**这条唯一通道上。systemd 在 dbus 会话异常、
systemd 无响应时会长时间挂起 → `ensure_guard` 永不返回 → 引导页永久停在
「正在启动守卫…」。**与环境探测卡死是同一根因，只是发生在下一步。**

**② `guard_start` 无外层超时（致命）**

```rust
// 旧：spawn_blocking(ensure_guard).await   ← 无超时
```

而前端调用它时是**裸 invoke**（无 `withTimeout`）—— 两侧都没有界。
现：Rust 侧加 `tokio::time::timeout`（180 秒）+ 前端加 `GUARD_START_BUDGET_MS`（200 秒）。

**③ `guard_start` 全程静默（UX 缺陷，会被误判为卡死）**

`ensure_guard` 最长可耗时约 2 分钟（服务管理器 30s + 兜底 spawn 后 60s），
而这段时间前端只有一句静态的「正在启动守卫…」。
**静默等待与卡死无法区分** —— 用户会误判并强杀进程，从而错失本可成功的启动。
现每个阶段经 `guard_progress` 事件上报，前端实时显示。

**④ `core_status` 是同步命令且在**主线程**执行二进制**

`fn core_status`（非 async）→ Tauri 在**主线程**执行 → 内部 `locate_core` 会
**逐个候选执行内核二进制**（每个 10 秒上限）取版本做仲裁，且随后又对选中项
再执行一次。候选一多（PATH + npm 目录 + 资源目录）即把主线程占住数十秒 ——
界面完全无响应。现改为 async + 阻塞线程池，并让 `locate_core` 一并返回版本，
消除重复执行。

**⑤ 托盘菜单在 UI 线程做网络 I/O**

`on_menu_event` 由 UI 线程派发，而分支里直接调 `post_local`（最长阻塞 60 秒）。
守卫挂起或端口无响应时，点击「启动/停止/重启」会**把整个界面冻结 60 秒** ——
用户看到的是「点了没反应」。同样地，「退出」在 UI 线程做完整退出握手（最坏约 70 秒），
会被感知为「程序关不掉」而强杀，从而**跳过退出握手、留下未停的 DSH**。
现两者均派发到独立线程。

**⑥ 内核候选定位未过滤可能阻塞的路径**

`locate_core_candidates` 的 `is_file()` / `canonicalize()` 会触网 —— 在断开的映射盘
或 UNC 上可能阻塞数十秒，而该函数在**内核定位的关键路径**上（引导页每步都用到）。
已在 `env.rs` 修过同类问题（PATH 探测），但此处漏修。现复用 `is_local_fixed_dir`。

#### 另两处隐患（非阻塞类）

**⑦ 互斥锁中毒后全部命令永久 panic**

6 处 `.lock().unwrap()`：任何线程在持锁期间 panic → 锁**永久中毒** →
此后**所有**命令在加锁处 panic。用户看到的是「重启也没用、功能永久失效」。
锁内是普通状态快照（不承载跨字段不变式），中毒后仍可用，故改为
`.unwrap_or_else(|e| e.into_inner())`。原则：**一次 panic 不应让功能不可恢复地失效。**

**⑧ `now_iso()` 为取时间执行外部进程，且 Windows 返回空串**

Unix 上 spawn `date`（无界、且系统可能没有该命令）；**Windows 分支直接返回空串**，
使 `installedAt` 在 Windows 丢失、跨平台行为不一致。
现改为纯 std 计算（Howard Hinnant civil-from-days 算法），三平台一致、无副作用。

#### 系统性门禁（本次审计最重要的产出）

逐个修完还不够 —— 缺陷之所以能**分散潜伏**，正是因为缺少系统性检查。
故新增 8 条架构门禁，其中 B32 是**全量源码扫描**：

| 断言 | 内容 |
|---|---|
| **B32** | **任何** Rust 源码都不得出现裸 `.output()` / `.status()`（豁免执行器自身） |
| B33 | `guard_start` 必须两侧都有超时（Rust tokio + 前端 withTimeout） |
| B34 | 必须上报 `guard_progress` 且前端监听（静默等待 ≠ 卡死） |
| B35 | `core_status` 必须 async 且复用版本（阻塞工作不得留在主线程） |
| B36 | 托盘分支必须走 `spawn_local_post`（不得在 UI 线程做网络 I/O） |
| B37 | 内核候选定位必须过滤非本地盘 |
| B38 | 时间戳不得执行外部进程 |
| B39 | 互斥锁不得用裸 unwrap（中毒恢复） |

#### 验证

- 壳测试 **50 项全通过**（bootstrap_flow 38 + updater_artifacts 6 + 单元 6）；
- `--env-plan` 50ms 命中；`--service-plan --service-apply` 在干净 HOME 下真实建立 unit；
- 全量源码扫描确认**零**裸 `.output()`/`.status()` 残留。

### 修复（架构级）：环境探测被无界系统调用卡死 —— 1.0.3/1.0.4 两次修复都未触及根因

用户真机反馈：1.0.4 **仍然**卡在「检测环境」，诊断信息为
`node=unknown | shell=unknown | error=环境检测超时（探针无响应…）`，
并指出「完全没有自动适配最佳镜像源」。要求从**底层架构**找根因、从架构层解决。

#### 为什么前两次修复都没解决

前两次都在**加超时**（Rust 侧 5 秒/候选、20 秒全局；前端 45 秒）。
但根因有两层，加超时对两层都无效：

**根因一：探测内含无法被自身预算约束的阻塞系统调用。**

原实现的 5 秒/20 秒预算，只在**候选之间**、以及 `spawn` **返回之后**才被检查：

```rust
for dir in split_paths(PATH) {
    if started.elapsed() >= NODE_PROBE_TOTAL_BUDGET { break; }   // ← 只在循环头检查
    let cand = dir.join(node_exe());
    if is_usable_candidate(&cand) { ... }   // is_file()/metadata() —— 无上限
    if let Some(v) = node_version(&cand) { ... }  // spawn —— 无上限
}
```

- `is_file()` / `metadata()` 底层 `GetFileAttributesW`：断开的网络盘、部分过滤驱动下可阻塞数十秒；
- `Command::spawn()` 底层 `CreateProcessW`：应用执行别名、网络路径、杀软实时扫描下同样可长时间不返回。

**单个阻塞调用即可击穿全部预算 —— 那不是真正的上限，只是提示。**

**根因二：探测任务被 Tauri 命令 `await`，于是「探测阻塞」等价于「命令永不返回」。**

前端 45 秒超时只是**停止等待**，并不解除挂起：Rust 侧那条线程永久悬挂，
每次重试再添一条，而每次结果都一样（仍卡在同一个候选）—— 用户被**永久挡住**。
这正好解释了诊断串里 `node=unknown` **且** `shell=unknown`：两条命令都没回来。

#### 架构修复（分层）

**① 新增 `src/nodeprobe.rs` —— 有界探测运行时（治本）**

```
命令线程 ──recv_timeout(budget)──> 探测线程（分离）
                                      └─ 可能永久阻塞，但**与命令返回时间无关**
```

- `std::sync::mpsc` + `recv_timeout`：**命令的返回时间与任何系统调用无关**。
  这是唯一能给 `CreateProcessW` / `GetFileAttributesW` 加界的手段；
- 状态机 `Idle / Running / Done`，**只允许一个在飞探测** —— 重试不再堆积线程；
- `STALE_AFTER`（90 秒）：阻塞若**永久**挂起，允许重启探测，
  避免「探测能力被永久剥夺」（代价受用户重试次数限制）；
- 结果缓存 + `invalidate()`：装完 Node 后必须能重新发现；
- `current_stuck()`：**记录当前卡在哪个候选、已多久** ——
  环境特有根因无法靠读代码确定，这是唯一可靠的定位手段。

**② 候选来源改为「廉价优先」**

顺序从「猜 PATH」改为：

```
① runtime.json 记录过的路径   ← 一次本地读（我们**装过**，却从未回读）
② 已知安装落点                ← 少数几次本地 stat（含 nvm/volta/fnm/scoop 等版本管理器，取最高版本）
③ PATH 扫描（最后）           ← 执行外部二进制是最贵也最危险的探测方式
```

这同时修掉一个隐藏缺陷：壳安装 Node 后写了 `runtime.json`，但**从未回读** ——
于是每次启动仍去猜 PATH，也就可能出现「装了却找不到」。

**③ PATH 扫描有界 + 跳过可能阻塞的路径**

- 加 `PATH_SCAN_BUDGET`（10 秒快速失败）；
- Windows 上跳过**非固定盘与 UNC**（`GetDriveTypeW` 判定，自身不触网）——
  从根上避开「断开的映射盘导致 `GetFileAttributesW` 挂起」这一整类问题。

**④ `node_status` 改为纯本地 + 可轮询（治本的另一半）**

- 等待预算 ≈900ms，未完成即返回 `probing=true`，由引导页轮询；
- **移除其中的网络 I/O**：原实现顺带触网查最新 LTS，使「本地环境判定」被网络质量左右 ——
  而这两件事在因果上毫无关系。网络侧改由独立的 `node_latest` 提供。

**⑤ `setup()` 不再同步探测**

`setup` 在**窗口创建之前**运行，同步探测挂起会**连窗口都推迟出现**，
使失败现象表现为「启动慢/无窗口」，与真正病因相距极远。改为仅触发探测（分离线程）。

**⑥ 镜像适配显式可见化（直接回应「完全没有自动适配」）**

镜像探测此前藏在下载内部，用户无从得知「有没有选、选了谁」——
于是「没有自动适配」成为**无法证伪的观感**。现独立成一步：

```
正在测速并选择最佳镜像源…
镜像已选：npmmirror.com（311ms）· 目标 Node v26.8.2
```

并在诊断串中输出**逐源延迟**（`mirror_probes=`）。

**⑦ 环境失败不再是死胡同**

新增「跳过检测，直接安装 Node」出口 —— 仅在环境超时后出现。
存在理由：**下载 Node 不需要本机已有 Node**，故「检测不出来」不应该挡住用户。
另修正 `fail()`：不再把「超时」当网络问题（此前会误导性展开镜像设置）。

**⑧ 新增无头自检 `--env-plan`**

```
$ dsh-supervisor-gui --env-plan
平台          = linux
候选 13 个（记录 0 / 已知 5 / PATH 8）
完成          = true
耗时          = 50 ms
node          = v26.7.0 @ /usr/local/bin/node
逐候选追踪:
  [已知] /usr/local/bin/node 50 ms ok=true v26.7.0
```

卡住时也能看到「卡在谁、多久」——**Windows 上若再次出问题，请把该命令输出发我**。

#### 本机实测

```
--env-plan     完成=true  耗时=50ms  经「已知落点」命中（未走 PATH）
--node-plan    镜像选择与 10 个源的延迟全部可见；跨源取最高版本后选中 npmmirror
```

#### 双仓隔离收尾（本次顺带修正）

壳仓从内核仓目录移出后，`bootstrap_flow.rs` 中两条**跨仓断言**（经 `../../` 读取内核的
`bin/dsh-supervisor` 与 `src/platform/config.js`）自然失效 —— 这正是隔离应当暴露的问题。
等价覆盖已迁至**内核仓** `test/package-root-test.js`（P1/P2 系列）：
**断言应与它验证的代码同仓**，而不是靠目录布局巧合成立。

#### 回归防护
`bootstrap_flow.rs` 增至 **30 断言**，新增 B26–B31：
- B26 必须有界探测运行时（`recv_timeout` + 分离线程 + 陈旧判定 + 缓存失效 + 追踪）；
- B27 `node_status` 必须可轮询且纯本地（回传 `probing`/`stuck`/`trace`，预算短，网络分离）；
- B28 PATH 扫描必须有界且过滤非固定盘；
- B29 引导页必须轮询且提供跳检测出口；镜像选择必须显式可见化；
- B30 `setup` 不得同步探测（否则推迟窗口创建）；
- B31 诊断串必须含 `env_trace`/`env_stuck`/`env_candidates`/`mirror_probes`。

## [1.0.4]（2026-09-11）

### 新增：守卫服务定义自检入口（P0 修复的可诊断性）

P0（壳自持服务定义）是本版最关键的修复，但若它在某台机器上失败，用户会卡在「守卫就绪」
却**无从自查** —— GUI 进不去、日志分散、也没有命令行入口。

故新增两个无 GUI 自检入口（任何平台可用）：

```
dsh-supervisor-gui --service-plan                # 只报告，不写盘
dsh-supervisor-gui --service-plan --service-apply # 实际建立服务定义
```

输出服务定义路径、是否现存、守卫可执行文件定位结果；`--service-apply` 时实际建立并报告结果。

另新增 `DSH_GUARD_BIN` 环境变量覆盖守卫路径 —— 用于①自动定位失败的机器做诊断 ②测试隔离 HOME。

**功能验证**（干净 HOME + 真实守卫路径）：

```
平台          = linux
服务定义路径  = ~/.config/systemd/user/dsh-supervisor.service
现存          = 否
守卫可执行    = /home/bowen/.npm-global/lib/node_modules/@dsh-sup/dsh-core-linux-x64/bin/dsh-supervisor
守卫存在      = 是

建立结果      = 已建立并启用 ~/.config/systemd/user/dsh-supervisor.service
建立后现存    = 是
```

生成的 unit 正文（`ExecStart` 指向真实守卫路径）：

```ini
[Unit]
Description=dsh-supervisor - DSH lifecycle guard
After=network.target
StartLimitIntervalSec=600
StartLimitBurst=3

[Service]
Type=simple
ExecStart=/home/bowen/.npm-global/lib/node_modules/@dsh-sup/dsh-core-linux-x64/bin/dsh-supervisor daemon
Restart=always
RestartSec=5
KillMode=process

[Install]
WantedBy=default.target
```

幂等性验证：第二次执行返回「已存在」，不重复写入。
`--service-plan`（不带 `--service-apply`）**不写盘**（已断言）。

**这同时修复了一个结构性问题**：`locate_core_candidates` 原先要求 `AppHandle`，
导致无 GUI 场景无法复用同一套内核定位逻辑。现改为 `Option<PathBuf>` 资源目录参数，
CLI 路径传 `None` —— 保证「自检」与「运行时」走**同一套路径推导**，避免自检通过但运行时找不到。

回归防护：B24（自检入口存在）、B25（无 AppHandle 可定位）。

本次为**架构级修复 + 镜像适配**，按用户要求逐项确认后才构建。

### 新功能：壳内镜像源适配（三条下载链路全覆盖，用户要求）

#### 问题（用户指正的核心）

> 「你根本在安装完壳之后去做的所有的动作，它是没有镜像源的，它是没有内核的。
>  你要理解整个工程的逻辑，在初始安装完壳的那一瞬的时候，里面是没有内核的。」

**装机那一刻机器上没有内核**（内核是随后由壳自己安装的），因此壳的每一处下载都**不能**
依赖内核的 `registry.json`。审计确认改造前壳的三条下载链路几乎没有镜像适配：

| # | 链路 | 改造前 |
|---|---|---|
| ① | Node 运行时（index.json + 安装包 + SHASUMS） | 2 源硬编码，**官方优先、仅报错才回退**，无测速/配置/UI |
| ② | 内核 npm 包（元数据 + `npm install -g`） | 4 源但**串行先到**，`registry.json` 缺失时用内建默认 |
| ③ | 壳自更新（清单 + 安装包） | 2 源**编译期写死**于 tauri.conf.json |

关键后果：`nodejs.org` 在受限网络下常常**可达但极慢**，于是永远不会回退镜像 ——
而安装包有 30–90 MB（macOS `.pkg` 实测 89.4 MB），用户要等到超时才失败。
（注：上一轮把下载超时从 60 秒放宽到 15 分钟后，该缺陷的代价从 1 分钟变成 15 分钟。）

#### 实测支撑（决定了实现方式）

| Node 镜像 | index | 版本新鲜度 | SHASUMS | .pkg |
|---|---|---|---|---|
| npm 官方 | ✅ | v24.21.0 | ✅ | ✅ |
| npmmirror | ✅ | v24.21.0 | ✅ | ✅ |
| 华为云 | ✅ | v24.21.0 | ✅ | ✅ |
| 腾讯云 | ✅ | **v24.20.0（滞后一版）** | ✅ | ✅ |

→ 腾讯云滞后一版，故必须「**跨全部源取最高版本**」；若「首个成功即采用」会静默装到旧版。

#### 实现

**新增 `src/mirror.rs`（壳自持镜像模块）**：
- `NODE_PRESETS` / `NPM_PRESETS` / `SHELL_PRESETS` 三组预设（全部实测可用）；
- `~/.dsh/shell/mirrors.json` 壳自持配置（装机即可用，**不依赖内核**）；
- `probe_all()` 用 `std::thread::scope` **并行**探测（零新增依赖）；
- `export_to_kernel()` 把 npm 偏好导出为内核 `registry.json`，让内核**继承**同一份选择
  （若内核已进入 `manual` 模式则不覆盖——尊重用户在面板里的显式选择）；
- 测速缓存 TTL 30 分钟（与内核 `selectRegistry` 一致）。

**① Node 下载（`node.rs` 重写）**：并行探测全部镜像 → 跨源取**最高 LTS** →
在提供该版本的源中选**延迟最低**者下载 → SHASUMS256 强校验，校验失败换源重试。

**② 内核 npm（`core.rs` 重写）**：`registry_origins()` 优先级改为
「内核 manual > **壳自持配置** > 内核 auto 列表 > 内建默认」；
`latest_version()` 由串行先到改为**并行 + 跨源取最高**。

**③ 壳自更新（`main.rs`）**：用 `UpdaterBuilder::endpoints()` **运行时覆盖**编译期端点，
使更新源可在不重新编译的前提下切换。

**引导页镜像自助入口**（新增 `mirror_status` / `mirror_set` 命令 + UI）：
- 壳装机时面板（`RegistryCard`）不可用，用户若遇镜像不可达将**没有任何出口**——这是可用性缺口；
- 故引导页在**失败态**提供镜像设置：显示各镜像实测延迟、可手填 Node/内核镜像、保存后自动重试；
- **默认隐藏**（不干扰普通用户），仅当失败信息含网络/镜像/超时/下载等关键词时自动展开。

#### 实测验证

干净 HOME 下运行 `--node-plan`（等价首启）：

```
mirror_selected=https://mirrors.huaweicloud.com/nodejs   ← 自动选中最低延迟
mirror_probe=https://mirrors.huaweicloud.com/nodejs      ok=true latency_ms=276
mirror_probe=https://npmmirror.com/mirrors/node          ok=true latency_ms=278
mirror_probe=https://mirrors.cloud.tencent.com/nodejs-release ok=true latency_ms=356
mirror_probe=https://nodejs.org/dist                     ok=true latency_ms=1458  ← 最慢
```

生成文件：`~/.dsh/shell/mirrors.json`（壳自持）+ `~/.dsh/supervisor/registry.json`（导出给内核）。
两次运行分别选中 npmmirror 与华为云 —— 证明是**真实测速**而非写死。

#### 回归防护
`tests/bootstrap_flow.rs` 增至 **21 断言**，新增：
- B18 壳必须自持镜像适配（三组预设 + 并行探测 + 缓存 + 导出内核 + 保护 manual）；
- B19 三条下载链路都必须接入镜像（不能只做一处）；
- B20 引导页必须有失败态镜像自助入口，且**默认隐藏**；
- B21 不得再出现「并发尝试」这类与实现不符的失真注释。

### 修复：守卫服务定义从未被建立 —— 全新机器上流程必然断裂（架构级，用户质疑驱动）

#### 问题（本次审计最严重的发现）

**旧设计的死锁**：壳只**启动**服务（`systemctl --user start` / `launchctl kickstart` /
`schtasks /Run`），把「服务定义的建立」留给内核的「所有者」语义。该设计在**首次安装**场景下必然失败：

1. 首次启动时**唯一的在场组件是壳** —— 内核此时可能尚未安装；
2. 内核的 `install` 子命令**不会被任何环节自动调用**（壳只执行 `npm install -g`）；
3. 且 npm 发行包**不含** `systemd/`、`desktop/` 模板（发布 `files` 字段未包含），
   即便调用 `install` 也会打印「跳过系统服务部署」后直接返回；
4. 于是服务定义从未建立 → 壳的 start 必然失败 → 引导卡在「守卫就绪」。

**实测证据**：
- 已发布 npm 包内 `package/systemd/` 与 `package/desktop/` 均为 **0 个文件**，无 `postinstall`；
- 在干净 HOME 下实跑内核 `install`：打印「未找到 systemd 模板…跳过系统服务部署」后返回，
  之后 `~/.config/systemd/user/` 为空；
- 壳仓**全部历史**中：写 systemd unit（`WantedBy`/`ExecStart`）**0 个提交**、写 LaunchAgent
  （`RunAtLoad`）**0 个提交**、写 schtasks（`/Create`）**0 个提交** → **不是回归，是从未通过**。
  本机能跑只是因为曾从**源码仓**手工跑过一次 `install`。

**Windows 另有一层错位**：`DSH-Supervisor` 计划任务指向的是 **GUI 壳**，而壳的
`start_guard_service()` 执行 `schtasks /Run /TN DSH-Supervisor` —— 于是「启动守卫」实际只是
再开一次壳（被单实例插件折回焦点），**守卫永远不会被启动**。

#### 修复

**新增 `src/service.rs`**：壳自持三平台服务定义（幂等，已存在则跳过）。
理由是结构性的 —— 壳是首启时唯一在场的组件，服务定义必须在任何东西启动守卫之前存在。

| 平台 | 建立方式 | 说明 |
|---|---|---|
| Linux | 写 `~/.config/systemd/user/dsh-supervisor.service` | **模板内嵌**（不再依赖外部文件）→ `daemon-reload` + `enable` + `enable-linger` |
| macOS | 写 `~/Library/LaunchAgents/com.dsh.supervisor.plist` | `RunAtLoad` + `KeepAlive` → `launchctl bootstrap` |
| Windows | 创建计划任务 `DSH-Supervisor` | 指向**守卫守护进程**；用包装 `.cmd` 规避 `/TR` 引号转义地狱 |

**`ensure_guard` 重写为三段**：建立定义 → 请求服务管理器启动（等 30s）→
**spawn 兜底**（服务管理器不可用时直接拉起守护进程，再等 60s）。

spawn 兜底需**放宽**原设计约束「壳绝不直接 spawn 守卫」：该约束的理由是避免产生游离于
服务管理器的第二实例，但它不能凌驾于**可用性**之上 —— 容器、无 user systemd session、
`launchctl` 被策略拦截、`schtasks` 被组策略禁止等场景下服务管理器根本无法使用，
无兜底则用户被永久挡在门外。第二实例风险由「spawn 前已确认端口不存活 + 以端口就绪为唯一成功判据」规避。

**Windows 任务名职责分离**（内核侧 `autostart.js` 同步修改）：

```
DSH-Supervisor          -> 守卫守护进程（壳建立；壳的 /Run 指向它）
DSH-Supervisor-GUI      -> 登录时打开桌面壳（面板 autostart 开关管理）
DSH-Supervisor-Watchdog -> 每 5 分钟保活（崩溃自拉）
```

关闭 autostart 时**不删除**守卫任务（它是服务定义），只 `/DISABLE` 其开机自启语义。

### 修复：macOS 上「运行环境」自动安装必然失败（格式不匹配）
- **根因**：`platform_file()` 下载 `node-v<ver>-darwin-<arch>.tar.gz`（tarball），
  却交给 `installer -pkg` 执行 —— 格式不匹配，必然失败。
- **修复**：改用官方 **`.pkg`**（`node-v<ver>.pkg`，实测通用包，arm64/x64 通用）。
  已验证官方可达（HTTP 200）且 `SHASUMS256.txt` 含其条目。
  （注：npmmirror 镜像的 SHASUMS 不含 `.pkg` 条目，故 macOS 实际依赖官方源；官方为主源，可接受。）

### 修复：Node 安装包下载超时过小（30-90 MB 却只给 60 秒）
- `ureq` 的 `timeout()` 覆盖**整次调用**（含响应体读取），而安装包体积：
  Linux 30.4 MB / Windows 31.7 MB / **macOS .pkg 89.4 MB**。
- 原值 60 秒在网络稍慢时必然超时，表现为「看似网络问题」实为超时配置过小。
- 修复：提到 **15 分钟**。

### 修复：Node 最低门槛被计算却从未生效
- 后端一直回传 `minOk`（DSH 要求 Node >= v22.12），但**前端从未使用** ——
  用户装了旧版 Node（如 v18）时流程照常放行，直到内核真正启动才失败，现象离根因很远。
- 修复：前端消费 `minOk`，不达标即触发升级；并回传 `minRequired` 供提示显示（避免前端硬编码漂移）。

### 修复：npm 发行态的包根解析 off-by-one
- `bin/dsh-supervisor` 会被 esbuild 打成 `core.cjs` 的 178 字节 launcher `require` 执行，
  此时 `__dirname` = **包根**（core.cjs 旁），而旧实现硬写 `path.join(__dirname, "..")` →
  指向**包外**：systemd/desktop 模板路径错位、`BIN_PATH` 指向不存在的文件。
  （干净 HOME 实测：`~/.local/bin/dsh-supervisor -> <pkg>/dsh-supervisor`，该文件不存在。）
- 修复：从 `__dirname` 逐级向上找含 `package.json` 的目录作为包根，两种形态均正确；
  `bin` 目标改为 `path.join(ROOT, "bin", "dsh-supervisor")`。

### 修复：内核 --version 探测无界（与「检测环境卡死」同类）
- `core::installed_version()` 用 `Command::output()` **无限阻塞**，且 `locate_core` 会对
  **每个候选**都调用一次 → 任一候选不可执行（损坏 shim / 被安全软件拦截 / 架构不符）即永久卡住。
- 修复：复用既有的有界执行器（10 秒上限）。

### 修复：Windows .cmd 垫片导致 --prefix 丢失
- npm 全局垫片位于 `%APPDATA%\npm\dsh-supervisor.cmd`，路径**不含 `node_modules` 段**，
  故 `global_prefix_for` 返回 None → `install_version` 丢失 `--prefix`，
  可能装到 npm 默认前缀而非内核当前所在前缀（旧内核遮蔽新内核）。
- 修复：① `global_prefix_for` 增加「父目录含 `node_modules` 即前缀」的兜底；
  ② `locate_core_candidates` 优先加入包内真实脚本路径，使版本读取与前缀推导都正常。

### 回归防护
`tests/bootstrap_flow.rs` 增至 **17 断言**，新增：
- B13 壳必须自持三平台服务定义（且模板内嵌）+ spawn 兜底；
- B14 macOS 安装格式必须与 `installer -pkg` 匹配（`.pkg`）；
- B15 下载超时必须足够大（禁止 60 秒）；
- B16 前端必须消费 Node 最低门槛；
- B17 包根解析必须形态无关。


### 修复：卡在「检测环境」不动（Windows 真机，1.0.3 仍复现）

**根因（两层叠加）**：

1. **Windows 应用执行别名存根**：Windows 的 PATH 默认含
   `%LOCALAPPDATA%\Microsoft\WindowsApps`，其中的 `node.exe` 是**别名存根**（重解析点，
   指向 Microsoft Store），并非真实 Node。`env::find_in_path` 仅用 `is_file()` 判定即选中它。
2. **探测无超时**：`env::node_version` 用 `Command::output()` **无限阻塞** —— 执行该存根会尝试
   唤起 Store 并永不返回，引导页从此永久停在「检测系统环境…」。
   另：`node_status` 是**同步** Tauri 命令，由主线程执行，探测变慢时连 UI 一起拖住。

**修复（四层防护）**：
- `probe_system_node` **遍历全部候选**而非取第一个（PATH 靠前的坏候选不再掩盖后面可用的 Node）；
- 过滤 Windows 别名存根（`\WindowsApps`）与 0 字节文件；
- 有界执行：单候选 5 秒、整个 PATH 扫描 20 秒硬上限（超时即 kill，视为不可用）；
- `node_status` 改为 async + `spawn_blocking`，不再占主线程；
- 前端 `stepEnv` 补 `withTimeout`（45 秒）——此前只给桌面/内核步骤加了超时，**漏了第一步**，
  而第一步恰恰最容易卡。

### 修复：托盘右键不可用（Windows 真机）
- `on_tray_icon_event` 原先匹配 `Click { .. }`（**任意键、任意状态**）→ **右键**也会执行
  `show_main()`，把刚要弹出的右键菜单顶掉/抢走焦点。
- 同时 `show_menu_on_left_click(true)` 让左键也弹菜单，与「左键显示窗口」的预期冲突。
- 修复：左键抬起才显示窗口（`MouseButton::Left` + `MouseButtonState::Up`），
  右键交系统弹菜单（`show_menu_on_left_click(false)`）。
  注：上游文档明确 Linux 不支持该开关（菜单由桌面环境决定），属平台限制。

### 修复：Windows 窗口四周有「隐形框框」
- **根因**：Tauri 的 `shadow` 默认 `true`，官方文档明确写明：Windows 上 `true` 会让**无边框**
  窗口多出 **1px 白色边框**（Win11 还会加圆角）。我们的窗口是 `decorations:false`，正中此条。
- 修复：新增 **平台配置** `tauri.windows.conf.json`（Tauri 自动按平台合并），仅覆盖 `shadow:false`。
  已验证合并语义为 RFC 7386 JSON Merge Patch（对象递归、数组替换），
  故 `security.csp`、`withGlobalTauri`、更新端点等全部保留，仅 `shadow` 改变。
  未在 `tauri.conf.json` 直接改是因其对 Linux/macOS 同样生效，而那两个平台的 shadow 语义不同。

### 语义统一：步骤名与产品概念对齐

用户明确指出「桌面更新」是错误表达。步骤条现为：

```
检测环境 → 运行环境 → 桌面版本 → 内核版本 → 守卫就绪 → 进入控制面板
```

- 「桌面更新」→「**桌面版本**」（该步是"检查/对齐桌面版本"，不是"更新"这一动作）；
- 「进入面板」→「**进入控制面板**」；
- 其余文案同步（跳过按钮、检查中/超时/失败提示、下载进度、选择页标题、托盘菜单「显示控制面板」）。

### 回归防护
`tests/bootstrap_flow.rs` 增至 **12 断言**，新增：
- B9 步骤标签必须与要求语义逐字一致（防文案被改回）；
- B10 托盘必须区分左右键；
- B11 Windows 配置必须 `shadow:false` 且与基础配置**除 shadow 外完全一致**（防两处漂移）；
- B12 用户可见文案不得再出现「桌面更新」。

## [1.0.3]（2026-09-11）

### 修复：内核步骤同样无界 —— 与引导卡死属同一类缺陷（主动审查发现）

修完桌面更新的超时后，主动审查其余网络步骤，发现内核路径存在**同类且更隐蔽**的问题：

**① `npm install` 无限阻塞（Rust）**
- `core::install_version()` 用 `cmd.output()`，**没有任何超时** —— npm 因网络停滞或 registry
  无响应而挂起时，引导页永久停在「正在安装内核…」。
- 修复：改为有界执行（15 分钟上限，超时即 kill 并如实报错，供引导页重试/回退）。
- **实现细节（易踩坑）**：子进程输出重定向到**临时文件**而非管道。若用 `Stdio::piped()` 且不读取，
  npm 的冗长输出会填满约 64KB 的 OS 管道缓冲区，导致子进程阻塞 —— 本想修超时却引入死锁。
  临时文件无此风险，且超时后还能保留最后输出用于诊断。

**② 前端对内核步骤无兜底（bootstrap.html）**
- `core_plan`（多镜像回退查询）与 `core_apply`（安装）均未包 `withTimeout`。
- 修复：分别加 90 秒 / 17 分钟预算（后者覆盖 Rust 侧 15 分钟上限），超时明确失败而非无限等待。

### 回归防护
`tests/bootstrap_flow.rs` 新增 B8：断言前端两个内核预算常量与 `withTimeout` 包裹、
Rust 侧 `NPM_INSTALL_TIMEOUT` 与有界执行函数存在、且**代码中不再出现** `cmd.output()`。
（断言只检查代码行，注释中对旧实现的说明不算违规。）

### 验证
壳仓全部 Rust 测试通过：`bootstrap_flow` 8 项、`updater_artifacts` 6 项、内置单元测试 3 项。
## [1.0.2]（2026-09-11）

### 修复：桌面壳引导顺序错误导致首次启动卡死（Windows 真机实测）

#### 现象
用户在 Windows 真机安装桌面壳后，引导页**第一步就是「壳更新」并卡住不动**，无法进入产品。

#### 根因（两个叠加缺陷）

**① 步骤顺序设计错误**（用户直接质疑的点）

原顺序为：`壳更新 → 检测环境 → 运行环境 → 内核版本 → 守卫就绪 → 进入面板`。
把「桌面壳自更新」当成第一步的理由是「新壳才可能带有新的 Node/内核安装要求」，
**该前提不成立**：
- 环境检测与 Node 探测都是**本地**判定，与壳版本无关；
- 用户刚手动安装完桌面壳，此时壳本就是最新，再强制检查更新毫无意义；
- 壳更新是**网络**操作（最慢、最不可靠），放在首位意味着「最可能失败的操作挡住所有后续步骤」。

**② 网络请求无超时 → 永久挂起**

`tauri-plugin-updater` 的 `Config`（tauri.conf.json）**没有 timeout 字段**，只能在 Builder 上设；
而底层 `reqwest` **默认无总超时**。于是网络不可达/连接挂起时（本项目端点用 unpkg CDN，
在部分网络环境下连接会长时间停滞），`check()` 既不返回也不报错 → 前端 Promise 既不 resolve
也不 reject → `.catch` 不触发 → 页面**永久停在「正在检查桌面更新」**，且当时**没有跳过入口**。

#### 修复

**顺序重构**：`检测环境 → 运行环境 → 桌面更新 → 内核版本 → 守卫就绪 → 进入面板`。
本地、快、确定的检查在前；网络类更新放在环境就绪之后。

**多重超时兜底**（三层，任一层生效即不会卡死）：
- Rust：`check()` 用 `updater_builder().timeout(20s)` **加** `tokio::time::timeout` 外层兜底
  （reqwest 的 request timeout 不保证覆盖 DNS 等阶段）；
- 下载用 20 分钟长超时（大安装包 + 慢网）；
- 前端：`withTimeout()` 再包一层（检查 45s / 下载 5 分钟无进展即判定失败）。

**用户随时可跳过**：新增「跳过桌面更新，直接启动」按钮 —— 任何网络类步骤都必须有即时出口，
否则一旦底层挂起，用户除了杀进程别无选择。

**下载进度可视化**：此前进度回调体是空的（`let _ = (chunk, total);`），4MB+ 安装包在慢网下
长时间零反馈，用户无法区分「正在下载」与「卡死」。现每秒级上报已下载/总字节并按百分比显示。

#### 附带修复

- **Windows 上护栏账本失效**：`tauri-plugin-updater` 在 Windows 安装时执行
  `ShellExecuteW` 启动安装程序后**立即 `std::process::exit(0)`**，其后的代码永不执行。
  原实现把 `mark_pending()` 放在 `download_and_install()` 之后 → Windows 上「更新成功确认 /
  连续失败拉黑」机制**完全失效**。现改为**安装之前**记录。
- **`bump.sh --shell` 在开发工作流下必然失败**：该分支硬编码 `./src-tauri`，假设在壳仓根执行；
  而本项目的实际流程是在内核仓根执行（壳仓由 `export-shell.sh` 同步）。现同时支持
  `./src-tauri` 与 `./.shell-work/src-tauri`，并同步更新 `Cargo.lock` 中的包版本。
- **移除「门 0」这一内部概念**：它从未出现在任何需求中，是我自行引入的编号并直接暴露给了用户。
  已全部改为「桌面更新」等直白表述，并在回归测试中禁止其回到用户可见文案里。

#### 回归防护

新增 `src-tauri/tests/bootstrap_flow.rs`（7 断言），锁定：步骤顺序、HTML 步骤条与 JS `stepNames`
一致、引导从环境检测启动、网络步骤必有超时与跳过出口、下载进度已接线、Rust 侧超时与
`mark_pending` 顺序、用户文案不含内部概念。
