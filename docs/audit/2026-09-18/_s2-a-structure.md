# 壳仓审计 S2-A：结构与分层

> 只读审计。对象仓库 = `wasi7mglns/dsh-supervisor-launcher`（本报告所有路径均相对**壳仓根**）。
> 未做任何 git 写；未跑任何测试/构建；只看 read/grep/wc/node --check/git 只读。

## 0. 审计基线（重要：基线在委派后被移动过）

| 项 | 值 |
|---|---|
| 审计对象 | HEAD `0930884`，分支 `release/shell-1.1.8`，**工作树干净** |
| 提交内容 | 33 文件 +2476/−228：`release_channel.rs`(761) 新增、`core.rs`(+426)、`main.rs`(+117)、bootstrap/js 5 个、tests 3 个新增、docs 多个 |
| 基线说明 | 父代理委派时描述为「已发布 main(5df08aa) + 本地 00f872c + 27 未提交 + 6 未跟踪；release_channel.rs 在未跟踪列表」。实际执行时该批内容**已被提交为 `0930884`**（reflog：`checkout release/shell-1.1.8` → `cherry-pick e41c1d8` → `commit 0930884`）。内容一致，**本审计按 `0930884` 出结论**；`release_channel.rs` 已是版本库文件（`git ls-files` 命中），非未跟踪。 |

## 1. 模块划分与依赖方向

### 1.1 声明（文档）

- `src-tauri/src/commands/mod.rs:1-6`：`commands -> domain -> platform -> infra`；命令体**只做**校验→调 domain/platform→组装。
- `src-tauri/src/domain/mod.rs:1-12`：同向；`domain` 不得依赖 `commands`。
- `docs/DESIGN-SHELL-ARCHITECTURE.md:64-69`：`commands ──▶ domain ──▶ platform ──▶ infra`；**禁止反向**（`infra 不得依赖 domain；platform 不得依赖 commands`）。
- `src-tauri/src/main.rs:28-30`：`platform/` 是「全仓唯一的平台分支所在地」。

### 1.2 实际模块树（`src-tauri/src/`）

- 根：`main.rs` `error.rs` `bounded.rs` `core.rs` `env.rs` `bridge.rs` `mirror.rs` `nodeprobe.rs` `node.rs` `runtime_contract.rs` `core_contract.rs` `update.rs` `update_plan.rs` `release_channel.rs`
- `commands/mod.rs`（单文件 779 行）
- `domain/{mod,cli,coreloc,guardctl,localhttp,windowing}.rs`
- `platform/{mod,service,linux,macos,windows,unsupported}.rs`

### 1.3 逐文件出边（`crate::X` 引用，去注释后代码级）

| 文件 | 出边 |
|---|---|
| `core.rs` | bounded, domain, env, mirror, platform, release_channel, runtime_contract, update_plan |
| `commands/mod.rs` | bridge, core, core_contract, domain, env, error, **log(根)**, mirror, node, nodeprobe, platform, **run_install(根)**, runtime_contract, **shell_updater(根)**, update, update_plan |
| `domain/coreloc.rs` | core, core_contract, domain, env, platform, runtime_contract |
| `domain/guardctl.rs` | core, core_contract, domain, env, platform, runtime_contract, update |
| `domain/cli.rs` | domain, mirror, node, nodeprobe, platform |
| `release_channel.rs` | core, env, update |
| `node.rs` | env, mirror, platform, runtime_contract, update |
| `env.rs` | bounded, nodeprobe, platform |
| `nodeprobe.rs` | env, platform |
| `runtime_contract.rs` | env, node, platform |
| `mirror.rs` | core, env, update |
| `update.rs` | env, platform |
| `platform/*` | bounded, env, runtime_contract |
| `bounded.rs` | （无 `crate::` 出边，叶子） |

### 1.4 P1：模块依赖图不是 DAG —— 至少 6 组**双向循环**（门禁未覆盖）

