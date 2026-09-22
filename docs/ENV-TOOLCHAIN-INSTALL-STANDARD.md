# 环境工具链检测与安装规范（ENV-TOOLCHAIN-INSTALL-STANDARD）

> **本文件是「壳的环境检测 / 安装 / 下载」的唯一事实源（SSOT）**，2026-09-16 立。
> 目标：Node 与 npm **并行同权**检测与安装；全平台同一逻辑；引导页所有安装/下载**同一 UI 规范**。

---

## 1. 根因（为什么立此规范）

真机：干净机器上 npm **没有被安装**。取证：

- `src-tauri/src/main.rs::run_install` 只做 `node::install` + `node::probe_after()`（**只校验 node 版本**），
  安装完成后即报成功并 `emit env_done` —— **npm 缺失也被判定为「环境就绪」**；
- 前端 `20-env.js` 的 npm 分支（`st.npmOk === false`）再次调用**同一个** `start_node_install`，
  而该管线仍只装 node → **永远补不上 npm**（死循环式重试）；
- 各步骤（Node / 内核 / 桌面壳）**各自持有自己的进度样式**：`showProgress/hideProgress` + `#prog/#progBar`
  与 `shell_update_progress` 的纯文字风格并存 → 四分五裂。

**架构结论**：node 与 npm 必须是**同一条工具链管线**里并列的必需项，检测与安装都不得只看 node。

**第二轮（2026-09-20）：装了 npm 却看不见 npm。** 上一轮把 npm 拉进了判定链，但**事实的形状**没跟上：

- 运行期契约（`runtime.json`）只记 node 的版本，npm 只记路径 —— npm 的版本在源头就不存在；
- 安装管线返回 `(node_path, version)` 元组，完成事件 `install_done { kind: "npm", version }`
  于是只能拿 node 的版本填进去 —— 引导页念出的「npm 已就绪（v22.x）」**从来不是 npm 的版本**；
- 前端把两个版本号散成 `NS.nodeVer` / `NS.npmVer` 两个字段，三个轮询点各写各的，
  成功路径只写 node 那一半 —— 于是「环境就绪」这一行只剩 `Node.js v22.x 已就绪`。

**架构结论（同一条）**：工具链的每一项都必须**带着自己的版本走完整条链**（契约 → 事件 → 文案），
且这份事实只有一个所有者、一个读取口。拼接字符串对类型系统完全合法，所以归属只能靠门禁对账（G-7/G-8）。

**第三轮（2026-09-21，Windows 真机）：npm「装过、探过」，用户看到的仍是缺失。** 前两轮把 npm 拉进了
判定与播报链，但**「可用」的判据本身在两处失守**：

- 契约的 npm 解析口 `probe_npm` 只判 `is_file()`，按优先级给出的第一个候选在 Windows 上是 `npm.cmd` ——
  `.cmd` 是 cmd.exe 的脚本，CreateProcessW **执行不了它**（ERROR_BAD_EXE_FORMAT），而所有消费者
  （`npm install -g`、`npm prefix -g`）都是 `Command::new(npm)` 直接 spawn。探针当时却经 `cmd /C`
  包装跑 `--version` —— **探针与消费者走了两条不同的 spawn 路径**：面板可以报「可用」，装内核必失败；
  而去掉包装后，同一个 npm 又立刻被报成「缺失」。**包装是胶水**：它把「本平台拉不起来」藏成了成功。
- Windows 的 Node 归档由 PowerShell `Expand-Archive` 解出：PS 5.1 走 .NET Framework 的 `ZipFile`，
  路径超过 260 字符的条目被**静默丢弃且退出码为 0**。npm 的依赖树必然超深，而浅层的 `node.exe` 完好 ——
  `commit_user_node` 当时只校验 node 可执行，于是**一棵残缺的树被落定为「安装成功」**。

**架构结论（同一条）**：「文件存在」不等于「可用」，「退出码 0」不等于「载荷完整」。可用性判据必须
**与消费者共用同一条 spawn 路径**（平台层给出可执行性判据，选择程序时就排除不可执行的垫片，见 T-10），
且**解包落定必须以整棵工具链校验为准**（T-11）。面板还要能说出**为什么**不可用（`npmWhy`，T-1d）：
「归档解残缺」「垫片拉不起来」「npm 自己报错」三类的处置完全不同，只报「缺少 npm」等于把排障推给用户。

