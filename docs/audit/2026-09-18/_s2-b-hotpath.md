# S2-B 审计报告：热路径 / 稳定性 / 工具链

> 只读审计。审计对象 = 壳仓当前分支 `release/shell-1.1.8` 的候选提交 `0930884`
> （= 已发布 `main`(5df08aa==1.1.7 内容) + 文档移交 + 27 个改动文件 + 新增 `release_channel.rs`）。
> **重要**：审计开始时该候选还是「未提交工作树」（`00f872c` + 27 未提交），审计中途由上层代理提交为
> `0930884`（reflog 可见 checkout 到 `release/shell-1.1.8` + cherry-pick + commit）；内容与审计所见一致，
> 版本号此刻仍为 `1.1.7`（1.1.8 尚未 bump）。
> 未跑任何测试/构建；只读工具。全部路径为**仓库相对路径**。

## 0. 范围（本片 S2-B）

- 启动链路与守卫拉起：`src-tauri/src/main.rs`、`src-tauri/src/platform/**`、`src-tauri/src/domain/guardctl.rs`、
  `src-tauri/src/domain/cli.rs`、`src-tauri/bootstrap/js/**`。
- 工具链探测/安装/修复：`src-tauri/src/node.rs`、`src-tauri/src/env.rs`、`main.rs` 的 `run_install`、
  `node::probe_npm_after` / `node::reinstall_for_npm`。
- 内核更新计划与执行：`src-tauri/src/core.rs`、`src-tauri/src/update_plan.rs`、`src-tauri/src/commands/mod.rs`。
- 稳定性横切：`src-tauri/src/bounded.rs`、`src-tauri/src/domain/localhttp.rs`、`src-tauri/src/platform/service.rs`。

## 1. 结论摘要

- **无 P0**（未发现"必然导致引导永久卡死/数据丢失/安全绕过"的开放项）。
- **P1 × 1**：`core_apply` 后端无总预算 —— 前端 17 分钟后放弃，后端仍可能继续数十钟（§2.1）。
- **P2 × 6**：前端超时不取消后端、spawn 兜底未 detach/reap、Node 下载无体积上限、契约序列化失败仍写盘、
  `locate_core` 无总预算、环境安装轮询无 in-flight 去重（§2.2）。
- **P3 × 3**：重复函数定义、跨盘迁移静默失败、前端传了后端已不接收的 `version` 参数（§2.3）。
- 横切的"有界执行/连接超时/失败如实上报"基础设施**质量高**，逐项见 §3。

## 2. 问题清单

### 2.1 P1

#### S2B-1 `core_apply` 后端无总时间预算（前端 17 分钟超时后后端仍继续）

- 证据：`src-tauri/src/commands/mod.rs:302-311`（对 `origins` **顺序**逐个 `install_version`，**无整体 deadline**）；
  `src-tauri/src/core.rs:633`（`NPM_INSTALL_TIMEOUT = 15 * 60`，**每个源**一次）；
  `src-tauri/bootstrap/js/00-runtime.js:33`（`CORE_APPLY_BUDGET_MS = 1020000` = 17 分钟，**仅前端**）；
  `src-tauri/bootstrap/js/50-kernel.js:42-43`（超时只 `NS.fail` 并停止等待，不取消 Rust 任务）。
- 影响：全部 6 个默认源都卡到时，后端最坏 6×15=**90 分钟**仍在跑；前端 17 分钟已报"内核安装超时"。
  用户此时点"重试"会**并发**再起一条同管线（`start_node_install` 有 `busy` 闸，但 `core_apply` **无 busy 闸**），
  多个 `npm install -g` 同时写同一 prefix → 可能互相破坏。且最终后端可能装成功，与前端"失败"矛盾。
- 最小修法：在 `core_apply_inner` 的源循环前取 `deadline = now + 17min`，每次 `install_version` 前判
  `now >= deadline` 即停并如实回报"已尝试 N 源 / 剩余未试"；或把 `core_apply` 包一层与前端一致的
  `tokio::time::timeout`（但 spawn_blocking 不可真正取消，仍建议循环内 deadline）。
  另建议给 `core_apply` 加进程内 `busy` 互斥，防重试并发。

### 2.2 P2

#### S2B-2 前端 `withTimeout` 只停止等待、不取消后端任务（预算语义与后端不一致）

- 证据：`src-tauri/bootstrap/js/10-ui.js:67-80`（超时 `resolve({__timeout:true})`，**不 abort** 底层 promise）；
  调用点 `50-kernel.js:42`、`60-guard.js:10`、`30-mirror.js:36`。