| # | 循环 | 证据（代码行） |
|---|---|---|
| C1 | `core ↔ mirror` | `core.rs:178` `crate::mirror::load` / `core.rs:224,366` `crate::mirror::probe_all` ∥ `mirror.rs:295` `crate::core::package_name` |
| C2 | `core ↔ release_channel` | `core.rs:238` `crate::release_channel::select` / `core.rs:320,321` `crate::release_channel::CH_ROLLBACK/CH_CANARY` ∥ `release_channel.rs:74,86,91` `crate::core::is_valid_version/semver_cmp` |
| C3 | `core ↔ domain` | `core.rs:385` `crate::domain::coreloc::locate_core_candidates` ∥ `domain/coreloc.rs:62,63,83,119` `crate::core::installed_version/semver_cmp/package_name` |
| C4 | `node ↔ runtime_contract` | `node.rs:279,280` `crate::runtime_contract::NodeRuntime/derive` ∥ `runtime_contract.rs:112,114` `crate::node::MIN_NODE/now_iso` |
| C5 | `env ↔ nodeprobe` | `env.rs:145` `crate::nodeprobe::resolve` ∥ `nodeprobe.rs:423,451` `crate::env::recorded_node_path/node_exe` |
| C6 | `env ↔ platform` | `env.rs:34,43` `crate::platform::current()` ∥ `platform/mod.rs:103` `crate::env::state_root()`、`platform/windows.rs:24` `crate::env::api_port()`、`platform/macos.rs:190` `crate::env::supervisor_dir()` |

- 性质：Rust 允许模块互引，故**能编译、CI 绿**；但违反 §1.1 的「单向依赖」意图，且使「加一层/换实现」无法只改一处。
- C6 额外性质：`platform → env` 是相对 `... → platform → infra` 的**反向边**（文档只显式禁 `infra→domain` 与 `platform→commands`，未覆盖此向）。
- **最小修法**：把被双方共用的叶子事实抽成真正无出边的 `infra` 模块：版本语义（`is_valid_version/semver_cmp` → `infra/version.rs`）、包名/平台标签（`package_name/core_platform_tag` → `infra/pkg.rs`）、Node 常量/时间（`MIN_NODE/now_iso` → `infra/node_facts.rs`）、路径（`state_root/api_port/supervisor_dir` → `infra/paths.rs`）；再把 `core` 拆出「选版」与「定位」两个无环子模块。**并新增一条「模块依赖无环」门禁**（当前 G1/G2/G3/G5 都不查环）。

### 1.5 P1：命令层越过声明的分层，且依赖 **crate 根自由函数**

- 声明：`commands -> domain -> platform`（`commands/mod.rs:5`；`DESIGN-SHELL-ARCHITECTURE.md:66`）。
- 实际：`commands/mod.rs` 直接引用 `core, mirror, node, nodeprobe, runtime_contract, update, update_plan, env, core_contract`（业务模块），并调用**根自由函数**：
  - `commands/mod.rs:46` `crate::log(&st)` → 定义在 `main.rs:68`
  - `commands/mod.rs:171` `crate::run_install(&handle)` → 定义在 `main.rs:156`
  - `commands/mod.rs:637,687` `crate::shell_updater(&app, ...)` → 定义在 `main.rs:241`
- 即 `commands → main.rs` 是**由下向上的反向依赖**，与「main.rs 仅组装 + 入口」相悖。
- 门禁 G2（`tests/bootstrap_flow.rs:1615-1642`）**只**拦 `process::Command::new`，不拦上述越层；G3 只查 `main.rs` 行数与 `#[tauri::command]` 归属。故这是**未覆盖的架构偏离**。
- 最小修法：把 `log/run_install/shell_updater` 下沉到 `domain/`（如 `domain/shell_update.rs`、`domain/provision.rs`），commands 只调 domain；或把声明的方向图改成实际图并加门禁。两者取一，不允许「文档一套、代码一套」。