**第四轮（2026-09-21，B4）：进度「有字段」，但它不对应任何被测出来的量。** 前三轮把 npm 拉进了判定、
播报与 spawn 链，剩下的缺陷在**进度语义本身**：

- `install_progress` 有**两个发射点**（`main.rs::push_status` 与 `commands/mod.rs` 里壳更新内联的 `json!`），
  同一件事两个作者，形态必然分叉；
- `kind` 只枚举 node/npm，内核与桌面壳以裸字符串过线 —— 「哪些东西可被安装」在 Rust 与前端
  （`10-ui.js` 的 `INSTALL_TARGET` 有四个键）各有一份账；
- **没有真进度源**：内核安装是 `npm install -g`（单源上限 15 分钟），期间一个字都不发；
  Node 归档下载只发一句写死的「约 30~50MB…」；阶段分数 0.1 / 0.3 / 0.85 按**代码顺序**编出来，
  与真实进度无关。前端删掉进度条（T-7）正是因为这种数字会骗人 —— 那它们就不该继续出现在线上协议里。

**架构结论（同一条）**：进度语义必须**只有一个所有者**（`src/domain/install.rs`），并且只承认两种可交付的
进度：**可测分母**（下载字节比，文案与比值由同一处从 `(done, total)` 算出）与**如实的阶段/心跳文字**
（`npm install` 不吐百分比，能如实说的只有已用时长 + 它自己写了多少行 + 末行）。
无分母时 `progress` 发 `null`，不猜（T-13）。

---

## 2. 后端契约（Rust，冻结）

### 2.1 状态查询 `node_status`（唯一环境状态读取口）

```jsonc
{
  "installed": "v22.12.0" | null,   // node 版本
  "minOk": true | false,              // node 是否达 MIN_NODE
  "minRequired": "v22.12.0",
  "npmOk": true | false | null,       // npm 是否可用（**与 node 同权**）；null=本轮未取到 node，未知
  "npmVersion": "10.9.2" | null,      // npm 真实执行 `npm --version` 得到的版本；未执行为 null
  "npmPath": "/abs/npm" | null,
  "npmWhy": "文字" | null,             // npmOk 为 false 时的**不可用原因**（查过哪些路径 / 执行失败详情）；可用时为 null
  "nodePath": "/abs/node" | null,
  "busy": true | false,
  "status": "文字（供 UI 直接显示）",
  "progress": null,                   // `0..1` 或 null —— null = 这一步**没有可测分母**（多数阶段行如此）
  "probeError": "..." | null,
  "stuck": {...} | null,
  "probes": [ { "probe": "node", "source": "PATH", "target": "/abs/node", "ms": 12, "ok": true, "note": "v22.12.0" }, ... ]
}
```

**不变量 T-1**：`npmOk === true` 当且仅当 npm 被**真实执行**通过（`probe_npm_usable`）；不得伪造。
**不变量 T-1b**：`npmOk` 是**三态** —— `true`=真实执行通过，`false`=node 已知而 npm 解析/执行失败，
`null`=本轮没取到 node 路径（未知）。未知**不等于**可用，UI 只允许在 `npmOk === true` 时放行；
也**不等于**不可用，不得因 `null` 触发重装（那是无根因的空转）。
**不变量 T-1c**：`npmVersion` 只在真实执行过 npm 时非空；未执行必须是 `null`，**不得**用空串或
node 的版本号占位。
**不变量 T-1d**：`npmOk === false` 时 `npmWhy` 必须非空，且它必须是 `probe_npm_usable` 的 `Err` 文案本身
（不是前端拼的猜测）。npm 不可用有三类成因（载荷缺失 / 垫片本平台拉不起来 / 执行报错），处置各不相同，
原因不上屏就等于把排障推给用户。
**不变量 T-2**：`busy === true` 期间 `installed/minOk/npmOk` 允许为中间态，UI 必须只依赖 `busy/status` 展示。

