# 内核启动规范（Kernel Launch Standard）

> **本文件是「壳如何把内核启动起来」的唯一事实源（SSOT）。**
> 内核是**跨平台产品**（linux-x64 / darwin-arm64 / darwin-x64 / win-x64），
> 本规范对**四个平台同一条流水线**，平台差异只允许出现在 `src-tauri/src/platform/`。
>
> 内核侧的对应契约（内核必须提供什么）见内核仓 `KERNEL-DAEMON-CONTRACT.md`。
> 二者互锁：任一侧不满足，启动即失败并**如实报出是哪一步**。

---

## 0. 硬规则（不可协商）

| # | 规则 | 理由（实测代价） |
|---|---|---|
| H1 | 内核包安装/升级**只有壳一个写入者**；内核从不自装 | 曾双写入者（内核自更新 + 壳），同一个全局 npm 包被两个进程写 |
| H2 | Node/npm/PATH 的**唯一来源**是 `runtime.json`（壳写） | 曾四处独立推导，nvm/GUI PATH 下 systemd 以 127 失败 |
| H3 | 内核**位置**的**唯一来源**是 `core.json`（壳写） | `locate_core` 只按 PATH+两个硬编码目录猜，nvm/自定义 prefix 下装了也找不到 |
| H4 | 启动前，磁盘内核版本**必须等于按发布通道契约选出的目标版本**（rollback > （名单内）canary > latest；仅 latest 缺失时兜底 versions 最高，**不是「全 tag 最高」**，见 `RELEASE-CHANNEL-CONTRACT.md` §3 / RC-1）；否则**先对齐再启动**，`guard_start` 不对齐即拒绝 | 「内核只有最新版本」是产品规则；否则会拉起磁盘上的旧内核，后半段逻辑全错 |
| H5 | 服务定义/启停的**唯一所有者**是壳（经平台服务管理器）；内核不自启、不自停、不自重启 | 曾多个启动器（systemd + 壳 spawn + Windows watchdog）并存 |
| H6 | 就绪判据 = `guardctl::Readiness` **四态**，唯一实现在 `domain/guardctl.rs::ready()`：`Ready`（`GET /healthz` 2xx）/ `PortClosed`（连不上）/ `Http(code)`（连得上却回非 2xx）/ `NoHttpResponse`（连得上却没走完 HTTP）。端口**每 tick 重取**内核 `ports.json` 的 `supervisor-api` 实际值 | 端口可被占用后顺延；盯固定端口会永远等不到已健康的守卫。四态必须**可分辨**：它们的结论完全不同（再等 / 去看日志 / 真失败），旧实现只回 `bool`，于是「healthz 回 500」与「端口都没开」在报错里是同一句话。探针问不出状态行时按 `None` 处理 → `NoHttpResponse`，**不得**糊成 `Http(0)`（那等于把「没回话」说成「回了 0」） |
| H7 | 平台分支**只**允许在 `src-tauri/src/platform/`；其余处经 trait | 门禁 G1 |
| H8 | 任何一步失败必须回传**结构化 stage + 证据**（搜索过的目录、命令、退出码） | 现场排障不靠猜 |
| H9 | 交给**外部工具**（node/npm/cmd/PowerShell/schtasks）的路径只经一个归一点 `platform::external_path`；壳自身路径只经 `platform::self_exe()`，取不到**必须报错** | Windows 的 `current_exe()`/`canonicalize()` 返回 `\\?\` verbatim 形态，外部工具不接受；旧仓里同时存在**两份规则不同**的剥前缀实现，且取不到路径时静默写空串 —— 定义一旦写坏就回读不到当时用的值，启动失败无从归因（2026-09-21） |
| H10 | 守卫子进程的 stdout/stderr **永不丢弃**：一律落 `<状态根>/shell/guard.log`（有界滚动） | 兜底拉起后「超时」却没有任何子进程原文，用户与排障者都无从判断守卫为什么起不来 |

> 本表编号只在本文件内有效（`RELEASE-STANDARD.md` 的 H0–H9 是另一套编号，跨文件引用请带文件名）。

---

## 1. 规范流水线（P0 → P6，四平台一致）

```
P0 运行期契约   resolve runtime      <状态根>/supervisor/runtime.json（node/npm/nodeBinDir/PATH）  壳写
P1 版本对齐     align to latest      resolve 通道选版（rollback>canary>latest，兜底 versions 最高）→ 与磁盘比较 → 需要则安装    壳写(唯一)
P2 位置落契约   record location      安装成功后写 core.json（bin/prefix/version/source）        壳写(唯一)
P3 定位         resolve aligned      读 core.json → 校验 bin 可执行 且 version==通道选出版本     壳读
P4 服务定义     ensure_defined       平台模板：只指向稳定入口 `<壳> --run-guard`（node/守卫每次启动重新检测）  壳写
P5 启动         start (owner)        服务管理器启动；**定义未建立则不出这一枪**，改走 spawn 兜底（输出落 guard.log）  壳发起
P6 就绪         readiness            `Readiness::Ready` = healthz 2xx（端口每 tick 重取 ports.json 的 supervisor-api 实际值）  壳读
```

**前置关系（硬）**：

- `P6 就绪` 前必须 `P1 已对齐`。绕过 P1 直接 P4/P5 = 拉起旧内核 = 违规。
- `P5 请求服务管理器启动` 的前提是 `P4 建立成功`。定义没建立就去 `/Run` / `systemctl start`
  必然失败（Windows 实测：退出码 1 + 码页乱码原文），而旧实现**照样发**这一枪，
  于是把「定义没建立」这一真实故障伪装成「启动失败」。现由 `ServiceAttempt` 承载阶段产物：
  定义失败 ⇒ `started = None` ⇒ 不出边，只走 P5 兜底，且 `evidence()` 如实说明是哪一段没走通。

---

## 2. 四平台规范矩阵

| 步骤 | Linux | macOS | Windows |
|---|---|---|---|
| 服务管理器 | systemd --user | launchd LaunchAgent | schtasks ONLOGON |
| 定义文件 | `~/.config/systemd/user/dsh-supervisor.service` | `~/Library/LaunchAgents/com.dsh.supervisor.plist` | 计划任务 `DSH-Supervisor` |
| 启动入口（四平台统一） | `ExecStart="<壳>" --run-guard` | `ProgramArguments=[<壳>, --run-guard]` | `/TR="<壳>" --run-guard` |
| Node/守卫解析 | **运行时**：`--run-guard` 每次启动重新检测（运行期契约 + core.json + 候选扫描），结果**不写入定义** | 同左 | 同左 |
| 启动 | `systemctl --user start` | `launchctl kickstart -k gui/<uid>/...`（失败退 bootstrap） | `schtasks /Run /TN DSH-Supervisor` |
| 停止 | `systemctl --user stop` | `launchctl bootout gui/<uid>/...` | 停 watchdog + 停任务 + 按命令行精确杀守卫 node（`Stop-Process`） |
| 自愈 | `Restart=always` | `KeepAlive` | 壳拥有的 watchdog 任务，动作 = `<壳> --watchdog`（判据与拉起序列与 GUI 启动同一实现，见 K-13；GUI 自愈归守卫） |
| 定位候选 | PATH + `~/.npm-global/bin` + `~/.local/bin` + **core.json** | 同左 + Homebrew 落点 + **core.json** | PATH/PATHEXT + `%APPDATA%\npm` + **core.json** |
| spawn 兜底 | `<壳> --run-guard` | `<壳> --run-guard` | `<壳> --run-guard`（同一 trait 默认实现） |
| 守卫子进程输出去向（四平台统一） | `<状态根>/shell/guard.log`（`platform::guard_stdio` 挂 fd，超 512KB 截尾） | 同左 | 同左 |
| 路径归一（交给外部工具前） | `platform::external_path`（全仓唯一实现，不带 `#[cfg]` → 三平台 CI 跑同一份规则） | 同左 | 同左 |