## 2. bootstrap/js 职责与装配序

装配入口 = `src-tauri/bootstrap/bootstrap.html`，以 `<script src>` **固定顺序**加载（`bootstrap.html:187-195`）：

`@
00-runtime → 10-ui → 20-env → 30-mirror → 40-shell-update → 50-kernel → 60-guard → 70-boot → 80-init
`@

- 文件自身注释声明了拆分理由与硬序（`bootstrap.html:184-186`）：「原为 802 行单块脚本：一处语法错会导致**全页不执行**；现每个文件独立语法检查（门禁 G5）；**80-init 必须最后加载**」。
- 实测各文件行数：`00-runtime 88 / 10-ui 157 / 20-env 173 / 30-mirror 78 / 40-shell-update 59 / 50-kernel 71 / 60-guard 66 / 70-boot 27 / 80-init 83`；装配按文件名字典序，与依赖序一致（前序注入全局、后序消费）。
- 引导页阶段固定五步（`bootstrap.html:129-139`）：检测环境 → 桌面版本 → 内核版本 → 服务就绪 → 进入面板。
- 逐文件职责边界 / 全局符号 / Rust 命令名对照见 §7（下级子代理结论）。

## 3. 与内核的边界（`docs/DESIGN-BOUNDARY.md` 声明 vs 实现）

### 3.1 结论：壳**没有**触碰内核运行期域（正向通过）

在壳 `src-tauri/src` 全量 grep `relay|frp|proxyInstance|providerApi|lan-daemon|router-daemon|instances\.json|sandbox` → **0 命中**。即壳未实现守卫监督循环、实例生命周期、relay/frp、端口仲裁、沙箱管理（这些按 R2 属内核运行期专有）。

### 3.2 声明职责的落地核对

| 声明 | 判定 | 实现证据 |
|---|---|---|
| D1 镜像目录/选择**壳拥有**、内核消费 `registry.json` | ✅ | `mirror.rs:198,203` `export_to_kernel(_with)`；写 `supervisor_dir()/registry.json`（`mirror.rs:207`），含 `catalog/selected/probe`（`mirror.rs:243-245`） |
| D2 安装内核**壳执行**（提权） | ✅ | `platform/*::install_node` / `core.rs` npm install（提权为平台专有） |
| D3 环境探测壳供给前、内核运行期自检 | ✅（壳侧） | `nodeprobe.rs` + `env.rs`（`MIN_NODE` 门槛） |
| D5 守卫服务**定义**归壳；壳**不是**守卫的所有者 | ✅ | `domain/guardctl.rs:1-6` 明写「壳不是守卫的所有者，只向 systemd/launchd/schtasks 提请求」；`platform/service.rs` 定义 |
| 壳自更新 | ✅ | `update.rs` / `update_plan.rs` / `release_channel.rs` |

### 3.3 P2：`DESIGN-BOUNDARY.md` 是 2026-09-11 快照，部分引用已过期

- 文首 `docs/DESIGN-BOUNDARY.md:5-9` 已声明「描述改造前状态、行号是快照」，诚实；但正文仍留**会误导的旧坐标**，例如 `DESIGN-BOUNDARY.md:85` 指向 `main.rs:1079`（实际 `main.rs` 仅 548 行）。
- 最小修法：把已过期行号改为「模块名 + 函数名」而非行号（与内核仓 R-5 口径一致），或整体移入「历史决策」段落。

### 3.4 反向（内核是否触碰壳职责）

- 不在本片对象内（内核在对照仓）。仅记：内核 `domains/shell/watchdog` 负责壳崩溃自愈（D5），属**有意**归属，非越界。

## 4. docs 结构与索引一致性

