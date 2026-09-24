# 内核 / 桌面壳 职责边界与抽取审计

> 2026-09-11 · 目的：回答「哪些能力应归桌面壳、哪些必须留内核、二者如何配合」
>
> ## 本文件描述的是**改造前状态**，结论已执行
>
> 文中的缺陷（§2.3）**绝大部分已修复**，`文件:行号` 是**审计时快照**（行号因重构已移位）。
> **执行状态与未修项见 `DESIGN-COMPLETE.md` 的「§25 诚实说明」（未确认项）与「§20 执行批次」。**
> 本文件是**决策记录**，不是愿望清单。每条判定都给出判据与证据。

---

## 一、判据（先定规则，再逐项判定）

四条**硬约束**决定了归属，不是偏好问题：

| # | 约束 | 来源 |
|---|---|---|
| **R1** | **引导顺序**：用户装壳时**内核尚不存在**（空壳形态）。凡在「装内核之前/期间」需要的能力，**必须在壳里**。 | 产品形态（用户明确表述）|
| **R2** | **无头运行**：内核是系统服务，必须能在**无 GUI、无壳**时完整运行（Linux 经 `loginctl enable-linger` 在注销后仍活）。凡运行期需要的能力，**必须在核里**。 | `service.rs` linger + 三平台服务定义 |
| **R3** | **语言边界**：壳是 Rust、内核是 JS，**无法共享代码**。故「抽到壳里」只能有三种落地方式：① 整体搬迁；② 壳拥有定义、内核消费**产物**；③ 统一**规格 + 测试向量**。 | 工程事实 |
| **R4** | **面板可脱离壳**：面板由内核托管，**浏览器也能打开**（`http://127.0.0.1:<port>/`）。故面板功能不得依赖壳在场。 | `api/index.js` resolveUiDir + README |

### 由 R1–R4 推出的判定规则

```
能力 X 的归属：
  X 在「装内核之前/期间」需要？           → 壳（R1）
  X 在「无 GUI/无壳」时仍需工作？          → 核（R2）
  两侧都需要，但只是「同一份事实/规格」？  → 壳拥有定义，内核消费产物（R3-②）
  两侧都需要，且是「同一套行为语义」？     → 统一规格 + 测试向量（R3-③）
  只有一侧需要？                          → 留在那一侧，不做无意义搬迁
```

---

## 二、审计结果（内核 80 文件 / 19354 行 —— 审计时快照，非当前规模）

### 2.1 内核**运行期专有**（R2 → 必须留内核，约 95%）

| 分域 | 行数 | 职责 | 为何不能抽走 |
|---|---|---|---|
| `guard/` | 5063 | 守卫自身：生命周期、监督循环、端口仲裁、进程守护 | 守卫在无壳时运行（R2）|
| `domains/instance` | 987 | DSH 实例生命周期（原生 + 沙箱）| 运行期管理，面板可脱离壳操作（R2/R4）|
| `domains/router` | 4774 | 智能路由 / 供应商 / 配额 | 纯运行期（R2）|
| `domains/relay` | 1444 | 局域网反代 / 公网暴露 | 纯运行期（R2）|
| `domains/plugin` | 1281 | 插件市场 + 安装管理 | 运行期 + 面板（R2/R4）|
| `domains/shell` | 507 | 壳更新安全网 + **壳看护** | **必须在内核**：壳崩溃时壳不存在，只有抗重启的守卫能救它（R2）|
| `api/` | 1508 | 本地 HTTP 网关 + 面板后端 | 面板由内核托管（R4）|
| `platform/` 的日志/事件/token/tasks/config/fs/exec/version/deploy | 约 1400 | 运行期基础设施 | 与 DSH 运行强绑定（R2）|

**结论：内核 19354 行中，约 18900 行归属明确，不该动。**

### 2.2 真正的重叠面

**统计口径**（精确到模块，非估算）：