- 影响：用户看到"超时失败"但后端仍在执行；对 `core_apply`/`start_node_install` 这类**写盘**操作，
  失败文案与最终状态可能矛盾（见 S2B-1）。属**设计取舍**（IPC 无取消通道），但应在超时文案里明说
  "后端可能仍在进行，请勿重复点击"，并配合 S2B-1 的后端 deadline。
- 最小修法：超时分支文案加"（后端可能仍在完成，请稍后再看）"；`core_apply` 加 busy 闸（S2B-1）。

#### S2B-3 `spawn_daemon` 兜底拉起的守卫既未 detach 也未 reap

- 证据：`src-tauri/src/platform/service.rs:60-73` —— `cmd.spawn()` 得到 `child` 后**立即丢弃句柄**；
  无 `setsid`/进程组分离（Unix），Windows 未加 `DETACHED_PROCESS`（仅 `CREATE_NO_WINDOW`）。
- 影响：① Unix 上父进程（壳）不 `wait`，守卫若崩溃退出会成为**僵尸**；壳常驻期间反复兜底会累积。
  ② 守卫是"应常驻"的 daemon，未真正脱离父进程 —— 壳退出/被更新替换时守卫生命周期依赖平台实现，不稳。
- 最小修法：Unix 用 `CommandExt::process_group(0)`（或 `pre_exec(setsid)`）；Windows 加 `DETACHED_PROCESS`
  (`0x0000_0008`，与 `platform/mod.rs:174` 的 `exec_guard` 一致)；并显式说明句柄丢弃后由 init 接管。

#### S2B-4 Node 下载无响应体体积上限（可被超大 body 拖垮内存）

- 证据：`src-tauri/src/node.rs:12-20` `http_get_bytes` 用 `read_to_end` 把整包读进 `Vec<u8>`，
  仅靠 `HTTP_TOTAL_TIMEOUT = 15min` 限时，**无 Content-Length 校验、无读取上限**；SHA256 在**整包读完后**才校验。
- 影响：镜像/中间人返回超大 body（或 Content-Length 撒谎）→ 引导进程内存暴涨直至 OOM。
  虽镜像默认是可信 https，但用户可经 `mirror_set` 自定义源。
- 最小修法：读取前检查 `Content-Length`（超阈值即拒绝，如 200MB）；或用 `Read::take(MAX)` 限制并校验长度。

#### S2B-5 契约序列化失败仍写盘（会把空/半截 JSON 落盘）

- 证据：`src-tauri/src/node.rs:336-343`、`src-tauri/src/core_contract.rs:56-62`、
  `src-tauri/src/runtime_contract.rs:118-126`、`src-tauri/src/update.rs:100-103` —— 四处均为
  `serde_json::to_string_pretty(...).unwrap_or_default()` 后无条件 `write tmp + rename`（同型四例）。
- 影响：序列化真失败时 `body == ""`，仍会写入 `runtime.json`/`core.json` 的空壳；下游 `read()` 解析失败 →
  退回启发式（可恢复），但**覆盖了上一份好文件**，且属"静默写坏"。
- 最小修法：`match serde_json::to_string_pretty(...) { Ok(b) => ..., Err(_) => return } `（失败即跳过写盘）。

#### S2B-6 `locate_core` 无总预算（候选数 × 单次 10s）

- 证据：`src-tauri/src/domain/coreloc.rs:165-175`（`for c in &cands` 逐个 `installed_version`）；
  `src-tauri/src/core.rs:422`（`VERSION_PROBE_TIMEOUT = 10s`）；调用点 `commands/mod.rs:243-258`（`core_plan`）
  与 `main.rs:219-237`（`core_status`）均**无外层预算**。
- 影响：候选多（PATH + 多个包内脚本 + 资源目录）且有坏候选时，`core_plan`/`core_status` 可达 N×10s；
  前端靠 90s 超时兜底（后端继续）。属延迟而非卡死，列 P2。
- 最小修法：给 `locate_core_with_version` 加总预算（如 30s，超预算即停止探测并保留当前最优）。

#### S2B-7 `stepNodeWait` 轮询无 in-flight 去重，且硬超时与后端不一致

- 证据：`src-tauri/bootstrap/js/20-env.js:124-146`（`setInterval(700ms)` 内直接 `invoke('node_status')`，
  上一次未 settle 时下一次仍会发起；硬超时 `600000` = 10 分钟）；
  `src-tauri/src/commands/mod.rs:34-40`（`node_status` 自身有 900ms budget，通常快）。
- 影响：查询偶发变慢时并发堆叠 IPC；10 分钟硬超时与后端工具链管线（node 下载 15min + 两次安装）不一致，
  到点后前端报失败而后端可能仍在装（同 S2B-1 的形态）。