**不变量 T-14（探测记录只有一个所有者）**：`probes[]` 是环境探测结论的**唯一**载体，由
`src/domain/probes.rs` 登记与渲染（`Record::json` 出 JSON、`Record::render`/`render` 出文本）。
命令层不得再自行拼字段形态，前端不得再抄一份维度表 —— 名字与顺序都来自数据。
判据：G-13。

**不变量 T-15（`ok` 三态，未知不等于失败）**：`ok` 为 `true` / `false` / `null`，`null` 表示
「这一步还没跑完」或「无从判定」（例：本轮没取到 node，npm 就无从判定）。旧实现在飞步骤记成
`ok=false`，于是「还有一个候选没试完」被显示成「这个候选坏了」；面板与 CLI 必须原样区分（`?`）。
与 T-1b（`npmOk` 三态）、T-13（`progress: null`）同一形态纪律。

**不变量 T-16（探测自身不得成为新的卡死源）**：`node_status` 每 400ms 被轮询，而 npm / prefix
维度是**真实子进程执行**，故依赖维度按 TTL（`DEPENDENT_TTL`，当前 10s）复用缓存；node 安装完成处
由 `probes::invalidate_all()` 与 node 侧缓存**一起**作废（只失效一半会留下「新 Node + 旧 npm 结论」）。
prefix 维度先做本地固定盘判定再碰目录（网络盘上的 `exists()` 本身就可能是无界阻塞调用）。

**版本号形态**：node 自带 `v`（`v22.12.0`），npm 与内核/桌面壳都不带（`10.9.2`）。归一规则只在
`10-ui.js::versionLabel` 实现一次；任何播报点不得自行拼 `v`，否则同一行会出现两种写法且每处都可能写错。

### 2.2 安装 `start_node_install`（唯一环境安装口）

**实现位置**：管线本体住 `src/domain/install.rs::run_install`（不是 `main.rs`，也不是命令层）。
命令层只做 IPC 边界的事（`busy` 闸、快照的 `installed/error`、把 `InstallFailure` 归到 `install_error`
的 `kind`），**下载/安装的文字与比值一律由 `install.rs` 成形**。

**职责（顺序执行，缺一不可）**：
1. 解析并安装/修复 **node**（现有 `latest_lts → download_verified → install` 不变）；
2. 安装后**校验 npm**；若缺失 → **修复 npm**（见 §2.3）；
3. 两者都就绪才成功；任一失败 → 如实失败（绝不 emit done）。

**返回形态**：管线返回**运行期契约本身**（`runtime_contract::NodeRuntime`：node 路径/版本 +
npm 路径/前置参数/版本），而不是字段子集或元组。外层每一条播报、每一次落盘都必须取自它 ——
「某个 kind 的版本」在整条管线里因此只存在一份事实。

**不变量 T-3**：成功返回时，后续 `node_status` 必须给出 `installed != null && minOk && npmOk === true`。
**不变量 T-4**：失败必须 `emit` 错误事件并保留可操作文案；不得静默。

**解包与落定（三平台同一收口 `platform::commit_user_node`）**：解包器**逐条尝试**，每条都以
`commit_user_node` 的工具链校验为准 —— 校验同时要求 node 可执行**与** `probe_npm` 解析得出可用 npm，
不通过就换下一条解包器，全部不通过才失败（`T-11`）。Windows 的候选顺序是 `tar.exe`（bsdtar，宽字符路径）
优先、`Expand-Archive` 兜底；后者在 PS 5.1 上会**静默截断**超 260 字符的条目并返回 0，
所以它的退出码**不构成**载荷完整的证据（见 §1 第三轮）。

### 2.3 npm 修复策略（按优先级，跨平台）

**解析口只有一个**：`runtime_contract::probe_npm(node, binDir)`。候选表 `npm_shim_candidates`（错误文案
`npm_search_summary` 与安装校验共用同一张表），**每条候选都必须过 `Platform::is_directly_spawnable`**
才可能被选中 —— 判据来自平台层：Windows 只有 `.exe` 能被 CreateProcessW 拉起；POSIX 的 execve 认 shebang，
判据恒真。它放在 `Platform` trait 上而不是写成 `cfg` 分支，是因为「交出去的程序必须可直接 spawn」是
每个平台都要兑现的契约承诺（G1 也要求平台知识只留在 `src/platform/`）。
返回的 `(program, prefix_args)` 承诺「program 能被 `Command::new` 直接拉起」，探针与所有消费者
（`npm install -g`、`npm prefix -g`）因此天然走同一条 spawn 路径。

