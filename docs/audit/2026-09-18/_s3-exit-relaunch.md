# S3 · 退出管家后桌面壳自启 —— 跨仓完整排查（2026-09-18）

> 症状：任务栏「退出管家」后并非完整退出，很久以后桌面壳又被自动启动。
> 方法：内核侧与壳侧各一名独立只读审计执行者 + 编排者源码复核；未跑测试/构建，验收以 CI 为准。
> 关联：内核仓 dsh-supervisor-core PR #20；本仓 PR #23。

## 1. 谁能在退出后把桌面壳拉起来（完整清单）

| # | 路径 | 触发条件 | 处置 |
|---|---|---|---|
| 1 | 内核壳看护 bootstrap 定时器 -> watchdog.tick() -> restartShell | 守卫存活且会话态允许 | 本修：halted 门下沉看护域 + 持久退出标记 |
| 2 | Windows 计划任务 DSH-Supervisor-Watchdog（/SC MINUTE /MO 5）-> watchdog.ps1 第二分支 Start-Process 壳 | 任务未被删除 | 本修：stop() 改 /Delete /F |
| 3 | 登录自启：Windows DSH-Supervisor-GUI（ONLOGON）/ Linux XDG desktop / macOS com.dsh.supervisor.gui（RunAtLoad） | 登录时 | 仅在用户开启自启时合法；非本缺陷（退出不关自启） |
| 4 | 守卫重启后内核看护（守卫被登录任务/外部自愈重新拉起，遗忘退出） | 守卫重启 | 本修：持久退出标记（shellHalted）跨守卫重启继承 |
| 5 | 手动：托盘 quit / closeAction=exit / POST /shell/restart / 更新 app.restart() | 显式动作 | 本修：/shell/restart 加会话门（409）；在飞复判 |
| 6 | Linux/macOS 守卫服务 Restart=always / KeepAlive | 崩溃 | 只重启守卫（--run-guard，非 GUI）；显式 stop 不触发 |

排除：引导页前端定时器随 GUI 进程消亡；无 Windows Startup/Run 注册表项；GUI 不由壳自身 autostart 建立（内核 win32.js 建立 DSH-Supervisor-GUI）。

## 2. 决定性根因

### 2.1 壳侧（Windows，题面主根因）
platform/windows.rs stop() 只用 schtasks /End 结束**本次运行实例**；而 DSH-Supervisor-Watchdog 是 /SC MINUTE /MO 5 的**计划任务**，/End 不禁用计划。watchdog.ps1 在无 dsh-supervisor* GUI 进程时 Start-Process 壳，在守卫端口 down 时 Start-Process 壳 --run-guard。于是退出后 <=5 分钟看护再次触发，守卫与桌面壳一起被拉回。

### 2.2 内核侧（同源缺陷）
- 退出门只判「壳是否缺失」，不看会话态；且只落在 bootstrap 定时器闭包，任何其它 restartShell 调用者天然绕过（K3）。
- shutdownAll（/session/stop 路径）未清 _shellWatchdogTimer（K2/K5），只有完整 shutdown() 清。
- 会话退出意图**不持久化**：会话态只活内存（app/session/machine.js），status 快照含 sessionState 且 writeState 整体落盘，但 loadState() 从不回读（app/state/store.js）。守卫被外部/登录重新拉起后 _sessionState=starting，退出门失效，看护 90s 后重新拉起壳（K1，决定性）。
- POST /shell/restart 无会话门且 shutdownAll 不关 API（K2）；restartShell 杀旧壳与 spawn 之间 ~8s 窗口不可取消（K4）。

## 3. 本修内容

### 壳仓（本 PR #23）
1. platform/windows.rs stop()：看护任务 /Delete /F（守卫任务保持 /End）；下次启动 ensure_defined 幂等重建。
2. domain/guardctl.rs ensure_guard：在「守卫已运行」提前返回分支补一次 Windows-only 幂等 ensure_defined —— 退出 /Delete 后若登录任务先拉起守卫，该分支原会跳过重建，使本会话 GUI 崩溃自愈失效。
3. guardctl::shutdown_all：service().stop() 失败除 stderr 外落盘 shell.log。

### 内核仓（PR #20）
1. K1：新增 host._shellHalted 持久退出标记（status.shellHalted）；shutdownAll 立即落盘；boot 经 loadState 继承；看护观测到壳在线且非退出中时自动清除（用户重开壳恢复自愈）。只抑制看护，不影响 desired/main 恢复语义。
2. K3：退出门下沉看护域（tick 依赖 halted/onShellAlive），一切 tick 调用者同受约束。
3. K2：POST /shell/restart 退出中/已退出返回 409。
4. K4：restartShell 增加 shouldAbort，在 spawn 前复判。
5. K5：shutdownAll 与完整 shutdown() 对齐，清全部周期定时器。

## 4. 验证

内核：test/shell-watchdog-test.js（W3-j/W3-k/W4-h/W4-i）、test/session-lifecycle-test.js（P3-G、shellHalted 落盘）。壳：由 CI 编译 + 既有三平台门禁裁决。全部由 CI 执行；本机未跑任何测试/构建。

## 5. 已登记残留（本次不修，理由）

| 编号 | 内容 | 影响 | 建议 |
|---|---|---|---|
| R1 | 内核 Windows setAutostart(false) 只删 GUI 任务，不删守卫 ONLOGON 任务（win32.js）；而 status().on = guard \|\| gui \|\| watchdog | 关闭自启后守卫仍随登录复活（但 K1 已防其拉壳）；开关语义不完整 | 由壳在关闭自启时一并删/停守卫任务，或内核获授权管守卫任务 |
| R2 | 壳 spawn_daemon 兜底进程不受服务管理器管（service.rs），service().stop() 停不掉 | 容器/无 user session 下退出后守卫残留 | 记录 pid + job object 以便退出终止 |
| R3 | Linux/macOS stop() 只 stop/bootout，不 disable；但 K1 已防拉壳 | 守卫仍随登录复活（自启开时合法） | 与 R1 一并由所有者语义统一 |
| R4 | 内核沙箱 supervise 兜底路径（instance/ops.js startTimer -> lifecycle supervise）无会话门（K6）；主路径 instance-adapter 有门 | 兜底时可能自动重启沙箱实例（非壳） | supervise 加 halting 门 |
| R5 | bin/dsh-supervisor uncaughtException x3 -> 非零退出 -> 外部自愈重启守卫（K8）；退出后旧守卫持锁（K9） | 与 K1 叠加放大 | 退出路径非零退出改可控；锁释放 |

## 6. 参考

- ci-out/shell-exit-audit/_kernel.md（内核侧 K1-K9 全量证据）
- ci-out/shell-exit-audit/_shell.md（壳侧 S1-S6 全量证据）