| 重叠形态 | 内核侧规模 | 说明 |
|---|---|---|
| **完全重复的常量** | 约 **30 行** | 6 个 npm URL 的**三份副本**（`mirror.rs` / `dist` / `config.js`），逐字节相同 |
| **同一关注点的双实现** | 约 **390 行** | `env-catalog.js` 83 + `exec-path.js` 92 + `file-protect.js` 94 + `process.js` 45 + `dist` registry 部分约 76 |
| **已定案、无需再抽** | — | `autostart.js` 354（2026-09-11 已按所有权矩阵划分）|

**注意措辞**：重复在**内核侧**（它持有副本）。壳侧对应模块（`mirror.rs` 402 /
`nodeprobe.rs` 639 / `core.rs` 466 / `env.rs` 272 / `service.rs` 258 = 2037 行）是
**供给层的所有者**，本身不是「重复」—— 除非我们让内核继续持有第二份。

**故本次统一的目标：内核侧约 420 行的副本 → 降级为「最小兜底」。**

| # | 能力 | 壳侧 | 内核侧 | 重叠性质 |
|---|---|---|---|---|
| **O1** | **npm 镜像目录** | `mirror.rs` NPM_PRESETS **6 条** | `dist/index.js` REGISTRY_PRESETS **6 条** + `config.js` registries **6 条** | **逐字节相同的 3 份副本** |
| **O2** | **镜像探测方法** | 真实包元数据（`@dsh-sup/dsh-core-<plat>`）| `/-/ping` | **方法不同 → 选出的源不同**（实测 ustclug 2613ms vs 389ms）|
| **O3** | **安装执行规格** | `core.rs` npm install（含三平台提权）| `dist/index.js` `runNpmInstall`（不提权）| 语义重叠，上下文不同 |
| **O4** | **环境探测** | `nodeprobe.rs`(639) + `env.rs`(272)：候选枚举、版本、PATH 扫描、**最低门槛 v22.12** | `env-catalog.js`(83) + `exec-path.js`(92)：`which --version` | **判定标准不一致**（壳有门槛，内核只看「which 成功」）|
| **O5** | **平台原语** | `bounded.rs`(191) 进程/超时；`env.rs` PATH 目录 | `file-protect.js`(94) / `process.js`(45) / `exec-path.js`(92) | 语言不同，**规格应统一** |
| **O6** | **自启与服务定义** | `service.rs`(258)：三平台服务定义（守卫）| `autostart.js`(354)：开关 + GUI 自启 | **已定案**（见 D5）|

### 2.3 已被证实的缺陷（重叠的代价）

| 缺陷 | 证据 |
|---|---|
| 镜像目录三份副本 | 壳 6 + 内核 6 + 内核 config 6 → 加一个源要改 3 处 |
| 探测方法分叉 → 选源不一致 | 内核选 `repo.huaweicloud.com`(75ms)、壳选 `registry.npmmirror.com`(57ms) —— **用户在两处看到不同镜像** |
| 契约**只写了 `origins`，未写 `catalog` 与探测规格** | `export_to_kernel` 有 **2 个调用点**：`node.rs:178`（`latest_lts()` 内，壳启动的 setup 后台线程会走到，**成功时**导出）与 `main.rs:1079`（用户手动改镜像）。但两者都只导出被选中的 `origins` —— **不含全集、不含探测方法规格**，故 O2 的方法分叉仍在，且 `latest_lts` 失败（离线/全源不可达）时完全不导出 |
| Node 判定标准不一致 | 壳要求 ≥ v22.12；内核 `which node` 成功即视为就绪 → 面板显示「环境就绪」而壳拒绝启动内核 |
| 版本校验语义分叉 | 壳 `is_valid_version` vs 内核 `VERSION_RE` —— 实测 3 处分歧（`1.0.0+`、`1.0.0+!!!`、`1.0.0+あ`）|

---

## 三、抽取决策

### D1 镜像目录与选择 —— **壳拥有，内核消费产物**