1. 官方分发包自带 npm：node 的用户级归档（zip/tar.gz）解包后，npm 通常已就位（先探测，命中即止）；
2. 未命中且存在 `<nodeBinDir>/node_modules/npm/bin/npm-cli.js` → 以 `node <npm-cli.js>` 形态可用（契约已支持 `npmArgs`）；
   Windows 上这是**常态而非兜底**：官方目录里的 `npm` / `npm.cmd` 都不可直接 spawn，只有 `node.exe + npm-cli.js` 可以；
3. 包内 CLI 也不存在（裁剪分发/解包不完整）→ **重新执行官方安装**（幂等）后复探；
4. 仍失败 → 如实失败，文案给出「手动安装 Node 官方分发包」的指引，并带上 `npmWhy`（T-1d）。

**禁止**：`npm config set`、写用户 `~/.npmrc`、改全局 registry（凭据与用户环境不得被污染）。

### 2.4 统一安装事件（**三平台同一形态**）

所有安装/下载类动作（node / npm / kernel / shell）统一发**同一组事件**：

```jsonc
// 进度（文字为主；progress 只在真有分母时非 null）
event "install_progress": { "kind": "node"|"npm"|"kernel"|"shell", "status": "文字", "progress": 0.35 | null }
event "install_done":     { "kind": ..., "version": "vX" | null }
event "install_error":    { "kind": ..., "error": "文字" }
```

- `kind` 是**枚举**，UI 据此决定文案前缀（不改变样式）；
- **`version` 归属 `kind`**：它就是该 kind 自己的版本号，不得填别的组件的版本（门禁 G-7）。
  node 与 npm 是两个 kind，因此工具链安装完成**各发一条** `install_done`；
- 该 kind 的版本未回读时发 `null`（事实层不写「未知」的文案变体，怎么念由 UI 唯一出口决定）；
- 旧事件 `env_progress` / `env_done` / `env_error` / `shell_update_progress` **一律删除**（无兼容层）。

**不变量 T-13（进度只有一个所有者，且只承认可测的量）**：

| 位置 | 唯一职责 |
|---|---|
| `src/domain/install.rs::emit_json` | `install_*` 三条事件的**唯一**发射点；`src/` 内出现第二处按事件名发射即红（G-12A） |
| `InstallKind::as_str` | `kind` 字面量的**唯一**来源（四类，与前端 `INSTALL_TARGET` 一一对应）；其余位置写 `"kind": "node"` 这类裸串即红（G-12B） |
| `install.rs::download_line` | 下载行的**唯一**渲染点：文案与比值同从 `(done, total)` 算出，杜绝「文字说 MB、比值另算一套」（G-12C） |
| `install.rs::npm_heartbeat` | `npm install` 期心跳文案的**唯一**渲染点：已用秒数 + 输出行数 + 末行，全部取自 `bounded::Live` 的真实现场 |
| `RunState.progress: Option<f32>` | 类型本身就是判据：裸 `f32` 无法表达「这一步没有可测分母」，而那正是编造分数的入口（G-12D/E） |

推论（写进代码的就是这几条）：

1. **只有两种合法进度**：按字节下载的比例（Node 归档、桌面壳安装包），或如实的阶段/心跳文字 + `null`。
   阶段分数（按代码顺序写的 `0.1 / 0.3 / 0.85`）**全部删除** —— 它们与真实进度无关。
2. 心跳数据来自**唯一的有界执行器**：`bounded::run_watch` 在子进程运行期间周期回调 `Live { elapsed, lines,
   last_line }`（输出重定向到临时文件，不经管道，所以能如实说的只有行数与末行）。`core::install_version`
   把它透传给调用方，**自己一字文案都不拼**（组装层不写用户文案）。