- 最小修法：改为"前一次 settle 后再 `setTimeout` 排下一次"的链式轮询；前端硬超时与后端 deadline 对齐。

#### S2B-11 前端检查的 `cannotSelfUpdate` 后端从不产出（不支持自更新的形态被卡在更新步）

- 证据：`src-tauri/bootstrap/js/40-shell-update.js:27`（`if (r.cannotSelfUpdate === true) { ...继续... }`），
  但 `src-tauri/src/commands/mod.rs:617-631` `shell_plan` 的键只有
  `artifact/current/latest/available/channel/source/error + ok/notes/date`，**无 `cannotSelfUpdate`**；
  真正的能力字段叫 `selfUpdateCapable`，由 `src-tauri/src/update.rs:139-146` 写入 identity.json，
  经 `shell_identity`（`commands/mod.rs:526-533`）回传 —— 与 `shell_update_check` 的 plan **不是同一个返回值**。
- 影响：`self_update_capable()`（`update.rs:81-87`）对 `install_kind()="unknown"`（源码/开发形态，
  `update.rs:62-73`）或 Linux `deb/rpm` 无提权通道时返回 false；此时前端**永远不会**走"不支持自更新 → 继续"，
  而是调用 `shell_update_check` 得到 `ok:false` → `showUpdRetry` → 用户卡在"桌面版本"步骤反复重试，
  **到不了内核步骤**（硬规则又禁止跳过）。这是"契约字段名/来源错配"造成的死分支。
- 最小修法（二选一）：① 前端改判 `NS.shellId && NS.shellId.selfUpdateCapable === false`（数据已在手）；
  ② `shell_update_check` 的 plan 显式加 `cannotSelfUpdate: !crate::update::self_update_capable()`（或 `selfUpdateCapable`），
  并加一条门禁断言二者同名同源。

### 2.3 P3

#### S2B-8 `10-ui.js` 重复函数定义（死代码）

- 证据：`src-tauri/bootstrap/js/10-ui.js:22` 与 `:24` 重复 `function wait`；`:25` 与 `:27` 重复 `function hideFail`。
  （已核：`HEAD` 版本同样含这 4 行，**非本候选引入**。）
- 影响：无行为差异（后定义覆盖前定义），但属死代码，且提示该文件曾经历机械编辑。
- 最小修法：各删一份。

#### S2B-9 状态根迁移用 `rename`，跨文件系统会静默失败

- 证据：`src-tauri/src/env.rs:214`（`let _ = std::fs::rename(e.path(), &dst)`）。
- 影响：旧状态目录与新状态根不在同一文件系统（如旧在 `~`、新在挂载的另一卷）时 `rename` 返回 `EXDEV`，
  静默丢弃 → "前向自愈迁移"不发生且无任何日志。
- 最小修法：`rename` 失败时回退 `fs::copy + remove_file`，并在两次都失败时 `update::log` 记一行。

#### S2B-10 前端传 `{version}` 但后端 `core_apply` 不接收（死参数 / 契约漂移）

- 证据：`src-tauri/bootstrap/js/50-kernel.js:42` `invoke('core_apply', { version: version })`；
  `src-tauri/src/commands/mod.rs:271-274` `core_apply(app)` 无 `version` 形参（目标版本由后端选版算法决定）。
- 影响：无功能损害（Tauri 忽略多余参数），但该参数会误导读者以为"可指定版本"，属契约漂移。
- 最小修法：前端去掉该参数；或后端显式收下并忽略（加注释说明"刻意不接受调用方版本"）。

#### S2B-12 `read_node` 在缺 `npmPath` 键时**伪造** npm 路径（与"绝不伪造"声明相悖）

- 证据：`src-tauri/src/runtime_contract.rs:139-143` —— `npmPath` 缺失时
  `unwrap_or_else(|| bin.join(npm_exe_name()))` 拼出一个**可能存在也可能不存在**的路径，
  而同文件头注/§"绝不伪造一个不存在的路径"正是该契约的卖点。
- 影响：`ensure()`（`:156-162`）会再校验 `rt.npm.is_file()` 兜住；但直接消费 `read_node()` 的调用点
  未兜（如 `core.rs:npm_global_prefix` 直接 `Command::new(&rt.npm)`）。旧 runtime.json（schema 2 前/缺键）
  会走到该路径，表现为"命令启动失败 → None"，难排。
- 最小修法：缺 `npmPath` 时返回 `None`（让调用方走 `ensure()` 重新解析），不要伪造。

#### S2B-13 `env_path` 组装失败时返回**空 PATH**