| 项 | 决策 |
|---|---|
| 目录（catalog）| **壳是唯一真源**（它有超集：Node 10 源 + npm 6 源 + 壳 2 源）|
| 探测方法与排序 | **规格随产物投放**，内核按其执行 → 两侧给出**同一答案** |
| 内核的硬编码副本 | 降级为**最小兜底**（契约缺失时用），不再是一等来源 |
| 投放时机 | **壳启动时无条件导出**（`main.rs:228` → `mirror.rs::export_on_boot`）+ **每轮 npm 逐源测速后重投**（`mirror.rs::record_npm_measurements`）+ **Node 源选定后重投**（`node.rs:151`）。探测失败也要写 —— 契约是内核的增强，不是壳启动的前提 |
| 契约文件 | **拆成两份，按写者分**：`<状态根>/supervisor/registry.json` = **壳写内核读的证据**（目录 + 探测规格 + 逐源实测，schema 3）；`<状态根>/supervisor/registry-choice.json` = **内核唯一写者的选择**（`mode`/`manualOrigin`/用户候选 `origins`）。壳从不写选择，内核从不写目录（路径记法见 §四 开头）|

**为什么不是「内核调壳」**：内核在无壳时也要选源（自升级、装 DSH/插件）—— R2。

### D2 安装执行 —— **规格共享，执行各自保留**

| 项 | 决策 |
|---|---|
| 执行器 | **两份**（不可合并）|
| 理由 | 装的是**不同对象**、活在**不同约束**里：壳装**内核**要在 GUI 里同步回传结构化证据（命令/prefix/源/两次输出），并按壳持有的镜像 catalog 选源；内核装 **DSH/插件**必须**无人值守**（壳可能根本没开）—— R2 决定它不能反过来调壳 |
| 提权 | **两条安装链都不主动提权**：Node 安装是用户级零权限（`platform/*.rs::install_node`，门禁 A-2）；内核 `npm install -g` 若落在只读前缀（如系统 Node 目录）只回传证据、**不擅自换前缀**（`core.rs::is_node_install_prefix`）。全仓唯一的提权消费者是**壳自更新**（deb 落系统位置），由 tauri-plugin-updater 承担 |
| 共享部分 | npm 参数形态、registry 注入、超时、输出捕获 —— 写进**规格**（文档 + 测试向量）|

### D3 环境探测 —— **规格统一，代码各自保留**

| 项 | 决策 |
|---|---|
| 壳 | **供给前探测**（决定装不装、装什么）|
| 内核 | **运行期自检**（面板展示、依赖可用性）|
| 统一项 | **最低门槛**：内核必须采用壳的 `MIN_NODE = v22.12` 判定，不得只用 `which node`（否则面板会谎报「环境就绪」）|

### D4 平台原语 —— **统一规格 + 测试向量**

跨语言无法共享实现，但可共享**行为规格**（同一组输入 → 同一组输出）：

| 原语 | 共享形式 |
|---|---|
| 版本比较 / 版本合法性 | 一份 `shared/version-vectors.json`，两侧测试套件都跑 |
| 可执行解析（PATH / 标准目录 / Windows 扩展名）| 规格文档 + 各自测试 |
| 敏感文件保护 | 规格文档（Unix `0600/0700`；Windows ACL 收紧继承）|

### D5 自启与服务定义 —— 已定案（2026-09-11）

| 产物 | 唯一所有者 |
|---|---|
| 守卫服务**定义**（unit / plist / 计划任务）| **桌面壳**（`platform/service.rs` 定 `ServiceControl` trait；三平台实现在 `platform/{linux,macos,windows}.rs`）|
| 守卫自启**开关**（enable/disable）| **内核**（面板）|
| 壳（GUI）自启产物 | **内核**（`app/settings/autostart.js`）|
| 壳崩溃自愈 | **守卫看护**（`domains/shell/watchdog.js`）|

详见内核仓 `PLATFORM-CAPABILITY-MATRIX.md §六`。

**自启开关单写者的执法点**（2026-09-22，IL-2 壳仓半边）：`platform/linux.rs::ensure_defined` 里
`systemctl --user enable` 与 `loginctl enable-linger` **只在定义首次建立时**执行；内容过时的自愈路径
只写 unit + `daemon-reload` 后立即返回。此前每次升级都重放 enable/linger，于是用户在面板关掉自启后，
下一次模板演进会把自启位重新打开 —— 那是本表第二行所禁的第二个写者。
判据：`d5_definition_self_heal_does_not_rewrite_autostart`（含三种回归形态的反向合成样本）。