3. 服务端未给 `Content-Length`、给的总量为 0、或**总量小于已取回量**（ureq 的 `into_reader()` 在
   `Transfer-Encoding: chunked` 时忽略 Content-Length 一路读到流结束）→ 一律退回「已取回 N MB」+ `null`。
   文案自相矛盾比少一根进度条糟糕得多。
4. 内核安装**开工即说清预算**：`正在安装内核 v…：共 N 个镜像源，逐源尝试（单源上限 15 分钟 · 总上限 17 分钟）`，
   换源时发「第 i/N 个源 …」——用户报的「不知道是不是卡住」，缺的从来不是进度条，是**说清预算**。
5. 「正在下载」/「正在安装内核」这类措辞在 `install.rs` 之外出现即红（G-12C）；反向保证 `install.rs` 必须
   **真的**持有它们，否则「不得有第二处」会退化成「一处都没有」的空转门禁。

---

### 2.5 环境探测记录（`domain/probes.rs`，2026-09-22 收口）

**维度表**（唯一来源是 `enum Probe`，新增维度只改这一处）：

| `probe` | 探测内容 | `source` | `target` | `note` |
|---|---|---|---|---|
| `node` | 逐候选定位（记录路径 / 已知落点 / PATH） | 候选来源 | 候选绝对路径 | 版本号，或不可用的原因 |
| `npm` | 与本轮 node **同源**的 npm 真实执行（T-1b） | `与 node 同源` | npm 路径或其所在 bin 目录 | npm 版本号，或 `probe_npm_usable` 的 `Err` 文案 |
| `registry` | npm 源可达性 | `镜像预热缓存`（**不是**实时探测） | 选中的源，或「N 个候选源」 | 延迟，或「全部 npm 源不可达」 |
| `prefix` | `npm prefix -g` 落点目录**可写** | `npm prefix -g` | 前缀目录 | 「目录可写」或写入失败原因 |

**为什么 `registry` 读缓存而不现测**：`node_status` 的「纯本地、无网络 I/O」是门禁 B27 的硬约束，
也是「探针不会把引导页拖死」的前提；镜像侧的测速本来就由 `mirror_warmup` 独立在飞线程做，
结果经 `mirror::cached()` 读。故本维度的 `source` 明写「预热缓存」，排障时不会把它误当此刻的连通性。

**为什么必须有 `prefix` 维度**：内核安装是往 npm 全局前缀写文件。前缀不可写（`EACCES` / 只读 ACL /
装在他人账户下）时，安装在**跑了十几分钟之后**才失败，而面板当时只剩一个退出码 —— 环境阶段一条
`prefix` 记录就能把根因说清。写测试用唯一命名探针文件（写完即删），且只在本地固定盘上做。

**为什么 `npm` 与 `prefix` 共用一条 spawn 路径**：两者都走 `runtime_contract::run_npm_line`
（同一程序 + 同一前置参数，只换尾参）。这正是 T-10 的教训：`.cmd` 垫片经 `cmd /C` 包装后
「探针可用、消费者拉不起来」，缺陷只在其中一条代码路径上复现。

## 3. 前端契约（引导页，冻结）

### 3.1 检测与安装顺序（并行同权，`20-env.js`）

```text
stepEnv 轮询 readEnv()（= node_status 的唯一读取口）
   ├─ busy            → 等待（status 文字 + 逐维度探测结论，见 T-17）
   ├─ !installed      → 安装（kind=node）
   ├─ minOk === false → 安装/升级（kind=node）
   ├─ npmOk !== true  → 安装/修复（kind=npm）← **必须与 node 并列，不得复用 node 分支文案**；文案须回显 `npmWhy`（T-1d）
   └─ 全部通过        → stepNodeDone
```

**不变量 T-5**：npm 分支失败时**不得**继续进入内核步骤（否则会用不存在的 npm 装内核）。
放行条件写成 `npmOk !== true` 而非 `=== false`：`null`（未知）同样**不得**放行（见 T-1b）。
**不变量 T-8**：工具链快照只有一个写入点（`20-env.js::applyToolchain`，由 `readEnv` 调用）
和一个读取口（`readEnv`）。轮询点/分支不得自行登记版本字段，也不得绕过 `readEnv` 调 `node_status`
（超时预算与快照写入就会各写一遍，门禁 G-8）。
**不变量 T-9**：「环境就绪」这一行必须**同时**念出 node 与 npm。任一半缺失时如实说「版本未回读」，
**绝不**用另一组件的版本顶替。