- 证据：`src-tauri/src/runtime_contract.rs:188-190` —— `std::env::join_paths(&dirs)` 失败时
  `.unwrap_or_default()` → 返回空字符串。
- 影响：`LaunchSpec::from_runtime`（`platform/mod.rs:98-106`）把它作为服务定义/spawn 的 `PATH`；
  空 PATH 会让守卫子进程找不到任何工具（含 node 自身以外的依赖）。
- 最小修法：失败时至少回退 `node_bin_dir` 单项（并记日志），不要给空串。

## 3. 已核、无问题（本片范围内的正面证据）

- **有界子进程统一**：`src-tauri/src/bounded.rs:62-138` `run` 用 `try_wait` 轮询 + 超时 `kill`；
  输出重定向临时文件（避免管道 64KB 死锁）；失败路径 `cleanup` 清临时文件（`bounded.rs:166-169`）。
  三平台服务命令全部经它（`platform/windows.rs`、`linux.rs`、`macos.rs` 的 `bounded::run`）。
- **本地 HTTP 全有超时**：`src-tauri/src/domain/localhttp.rs:39-75` —— `connect_timeout`（800ms）+
  `set_read/write_timeout`；UI 线程经 `spawn_local_post` 派发（`localhttp.rs:27-29`），不冻结界面。
- **守卫拉起有界**：`src-tauri/src/domain/guardctl.rs:170-195`（`wait_alive(60)`+`wait_alive(120)`，每 tick 500ms）；
  `commands/mod.rs:422-448` `guard_start` 外层 `tokio::time::timeout(180s)`。
- **`--run-guard` 入口**：`src-tauri/src/domain/cli.rs:105-119` + `platform/mod.rs:135-177` ——
  Unix `exec`（替换进程，systemd 追踪真实 node）；Windows `DETACHED_PROCESS|CREATE_NO_WINDOW`。
  服务定义只指向 `<壳> --run-guard`（`platform/mod.rs:108-114`，含单测 `launch_spec_tests`）。
- **镜像探测有界**：`src-tauri/src/mirror.rs:71,304-330` `thread::scope` + ureq `PROBE_TIMEOUT=8s`，整体≈最慢源。
- **Node 探测有界**：`src-tauri/src/nodeprobe.rs` 分离线程 + 有界等待；`node_status` 900ms budget（`commands/mod.rs:36-40`）；
  `env.rs:93-131` `node_version` 5s 轮询 `try_wait` 超时 `kill`。
- **引导前端每步有超时且不默认成功**：`20-env.js:130-146`（`failOnMissingNpm` 不默认成功）、
  `50-kernel.js:27-30`（查询失败如实报错，不谎报"已是最新"）、`60-guard.js:30-39`（就绪轮询有上限）。
- **安装事件统一形态**：`main.rs` `InstallKind`/`push_status` 发 `install_progress {kind,status,progress}`；
  `80-init.js:68-81` 唯一消费入口。失败按 kind 归属（`InstallFailure`）—— 与内核 SSOT §2.4 一致。

## 4. 下级分片结论（release_channel / mirror / nodeprobe / runtime_contract / update）

> 由下级分片 S2-B-rel 产出，全文见 **同目录 `_s2-b-release-probe.md`**（只读、无 git 写、未跑测试/构建）。
> 审计对象同为 `0930884`。其结论摘要如下；与 §2 重叠项见末尾去重说明。

### 4.1 下级 P1（2 条）

- **S2B-MIR-01**｜并行探测对「最慢线程」无上界，DNS 阶段不受超时约束。
  证据：`src-tauri/src/mirror.rs:314`（`std::thread::scope`）、`:317`（每源一线程）、`:330`（ureq `timeout(PROBE_TIMEOUT)`）、
  `:347`（scope 结束=等全部线程）。ureq 2.12.1 对 DNS 明确留 TODO（deadline 不覆盖 DNS）。
  影响：任一镜像 DNS 挂起 → `probe_all` 整体挂起，选版/导出镜像契约/Node 检查全链路无限期阻塞。
  最小修法：给 `probe_all` 加**整体截止**（工作线程只写共享槽，主线程到点即返回已收集结果）。
- **S2B-RC-01**｜灰度名单包拉取按镜像数**串行**放大（与注释「最多一次 PROBE_TIMEOUT」相反）。
  证据：`src-tauri/src/release_channel.rs:376`、`:357`；`src-tauri/src/core.rs:361`（逐 origin 串行 `probe_all(one,path)`，各 8s）、`:338`（注释声称不随源数放大）。
  影响：显式 opt-in `canaryAllowlist` 的机器在名单包不可达时最坏 ≈6×8s=48s 无响应。
  最小修法：一次 `probe_all(&origins, path)` 并发取首个合法 JSON，或给 `fetch_pkg_meta` 单一总截止。