**为什么不把 node/guard 写进定义（2026-09-15 二次修正）**：固化检测结果后，node 一迁移
（nvm/fnm/volta）即失效；Windows 还需现场生成 `.cmd/.ps1`，被码页/前缀/垫片细节反复咬
（1.1.4 `exit 3`、1.1.5 `EISDIR`）。现定义只指向**稳定入口** `<壳> --run-guard`，检测在
每次启动重新执行；定义中**不得**出现 node/guard 路径（门禁 L-1 / K-4 / K-10）。

**跨平台不变量**：上表每一行，四平台都必须有实现或**显式 Unsupported**；不存在「某平台少一步」。

---

## 3. core.json（位置契约，P2 产物）

路径：`<状态根>/supervisor/core.json`（`env::supervisor_dir()`，见 `DESIGN-BOUNDARY.md` §7）；schema 1；**壳是唯一写入方**；原子写（tmp+rename）。

```json
{
  "schema": 1,
  "writtenBy": "dsh-supervisor-gui@<ver>",
  "bin": "<内核可执行绝对路径>",
  "prefix": "<npm 安装前缀>",
  "version": "<内核版本，== 发布通道选出的目标版本>",
  "source": "<命中的镜像 origin>",
  "installedAt": "<ISO>"
}
```

P3 定位顺序（**先契约，后启发式**）：
1. `core.json.bin` 存在且可执行且 `--version == core.json.version` → **采用**；
2. 否则由 `runtime.json` 派生：`nodeBinDir/<exe>`（覆盖 nvm/volta/fnm default prefix）；
3. 否则现有启发式（PATH / 固定目录 / 平台额外项）；
4. 仍无 → **不启动**，返回 `KERNEL_NOT_ALIGNED`（含已搜索目录 + 契约中的 bin + 通道选出的目标版本）。