**不变量 T-17（探测进度必须上屏，失败必须带「已探明」）**：`busy` 期间的 status 行尾附
`NS.probeSummary(st)`（按维度压缩的一行结论，进行中的那一步以 `?` + 耗时呈现）；两条环境失败路径
（`probeError` 与检测超时）同样必须带「已探明：…」。用户实测投诉的「检测环境过程当中看不到完整的
npm 检测」，根因就是这个出口不存在 —— 探测在跑，但结论只在成功后才被读出来。渲染只有
`10-ui.js::probeSummary/probeList` 一处，维度名与顺序全部来自 `probes[]` 数据（判据 G-13）。

### 3.2 统一 UI 规范（唯一实现，参照壳更新）

以**壳更新（`40-shell-update.js`）的纯文字风格**为基准：

- 结构：沿用既有 `steps` 步骤条 + `status` 单行文字；
- **删除进度条**：`#prog` / `#progBar` 元素与 `NS.showProgress` / `NS.hideProgress` **全部移除**；
- 所有安装/下载（node / npm / kernel / shell）**只显示文字**，形态统一为：
  `正在下载 <目标> … <进度文字>` → `正在安装 <目标> …` → `<目标> 已就绪`；
- 统一入口（`10-ui.js` 导出，**唯一实现**）：

```js
NS.install.begin(kind, text)   // 显示安装态并写文字（无进度条）
NS.install.text(kind, text)    // 仅更新文字
NS.install.done(kind, text)    // 完成文字
NS.install.fail(kind, text)    // 失败文字（走既有 fail 面板）
NS.versionLabel(v)             // 版本号形态归一（唯一实现）；null/空 → ''，由调用方说明未知
```

**不变量 T-6**：任何模块**不得**自行拼装下载/安装样式；一律调用 `NS.install.*`。
**不变量 T-7**：全仓不得再出现 `showProgress` / `hideProgress` / `progBar` / `#prog`（门禁强制）。
**不变量 T-7b**：诊断串（`10-ui.js::diagText`）与就绪行同源，同样必须 node 与 npm **并列** ——
排障时「npm 到底探到了没有」不该再靠读代码猜。

### 3.3 事件消费（`80-init.js`，唯一入口）

- `install_progress` → `NS.install.text(p.kind, p.status)`；
- `install_done` → `NS.install.done(p.kind, ...)`；
- `install_error` → `NS.install.fail(p.kind, p.error)`；
- 删除对旧事件的监听。

`p.progress` **可以**是 `null`（T-13：多数步骤没有可测分母）。前端只消费 `p.status` 文字，
不得为「把进度条画回来」而把 `null` 读成 `0` —— 那会诱导后端重新编造分数（T-7）。

---

## 4. 跨平台不变量

| 项 | Linux | macOS | Windows |
|---|---|---|---|
| node 可执行名 | `node` | `node` | `node.exe` |
| npm 垫片候选名（按优先级） | `npm`、`npm.cmd`、`npm.exe` | 同左 | `npm.cmd`、`npm`、`npm.exe` |
| **实际交出去的 npm** | 垫片本身（execve 认 shebang） | 同左 | `node.exe` + `npm-cli.js`（垫片全都不可直接 spawn） |
| 官方分发包 | `tar.gz` | `tar.gz` | `zip` |
| 解包器（按尝试顺序） | `tar` | `tar` | `tar.exe`(bsdtar)、`Expand-Archive` |
| 落定校验 | node **且** `probe_npm` 命中（`commit_user_node`） | 同左 | 同左 |
| 安装位置 | `<状态根>/node`（用户级） | `<状态根>/node` | `<状态根>/node` |
| 是否需要提权 | **否** | **否** | **否** |
| 事件形态 | 统一 `install_*` | 同左 | 同左 |