- `docs/README.md` 的「文档角色表」覆盖 `docs/` 下**全部 12 个 .md**（含 README 自身），无遗漏；**无指向不存在文件的悬空条目**（`grep AUDIT-SHELL-BOOTSTRAP|SHELL-STABILITY-AUDIT` → 0 命中）。
- **P2（可追溯性）**：壳的两份审计报告 `docs/AUDIT-SHELL-BOOTSTRAP.md`、`docs/SHELL-STABILITY-AUDIT.md` 已在 `a8fa376`（v1.1.0「全仓审计清理」）中**删除**，当前 `docs/` 无审计台账条目。与内核仓「审计报告留档于 design-notes」的口径不一致。
  - 最小修法：把审计报告归档到 `docs/audit/`（或 `design-notes/`）并在 `docs/README.md` 增设「审计记录」小节；**不要**以「清理」为名删证据。
- **P2（文档↔代码漂移）**：`docs/DESIGN-SHELL-ARCHITECTURE.md:36-62` 的目标树与实现不符：
  - 声明 `infra/{bounded,fs,net,proc}.rs`、`commands/{env,node,core,mirror,shell,window}.rs`、`domain/{probe,provision,mirror,update,contract}/`；
  - 实际 `bounded.rs` 在根、无 `infra/`；`commands/` 只有 `mod.rs`(779 行)；`domain/` 是 `cli/coreloc/guardctl/localhttp/windowing`。
  - 文档头 `:7` 声称「分层...已落地」，但落地的是「依赖方向意图」，**目录树并未落地**。最小修法：把 §2.1 标为「目标态（未落地）」或改成实际树。

## 5. 问题清单汇总

| 级别 | # | 问题 | 证据 | 最小修法 |
|---|---|---|---|---|
| **P1** | S2A-1 | 模块图非 DAG，≥6 组双向循环 | §1.4 C1–C6 | 抽 `infra` 叶子事实 + 新增无环门禁 |
| **P1** | S2A-2 | 命令层越层（commands 直连 core/mirror/node/…）+ 依赖 crate 根自由函数 | §1.5；`commands/mod.rs:46,171,637,687`；`main.rs:68,156,241` | 下沉 `log/run_install/shell_updater` 到 domain；或改声明并加门禁 |
| **P2** | S2A-3 | `main.rs` 548 行，G3 上限 550，**仅剩 2 行余量** | `main.rs` 实测 548；`tests/bootstrap_flow.rs:1661` | 拆 `setup()` 的窗口/托盘装配（`bootstrap_flow.rs:1652-1654` 已把它列为后续）或显式上调上限 |
| **P2** | S2A-4 | `DESIGN-SHELL-ARCHITECTURE.md §2.1` 目录树与实现漂移；`main.rs` <150 目标 vs 实际 548 | §3.4 证据行 | 标注目标态或改实际树 |
| **P2** | S2A-5 | 壳审计报告被删除，无审计台账 | `docs/` 无 AUDIT 文件；`git log -1 -- docs/AUDIT-SHELL-BOOTSTRAP.md` = `a8fa376` | 归档审计报告 + 索引 |
| **P2** | S2A-6 | `DESIGN-BOUNDARY.md` 旧行号（如 `main.rs:1079`）已失效 | `DESIGN-BOUNDARY.md:85` | 行号改符号名 |
| **P2** | S2A-7 | 两个同名 `log`：根 `main.rs:68` 与 `update.rs:43`；commands 用的是根函数 | §1.5 | 重命名（如 `update::log_line`） |
| **P2** | S2A-8 | 平台分支白名单例外 `bounded.rs` 合法但暴露 `platform→infra` 耦合 | `tests/bootstrap_flow.rs:1483-1486`；`bounded.rs:49-57` | 保持白名单；在架构文档「例外」处登记（当前只在测试注释里） |
| **P1** | S2A-9 | 前端「不支持自更新」分支是死代码（Rust 从不下发 `cannotSelfUpdate`） | §7.2；`40-shell-update.js:27`；`update.rs:81-87,146` | 改用 `identity.selfUpdateCapable` 或后端补键 |
| **P1** | S2A-10 | 前端下载预算 5min ≪ Rust 20min，无在途保护 → 重试并发安装 | §7.2；`00-runtime.js:29`；`commands/mod.rs:24`；`80-init.js:5-9` | 预算对齐 + in-flight 标志 |
| **P2** | S2A-11…18 | bootstrap/js 其余 8 项（core_apply 死参、_alignRetried 漏重置、重复声明、__error 吞错触发安装、轮询在途、updPlan.skipped、DOM 分散、skipAction 死状态等） | §7.3 逐条证据 | 见 §7.3 |

