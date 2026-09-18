# 开发轨道（DEVELOPMENT-TRACK · 壳仓）

> 本文件是**壳仓改代码规则**的唯一事实源。与内核仓同名文件同义、同纪律。
> 机器校验：`src-tauri/tests/dev_runtime_safety_test.rs`。

---

## 1. 运行时禁区（**源码开发绝不触碰系统安装版**）

> **血泪案例（2026-09-16）**：清理临时文件时执行 `rm -rf /tmp/dsh-*`，
> 而 **DSH 自身正在用 `/tmp/dsh-subprocess-<随机>/` 存放子进程输出** ——
> 目录被删后 DSH 写日志 `ENOENT` **崩溃退出（code=1）**，约 6 分钟后才由系统守卫重新拉起。
> 源码开发**绝不应影响**系统正在运行的 DSH 与已安装的 supervisor。

### 铁律 R-1：开发只作用于工作区

| 允许 | 禁止 |
|---|---|
| 读写**本仓工作区**（克隆目录）内文件 | 改/删 `~/.local/state/dsh-supervisor/`（系统状态根） |
| 跑本仓测试（`cargo test`，自带隔离 tmp） | 停/启/改 `dsh-supervisor.service`（系统已安装的服务） |
| 读系统状态用于**诊断**（只读） | 覆盖 `/usr/bin/dsh-supervisor-gui`、`~/.npm-global/lib/node_modules/@dsh-sup/*` |
| 操作 `/tmp` 下**自己创建的具名路径** | 触碰 `~/.dsh`（DSH 自身数据目录） |

### 铁律 R-2：`/tmp` 清理必须**逐路径具名**，禁止通配符

DSH 与 AI 运行时**都在** `/tmp` 用 `dsh-*` / `dsh-spill-*` / `dsh-subprocess-*` 作为**活动目录**。
一个 `/tmp/dsh-*` 通配符会**同时命中**它们 → 直接打崩正在运行的进程。

- **禁止**：`rm -rf /tmp/dsh-*`、`/tmp/tmp.*`、`/tmp/*.log` 之类**通配/前缀**删除；
- **必须**：只删**自己明确创建、且知道其全名**的具体路径；
- **删除前先核对**：执行 `ls -d <pattern>` 看清匹配到谁，再决定；
- 测试临时物一律经 `std::env::temp_dir()` + **自己唯一前缀**创建，自带清理。

### 铁律 R-3：发现「好像动了系统」时，先取证再行动

只读取证优先：`systemctl --user show dsh-supervisor.service -p NRestarts -p ActiveEnterTimestamp`、
系统状态根下的 `events/*.log` / `log/dsh.log`。**不得**用「重启服务」作为排查手段。

---

## 2. 边界（两仓纪律）

- **两仓不得共享代码**，只经文件契约：`registry.json` / `identity.json` / `runtime.json` / `core.json`；
- 壳**不持有 DSH 令牌**（见 `DESIGN-BOUNDARY.md`）；令牌全在内核；
- 壳改动必须过 `cargo test --bins --tests`（含各结构门禁）。

---

## 3. 无控制台窗口

壳为 GUI 子系统；**任何**子进程启动必须隐藏控制台，
详见内核 `NO-CONSOLE-WINDOW-STANDARD.md`（两仓共遵），壳侧由 `no_console_window_test.rs` 校验。