**不变量 T-10（spawn 路径同源）**：`probe_npm` 交出的 `program` 必须能被 `Command::new(program)` 直接
拉起 —— 判据是 `Platform::is_directly_spawnable`，它在**选择程序**时就排除 `.cmd`/`.bat`/无扩展名脚本这类
本平台拉不起来的垫片。探针不得对程序做任何 shell 包装：**包装会把「不可执行」藏成「探针成功」**，
而消费者（`npm install -g`、`npm prefix -g`）随后必然失败。平台知识只允许出现在 `src/platform/`（门禁 G1），
因此这条判据是 trait 方法而不是调用点的 `cfg` 分支（门禁 G-9）。

**不变量 T-11（落定以整棵工具链为准）**：解包成功 ≠ 载荷完整。`commit_user_node` 必须同时验 node 可执行
**与** npm 可用（同一个 `probe_npm`），否则拒绝落定并换下一条解包路；既有安装不得被半成品覆盖（门禁 G-10）。
理由：`Expand-Archive` 在 PS 5.1 上会静默丢弃超 260 字符的条目**且退出码为 0**，npm 的依赖树必然超深。

**不变量 T-12（真实归档必须在 CI 上跑过）**：`zip 解包 → npm 可用` 整条链由 Windows leg 的
`official_artifact_installs_usable_npm`（`--ignored`，下载官方归档、走生产 `install_node`）驱动。
静态门禁只能证明「代码里写了判据」，证明不了「本平台解出来确实有 npm」—— 这条链此前从未被执行过，
所以缺陷只能靠用户报障发现。
该步骤以**全路径** `--exact` 指定测试，并对 `test result: ok. 1 passed` 把一次关：
`--exact` 配短名是零命中且退出码 0，「加了实测步骤」与「步骤什么都没跑」在 CI 上完全同形（门禁 G-11）。

---

## 4bis. 权限模型（2026-09-18 重写，跨平台）

**结论：Node 安装不再需要任何提权。** 三平台统一把官方归档解到用户可写的
`<状态根>/node`，不做系统级安装。

**为什么**（原系统级安装的失败模式）：