### D6 不做的事（诚实说明）

| 不做 | 理由 |
|---|---|
| 把 DSH/插件安装移到壳 | 内核在壳关闭时必须能自升级/装插件（R2）|
| 把服务管理（`platform/os/service.js`）移到壳 | 它管的是 **DSH 实例**的 systemd transient 单元，与壳的守卫服务是**不同对象**（Linux 专有属设计使然）|
| 把日志合并 | 内核 `EventHub` 是**有不变量的子系统**（单写者、seq 全局单调、跨重启续号），壳日志是启动轨迹。**合并会破坏不变量**；应做的是**统一格式 + 统一读取视图** |
| 追求「代码量减少」| 审计时估算：内核侧待统一副本约 420 行（占当时内核 2%）。**已按 D1 落地为「契约优先 + 最小兜底」**（内核 `platform/distribution/registry.js` 读契约，`platform/contract/registry.js` 校验），兜底副本按设计保留 —— 收益是**「只有一个答案」**，不是行数 |

---

## 四、合作契约（跨语言「共享」的唯一可行形态）

> **路径记法**：本章的 `<状态根>` = **产品状态根**，与 DSH 的 `~/.dsh` 无关。
> 覆盖变量 `DSH_SUPERVISOR_HOME`；默认 Linux `~/.local/state/dsh-supervisor`、
> macOS `~/Library/Application Support/dsh-supervisor`、Windows `%LOCALAPPDATA%\dsh-supervisor`。
> 单一事实源：内核 `src/platform/service/state-root.js`、壳 `src-tauri/src/env.rs::state_root()`，
> 两侧 schema 常量由门禁握手。契约文件只落在 `<状态根>/supervisor/`（内核读写）与
> `<状态根>/shell/`（壳读写）；`~/.dsh/{supervisor,shell}` 仅是启动期一次性前向迁移的**源**，
> 不再是任何读写路径。

### 4.1 契约文件：`registry.json`（证据）与 `registry-choice.json`（选择）

同一份文件不允许有两个写者。schema 2 时代「证据」与「选择」挤在 `registry.json` 里，留下了两处互相
让步：内核读回原文只为覆盖 `mode/manualOrigin/origins` 三个键；壳见到 `mode=manual` 就得冻结自己写的
契约。schema 3 按写者拆开后，两处让步一起删除 —— **壳只投证据，选择归内核**。

`<状态根>/supervisor/registry.json` —— **壳写、内核只读**（`mirror.rs::contract_doc`，键集合由壳单测
`contract_doc_is_evidence_only` 逐键**等值**判定，多出一个 `mode`/`selected` 就红在这里）：

```jsonc
{
  "schema": 3,                      // 内核 SUPPORTED_SCHEMA 校验：高于它的值整份不采用（contract-schema-newer）
  "writtenBy": "shell@1.2.9",       // 谁写的、什么版本（排障用，不参与选源）
  "writtenAt": 1789147076,          // 同上
  "catalog": [ "…" ],               // 镜像目录全集（壳持有的那份超集）
  "probe": {                        // 探测规格 —— 内核照它执行即可与壳得到同一答案
    "kind": "package-metadata",
    "pathTemplate": "@dsh-sup/dsh-core-win-x64",   // 本机平台事实拼出的具体包名（core.rs::package_name）
    "timeoutMs": 20000              // 由壳 PROBE_TIMEOUT（20s）派生：两侧超时不同会让「介于两者之间」的源一侧可达、一侧不可达
  },
  "measurements": [                 // 逐源实测：这是**证据**不是结论，内核按条件采用
    { "origin": "https://…", "ok": true, "latencyMs": 57, "error": null, "checkedAt": 1789147070 }
  ]
}
```

