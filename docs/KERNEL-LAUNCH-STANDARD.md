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
| H4 | 启动前，磁盘内核版本**必须等于线上最新**（全 tag 最高）；否则**先对齐再启动**，`guard_start` 不对齐即拒绝 | 「内核只有最新版本」是产品规则；否则会拉起磁盘上的旧内核，后半段逻辑全错 |
| H5 | 服务定义/启停的**唯一所有者**是壳（经平台服务管理器）；内核不自启、不自停、不自重启 | 曾多个启动器（systemd + 壳 spawn + Windows watchdog）并存 |
| H6 | 就绪判据 = `GET /healthz` 200，端口取自内核 `ports.json` 的 `supervisor-api` **实际值** | 端口可被占用后顺延；盯固定端口会永远等不到已健康的守卫 |
| H7 | 平台分支**只**允许在 `src-tauri/src/platform/`；其余处经 trait | 门禁 G1 |
| H8 | 任何一步失败必须回传**结构化 stage + 证据**（搜索过的目录、命令、退出码） | 现场排障不靠猜 |

---

## 1. 规范流水线（P0 → P6，四平台一致）

```
P0 运行期契约   resolve runtime      ~/.dsh/supervisor/runtime.json（node/npm/nodeBinDir/PATH）   壳写
P1 版本对齐     align to latest      resolve 线上最新(全 tag 最高) → 与磁盘比较 → 需要则安装    壳写(唯一)
P2 位置落契约   record location      安装成功后写 core.json（bin/prefix/version/source）        壳写(唯一)
P3 定位         resolve aligned      读 core.json → 校验 bin 可执行 且 version==线上最新       壳读
P4 服务定义     ensure_defined       平台模板：绑定 node + PATH + `daemon`                     壳写
P5 启动         start (owner)        服务管理器启动；不可用则 spawn 兜底                         壳发起
P6 就绪         readiness            healthz 200（端口 = ports.json 的 supervisor-api 实际值）   壳读
```

**前置关系（硬）**：`P6 就绪` 前必须 `P1 已对齐`。绕过 P1 直接 P4/P5 = 拉起旧内核 = 违规。

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
| 自愈 | `Restart=always` | `KeepAlive` | 壳拥有的 watchdog 任务（动作 = `--run-guard`） |
| 定位候选 | PATH + `~/.npm-global/bin` + `~/.local/bin` + **core.json** | 同左 + Homebrew 落点 + **core.json** | PATH/PATHEXT + `%APPDATA%\npm` + **core.json** |
| spawn 兜底 | `<壳> --run-guard` | `<壳> --run-guard` | `<壳> --run-guard`（同一 trait 默认实现） |

**为什么不把 node/guard 写进定义（2026-09-15 二次修正）**：固化检测结果后，node 一迁移
（nvm/fnm/volta）即失效；Windows 还需现场生成 `.cmd/.ps1`，被码页/前缀/垫片细节反复咬
（1.1.4 `exit 3`、1.1.5 `EISDIR`）。现定义只指向**稳定入口** `<壳> --run-guard`，检测在
每次启动重新执行；定义中**不得**出现 node/guard 路径（门禁 L-1 / K-4 / K-10）。

**跨平台不变量**：上表每一行，四平台都必须有实现或**显式 Unsupported**；不存在「某平台少一步」。

---

## 3. core.json（位置契约，P2 产物）

路径：`~/.dsh/supervisor/core.json`；schema 1；**壳是唯一写入方**；原子写（tmp+rename）。

```json
{
  "schema": 1,
  "writtenBy": "dsh-supervisor-gui@<ver>",
  "bin": "<内核可执行绝对路径>",
  "prefix": "<npm 安装前缀>",
  "version": "<内核版本，== 线上最新>",
  "source": "<命中的镜像 origin>",
  "installedAt": "<ISO>"
}
```

P3 定位顺序（**先契约，后启发式**）：
1. `core.json.bin` 存在且可执行且 `--version == core.json.version` → **采用**；
2. 否则由 `runtime.json` 派生：`nodeBinDir/<exe>`（覆盖 nvm/volta/fnm default prefix）；
3. 否则现有启发式（PATH / 固定目录 / 平台额外项）；
4. 仍无 → **不启动**，返回 `KERNEL_NOT_ALIGNED`（含已搜索目录 + 契约中的 bin + 线上最新版本）。

---

## 4. 失败分类（前端/日志据此给可操作结论）

| code | stage | 含义 / 处置 |
|---|---|---|
| `RUNTIME_MISSING` | P0 | Node/npm 未就绪 → 回环境步骤 |
| `ALIGN_RESOLVE_FAILED` | P1 | 线上版本查询失败（离线/源不可达）→ 停在原地、如实报因 |
| `INSTALL_FAILED` | P1 | npm 安装失败 → 回传命令/prefix/源/输出 |
| `KERNEL_NOT_ALIGNED` | P3 | 磁盘无「== 线上最新」的内核 → 必须先 P1 对齐 |
| `SERVICE_DEFINE_FAILED` | P4 | 平台服务定义写入失败 → 回传平台错误 |
| `SERVICE_START_FAILED` | P5 | 服务管理器启动失败且 spawn 兜底也失败 |
| `READY_TIMEOUT` | P6 | healthz 超时 → 回传端口与 kernel 日志路径 |

---

## 5. 现状与缺口登记（2026-09-15 审计，逐条可复核）

> **2026-09-15 收口**：G1–G6 均已落地（`core.json` 位置契约、locate 先读契约、`guard_start` 对齐门、
> 端口登记实际值、`install` 不建服务定义、Windows 看护收归壳、proxy 日志经 `stateDir`），
> 且各有门禁（K-1..K-7 / D-1..D-8）。下表保留为**审计记录**（写的是修复前状态）。

| # | 缺口 | 现状证据 | 规范要求 |
|---|---|---|---|
| G1 | **位置未落契约**（H3） | `domain/coreloc.rs` 全仓 0 处引用 `runtime_contract`；Linux `core_extra_candidates` 返回空（`platform/linux.rs:154`） | P2 写 core.json；P3 先读契约 |
| G2 | **启动未做对齐门**（H4） | `guard_start`→`ensure_guard` 直接 `locate_core`（取**磁盘最高**，非线上最新）；`locate_core` 不查网络 | P1 前置；`guard_start` 未对齐即拒绝 |
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
| K-3 | `guard_start` 未对齐（磁盘 != 线上最新）时返回 `KERNEL_NOT_ALIGNED`，且**不**调用服务管理器 |
| K-4 | 四平台 `ServiceControl` 均绑定 node + PATH + `daemon`；反向：旧「只 guard」形态被判违规 |
| K-5 | 就绪端口只来自 `ports.json` 的 `supervisor-api`；反向：config 默认端口不得作为唯一判据 |
| K-6 | 平台分支只在 `platform/`（G1 既有门禁扩展覆盖 core.json 读取） |

---

## 7. 迁移（前向自愈，不双读）

- 已装用户：无 `core.json` → P3 走启发式 + `nodeBinDir` 派生；首次成功启动/安装后写入 `core.json`（前向自愈）。
- 不保留旧 schema；不新增双读分支。
- 四平台各自 CI 验证；本地不跑构建。