---

## 4. 失败分类（前端/日志据此给可操作结论）

> 下表 = `domain/guardctl.rs` + `commands/mod.rs`（`guard_start` 的有界包装）实际产出的**全部** code，不多不少。

| code | stage | 含义 / 处置 |
|---|---|---|
| `RUNTIME_MISSING` | P0 | Node/npm 未就绪 → 回环境步骤 |
| `ALIGN_RESOLVE_FAILED` | P1 | 线上版本查询失败（离线/源不可达）→ 停在原地、如实报因 |
| `KERNEL_NOT_ALIGNED` | P3 | 磁盘无「== 通道选出版本」的内核 → 必须先 P1 对齐 |
| `LAUNCH_SPEC_FAILED` | P4 前 | 启动规格装不出来（取不到壳自身路径）→ 不写定义、不拉起，直接如实报错 |
| `SERVICE_START_FAILED` | P5 | 服务管理器启动失败**且** spawn 兜底也失败 |
| `READY_TIMEOUT` | P6 | healthz 超时 → 回传阶段证据 + 兜底进程 pid + `guard.log` 路径 |
| `JOIN_ERROR` | 命令层 | `guard_start` 的阻塞任务异常（线程池侧），与启动阶段无关 |

**P1 安装失败、P4 定义失败刻意没有独立 code**：

- P1 由 `core_apply` 承担，失败原因是 npm/网络的原始输出，不经过 `LaunchError`；
- P4 `ensure_defined` 失败**不阻断启动**，因为服务管理器不可用（容器 / 无 user session /
  策略拦截）正是 spawn 兜底要覆盖的场景。但「不阻断」不等于「忘掉」：
  2026-09-21 之前的实现是**只写一条日志然后照样 `start()`**，于是真机报错只剩
  「schtasks /Run 失败（退出码 1）」，把 P4 的真实故障伪装成了 P5 的下游症状。
  现在 P4/P5 的结果由 `ServiceAttempt` 承载（见 §1 前置关系），定义失败 ⇒ 不发启动请求，
  且 `evidence()` 原样进入 `SERVICE_START_FAILED` / `READY_TIMEOUT` 正文。