（P0：未发现。壳未触碰内核运行期域，边界正向通过。）

> 说明：S2A-1..8 为主控（Rust 模块/边界/文档）发现；S2A-9..18 为下级子代理（bootstrap/js）并入（见 §7）。

## 6. 门禁覆盖评估

| 门禁 | 覆盖 | 未覆盖（本片发现） |
|---|---|---|
| G1（`bootstrap_flow.rs:1480`） | 平台分支只在 `platform/`（白名单 `bounded.rs`） | — （实现与白名单一致） |
| G2（`bootstrap_flow.rs:1615`） | `commands/` 不得 `Command::new` | **不查**越层依赖与 commands→根 |
| G3（`bootstrap_flow.rs:1657`） | `main.rs` ≤550、无 `#[tauri::command]` | `main.rs` 余量 2 行（接近触红） |
| G5（`bootstrap_flow.rs:1693`） | 前端脚本逐个语法 + 错误上报 | 见 §7；**不锁**跨文件调用顺序（B53 `:1316-1349` 只查语法+被引用） |
| B4（`bootstrap_flow.rs:144-155`）/ B6（`:184-190`） | 超时常量**存在** | **不校验** JS 预算与 Rust 超时的**数值一致**（S2A-10 的缺口） |
| **无** | 模块**无环**、声明依赖方向、`pub` 面收敛、doc↔code 树一致、`main.rs` 余量、commands→根 | §1.4 / §1.5 / §4 / S2A-3 / S2A-7 |

## 7. 下级子代理结论（bootstrap/js 分片，id `eb159d45`）

> 下级子代理只读审计 `src-tauri/bootstrap/js/*.js`（9 文件/802 行），对照 bootstrap.html / tauri.conf.json / commands/mod.rs / main.rs；9 文件 `node --check` 全通过。证据行号由该子代理给出，主控已抽验一致。

### 7.1 正向通过

- 9 文件由 `bootstrap.html:187-195` 以经典 `<script>` 顺序同步加载；`frontendDist=bootstrap`（`tauri.conf.json:7`）；无第二份拼接/生成副本。
- 前端 19 个 invoke 命令名**全部**在 `main.rs:408` 的 `generate_handler!` 注册，无孤儿调用；Rust 已注册但引导页未用的 `kernel_update_apply/shell_bridge_contract/shell_state_root/shell_panel_url` 供面板/`shell.html`，非孤儿。
- 唯一全局符号 `window.__BOOT_NS`（`00-runtime.js:8`）；各文件 IIFE 取 `NS` 参数，无隐式全局。
- **未发现 P0。**

### 7.2 P1

| id | 问题 | 证据 | 最小修法 |
|---|---|---|---|
| **S2A-9** | 「不支持自更新」分支是**死代码**：前端判 `r.cannotSelfUpdate`，但 Rust 全仓从不下发该键；真正能力是 `identity.selfUpdateCapable`（`40-shell-update.js:18-19` 已存进 `NS.shellId` 却从未使用） | `40-shell-update.js:27`；`update.rs:81-87`、`:139,146`；`commands/mod.rs:526`；`commands/mod.rs:617-631`（`shell_plan` 无该键） | 条件改 `NS.shellId.selfUpdateCapable === false`，或 `shell_plan` 补 `cannotSelfUpdate` |
| **S2A-10** | 前端 shell 下载预算 5min ≪ Rust 命令超时 20min，且**无在途保护** → 重试与首次**并发**执行同一安装器 | `00-runtime.js:29` `300000`；`commands/mod.rs:24` `20*60`；`40-shell-update.js:43`；`10-ui.js:67-80`（超时不取消底层 invoke）；`80-init.js:5-9`（重试直接再调 `stepShellApply`） | JS 预算 >20min 或下调 Rust；加 `shellApplyInFlight` 在途标志 |