内核侧展开探测目标的是 `policies.js::resolveProbe`：`kind='package-metadata'` 且模板里有 `{platform}`
占位时按平台标签替换，模板已是具体包名时原样使用 —— 两条路得到同一个 URL；宿主不可产平台标签时**必须**
退化为 `/-/ping`，否则字面量 `undefined` 会被拼进路径而恒 404。

内核采用 `measurements` 的条件是三条同时成立（`src/platform/distribution/policies.js::shellProbeResults`）：
新鲜（`SHELL_PROBE_MAX_AGE_SEC` = 30 分钟内）、**覆盖本轮全部候选**、每源过形态闸；否则整批回退内核自测。
「只有全覆盖时『信壳测过』才等于『自己不用再测』」—— 缺一个源就下结论，面板的逐源卡会变成半空。

`<状态根>/supervisor/registry-choice.json` —— **内核写（唯一写者，`src/platform/distribution/registry-config.js`，
`CHOICE_SCHEMA = 1`，落盘 0600）、壳只读**：

```jsonc
{
  "schema": 1,
  "updatedAt": 1789147076,
  "mode": "auto",                   // auto | manual —— 用户在面板里固定过源没有
  "manualOrigin": "https://registry.npmmirror.com",
  "origins": [ "…" ]                // 用户自己维护的候选（优先于契约 catalog）
}
```

壳读它只为一件事（`src-tauri/src/core.rs::kernel_choice`）：用户在内核面板固定过源时，壳的内核安装/更新
必须打在同一个源上；`origins` 参与候选优先序，`updatedAt` 忽略。

### 4.2 生命周期

```
壳启动（main.rs:228）      → 无条件导出（探测失败也要写：内核完全拿不到源比拿到旧结果更糟）
每轮 npm 逐源测速后        → 落盘并重投（record_npm_measurements：只放内存快照的话重启就丢）
Node 源选定后（node.rs:151）→ 重投目录（Node 发行源与 npm 逐源实测无关，不碰 measurements）
用户在面板改选择            → 内核写 registry-choice.json，壳不感知也不需要感知（契约里从没写过选择）
内核读取                   → 选择文档 origins → 契约 catalog → 最小兜底（policies.effectiveOrigins 的顺序）
内核复测                   → 契约证据不满足三条采用条件时，按契约 probe 规格自测 → 与壳同法
```

### 4.3 其它契约

| 契约 | 方向 | 内容 |
|---|---|---|
| `<状态根>/shell/identity.json` | 壳 → 内核 | 版本、phase、pid、**exe**（看护定位用）、lastSeenAt |
| `<状态根>/shell/update-journal.json` | 内核内部 | 壳更新账本（to/confirmed）；**不含隐式回退/拉黑/冷却字段**（紧急回退走发布通道契约的 `rollback` dist-tag，不经此账本）|
| `shell-release/version-vectors.json` | 双向（测试）| 版本比较/合法性的共享测试向量（内核侧副本为 `shared/version-vectors.json`）|

### 4.4 不变量

| # | 不变量 | 违反后果 |
|---|---|---|
| **C1** | 契约文件**只有一个写入方** | 双写必然漂移（macOS plist 已发生过）|
| **C2** | 内核在契约缺失时**必须能降级运行** | 否则全新安装或契约损坏即不可用 |
| **C3** | 契约带 `schema`，不匹配时**明确拒绝**并记录 | 否则两仓静默错位 |
| **C4** | 两侧对同一问题的判定**必须给同一答案**（探测方法随契约投放）| 否则「面板显示一个、实际用另一个」|

---

## 五、结论

```
内核        19354 行，其中约 18900 行运行期专有（不动）
重叠        内核侧约 420 行副本（含 3 份逐字节相同的镜像副本）；壳侧为所有者

抽取方向（全部由 R1–R4 推出，非偏好）：
  镜像目录与选择   壳拥有，内核消费产物        D1
  安装执行         规格共享，执行各自保留      D2
  环境探测         规格统一（最低门槛一致）    D3
  平台原语         规格 + 测试向量             D4
  自启/服务定义    已定案                      D5

真实收益：不是行数，而是「同一个问题只有一个答案」。
```