---

## 5. 现状与缺口登记（2026-09-15 审计，逐条可复核）

> **2026-09-15 收口**：G1–G6 均已落地（`core.json` 位置契约、locate 先读契约、`guard_start` 对齐门、
> 端口登记实际值、`install` 不建服务定义、Windows 看护收归壳、proxy 日志经 `stateDir`），
> 且各有门禁（收口当时为 K-1..K-10 / D-1..D-8；现行全表见 §6）。下表保留为**审计记录**（写的是修复前状态）。

| # | 缺口 | 现状证据 | 规范要求 |
|---|---|---|---|
| G1 | **位置未落契约**（H3） | `domain/coreloc.rs` 全仓 0 处引用 `runtime_contract`；Linux `core_extra_candidates` 返回空（`platform/linux.rs:154`） | P2 写 core.json；P3 先读契约 |
| G2 | **启动未做对齐门**（H4） | `guard_start`→`ensure_guard` 直接 `locate_core`（取**磁盘最高**，非通道选出版本）；`locate_core` 不查网络 | P1 前置；`guard_start` 未对齐即拒绝 |
| G3 | **Windows 双启动器**（H5） | 壳建 `DSH-Supervisor` 任务，内核 `autostart.js` 另有 watchdog 任务；壳 stop 必须知道内核的任务名（跨仓耦合） | 唯一所有者 = 壳；内核不建/不启服务 |
| G4 | **就绪端口竞态**（H6） | 壳先读 `ports.json`，缺失回落 config `apiPort`；内核占用顺延后才写 `ports.json` | 就绪只认 `ports.json` 实际值；内核须在其契约中声明 |
| G5 | **服务定义双写者**（H5） | 壳写三平台定义；内核 `install` 也写 systemd/launchd/schtasks | 定义只由壳写；内核 install 只装 config |
| G6 | **本地跑测污染**（非本规范，附带） | `domains/router/providers/proxy.js` 用 `os.homedir()` 直写 `~/.dsh/supervisor/logs`，测试未隔离 HOME | 数据目录经 config；测试强制隔离 HOME |

> 本规范落地前，G1–G5 任一存在即「内核可以通过壳启动」不成立。

---

## 6. 门禁（规范 = 会失败的测试）