- Windows `.msi` + `Start-Process -Verb RunAs`：UAC 提升到**管理员账户**后，常读不到
  当前用户 profile 下的 `.msi` → **msiexec 退出码 1619（安装包无法打开）**；
  且 `canonicalize()` 在 Windows 返回 `\\?\` 前缀路径，msiexec 不认。
- macOS `.pkg` + `osascript ... with administrator privileges`：需要系统授权弹窗，
  且官方**没有** osx-arm64 pkg。
- Linux `tar -C /usr/local` + pkexec/sudo：容器 / WSL / SSH / 精简发行版常无可用
  polkit agent 或 sudo；且 `tar -xJf` 依赖 xz。

**模型**：

1. **默认（唯一）路径 = 用户级、零权限**：下载官方归档（Windows `win-{arch}.zip` /
   macOS `darwin-{arch}.tar.gz` / Linux `linux-{arch}.tar.gz`）→ 解到
   `<状态根>/node.extract` → 校验 node 可执行 → **原子替换** `<状态根>/node`。
2. 运行期契约（`runtime.json`）记录该绝对路径；守卫由 `<壳> --run-guard` 经契约定位 node，
   故 **systemd/launchd/schtasks 不需要 PATH 里有 node**。
3. **提权只与壳自更新（替换安装包）有关**，且由各平台自身通道完成
   （Windows 安装程序 / macOS updater / Linux pkexec→sudo），与 Node 安装解耦。
4. 无权限、跨账户、跨文件系统都不再影响 Node 安装。

---

## 5. 门禁

| 门禁 | 断言 |
|---|---|
| G-1 | `node_status` 含 `npmOk`/`npmPath`/`npmVersion`，且来源是真实探测（`probe_npm` / `probe_npm_usable`）；`npmWhy` 必须在回传字段里，且 `probe_npm_usable` 的签名是 `Result<NpmUsable, String>`（返回 `Option` 就把三类成因合并成一个 `None`，原因无法上屏） |
| G-2 | `run_install` **函数体**（现在住 `src/domain/install.rs`；或被委派的 `node::finalize_install` 函数体）里真实调用 npm 可用性探测（`derive_usable`/`probe_npm_usable`）；行注释不算证据 |
| G-3 | 全仓无 `showProgress`/`hideProgress`/`progBar`/`id="prog"` |
| G-4 | 全仓无旧事件名（`env_progress`/`env_done`/`env_error`/`shell_update_progress`） |
| G-5 | `20-env.js` 存在独立的 `npmOk !== true` 分支且**不得**与 node 分支共用安装文案；分支文案必须回显后端的 `st.npmWhy`（T-1d） |
| G-6 | 前端所有 install 文案经 `NS.install.*`（无自行拼装） |
| G-7 | 每条 `install_done` 的 `version` 归属其 `kind`：node 事件取 node 版本、npm 事件取 npm 版本，npm 事件**混入 node 版本即红**（按 `handle.emit(...)` 调用切块对账，不按字符窗口） |
| G-8 | 工具链快照只有一个写入点（`applyToolchain`）与一个读取口（`readEnv`）；`NS.nodeVer`/`NS.npmVer` 不得复活；就绪行与诊断串同时含 node 与 npm |
| G-9 | 探针与消费者**同源**（T-10）：`probe_npm` 的函数体必须过 `is_directly_spawnable`；`src/` 在 `platform/` 之外不得出现 `cmd.exe` / `cmd /C` 包装（只看代码行，注释里的历史说明不算证据也不触发红） |
| G-10 | 落定校验（T-11）：`commit_user_node` 函数体必须调用 `probe_npm` —— 只看 node 可执行就会把截断树落定成「安装成功」 |
| G-11 | 真实归档实测（T-12）：`build.yml` 必须按**全路径** `--exact` 执行 `official_artifact_installs_usable_npm`，并断言「恰好 1 passed」；测试本身须带 `#[ignore]`（短名零命中也会绿，那一步就成了空转） |
| G-12 | 进度语义单一所有者（T-13）：`src/` 内不得有第二处发射 `install_*`；`kind` 字面量不得以裸串出现在 `InstallKind::as_str` 之外；「正在下载」「正在安装内核」不得在 `install.rs` 之外拼装；`.progress =` 在非所有者处必须显式 `Some(..)/None`；`RunState.progress` 必须是 `Option<f32>`。前置断言要求所有者**真的**发射三条事件并持有两句措辞（否则「不得有第二处」= 空转） |
| G-13 | 环境探测记录单一所有者（T-14/T-15）：`pub enum Probe`、记录 → JSON 的字段形态（同时含 `"probe":` 与 `"note":`）、`probe_npm_usable(` 的调用，三者只允许出现在 `src/domain/probes.rs`（`probe_npm_usable` 另允许其定义处 `runtime_contract.rs`）；全仓不得有 `struct TraceEntry`；记录形态里的 `ok` 不得是裸 `bool`（与 `pub ms:` 同现即红）；前端 `.probes` 只在 `10-ui.js` 解析，且不得出现 `env_trace=`。前置断言要求所有者**真的**登记四个维度（`Probe::Node/Npm/Registry/Prefix`）、持有两种渲染（`Record::json` 与 `render`）并带缓存上界（`DEPENDENT_TTL`）与本地固定盘门槛（`is_local_fixed_dir`） |

判据位置：`src-tauri/tests/env_toolchain_standard_test.rs`（G-7/G-8/G-13 的判据抽成纯函数，并各自带
**旧形态反向夹具** —— 认不出旧形态的判据等于空转）。契约字段的行为面在
`src-tauri/src/runtime_contract.rs` 的内置单元测试（写读往返、不伪造 npm 版本）。
G-9/G-10 的**平台侧行为面**在 `src/platform/windows.rs` 的 `toolchain_tests`（`.cmd` 不可直接 spawn、
完整树解析为 `node.exe + npm-cli.js`、残缺树拒绝落定），以及 `official_artifact_installs_usable_npm`
（T-12：真机下载官方 zip 并走生产 `install_node`，由 `build.yml` 的 Windows leg 以 `--ignored` 执行）。