### 7.3 P2

- **S2A-11** `core_apply` 的 `version` 是死参数/桥接面不一致：`50-kernel.js:42` 传 `{version}`，`commands/mod.rs:272` 的 `core_apply(app)` 不收，`:285-287` 明写「不接受调用方指定版本」。
- **S2A-12** `NS._alignRetried` 不随 boot/重试重置：`60-guard.js:16-17` 置位，`70-boot.js:14-17` 重置时漏它，`80-init.js:37-40` 重试走 `NS.boot()` → 一次对齐失败后本会话不再自动对齐。
- **S2A-13** `10-ui.js:22-27` 同一 IIFE 内 `wait(ms)`/`hideFail()` 各声明两次（后覆盖前），`node --check` 不报、无门禁拦截。
- **S2A-14** `stepEnv` 未处理 `withTimeout` 的 `__error`：`20-env.js:19` → `10-ui.js:76-78` 归一为 `{__error}`，但 `stepEnv` 只查 `__timeout`/`probeError`/`probing`（`:22,33,40`），`:56` 无条件 `afterEnv`，`:82` 的 `!st.installed` 走到 `:88/:97/:110` 的 `start_node_install` → **IPC 拒绝被误判为「没装 Node」并触发安装**（违反「绝不吞错」）。
- **S2A-15** `stepNodeWait` 轮询无在途去重、兜底 10min < Rust HTTP 15min：`20-env.js:124-134,146`；`node.rs:10`。
- **S2A-16** `NS.updPlan.skipped` 后端从不返回（`10-ui.js:118`；`commands/mod.rs:617-631`；`update_plan.rs:73-84`）。
- **S2A-17** DOM/样式操作分散，未收敛到 `10-ui.js:30-33` 声明的「唯一文字出口」：`style.display` 直改见 `00-runtime.js:41`、`20-env.js:36,77`、`30-mirror.js:50,55`、`40-shell-update.js:5,11`、`60-guard.js:52`、`70-boot.js:18`、`80-init.js:12,21,31,38,43-44`。
- **S2A-18** 其它冗余/边界：`NS.skipAction` 全仓零读（`00-runtime.js:15`）；`NS.showUpdChoice` 与 `NS.showUpdRetry` 指向同一函数（`40-shell-update.js:55-56`）；`00-runtime.js:46` 直 `invoke('shell_set_phase')` 与 `10-ui.js:85-87` 的 `NS.phase` 形成两个上报入口（因加载更早，可接受但应注释锁定）。

### 7.4 装配序专项

- 顺序本身正确：`00-runtime.js:54-64` 最先注册 `onerror/unhandledrejection`（不变量 F2）；`80-init.js:82` 最后才 `NS.boot()`。
- 但**数字前缀 ≠ 依赖 DAG**：真实调用图是前向边（`10-ui.js:108`→`30`；`20-env.js:84`→`30`、`:163`→`40`；`40-shell-update.js:29,33`→`50`；`50-kernel.js:36`→`60`）。当前安全仅因所有跨模块调用都推迟到 `boot` 之后；前序文件若做加载期调用即引用未定义。
- B53（`tests/bootstrap_flow.rs:1316-1349`）只校验语法与「文件被 bootstrap.html 引用」，**不锁调用顺序**；B4（`:144-155`）/B6（`:184-190`）只断言超时常量**存在**、不校验与 Rust 的**数值一致**——这正是 S2A-10 长期存在的门禁缺口。