| 门禁 | 断言 |
|---|---|
| K-1 | `core.json` schema/字段/原子写；只有壳写 |
| K-2 | P3 候选**包含** core.json.bin 与 `nodeBinDir/<exe>`；反向：构造无契约+非标准 prefix → 判为找不到（非空转） |
| K-3 | `ensure_guard` 先对齐后启动（函数内顺序判据）；且**服务定义未建立 ⇒ 不向服务管理器发启动请求**（出边只在建定义成功分支里，反向判据识别「无条件 start」旧形态） |
| K-4 | 四平台 `ServiceControl` 服务定义均经统一稳定入口 `<壳> --run-guard` 组装（`service_exec_line`），平台分支经 trait 的 prefix 候选覆写；反向：旧「定义里绑定 node/guard 绝对路径」或「只 guard」形态被判违规 |
| K-5 | 就绪端口只来自 `ports.json` 的 `supervisor-api`；反向：config 默认端口不得作为唯一判据 |
| K-6 | 平台分支只在 `platform/`（G1 既有门禁扩展覆盖 core.json 读取） |
| K-7 | Windows 看护任务的所有者 = 壳（`ensure_watchdog` + `WATCHDOG_TASK` + 间隔）；任务**形态**判据在 K-13 |
| K-8 | 产品状态根独立于 DSH（XDG），`supervisor_dir` 函数体内不得再拼 `~/.dsh`；平台默认值在 `platform/`；`shell_state_root` 命令已注册 |
| K-9 | 状态根随启动注入：`LaunchSpec.state_root` 由壳解析并规范化（`external_path(&crate::env::state_root())`），内核只消费 |
| K-10 | Windows 稳定入口 + 按命令行精确杀守卫（`Win32_Process`/`Stop-Process`）；反向：包装脚本残留（`guard-task.ps1/.cmd`）判违规 |
| K-11 | 交给外部工具的路径归一**只有一个实现**（`platform::external_path`）、壳自身路径**只有一个入口**（`platform::self_exe`），且 `LaunchSpec` 四个路径字段在构造处全部过归一；反向：`fn strip_verbatim`/`fn simplify(`/`current_exe(` 出现在业务层或平台实现文件即判违规 |
| K-12 | 守卫子进程输出永不丢弃：`platform::guard_stdio` 是唯一挂流点，spawn 兜底与两条 `exec_guard` 分支都必须用它；反向：这些片段里出现 `Stdio::null()` 即判违规 |
| K-13 | 看护（watchdog）不自带第二套判据/第二套动作：任务动作指向 `<壳> --watchdog`（`WATCHDOG_ARGS`，`/TR` 仍由 `service_exec_line` 单源装配），无头入口的存活判据取 `guardctl::ready`、拉起动作取 `guardctl::ensure_started`（`ensure_guard` 亦委托同一段，体内不得重复 spawn）；反向：windows.rs 代码里出现 `fn watchdog_script`/`Test-NetConnection`，或看护切片里出现 `powershell`/`Start-Process`，即判违规（反向判据在剥注释后的代码上跑） |
| K-14 | H6 的**四态真的能分辨**（行为判据，不靠符号搜索）：`domain/guardctl.rs` 的 `tests` 用回环真 socket 夹具逐一看四种现场 —— 应答 200 ⇒ `Ready`、应答 503 ⇒ `Http(503)`、连得上却一个字节不回 ⇒ `NoHttpResponse`、无人监听 ⇒ `PortClosed`；并断言 `describe()` 四条文案互不相同且带状态码。谁把「问不出状态行」糊成 `(0, …)`（`localhttp::http_get_local` 的旧形态），第一条用例就会把 `Ready` 判成 `Http(0)` 而变红 |
| CI 真机冒烟 | `.github/workflows/build.yml`：① `--node-plan` 必判退出码**且**必看到 `node=` / `node_probe_candidates=` / `latest_lts=` / `mirror_selected=`（原先带 `|| true`，自检失败也绿 = 空转门禁）；② Linux 伪内核（`core.json` + 会按实际端口写 `ports.json` 的最小 node HTTP 服务）→ `--watchdog` 端到端判就绪 → 再跑一次必须短路不再拉起；③ Windows `--service-plan --service-apply` → 真 `schtasks /Create` → `建立结果` 里的定义行可核对 + `--service-plan` 只读复跑报 `现存 = 是`（即 `is_defined()` 走平台事实）。**已知缺口**：Windows 的 P5/P6（`/Run` → `/healthz`）未在 CI 闭环，理由见该步注释（稳定入口不携带状态根 ⇒ 计划任务进程解析不到隔离的状态根；`ensure_started` 有 spawn 兜底 ⇒ 退出码 0 也证明不了 `/Run` 成功）。Linux ②同理只覆盖到「服务管理器路径可用时的那一段」，容器/无 user session 下走的是兜底 spawn。 |

同一族的其它判据（按文件命名，勿混用同号）：`platform_launch_contract_test.rs` 的 L-1..L-5
（稳定入口 / 运行期契约 / 端口发现 / schema 握手 / 反向自检）、`kernel_install_evidence_test.rs`
的 K-1..K-8（安装证据与 verbatim 的**安装侧后果**）、`round13_p3_batch_test.rs` 的 P-a..P-e、
`bootstrap_flow.rs` 的 G1/G3/B24/B25。

---

## 7. 迁移（前向自愈，不双读）

- 已装用户：无 `core.json` → P3 走启发式 + `nodeBinDir` 派生；首次成功启动/安装后写入 `core.json`（前向自愈）。
- 不保留旧 schema；不新增双读分支。
- 四平台各自 CI 验证；本地不跑构建。