### 4.2 下级 P2（18 条，按文件）

- `release_channel.rs`：**RC-02** 壳读 `install-id` 不校验 UUID v4/不 lower（内核校验，跨仓口径分叉，灰度静默不命中）；
  **RC-03** `read_to_string(...).ok()?` 把权限/IO 错误与「不存在」合并；**RC-04** hostname 兜底只读
  `HOSTNAME`/`COMPUTERNAME`，GUI 启动的 Linux 常无 `HOSTNAME`。
- `mirror.rs`：**MIR-02** 探测响应体无大小上限（8s 内可读入任意大 body）；
  **MIR-03** `WARMING` 先置 true 再 spawn，spawn 失败/panic 后**永久不复位**（预热永久失效）；
  **MIR-04** 预热是「load→计算→整体 save」无锁 read-modify-write，可覆盖用户 `mirror_set`；
  **MIR-05** mirrors.json/mirror_set 列表无上限 → `probe_all` 每源一线程，可被拉成百上千线程；
  **MIR-06** npm 探测存在三条路径口径（未编码 `@scope/pkg` vs `%2F` vs 导出的已解析 `pathTemplate`），
  与 `DESIGN-BOUNDARY.md:174` 的 `{platform}` 模板漂移。
- `nodeprobe.rs`：**NP-01** caller budget>25s 时硬上限未强制（实测可阻塞 45s/60s）；
  **NP-02** 并发 status 的旧 rx 覆盖新 rx（新 worker 结论被丢弃）；
  **NP-03** ORPHANS 计数代际混算 → 可能永久拒绝新建或误判可新建；
  **NP-04** `stage/finish/set_summary` 锁中毒后静默 no-op（snapshot 却能恢复）→ 卡住线索消失；
  **NP-05** worker spawn 失败被丢弃，原因不落日志。
- `runtime_contract.rs`：**RTC-01**（= 本报告 S2B-12）、**RTC-02** `write()` 全 `let _ =` 且返回 `()`，
  写失败静默、`ensure()` 照常返回 Some；**RTC-03** `read_node()` 不校验 `SCHEMA`（写了 2 从不比对）；
  **RTC-04**（= 本报告 S2B-13）；**RTC-05** `ensure()` 慢路径最坏 ≈45s 且 `core_apply_inner` 在 async 体内
  **直接同步调用**（未 `spawn_blocking`）→ 阻塞 tokio 工作线程、拖慢 IPC。
- `update.rs`：**UPD-01** `init_identity/set_phase` 无锁 read-modify-write + 固定 tmp 名（并发丢字段）；
  **UPD-02** 日志滚动与 append 无锁（并发丢行）；**UPD-03** identity.json 损坏时 `read_json` 返回 `{}`，
  下次 `set_phase` 整体覆盖抹掉 `version/exe/pid`。

### 4.3 下级已核、无问题（要点）

- **选版算法与 `RELEASE-CHANNEL-CONTRACT` §3 逐条一致**（rollback > canary > latest > versions 兜底 > 明确 Err），
  且与内核 `pickReleaseVersion` 语义一致；beta/rc 不在自动算法内，两仓一致（非漏改）。
- `release_channel.rs` 为纯函数 + 注入闭包，自身不触网，非测试代码无 `unwrap/expect/panic`。
- 镜像 URL 拼接两侧 trim、原子写；`mirror_set` 强制 `http(s)://`。
- nodeprobe 的 `catch_unwind`、代际校验、ORPHANS 防堆积、枚举上限 64 均已实现（NP 项是边界/竞态）。
- `update.rs` `write_identity_for` 的 `expect` 非可达（前面已 `is_object` 归一）。

### 4.4 去重说明（本报告 §2 与下级报告）

- 下级 `RTC-01` ≡ 本报告 **S2B-12**；下级 `RTC-04` ≡ 本报告 **S2B-13**。
- 下级 `MIR-02` 与本报告 **S2B-4** 同型（都是「HTTP 响应体无大小上限」），但分处 `mirror.rs` 与 `node.rs`，
  建议按同一修法一并处理（限长/查 Content-Length）。
- 下级 `RTC-05` 是本报告未单列的**新 P2**：把「`ensure()` 同步调用 + 未 spawn_blocking」补进
  S2B-1/S2B-2 的同族（后端预算/阻塞）一起收口。

---
*本报告为 S2-B 片（热路径/稳定性/工具链）；含下级分片 S2-B-rel 的结论。*
