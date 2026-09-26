# Changelog（桌面壳）

本文件记录桌面壳（`dsh-supervisor-gui`，公开仓 `lobbowen/dsh-supervisor-launcher`）的重要变更。

## [未发布]

### 观测报告投放（P7）：壳把「本机实况」交给内核的环境表单

内核新增了一维「桌面壳所见」，读 `<状态根>/supervisor/shell-report.json`，而这份文件此前**没有写入者**：
装了壳的机器上这一维也永远显示「壳还没报过」。排障最需要的恰好是「装内核时真正用的那套 Node/npm」，
那只有壳知道（它的 npm 探针真实执行过，见 T-1b/T-10），内核自己解析到的可能是另一套。

- 新增 `src-tauri/src/shell_report.rs`：**只做投影与投放** —— Node 结论取 `nodeprobe::Outcome`、
  npm/prefix 与逐条记录取 `domain::probes`（形态仍出自 `Record::json`）、镜像源取 `mirror::cached()`。
  schema 1 与内核 `src/platform/contract/shell-report.js` 的 `SUPPORTED_SCHEMA` 握手；`tmp + rename`
  原子写（形态同 `runtime_contract::write`）；三态原样透传，`null` 不折成 `false`；写失败只记
  `shell.log`，不阻断引导、不排队重试。
- 投放点全仓唯一：`commands::node_status` 内、与启动契约同一轮探测的两个出口；频控下限取
  `DEPENDENT_TTL`（10 秒）—— 过了窗口 npm/prefix 才是重新真实执行得到的结论，刷新投放时刻才名副其实。
- 门禁 G-14（`env_toolchain_standard_test.rs`，带旧形态反向夹具）与不变量 T-18、规范 §2.6 同批落地。
- 跨仓对侧：内核接收口与环境表单的 `shell` 维已合入内核主干（`platform/contract/shell-report.js`），
  尚未随发布通道投放；两侧 schema 不匹配时内核整份按「读不出」处理，不会猜字段。

### 镜像契约文档同步（P0-D，与内核 `refactor/p0d-gates-docs` 同批）

1.2.9 把镜像源的证据与选择拆成两份文件，代码收口了、文档没有：`docs/` 里仍以 schema 2 的形状描述
`registry.json`，仍以「壳写 `selected`、内核优先采用」描述所有权。读文档的人据此写出的下一版壳会
把选择字段投回契约，而那份文件已不允许有第二个写者。本批把规范面改成与代码一致：

- `DESIGN-BOUNDARY.md` §4.1 重写为**两份文件各一份形状**（证据 schema 3 + 选择 schema 1），写清键的
  来源（`pathTemplate` 是 `core.rs::package_name` 按本机平台事实拼出的具体包名、`timeoutMs` 由
  `PROBE_TIMEOUT` 派生）与内核采用 `measurements` 的三条同时成立条件；§4.2 投放时机改为
  「启动无条件 + 每轮测速后 + 选定 Node 源后」，并写明内核侧候选顺序
  （`policies.effectiveOrigins`：选择文档 origins → 契约 catalog → 最小兜底）。
- `DESIGN-COMPLETE.md`：契约清单与「逐项核实」表按现状重列（3 个投放点、schema 3、新增
  `registry-choice.json` 一行）；§18.1/§18.2 改为现行形状；§27.2 的 `selected` 表述改为
  「已随 schema 3 废除，内核按三条条件采用证据」；§29 顶部加**提案原貌**横幅（`M3-a` 写的
  `selected`、`_selectedFromContract()` 都已不存在，别再照它找代码）；§34.1 的 AFTER 图重画成
  证据/选择两条线；§28 补进 §6 的「提案原貌」指针，§28.2 的内核侧改写为已落地形态
  （`registry.js::probeRegistry` 按 `policies.js::resolveProbe` 给出的契约规格探测）。
- `DESIGN-SHELL-ARCHITECTURE.md` §3.2：`CONTRACT_SCHEMA=3`、补内核选择文档的只读关系，不变量 C1 改成
  三份文件各一个写者，C4 的契约内容补 `measurements`。
- `DEVELOPMENT-TRACK.md` §2：两仓文件契约清单补 `registry-choice.json` 与 `update-journal.json`。
- 1.2.9 条目末尾的「未在本批处理」两项里，文档同步一项由本批收口；`probe_all` 的逐跳复验（S4）仍开放。

## [1.2.9]（2026-09-24）

### 镜像契约 schema3 与两文件所有权拆分（P0-C，跨仓与内核同批；**内核 0.1.6-BETA.10 已先发**）

契约 `registry.json` 过去既住壳的目录又住内核的选择，两个写者只能互相让步：内核每次保存要「读回原文、
只覆盖自己那三键」，而壳一旦读到 `mode=manual` 就**不再重写整份契约**。后果是用户在面板固定过一次源之后，
镜像目录与探测规格永久停在那一刻 —— 新镜像上线、目录里某个源死掉，内核都再也拿不到。本批按「谁写哪份」拆开：
`registry.json` 壳有、内核只读；选择（mode / manualOrigin / 候选）搬到内核自持的 `registry-choice.json`。

- `contract_doc` 的写面只剩证据（schema / writtenBy / writtenAt / catalog / probe / measurements）。
  壳只要还写一次选择字段，就等于覆盖用户在内核面板固定的那个源，所以 `CONTRACT_SCHEMA` 升 3 并让
  `export_to_kernel` 成为契约的唯一出口。
- 新增逐源实测 `Measurement` / `npm_measurements` 与 `record_npm_measurements`：面板点一次「重新探测」
  就落盘并重投契约。只在面板上显示是不够的 —— 内核选源时读的是文件，那样会出现「面板一个源、下载另一个源」。
  实测与目录同源：`mirror_set` 改 npm 候选即清空 `npm_measurements`（上一轮说的是另一批地址）。
- 壳改为**读**内核的选择文档（`core.rs` 的 `kernel_choice` / `registry_origins`），优先序与内核
  `policies.effectiveOrigins` 一致：内核手动源 > 内核里用户维护的候选 > 壳自持目录。不回读壳自己投出的
  契约候选 —— 绕一圈回来等于把壳的目录当成用户意图。
- 形态尺的两端各管一处：`registry_base`（写入口：形态 + 私网主机闸，拒 query/fragment）用于 `mirror_set`；
  `asset_url`（产物地址：**允许**签名 query，但**必须**过主机闸）用于 `core.rs` 取 tarball。此前那里只判
  `starts_with("http")` —— registry 替我们选一个 `http://127.0.0.1:4873/…` 就照拿，与内核侧的跳转复验不对齐。
- 删掉 `Mirrors.checked_at` 与 `selected_npm`：一个是 Node 探测的时间戳、一个是 npm 的选择结果，
  挤在同一个字段里导致 `export_to_kernel` 拿 Node 的延迟去描述 npm 的选择。Node 的选用源仍记（`selected_node`），
  时间戳不再落盘 —— 本轮实测延迟已随进度上屏，再存一份就是第二份「何时测的」事实。
- 私网主机 golden vectors 与内核 `test/npm-resolution-test.js` 的 C-m 同表（形态表 + 私网表 + 跳转/产物地址表）。
  同表立刻暴露两处真实分叉：127/8 与 0/8 —— Rust 的 `is_loopback`/`is_unspecified` 各认半段，内核原来只认
  `127.0.0.1`。两侧一并改成整段判定。
- 门禁改造：M-a / M-b / M-b2 / M-d / M-e 由「数条数、读含注释原文、切到文件末尾」改为剥注释 + 大括号配平取
  函数体 + 判据抽成共享函数；每条判据都配一个**旧形态样本走同一个函数**的反向断言，否则「门禁空转」直接红。
  B18 的尾部判据改锚到 mirror.rs 代码（含 `manualOrigin` / `"mode"` 即红，契约 schema 必须声明为 3）。
- 未在本批处理：`probe_all` 仍走 ureq 默认跟随跳转（内核侧已是逐跳复验的有界传输，对齐列为 S4）；
  `docs/` 里 schema2 / `selected` 的所有权表述仍在描述旧契约（P0-D 跨仓文档同步）。

## [1.2.8]（2026-09-24）

本版收全仓审计的壳侧第三组（PR #34）：**门禁判据空转**与产线小项。不含内核版本要求变化，产品运行时代码
除删掉一处零消费的打包资源外未改动。版本三处互锁由 `scripts/bump-shell.sh` 提升。

### 五处「永远不可能判红」的门禁改为真能判红

- 注释纪律扫描器 `scripts/check-comment-discipline.js` 的 HTML 掩码**只认 `<!-- -->`**，而引导页的注释几乎
  全写在 `<script>` 与 `<style>` 里面 —— 等于 CS-1/CS-2 对 `bootstrap.html`、`shell.html` 的注释零覆盖。
  现在按 JS 词法抽内联脚本（`type` 非 JS 系的不当代码，importmap 不会被误判）、按块注释抽内联样式，
  掩码与原文等长以保证报告行号不错位。覆盖面一开就暴露出存量违规：两份 HTML 里的图标字符、日期、
  节号引用、制表符框线与一段超上限的连续注释块，全部按规改写（语义未动）。
- `guard_resolution_test.rs` 的接线判据读的是**含注释的原文**：`--run-guard`、`spec.node`、`probe_npm`
  这类「代码有没有接上」的断言，注释里提一句就恒真，反向断言还会被说明文字误判红。改走剥整行注释的
  `read_code()`，并加一条夹具证明「剥注释确实改变了判定」—— 否则改造本身也是空转。
- `kernel_install_evidence_test.rs` 的 K-1 从锚点**一路切到文件末尾**：cmd/prefix/registry 只要出现在文件
  任意位置即通过，锚点形同虚设。改为逐函数切定，断言挂到真正的证据组织者，并反向证明切片没越界。
- `kernel_launch_standard_test.rs` 的 K-16 拿**注释文本**当切片右界：注释被改写或挪走，切片就悄悄变宽或
  变空，而 1.2.5 那次主帧导航绕过正是这条判据该拦的。右界改为下一个函数签名。
- 新增 C-i：打包步骤必须引用精确三段号的 `TAURI_CLI_VERSION`、全文件不得再有浮动形态的 CLI 引用，
  且产线文档的 H4 行必须与 `build.yml` 同形态（命令改了、文档留着旧写法＝把浮动版本当成事实传播）。

### 产线与资产

- **Tauri CLI 版本钉死**：`@tauri-apps/cli@2` 让三条腿各取「当天的最新 2.x」，同一 commit 的三平台产物
  可能出自不同 CLI 版本，且换版本不需要任何人改代码 —— Rust 侧有 `Cargo.lock`，JS 侧此前没有等价的锁。
  现钉 `2.11.5`（钉前核对过 registry：它就是当时的 2.x 最新），升版本只改那一行；四处文档同步，
  其中「本机禁语」段落改为不带版本的写法。
- `ci/check-glibc.sh` 与内核侧副本的判据对齐：旧实现「提取不到 GLIBC 符号 → 警告 + 退出 0」把**静态链接**
  （合法豁免）与**工具缺席 / 产物读不动**（只是看不见）合并成同一条通过路径。现在版本比较器先自校、
  两个工具都不可用或读取失败一律 `exit 2`，只有 `readelf -lW` 成功解析 ELF 且确无 `PT_INTERP` 才算豁免。
- `shell-release/make-manifest.js`：标志缺取值一律判为用法错误（`exit 2`）。真实劣化形态不是崩溃，而是
  `--out` 缺值时**静默回落到默认输出路径且返回码 0**。
- `tauri.conf.json` 删去 `bundle.resources`：它把整个 `icons/` 目录作为资源打进每个安装包，全仓零消费者
  （图标本身由 `bundle.icon` 正常打入）。
- 三处前向兼容 / 自愈路径补上**可判的退役条件**（`migrate_legacy`、`watchdog.ps1` 清理、无制表符的旧动作
  记录解读）：写明删除前置 = 活跃装机最低版本，以及必须同步改的门禁编号，把它们从「无限期保留」
  变成有期限的待办。
- `docs/audit/2026-09-18/README.md` 顶部加作废说明：该台账基线之后主干又前进 89 个提交，其引用的路径
  当前只有不足三分之一仍精确存在。文件保留作历史取证，但不得当现状依据。

## [1.2.7]（2026-09-23）

本版收全仓审计（内核 0.1.6-BETA.6 同批）的壳侧缺陷：引导页与运行时的十五组，逐条带门禁；
另把「安装冒烟」从文档承诺变成 CI 判据。不含内核版本要求变化。
版本三处互锁（`Cargo.toml` = `tauri.conf.json` = `Cargo.lock`）由 `scripts/bump-shell.sh` 同步提升。

### 引导页与运行时的缺陷收口（逐条带门禁）

- 引导页的 DSH logo 从来没显示过：CSS 用 `mask: url("/dsh-logo.svg")`，而 `bootstrap/` 下没有这个
  文件。webview 对取不到的 mask 资源不报错、不降级，只是让整块元素静默消失。补上
  `src-tauri/bootstrap/dsh-logo.svg`，并加门禁 `tests/bootstrap_assets_test.rs`（A-1..A-5）：把两条
  引导页的每条 `url()` / `src=` 解析出来，逐条要求在 `frontendDist` 根下真实存在（外链与 `data:` 不
  参与判定），含缺资源反向样本与「实盘解析条数下限」—— 标记写法一变先红在下限，防门禁空转。
- 内核安装的前端等待上界原先写死 1020000，而它注释自称的「后端 15 分钟 + 前端 17 分钟兜底」与
  后端实际契约（预算 1020000 + 收尾余量 60000 = 1080000）不符：前端比后端先放弃，会把一次其实
  成功的安装报成失败。改为派生 —— 引导早期非阻塞取 `shell_bridge_contract`，上界 =
  `bridge.maxWaitMs + 90000`，取不到契约才退到兜底常量；兜底常量与后端等值由 SW-9 钉住。
- 超时不再直接判死：`withTimeout` 只中止等待、不取消后端安装，安装仍可能随后落盘。现超时后转
  `core_status` 轮询（15 秒一拍、最多 20 拍）确认实际结果 —— 升级场景比对 `latest`（装着旧版本不算
  成功），全新安装则「装出了任何版本」即算。`core_apply` 后端本就不收 version 参数，两处 `null`
  实参一并删。
- `core_apply` 加在飞互斥：boot 链、`guard` 的 `KERNEL_NOT_ALIGNED` 自动对齐、重试按钮三条路都会
  再打这条命令，而 npm 安装不是可重入操作，缺互斥就是并发写同一前缀。`_alignRetried` 闩随「重试
  引导」置换 —— 不重置会让自动对齐在第一次失败后的整个页面生命周期里永久不再走。
- Windows 上超时/等待出错只 `child.kill()` 直接子进程：npm 类命令实为 `npm.cmd -> cmd.exe ->
  node.exe`，杀掉 cmd.exe 等于放跑孙进程（占着端口与状态根）。新增 `bounded::kill_tree`（Windows
  走 `taskkill /T /F`，其余平台保持原语义，平台分支集中一处），`env.rs` 的两处 kill 一并收敛。
- 探测代际原先只挡最终回写：被作废的旧 worker 仍继续向共享进度写 `stage/finish/set_summary`，新一
  代面板读到的是上一轮留下的「正在做什么」与记录 —— 卡住时唯一线索被串扰成假现场。现按线程所属
  代际拒写，门禁 `tests/probe_generation_test.rs`（G-p1..G-p4）自动枚举四个写入口、缺闸门即判红，
  并保留旧形态反向样本（四条全判出）。
- 三处毒锁丢写：`nodeprobe` 的活进度写入用 `if let Ok(...)`，`mirror` 的快照落盘与探测结果 push 同
  病。毒锁里的值仍是完好结构体，丢写的后果是 UI 永久停在最后一条进度上再也得不到更新、测速白跑
  一轮。统一为容毒读写（`live_lock()` / `into_inner()`）。
- 预热标记 `WARMING` 由线程体末尾手写 `store(false)` 落下，只覆盖不 panic 的路径：探测链上任一处
  panic 都会让标记永久停在 true，此后每次预热都被在飞判据挡掉 —— 表现为 registry 那一格再也不更
  新，且无日志。改为 RAII guard，`Drop` 落下。
- `locate_core_with_version` 用 `unwrap_or("0.0.0")` 兜底：版本探测失败被当成「版本 0.0.0」参与仲裁
  并一路显示到面板。改为返回 `Option`，探不到的候选不参与仲裁，整批探不到则报「已装但版本未知」。
  `resolve_aligned` 判「已对齐」原用字符串全等，而两侧文本分别来自契约与 registry 快照，文本差
  （build metadata 等）会把已对齐的内核判成未对齐、触发无谓重装，改按 `semver_cmp == 0`。
- `shell:goto-panel` 事件缺 `url` 时兜到相对路径 `supervisor.html`：那会把主帧导航到一个不存在的文
  档，用户看到的是引擎错误页。改为回引导页重跑启动链（与 `reloadPanelWhenReady`「无判据不导航」同
  一口径）。
- npm 镜像清单原先在 `core.rs` 另存一份 `DEFAULT_ORIGINS`（6 条与 `mirror.rs` 手写重复），且第三档
  兜底读内核 `registry.json` 的 auto 列表 —— 那一格永不到来（`mirror::load()` 绝不返回空）。删两
  处，`registry_origins()` 只留「内核 manual 优先，其余取 `mirror::load().npm`」。门禁 B23 由「字符串
  里出现过就算」改写为能红的判据：presets 与 `tauri.conf.json` 的 endpoints 逐项比对、`core.rs` 出现
  任何 npm 源字面量即判失败、抽取器带反向样本。
- `node.rs` 的下载缓冲没有字节上限：`Content-Length` 出自服务端、不可全信，一个谎报或持续产字的源
  就能把壳进程喂到 OOM（超时前无人拦）。现设上限：声称总量 + 1MB 抖动，无总量时 512MB 硬顶。
- macOS 的 `launchctl bootstrap` 把 plist 路径拼进 shell 脚本文本：路径含 `$`、反引号或引号时会被
  shell 二次解释，bootstrap 静默打到错误目标。改为 `"$1"` 位参传入，两处调用点共用一份脚本。
- `go_panel` 的重发拍数表与「最后一拍」判据分写两处（数组字面量 + 硬编码下标）：只改延时表而忘改
  判据，回引导页那一拍会静默失效。现同源（`PANEL_PUSH_DELAYS.len()`）。
- 死代码与静默 panic 两处：`40-shell-update.js` 判的 `r.cannotSelfUpdate` Rust 侧从不下发（唯一真值
  是 `shell_identity.selfUpdateCapable`），删；`main.rs` 托盘图标 `expect("no default icon")` 改为向
  setup 传播错误。零调用方的诊断 IPC `shell_state_root` 删除，其信息改由 `shell.log` 启动首行落出
  （schema + 三个实际路径）—— 本壳的观测通道是日志；门禁 K-8 改钉这条日志行，K-17 新增「注册进
  `generate_handler!` 的命令必须有调用方」（含缺失调用方的反向样本）。
- 已知未验证：真机 Windows 上「壳自更新之后首次进面板」那条报障路径仍待真人复测。本版把它的判据
  （URL 与「能不能投」同源、事件缺 url 就不导航）与装机/覆盖升级（H10 的 A=1.2.6 -> B=1.2.7）都收
  进了 CI，但「装好的那份字节在真人机器上把面板投出来了」这一句只有真机能签。

### 安装冒烟与发布通道冒烟进产线（补齐 1.2.3 以来缺失的那一层）

`docs/RELEASE-STANDARD.md` §5 一直承诺「安装冒烟 | 各平台安装包 | 能装、能起、能更新」，
但产线里从来没有一步安装过包：H3 的全部判据跑的都是 `./target/debug/` 下的构建产物。
装机形态（deb 落盘 / dmg 复制 / NSIS 静默装 -> 覆盖升级 -> 装好的那份二进制起得来）因此从未被执行过，
而真机报障正落在这条路径上。本版把它变成 CI 判据，不含产品代码变化。

- H10 `install-smoke` job（四平台矩阵，`ci/install-smoke.sh` + `ci/install-smoke-win.ps1`）：
  A = 上一个已发布版本的安装包（`gh release download`），B = 本次构建产物；先装 A 再覆盖装 B
  就是用户的升级路径。每次安装后读四项独立事实：二进制自报版本、包管理器记录的版本、
  `identity.json` 里的 exe 就是刚装进去的那份、`shell.log` 有对应启动行；覆盖安装后比对 sha256
  确认字节换了（版本未提升时产物可逐字节相同，故该判据按版本分流）。Linux 腿额外把装好的壳
  跑到 `/healthz` 判就绪。`publish` 改为 `needs: install-smoke`：装不上的包发不出去。
- H11 `published-channel-smoke` job（`shell-release/verify-channel.js`）：端点与公钥都从
  `tauri.conf.json` 读（不再另写一份 URL，那就会与真客户端漂移），主端点必须取到目标版本的
  完整清单，逐平台剥出 minisign 签名块、比对 key id、sha256 核对下载字节；tag 构建还比对
  通道字节与本次产物是否逐字节一致。**签名有效性不在这里判**，那是 H7 `updater_artifacts`
  的 V2/V3/V4（与用户端同一个 minisign-verify crate）。`workflow_dispatch(ver=…)` 是复核历史已发布版本的入口。
- 伪内核夹具收为单源（`ci/fake-core.js`）：H3 与 H10 共用，不再在 workflow 里内联第二份。
- 门禁：`src-tauri/tests/installer_smoke_coverage_test.rs`（I-a..I-h）钉住以上全部形态，
  判据抽成 `missing_channel_checks()` 以便反向样本与正样本用同一把尺子；含「只解包不安装」
  「只下载清单不读签名」「单层 base64 + `crypto.verify` 的旧验签形态」三种假冒烟的反向判据。
- H11 首跑（tag v1.2.6 run）判红在 `linux-x86_64 签名长度 329 字节，期望 72`：清单里的
  `signature` 是**双层 base64**（外层解出 4 行 minisign 文本，第二行再解出 alg(2)+keyId(8)+ed25519(64)=74 字节），
  且 alg 标记为 prehash 变体 `"ED"` —— Node stdlib 的纯 Ed25519 `crypto.verify` 对已知正确的三元组也验不过，
  在 node 里重实现只会造成年年假红或口径错了还判绿。改为只判钥匙出处与字节，有效性交给 updater_artifacts；
  四平台 key id 实测全等于配置公钥 `54A15461E39C8AEF`。
- 首跑（Linux 腿通过）暴露两个形态缺陷，随本批修掉并各加判据：macOS 腿里紧贴全角括号写 `$B（`，
  bash 在非 UTF-8 语域下吃掉该字符的半个字节，`set -u` 立即判失败；Windows 腿的探针用调用运算符
  跑 GUI 子系统的壳，既不等待也接不到 stdout，探针恒读到空。后者由 I-f 的
  `RedirectStandardOutput` + `WaitForExit` 判据与 i_h 的反向样本钉住。

## [1.2.6]（2026-09-23）

本版是 1.2.5 那条判据的接续收口：真机回报「没有任何改变，进内核就是 127.0.0.1 拒绝连接」，
复查代码确认 1.2.5 的判据结构上从未参与过那次导航。不含内核版本要求变化。
版本三处互锁（`Cargo.toml` = `tauri.conf.json` = `Cargo.lock`）由 `scripts/bump-shell.sh` 同步提升。

### 面板的 URL 与「此刻能不能投」同出一个答案；面板显示后壳持续看护服役

- 判据收回唯一出口（`guardctl::panel_view`）：返回 `(url, serving)`，`serving` = 当前端口的
  `serving_state()` 为 `Alive`。`shell_panel_url` 改回 `{url, serving}`，`shell.html` 主帧加载时
  未服役就 `backToBootstrap()`（引导页重跑 `guard_start` + 就绪轮询，是全仓唯一恢复链）；
  `go_panel` 不再自行问一次服役。**同一个事实只允许一处判定**：判据与 URL 分两处问，
  走了 URL 那条而没走判定那条，就是 1.2.5 的失效形态。
- 面板稳态看护（新增 `watch_panel`，由 `finish_boot` 武装）：5s 一拍复核 `serving_state`，
  连续 3 拍（约 15s）不在服役才发 `shell:goto-bootstrap`。此前壳只在「进入面板」那一刻判一次，
  之后守卫因任何原因消失（更新后重启失败、被所有者停掉、崩溃）界面都永久停在引擎自己的拒绝连接页。
  纯决策抽成 `panel_watch_tick`，单次抖动不甩页、回一次引导页即解除观察态、`EXITING` 握手中闭嘴，
  看护线程只允许一个。门槛 3 拍是权衡结果：短于用户对「页面死了」的判断，长到能骑过守卫正常重启的间隙。
- 内核更新后的重载（`shell.html::reloadPanelWhenReady`）：固定 900ms 延时导航改成有界等
  `guard_ready` 后再取 `shell_panel_url`，URL 现取（守卫可能已顺延端口），预算耗尽（40 拍 / 20s）
  回引导页 —— 固定延时等于赌新守卫已在新端口听完请求。
- 端口取值两处「同一个事实两套答案」：
  `env::discovered_api_port` 原取 records **首条**，而登记表按端口号为键、避让的新记录是追加的
  （内核侧 `release(旧端口)` 被 `catch {}` 吞掉时旧记录会留着），改为按 `createdAt` 取最新；
  托盘端口原在 setup 期一次性快照 `config.apiPort`，守卫顺延后启动/停止/退出全打在没人监听的端口上，
  改为每次点击现取 `current_api_port()`。
- 门禁：K-16 改写为钉「`panel_view` 是判据唯一实现、三条导航路径（主帧加载 / goto-panel /
  更新后重载）都问它」，并补齐 1.2.5 没钉的看护与重载两侧；各反向样本保留，`panel_watch_tick`
  的决策表由单元测试逐个档位钉住。壳侧 `shell.log` 新增「服役判定=」一行，使真机可从日志区分
  「URL 错」与「URL 对但未服役」。
- 已知未验证：安装产物四平台冒烟（H9 的 1.2.4 遗留项）仍未做；面板空白一侧的内核 CSP 修复
  走内核发布链，不随本版。

## [1.2.5]（2026-09-22）

本版是一处真机缺陷的四处收口（判据 / 导航 / 端口源 / 退出留痕），不含内核版本要求变化。
版本三处互锁（`Cargo.toml` = `tauri.conf.json` = `Cargo.lock`）由 `scripts/bump-shell.sh` 同步提升。

### 「端口通」不再被当成「在服役」：面板不再停在 127.0.0.1 拒绝连接

现场（Windows 真机，`guard.log` 13:50:27 与 13:56:49 两轮）：刚报完「启动完成 · 面板 URL
http://127.0.0.1:36360/」，进面板就是引擎自己的「拒绝连接」页，且窗口自己再也回不来。
不是崩溃：那两轮守卫都完整走过 `POST /session/stop`（唯一的调用者是壳的托盘「退出管家」），
`shutdownAll` 拆光服务链之后**守卫进程仍在监听端口**、`/healthz` 仍回 200。

- 判据收口（`guardctl.rs::ensure_guard`）：早退不再只看裸 TCP，改问 `serving_state()`
  = `ready()`（TCP + `/healthz` 2xx）**且** `/session/status` ∉ {stopping, stopped}。
  三态各自的下一步明确：`Alive`/`Sick` 跳过启动（病了也不重拉，交给就绪判定），
  `SessionHalted` 由所有者 `stop()` + 等端口让出（预算 15s）后落回正常启动序列自愈；
  等不到让出就如实报 `GUARD_STOP_FAILED` 并带上守卫日志末段，绝不假装重启过。
  读不到会话态一律降级为 `Alive` —— 探针抖动不许升级成「停掉一个健康守卫」。
- 导航收口（`windowing.rs::go_panel`）：三拍延时重发从「重复投递同一个 URL」改成**每拍复核**，
  最后一拍仍不在服役就发 `shell:goto-bootstrap` 回引导页（那里重跑 `guard_start` + 就绪轮询）。
  这条兜底只能做在 Rust 侧：WebKit 对「连接被拒」的 iframe 导航不触发 `error` 事件，
  `shell.html` 原有的两次重试兜底在这种现场形同不存在。
  **1.2.6 更正**：本条描述的收口在真机上没有生效过，上面的根因也不完整。真正必然发生的面板
  导航是 `shell.html` 主帧加载时向 `shell_panel_url` 取 URL 并直接写进 `iframe.src`，它不经
  `go_panel`；且 `iframe.src` 已被写成同一个值之后，`force=false` 的 goto-panel 不再覆盖，
  所以 `go_panel` 里那次复核根本来不及参与。判据已收回 URL 与结论同源的那一处（见 1.2.6）。
- 端口源收口（`env.rs::api_base_url`）：面板 URL 与就绪判据取同一个端口源
  （`ports.json` 的 `supervisor-api` 实际登记优先，其次 `config.json` 的 `apiPort`，最后默认常量）。
  此前导航侧只认 `config.json`，守卫一旦因端口占用顺延并持久化，就绪判定看 36361 而窗口导航去 36360。
- 退出留痕（`guardctl.rs::shutdown_all`）：`stop()` 成功时补一行「本次登录内不会自动拉起；重新打开
  程序即恢复」—— 三平台的自启/看护通道都不会在本次登录内把守卫拉回（Windows 的 `stop()` 会
  `/Delete` 看护任务、Linux unit 保持 enabled 只在下次登录起、macOS 已 bootout），
  不说清这句话，用户只会看到「重开窗口面板就废了」而无从下手。失败分支仍双写 stderr + 落盘。
- 门禁：新增 **K-16**（形态，`kernel_launch_standard_test.rs`）—— `ensure_guard` 体内按
  `if port_open` → `serving_state` → 服役判定 → **唯一**的 `return Ok(())` → `stop_and_await_release`
  → `ensure_started` 的顺序链判定，`go_panel` 必须先复核再导航且留有 `shell:goto-bootstrap` 出口
  （`shell.html` 侧同断言），`api_base_url` 必须取 `discovered_api_port()` 且不得绕道 `api_port`；
  三种旧形态各有反向样本防空转。语义侧新增行为用例 `serving_state_separates_alive_from_halted_but_listening`
  （`guardctl.rs` 的 `tests`，与 K-14 同一手法：`fake_serving` 按路径分别应答 `/healthz` 与 `/session/status`）。
  同时补齐 §6 表缺失的 K-15 行。
- 不改内核：`/healthz` 的契约语义是「进程活着」（`src/app/self/health.js` 明写），让停链的守卫回非 2xx
  等于换掉一条已声明的契约；真相本来就暴露在 `/session/status`，判据该在消费侧收口。
  看护入口（`--watchdog`）仍用 `ready()`：守卫进程活着而服务链被用户主动停掉，不是它该重拉的场景。
- 已知未验证：安装产物四平台冒烟（H9 的 1.2.4 遗留项）仍未做，本次只到 CI 门禁与真机行为推理。

## [1.2.4]（2026-09-22）

本版只含一处行为修复：守卫自启位的写者唯一化（IL-2 壳仓半边 / D5 扩展）。
版本三处互锁（`Cargo.toml` = `tauri.conf.json` = `Cargo.lock`）由 `scripts/bump-shell.sh` 同步提升。

### 守卫定义自愈重写不再重放 enable/enable-linger，自启位收归内核面板单写

现场：`ensure_defined` 在「内容过时 → 重写」分支上照跑 `systemctl --user enable` 与
`loginctl enable-linger`。于是 D5 表第二行的写者不唯一 —— 用户在面板关闭守卫自启之后，
任何一次模板演进（例如再给 `ExecStart` 改一次引号）都会把自启位悄悄重新打开。

- 自愈分支写完 unit、`daemon-reload` 之后立即返回；`enable` + `enable-linger` 只在定义**首次建立**时执行。
- 能力零损伤：内容一致的机器本就走 `!needs_write` 早返回，本次收窄没有拿走任何原有的重试机会。
- 判据 `d5_definition_self_heal_does_not_rewrite_autostart`：剥注释后只读 `ensure_defined` 函数体，
  含三种回归形态的反向合成样本与一条合法形态正向样本；契约同步写进 `docs/DESIGN-BOUNDARY.md` 的 D5。
- macOS 不在此列：内核 `darwin.js` 的开关位落在 `launchctl override` 库，壳的 `bootstrap` 不覆盖它。
  同一方向的三平台判据在内核侧（`KERNEL-DAEMON-CONTRACT` D-10 / P7，随内核 0.1.6-BETA.4 发布）。

## [1.2.3]（2026-09-22）

### 内核下载第一次有了真分母：壳按 dist 元数据先取包，再装本地 tarball（T-7 反转）

现场（1.2.1 起多次报告，1.2.2 仍在）：「内核的下载依然没有整个的下载进度显示」。这条不是没做，
是**做不出来**：写入路径挂在 `npm install -g <pkg>@<ver>` 上，而 npm 根本不吐取件进度，
所以无论前端怎么画，能拿到的只有心跳文字。要真进度只能换路径。

- `core::dist_from` 读**该源自己**的 `versions[v].dist`（`tarball` / `size` / `integrity`），
  `core::fetch_dist` 经全仓唯一的带进度 GET（`node::http_get_bytes_progress`，新增 `total_hint`
  以承接 registry 声明的字节数）边下边报，落到 `<状态根>/dl/<slug>.tgz`，再 `core::install_local`
  以 `file:` 形态装本地包。下载行由 `install.rs::download_line` 一处算出「已取 / 总量 / 百分比」。
- 核对没有省：字节数与声明不符即判截断，`integrity` 的 SHA512 不符拒绝安装，两者都上屏；
  源没给校验值时明说「按字节数核对」，不替源背书。
- 任一步失败（源不给 dist / URL 异常 / 截断 / 摘要不符）发**降级行**后退回 npm 直装 ——
  降级可见，且不因新路径砍掉原有能力。逐源循环、总预算、`--prefix` 反推一律不变。
- 进度条以「只准由分母驱动」的形态回归：`#dlMeter` 是唯一条元素、`installMeter` 是唯一写入点，
  `progress === null` 一律隐藏（画 0% 会被读成「还没开始」）。门禁 G-3 从「不得有条」改成
  「条必须由分母驱动」，并加 K-9/K-10 正向与反向判据；SSOT `ENV-TOOLCHAIN-INSTALL-STANDARD.md`
  §3.2/§3.3 同步改文。node 归档与桌面壳安装包共用同一条渲染路径，也一起获得了条。
- 内核包零运行时依赖（实测 `package.json` 无 dependencies），故取件阶段就是下载的全部成本；
  若将来引入依赖，npm 解包段仍回落到心跳行，不会伪装成分母。
- 取件文件（`<状态根>/dl/<pkg>-<ver>.tgz`）**装完即删**：它没有复用方 —— 每次换源都重新取并覆盖，
  留着只会按版本逐份占盘。K-9 把这条清理判进门禁。

### 环境探测过程上屏：预热在引导即启动，registry 问号不再被 TTL 回放

「看不到 npm 的检测过程」有另一半原因：registry 那一格只读预热快照，而预热**只由引导页 70-boot
那一枪触发**，任何不走它的路径都会让该格永久停在「测速尚未完成」；`WARMING` 在线程 `spawn` 失败时被
`let _ =` 吞掉后永久为 true，之后再也不会有第二次预热。

- `main.rs` 在 `nodeprobe::start()` 旁一并 `mirror::warmup_async()`；`spawn` 失败回滚 `WARMING` 并记日志。
- `probes.rs` 里 registry 记录绕过依赖维度的 10 秒 TTL 复用：它只是读一次内存快照，复用会让预热完成后
  面板仍念着旧问号 —— 那格看起来像「检测卡住」，实际是缓存在回放。
- `30-mirror.js` 的轮询窗口从 24 秒放宽到 60 秒，盖过后端两轮 `probe_all` 的最坏耗时。


### 注释纪律进壳仓并全量压缩：门禁缺失才是论文式注释的根因

现场（用户 Windows 真机 1.2.2）：环境检测看不到 npm 的探测过程、内核下载没有进度、
拉起仍报 `schtasks /Create 失败（含降级重试）: 首次=拒绝访问 / 降级=拒绝访问`，白等 60 秒后
只剩一句「端口不可达」。本版本按此逐条收口（守卫定义与兜底可观测见下节，探测与进度见后续提交）。

- 壳仓此前**没有**注释纪律条款，所以 B1–B6 把「为什么」写成了论文：扫描面 46 文件、注释
  3264 行、1397 个白名单外图标字符、284 处过程叙事、209 个 >4 行连续块、4 个文件注释多于代码。
- `DEVELOPMENT-TRACK.md` 新增第 4 节（与内核仓同规），并落为 `scripts/check-comment-discipline.js`
  （CS-1..CS-7，含反向自检与契约字面量在册校验），由 CI 的 `version` job 在四条腿起飞前调用。
- 压缩结果 3264 -> 2007 行，超块/图标/叙事全部归零；全部 45 文件**剥注释后的代码骨架逐字节不变**
  （掩码比对）。删的是历史叙述（日期、批次、改判链、红绿过程），留的是当前为真的约束。
- CS-7 登记的 11 处契约字面量在压缩前逐个查过引用方；其中 `npmOk !== true` 经核实
  `env_toolchain_standard_test.rs` 读的是 `20-env.js` 的代码而非 src 注释，登记表按真实读取方登记。

### Windows 守卫定义不再请求最高权限，权限类失败才换通道（K-15）

真机报错的形态本身就是缺陷证据：两次尝试只差一个 `/RL HIGHEST`，其余一字不变，于是同一句
「拒绝访问」被打印两遍；而 `ensure_defined` 只 `/Query` 从不 `/Delete`，同名任务若由提权窗口
建过，非提权进程两次都必然被拒。全仓没有任何一处读这个文本。

- 守卫与看护任务都不再请求 `/RL HIGHEST`：用户态守卫不需要它，带上它就把可用通道换成必失败。
- 失败按类别走：`is_access_denied` 认中英两种文案（`拒绝访问` / `Access is denied`），只有权限类
  才先 `/Delete` 再建一次；删不掉就把「同名任务由更高权限持有」写进原因，不再盲试同一条命令。
- 免提权兜底通道：HKCU `CurrentVersion\Run` 写入稳定入口（标准用户可写，语义同 ONLOGON 任务）。
  动作记录改为 `<通道>\t<动作串>`（旧格式按计划任务解读，升级不重建已装用户的任务），`start()`
  按通道分派 —— Run 键不支持即时启动就如实报 `Err`，不假装服务管理器接受了请求。
- 兜底直拉的存活可观测：`spawn_daemon` 返回 `Child` 而非 pid，等待期间 `try_wait` 非零退出即早停
  （退出码 0 不算失败：`--run-guard` 在 Windows 上先 detach 出 node 再退），报错直接带守卫输出末段。
- `start_requested()` 收紧为「请求被接受」（`Some(Ok(()))`）：被拒的请求等 30 秒只会把
  「没人被请求过」说成「服务管理器慢」。新判据 K-15 钉住以上形态，并含旧形态反向自检。

## [1.2.2]（2026-09-22）

### 子进程真话恢复可读：码页解码收口为一点，失败文案收口为一条记录（B1）

现场（用户 Windows 真机）：`守卫启动超时…服务管理器错误：schtasks /Run 失败（退出码 1）：<乱码>`。
乱码形态（`锟斤拷` 连排）说明原文是**GBK 系的中文控制台消息**被按 UTF-8 做 lossy 的产物；
具体字串已不可恢复，但它是整条启动链上**唯一**能指向根因的一句话——修复后它才会第一次被人读到。

- **根因不是"详情丢了"，是"详情被毁容"**。2026-09-12 那次「GBK 修复」用 `String::from_utf8_lossy`
  解决了「非 UTF-8 时详情全丢」，但中文 Windows 控制台程序按 **OEM 码页**（zh-CN=936）写 stderr，
  按 UTF-8 做 lossy 就把每个双字节换成 U+FFFD。丢字节与丢可读性是同一个缺陷的两半，只补一半等于没修。
  现 `bounded.rs::decode_console` 为**全仓唯一解码点**：先按 UTF-8 直通（node/npm 本就是 UTF-8），
  非 UTF-8 才交给操作系统按当前控制台码页做 MBCS→UTF-16（`MultiByteToWideChar`）——
  对 936/950/437/1251 等任意本地码页同时成立，不把「中文=GBK」写死；仍失败才 lossy 保底不丢字节。
  零新增依赖（沿用 `platform/windows.rs` 既有裸 `extern "system"` 先例）。
- **失败文案此前在 4 个文件各拼一遍**（`bounded.rs` / `core.rs` / `runtime_contract.rs` /
  `platform/windows.rs`），措辞互异（`killed` / `超时被终止` / 裸数字）且**一律不带命令原文**，
  于是「退出码 1」成为用户与开发者能拿到的全部信息。现 `Output` 升级为 `ExecRecord`：
  `run()` 内部从 `Command` 捕获 `program`+`args`（调用方无从漏记），`code` 改回 `Option<i32>`，
  「超时」由 `timed_out` 承载，渲染只有 `failure()` 一处。
- **超时不再降格成 `Err` 字符串**：原先超时路径 `return Err(format!(...))` 把已拿到的输出与退出状态
  一起丢掉，调用方分不清「命令不存在」与「命令挂了」——这两件事对用户的可操作结论完全不同。
  现超时返回 `Ok(ExecRecord { timed_out: true, .. })`；`Err` 只代表「没能跑起来」。
  全部 17 个 `bounded::run` 调用点均已核实是按 `.success` 判定，行为不变。
- **门禁跟着改动走**：旧 B62 断言「`read_log` 必须含 `from_utf8_lossy`」——它把上一次的**手段**
  当成了**不变量**，会把正确的修复判为违规。B62 重做为锁住「采集阶段不得 lossy / Windows 必须问码页 /
  不得硬编码单一语言」，解码正确性交给 `bounded.rs` 的行为单测（含 `#[cfg(windows)]` 的 GBK 字节回归用例，
  在 Windows CI leg 执行）；新增 B65 负向门禁，禁止 `platform/`·`domain/`·`commands/` 再出现手写「退出码」文案。
  G1 白名单理由同步为「进程创建 + 进程输出」两个 infra 原语。

### 启动链收口为「一处归一 / 一处自身路径 / 一处阶段产物 / 一处子进程落点」（B2）

接 B1。B1 让 `schtasks` 的真话第一次可读，但那条真话之所以是**最后**一环，根因在结构里：
阶段没有产物、路径形态没有单一所有者、兜底子进程的输出被丢掉。本版三件事一起做。

- **路径形态归一为唯一实现 `platform::external_path`**。此前仓里有**两份规则不同**的剥
  verbatim 前缀实现（`core.rs` 的 str 版：大小写不敏感、含 `\\.\`；`domain/coreloc.rs` 的
  Path 版：`UNC` 段大小写敏感、无 `\\.\`），同一个 `canonicalize()` 结果经两条路径可得两个
  字符串，而「外部工具能否执行它」恰好取决于这个字符串。现取两者规则的**并集**落在平台层
  （路径形态属平台事实，不是业务选择），两份旧实现连同其 5 个私有测试**一并删除**。
  规则刻意**不带 `#[cfg]`**：写成 `#[cfg(windows)]` 的话 POSIX CI 既编译不到也测不到它 ——
  那正是两份分叉能长期存活的原因；非 Windows 在此恒等，于是三条 CI 腿跑同一份实现与同一组测试。
- **壳自身路径归一为唯一入口 `platform::self_exe()`**。三个调用点对同一个失败的处理各不相同：
  `LaunchSpec::from_runtime` 用 `unwrap_or_default()`（取不到 ⇒ **空路径**原样写进服务定义，
  事后回读不到当时写的值），另两处把失败咽成 `null`。现签名返回 `Result`：取不到必须报错、
  取到必须规范。`LaunchSpec` 四个路径字段**构造即归一**（唯一装配点），故三平台的服务定义与
  spawn 拿到的路径形态一致；装配失败新增 code `LAUNCH_SPEC_FAILED`（规范 §4）。
- **P4/P5 有了阶段产物 `ServiceLaunch`**，「定义失败 ⇒ 不请求服务管理器启动」从此是**结构**
  而非注释：`started` 只在 `defined` 为 `Ok` 的分支里赋值（出边关闭），也就不再为一条必然
  失败的请求空等 30 秒。`evidence()` 的五种阶段结论进 `READY_TIMEOUT` / `SERVICE_START_FAILED`
  正文（H8），报错从此能说明「走到了哪一步、为什么停在那一步」。
- **守卫子进程的输出永不丢弃**：`platform::guard_stdio` 是唯一挂流点（兜底 spawn 与两条
  `exec_guard` 分支共用），输出汇入 `<状态根>/shell/guard.log`（>512KB 截尾保后 256KB，
  写它的是**子进程**、它不会自己滚动）。原先三处 `Stdio::null()` 删掉；`service.rs` 里重复的
  `CREATE_NO_WINDOW` 块也删掉，窗口标志回归 infra 单点（`bounded::prepare` / `exec_guard`）。
- **`--service-plan` 从此打印「真正会写进定义的那一行」**，且走运行时同一条装配路径
  （`LaunchSpec::from_runtime` + `service_command`），只读契约 `read_node()` 故不加
  `--service-apply` 时无写盘副作用。实现整体从 `main.rs` 移到 `domain/cli.rs`——与另三个
  plan 入口同处，`main.rs` 只留派发（它当时正好 550 行，顶在 G3 上限）。
- **门禁与规范跟着走**（不把旧形态留在原地）：K-3 重做为函数切片顺序 + 「`start` 只出现在
  定义成功分支」的结构判据（附越界反空转）、K-9 字面量对齐、安装证据侧 K-7/K-8 重做为
  「npm 拿到的路径必须已归一 + 反向识别第二份实现」、新增 **K-11**（归一与 `self_exe` 单点）
  与 **K-12**（子进程输出永不丢弃）；`P-a`/`B24` 两条读 `main.rs` 的判据随实现换文件。
  `KERNEL-LAUNCH-STANDARD.md` 补 §0 H9/H10、§1 P4→P5 出边、§2 两行矩阵、§4 补
  `LAUNCH_SPEC_FAILED` 与 `JOIN_ERROR`（原表自称「全部 code，不多不少」却少一个）、§6 补
  K-7..K-12；同时删掉「P4 失败只记一条日志后继续 P5」这句已不成立的规范文字，并把三处
  **把未实测因果当成已确认根因**的措辞改为可核实的说法。

### 就绪只认一条判据、拉起只走一条序列：看护从内嵌脚本变回壳自己的无头模式（B3）

「守卫起来了吗」在仓内原本有**三个**答案，且互相矛盾：`wait_alive` 只看 TCP 能否连上（它却是
`ensure_guard` 的成功判据）、`guard_ready` 看 TCP + `/healthz`、Windows 看护脚本用
`Test-NetConnection`（又是 TCP）。端口被占但服务没起 = 前两者判「就绪」、第三者判「活着」，
这正是「环境全绿却进不去面板」的机制：我们一直在用「端口通」当「服务健康」。

- **B3a 判据合一**：`guardctl::Readiness`（`Ready` / `PortClosed` / `Http(code)` / `NoHttpResponse`）
  + `ready(port, timeout)` 成为全仓唯一就绪实现，契约即 H6（TCP 可达**且** `GET /healthz` 2xx）。
  `port_open()` 明确降级为**反向**用途（「它是不是已经不在了」：重启等待、看护短路），不再充当成功判据；
  面板探针 `guard_ready` 改为委托 `ready()`（此前是同一逻辑的第二份手写体）。
  等待预算由 tick 数改为**时长**（每 tick 不再是常数成本：多一次 `/healthz` 往返），
  服务管理器路径 30s、兜底 spawn 后 60s，单次探针 800ms（必须远小于 500ms tick 的邻域约束写进注释）。
  `READY_TIMEOUT` 的文案现在带**放弃原因**（`verdict.describe()`）与证据落点，不再只报「超时」两个字。
- **B3b 看护合一**：Windows 看护任务的动作由「内嵌 PowerShell 脚本」改为稳定入口的无头模式
  `<壳> --watchdog`（`WATCHDOG_ARGS`；`/TR` 仍由 `service_exec_line` 单源装配，升级时就地删除遗留的
  `watchdog.ps1`）。该入口的存活判据取 `guardctl::ready()`、拉起动作取 `guardctl::ensure_started()` ——
  后者是从 `ensure_guard` 拆出的 P4→P6 序列本体（定义 → 服务管理器 → 就绪 → 兜底 spawn），
  GUI 启动与看护共用**同一段**代码；看护不做 P1 线上对齐（那会改变安装态，且需要 `AppHandle` 上报进度）。
  「守卫进程在但端口不通」会不会被双启动？不由看护嗅进程负责：兜底那次 `spawn_daemon` 与
  服务管理器请求最终都落到同一份 `guard.lock`（内核侧单实例，D5），看护自己不再判断进程。
  注：`schtasks /Run` 对已在运行实例的行为按 Task Scheduler 的 `MultipleInstances` 策略，
  本仓**未**在真机实测该项 —— 因此不把它写成立论依据，真正的单实例保证是 `guard.lock`。
- **能力搬家而非砍掉**：旧脚本里「无 GUI 进程就 `Start-Process <壳>`」这一半**删除**，因为桌面壳自愈的
  所有者本就是守卫（内核 `domains/shell/watchdog`，20s tick、三平台一套），而它带着脚本完全没有的
  四条护栏：连续缺失达宽限才动作、更新相位/账本时效（避免壳自更新几分钟的空窗被当成崩溃）、
  无图形会话跳过、窗口内拉起上限防风暴。脚本那份既无宽限也无会话判定，会在壳自更新期间把 GUI 拉回来。
  壳监督不了自己 —— 它的监督者会随它一起死（F3）。
- **随之暴露的跨仓耦合**（已在内核侧修）：守卫靠 `isShellProcess` 区分「真壳」与「壳的无头进程」，
  其排除清单**漏了** `--platform-matrix`，而新的 `--watchdog` 与被排除判据同样瞬时（`--run-guard` 此前也未登记）
  ⇒ 漏项会让看护把一次瞬时进程当成「壳在运行」而永不拉起真壳。现收为 `HEADLESS_FLAGS` 单一清单 + 契约 D-9。
- **门禁跟着改动走**：K-3 的切片与顺序判据随函数拆分重写（并新增「序列本体只有一份」判据）；
  K-7 让位为「任务归属」，形态判据新增 **K-13**（看护复用单一判据与单一序列；反向 bans
  `fn watchdog_script` / `Test-NetConnection` / 看护切片里的 `powershell`、`Start-Process`，
  且反向判据在**剥注释后**的代码上跑 —— 解释「为什么删掉它」的注释合法地提到旧名字）；
  K-12 的挂流判据不受影响。`code_only()` 为本文件新增的注释剥离工具。

### 安装/下载进度只有一个所有者，且它只承认可测的量（B4a）

现场（用户报障原话）：「强制更新」看不到下载进度、检测环境过程里「没有看到完整的 NPM 的检测」。
前三轮（1.2.0 / 1.2.1）修的是**判定链**，这一轮修的是**进度语义本身**——它此前不对应任何被测出来的量。

- **一条事件两个作者**。`install_progress` 同时由 `main.rs::push_status` 与命令层壳更新里内联的
  `json!` 发射；`kind` 只枚举 node/npm，内核与桌面壳拿裸字符串过线，于是「哪些东西可被安装」在 Rust 与
  前端（`10-ui.js` 的 `INSTALL_TARGET` 四个键）各有一份账。现新增 `src/domain/install.rs` 作为**唯一**
  所有者：`emit_json` 是三条 `install_*` 事件的唯一出口，`InstallKind::as_str` 是 `kind` 字面量的唯一来源
  （node/npm/kernel/shell 四类齐），`download_line` / `npm_heartbeat` 是两类文案的唯一渲染点。
  `push_status` 与命令层的内联发射**全部删除**（不留兼容层），`Emitter` 依赖随之从 `main.rs` 退场。
- **进度分数按代码顺序编**。旧的 0.1 / 0.3 / 0.85 与真实进度无关；前端删掉进度条（T-7）正是因为这种数字
  会骗人，那它们就不该继续出现在线上协议里。`RunState.progress` 改为 `Option<f32>` —— 类型本身就是判据：
  裸 `f32` 无法表达「这一步没有可测分母」，而那正是编造分数的入口。无分母一律发 `null`。
- **Node 归档下载从此真按字节报**。`node::download_verified` 收 `on_bytes: &dyn Fn(u64, Option<u64>)`，
  以 64KB 块读、按 ~1% 或 512KB 节流播报，并在收尾**强制**报一次；桌面壳安装包下载走同一出口
  （`install::download`）。文案与比值由 `(done, total)` 同处算出，杜绝「文字说 MB、比值另算一套」。
  `Content-Length` 缺失/为 0/**小于已取回量**（ureq 在 `Transfer-Encoding: chunked` 下忽略它）时退回
  「已取回 N MB」+ `null`，绝不输出「12.0 / 8.0 MB」。
- **内核安装 15 分钟的沉默换成如实心跳**。`npm install -g` 不吐百分比，输出又被重定向到临时文件
  （不经管道），于是能如实说的只有它自己的现场：`bounded::run_watch` 每 2s 回调
  `Live { elapsed, lines, last_line }`，渲染为「npm 安装中 · 已用 95s · 输出 7 行 · 最后一行「…」」。
  `core::install_version` 只做透传（`Option<&dyn Fn(&Live)>`），**一个字的用户文案都不拼**。
  开工行把**预算说清**（单源 15 分钟 · 总上限 17 分钟 · 共 N 个镜像源），换源时发「第 i/N 个源 …」——
  用户报的「不知道是不是卡住」，缺的从来不是进度条，是说清预算。
- **完成播报按 kind 各一条，版本各归各的**：`install_done { kind: node, version }` 与
  `{ kind: npm, version }` 分别取自运行期契约；npm 未回读版本号时发 `null`（旧实现发过一条 `kind=npm`
  却带 node 版本，引导页念出的「npm 已就绪（v22.x）」从来不是 npm 的版本）。
- **门禁跟着改动走**：新增 **G-12**（五条子判据：第二处发射 / `kind` 裸串 / 措辞在所有者之外拼装 /
  `.progress =` 写裸数字 / `RunState.progress` 仍是 `f32`），并带**前置断言**要求所有者真的发射三条事件、
  真的持有两句措辞 —— 否则「不得有第二处」会退化成「一处都没有」的空转门禁；反向旧形态夹具五条逐条判红。
  G-2/G-7/E-c 随实现换文件（不再读 `main.rs`）。行为面新增 `install.rs` 五个单测（有/无分母、
  比值不得越过分母、心跳只说可测量、每个 kind 有自己的标签）、`bounded::run_watch` 的心跳行为测试
  （≥2 次、elapsed 单调前进、末行真实），以及 `platform/windows.rs` 真实归档测试里的**字节进度**断言
  （心跳≥2 次、不回退、收尾量 == 落盘大小、服务端 Content-Length == 真实大小）。
- **B4b 的这一步跨两个仓**：B4a 做出来的真实进度只到引导页；面板那条路径仍是一问一答。
  收口见下一节（同批完成）。

### 面板的内核更新不再是黑箱：进度经桥中继、等待上界由壳下发（B4b，跨仓）

现场延续 B4a：用户从面板点「更新」后，界面在十几分钟里**一个字都不变**。B4a 已经让壳侧知道
「现在在试第几个源、npm 已经跑了多久」，但这些真话止步于 `shell.html`——面板拿到的第一帧之后就是沉默。

- **根因是桥只做了一半**。`shell.html` 收到请求后回的唯一一条 progress 是硬编码的 `stage:'start'`，
  它与 Rust 的 `install_progress` 事件族**从来没有接起来**；面板侧 `kernelUpdateBridge.ts` 又把 progress
  帧整个丢弃（`if (d.type === PROGRESS) return;`）。现补的是这条中继线，不是新协议：
  壳主帧 `listen('install_progress')`，**只中继 `kind == kernelKind` 的帧**（否则 Node/npm/壳自身的
  进度会串台到这条请求上），并只在有在途请求时中继（`kernelPending`）。
- **等待上界不再是面板的秘密**。面板原先写死 `timeoutMs = 6 * 60 * 1000`，而 Rust 侧总预算是 17 分钟
  ——超时先到时面板给出的是**自己编的结论**（「桌面壳无响应」），用户重试就等于两个进程并发写同一个
  npm 全局前缀，正是单写入者契约要消灭的事故形状。现预算只有一个事实源 `bridge::KERNEL_UPDATE_BUDGET_MS`
  （+ `KERNEL_UPDATE_GRACE_MS` = 面板可见的 `maxWaitMs`），经 `shell_bridge_contract` 与 `kernelKind`
  一起下发；面板在首帧按 `maxWaitMs - 已等时长` 重设上界，超时文案带**最后一次进度原文**。
  面板仍保留一个明显大于预算的兜底值，用于对旧壳（契约里没有 `maxWaitMs`）保持可用。
- **同一条预算原本有三个数字**：Rust 里手抄的 `from_secs(17 * 60)`、引导页的 `CORE_APPLY_BUDGET_MS = 1020000`、
  面板桥的默认 `6 * 60 * 1000`（前两个相等纯属巧合）。现 Rust 的 deadline 与 `bridge::KERNEL_UPDATE_BUDGET_MS`
  共用一个定义，`install.rs` 的开工行数字改成**从常量推导**（单源分钟数 = `core::NPM_INSTALL_TIMEOUT`，
  总分钟数 = 预算常量），不再手抄「15 分钟 / 17 分钟」；面板经 `maxWaitMs` 取真实值。
  引导页那份**刻意不动**：它是 UI 侧的等待上界而非后端预算，且 17 分钟 < 1020s 是有意留的余量。
- **协议版本刻意保持 1**：progress 语义本就是「非终结、可多次」，旧面板对未知帧直接忽略；
  递增反而会让 K1 在升级期间（新旧面板并存）拒收请求 —— 那是真实的兼容悬崖，不是谨慎。
- **门禁**：壳侧 `kernel_update_single_writer_test.rs` 扩到 **SW-1..SW-8**：SW-1 钉住预算常量、
  SW-4 钉住 deadline 只有一个来源（并负向禁止 `from_secs(17 * 60)` 手抄）、SW-7 钉住中继接线
  （`listen('install_progress'` / `kernelKind` 过滤 / `kernelPending` / `maxWaitMs`）、
  SW-8 反向夹具证明判据会咬「只回一条 start」的旧桥。面板侧 `plus/test/kernel-update-single-writer-test.js`
  扩到 **SW-9**（消费进度 + 禁止写死上界，两条判据各配反向例）。

### 环境探测从此是一张记录表：四个维度一个所有者，未知与失败不再同形（B5）

现场（用户报障原话）：「NPM 到现在在检测环境的过程当中，我们没有看到完整的 NPM 的检测」；
以及内核安装在跑了十几分钟后失败、面板只剩一个退出码。B4a 修的是**安装期**的沉默，这一轮修的是
**安装前**的沉默 —— 而后者才是那条 17 分钟失败唯一能提前说清的地方。

- **根因是形状，不是漏了一个字段**。同一条探测结论此前拆在三处各写一遍：`nodeprobe` 的逐候选追踪
  （`TraceEntry`，只有 node 一个维度）、`node_status` 里现拼现用的 npm 探测（结论进了 JSON，却从来不
  是一条可渲染记录）、前端 `diagText` 手工拼接的 `env_trace=` / `env_stuck=`。后果：加一个维度要改
  三遍文案，漏改**不报错**，只显示成「检测不完整」；而探测结论只在全部成功后才被读出来，于是
  「探针在跑」与「用户看得见探针」是两件事。
- **新增 `src/domain/probes.rs` 为唯一所有者**，冻结三件事：维度表（`enum Probe` → `node/npm/registry/prefix`）、
  记录形态（`Record { probe, source, target, ms, ok, note }`）、两种渲染（`Record::json` 出面板、
  `render` 出 CLI 与 `--self-check`，两者同源）。node 侧的逐候选结论从此登记为 `Probe::Node` 记录，
  `TraceEntry` / `render_trace` / `out.trace` **连同前端拼接片段一并删除**，不留兼容层。
- **`ok` 是三态**（`true` / `false` / `null`）：旧实现把「某一步还在飞」记成 `ok=false`，于是「还有一个
  候选没试完」在诊断串里显示成「这个候选坏了」，还会让机器被判成缺 npm 触发无谓重装。面板渲染 `?`、
  CLI 渲染 `?`，与 T-1b（`npmOk`）、T-13（`progress: null`）是同一条形态纪律。
- **两个此前根本没被探测过的维度**（这是本轮真正的能力增量，不是改名）：
  · `registry` —— npm 源可达性**只读镜像预热缓存**（`mirror::cached()`），故 `node_status` 的「纯本地、
    零网络 I/O」（门禁 B27）不被破坏，且 `source` 明写「预热缓存」，排障时不会把它误当此刻的连通性。
  · `prefix` —— `npm prefix -g` 落点目录的**可写性**。`EACCES`/只读前缀会让 `npm install -g` 在十几分钟
    之后才失败，而面板当时只剩一个退出码；环境阶段一条 `prefix` 记录就能把根因说清。写测试用唯一命名
    探针文件（写完即删），且**先**做本地固定盘判定再碰目录 —— 网络盘上的 `exists()` 本身就是无界阻塞。
- **npm 从「每 400ms 轮询跑两次」降到「每 10s 窗口跑一次」**：依赖维度按 `DEPENDENT_TTL` 复用缓存，
  复用判据抽成**纯函数** `probes::reusable(缓存的 node 路径, 本轮 node 路径, 年龄)`（换 node 必须重探、
  到界必须重探 —— 只能靠真实 spawn 与时钟验证的判据等于没验证，单测因此是确定性的），
  安装完成处由 `probes::invalidate_all()` 与 node 侧缓存**一起**作废（只失效一半会留下「新 Node +
  旧 npm 结论」这种自相矛盾快照，而它恰好出现在刚装完 Node 的那一刻）。顺带把 spawn 路径收口：
  `runtime_contract::run_npm_line` 成为 `--version`（可用性）与
  `prefix -g`（前缀）的**唯一**执行口（T-10 的同一条路），`usable_runtime` 成为唯一的 `NodeRuntime`
  装配点、`derive_usable` 只委派；`run_version_probe` 那层只剩转发的包装与 `core::npm_global_prefix`
  里自建的 `Command` 一并删除。
- **探测进度从此上屏**（判据 T-17）：`busy` 期间 status 行尾附 `NS.probeSummary(st)`（按维度压缩的一行
  结论，在飞的那一步以 `?` + 耗时呈现），两条失败路径（`probeError` 与检测超时）都带「已探明：…」。
  诊断串里的手工 `env_trace=` / `env_stuck=` 换成 `env_probes=`（全部来自壳给的记录表）。
- **门禁**：G-1 换锚到 `probes::dependents`，并负向禁止命令层自行调 `probe_npm_usable`；新增 **G-13**
  （六条子判据：第二份维度表 / `TraceEntry` 复活 / 记录形态里的裸 `bool` / 第二处拼字段 / 第二处调
  npm 探针 / 前端在 `10-ui.js` 之外解析 `.probes` 或再现 `env_trace=`），带**前置断言**要求所有者真的
  登记四个维度、真的持有两种渲染、真的带 TTL 与本地固定盘门槛，另配**旧形态反向夹具**逐条判红。
  B26/B27/B31 三条既有门禁同步换锚到记录形态。
- **废代码清扫**：`commands/mod.rs` 里一段属于四个已删除实现的**堆叠文档注释**（挂在 `shell_panel_url`
  头上，描述的代码早已不在）删除，其中唯一仍然成立的事实（面板与壳同源）在
  `DESIGN-SHELL-ARCHITECTURE` §3.3 有出处。
- **文档**：`ENV-TOOLCHAIN-INSTALL-STANDARD` 新增 §2.5（四维表 + 三个「为什么」）、不变量
  **T-14/T-15/T-16/T-17**、§5 门禁表 G-13 行；`DESIGN-SHELL-ARCHITECTURE` 的 domain 名录与探测表同步。
  本轮**不触碰面板契约**（`probes` 只出现在引导页的 `node_status`），故跨仓文档无需跟改。
- **本机验证边界**：`node --check` 两个 bootstrap 文件、G-13 判据的静态仿真（含反向夹具）在本机完成；
  Rust 编译与 `cargo test` 按项目纪律由 CI 裁决，本机无 toolchain 也未执行。

### 门禁自己也得被门禁管：判据落到行为与真机，CI 里不再有「绿得什么都没跑」（B6）

B1–B5 把事实收回单点之后，剩下一个更早的问题：**门禁声称的东西，代码里到底还有没有**。
本轮把 `src-tauri/tests/*.rs` 的每一个正向锚点逐条对着源码验一遍（存在 / 在代码里而非注释里 /
不是被自己的夹具凑出来的），并把能变成行为判据的地方从「搜符号」改成「做出真的现场看它怎么答」。

- **四处正向锚点其实只有注释满足**（B22 断言文档散文、B28 断言已改写的说明、B47/B48 的函数切片
  边界停在注释上）：这类门禁在实现被删掉之后仍会长期绿着，比没有门禁更糟 —— 它给出的是「已验证」的
  假象。全部重锚到代码符号（`hex::encode(Sha256::digest(…))`、`GetDriveTypeW(root.as_ptr())` 等），
  切片边界改指 `fn try_probe(`。
- **K-3 是一条必红的门禁**（不是判据太松，是判据写错了）：B2 把调用形态改成
  `ServiceLaunch::define_and_start(spec`，而门禁仍搜 `define_and_start(&spec`，`expect` 直接炸。
  正向锚点同步换成新形态，并加一条负向锚点（`ensure_guard` 切片内**不得**出现该调用 = 防越界恒真）。
- **就绪探针把「问不出状态行」糊成 `(0, "")`** —— 于是 B3 引入的 `Readiness::NoHttpResponse`
  是一条**不可达**分支：端口通着却不回字节的现场（守卫刚绑定、还没能服务）被报成「/healthz 返回 0」，
  与真的 5xx 混成同一句话。`localhttp::http_get_local` 改为如实返回 `None`，状态码取状态行的
  **第二段**（`HTTP/1.1 <code>`；第一段是版本）—— 这里本仓一度改成取第一段，是用真 node 起服务
  抓响应头实测才纠正回来的，凭印象改解析正是这类缺陷的来源。
- **H6 的四态从此有行为判据（新增 K-14）**：`guardctl.rs` 的 `tests` 用回环真 socket 逐一看四种现场
  （200 / 503 / 连上不吐字节 / 无人监听），夹具**持续接管连接**——`ready()` 先看端口再看 HTTP，
  一次判定开两个连接，只 accept 一次的夹具会把真正的 HTTP 连接留在队列里，把「200 = 就绪」那条
  用例变成假阴性（这是本轮自己踩到的）；另断言 `describe()` 四条文案互不相同。
  `probes` 侧补两条确定性判据：无 node 时 node 派生维度必须为「未知且说明原因」（registry 与 node
  无关，有缓存给结论是正当的 —— 不锁它的取值，否则测试就成了第二个事实源）、`reusable()` 真值表。
- **CI 三条腿从「空转」变「把关」**：
  ① `--node-plan` 那一步原本带 `|| true`，自检失败也绿 —— 现在必判退出码，且必须看到
  `node=` / `node_probe_candidates=` / `latest_lts=` / `mirror_selected=`（判据是「产出了结论」，
  不是「进程没崩」）；② 新增 **Linux 真机启动链冒烟**：伪造位置契约 `core.json` + 一个按实际端口
  绑定并持久化 `ports.json` 的最小 node HTTP 服务，跑 `--watchdog` 走完「解析守卫 → 组装规格 →
  拉起 → `/healthz` 判就绪」，再跑一次必须短路（不重复拉起，按 `guard.log` 里的启动次数判），
  结束前显式 `pkill` + `systemctl --user disable --now` 收口，不给 runner 留脱离进程；
  ③ 新增 **Windows 服务定义真机冒烟**：`--service-plan --service-apply` 走真 `schtasks /Create`，
  断言 `建立结果` 里的定义行（带引号、以 `--run-guard` 结尾、不含 `\\?\` verbatim 前缀）与
  `建立后现存 = 是`，再用**只读**复跑证明 `is_defined()` 走的是平台事实（计划任务不是文件）。
- **已知缺口（如实登记，不假装覆盖）**：Windows 的 **P5/P6**（`/Run → /healthz`）没进 CI。
  根因是架构性的两条，绕开任何一条都会得到一条假绿：稳定入口**只在定义里写 `<壳> --run-guard`**、
  不携带状态根，而计划任务进程用的是任务计划程序里的用户环境 —— 本步设的隔离 `DSH_SUPERVISOR_HOME`
  传不到被拉起的一侧；且 `ensure_started` 有 spawn 兜底，`--watchdog` 退出 0 也证明不了 `/Run` 成功。
  另外 `ServiceControl` 只有 `kind/definition_path/is_defined/ensure_defined/start/stop/spawn_daemon`，
  **没有「删除定义」**（清理计划任务只能在 yaml 里直接喊 `schtasks /Delete`；生产侧不需要它，
  因为 `ensure_defined` 是覆盖语义 —— 不为测试新增产品能力）。要闭合这一格需要的是「让计划任务
  进程也能确定状态根」的受支持口子，登记待议，不在本轮顺手发明。
- **文档**：`KERNEL-LAUNCH-STANDARD` §0 H6 改写为四态判据（含 `None` 语义与「不得糊成 `Http(0)`」）、
  §1 的 P6 行随之、§6 新增 **K-14** 与「CI 真机冒烟」行（含上面那个已知缺口的出处指引）。
- **CI 轮1–5 抓到的自身缺陷（发布链记录，不粉饰）**：本批代码在本机从未编译（无 Rust toolchain），
  轮1 红在四处编译错（`kernel_update_apply` 里四个引用点漏改 `install_out`、`npm_heartbeat` 的 `if/else`
  两支 `&str`/`String`、`NpmFact` 派生 `Clone` 但字段 `NpmUsable` 没有、`fn_slice` 返回 `&str` 缺生命周期）；
  轮2 红在三条单测，其中两条是**断言写错**而非实现错：
  ① `ExecRecord` 的 stdout 回退用例用 `..r` 做函数式更新，`stderr` 被继承成非空，于是「空 stderr 才回退
  stdout」的现场根本没造出来；② `guard_stdio_streams` 的反向判据指望 `format!("{:?}", Stdio::null())`
  含 `"Null"` —— std 的 `impl Debug for Stdio` 是 `debug_struct("Stdio").finish_non_exhaustive()`（1.98.1
  源码实测），null 与文件句柄渲染成同一个字符串，**这条断言按设计永远不可能通过**。现改为：正向拉起真子进程
  看两条流的字节有没有落进日志文件（现场判据），反向钉本文件里 `None =>` 分支的形态并禁止 `Stdio::inherit()`
  —— 判不出来的东西不配当门禁；③ GBK 解码用例走 `decode_console`，而它按契约问操作系统要码页，
  英文 runner 的 OEM 码页是 437，同一批 936 字节被如实解成另一副样子（实现没错，用例把「机器语言」当成了
  判据）。现把 Windows 解码拆成 `decode_console`（唯一入口）+ `console_code_page`（问 OS）+
  `decode_codepage`（可显式传码页），回归用例显式传 936，B62 的切片锚点随之路径化（不再用字符窗口）。
- **轮3 是一处极小的编译错**：B62 新文案里为说明「原先的断言用了 `{:?}`」而把这个占位符原样写进了
  `assert!` 的消息串，Rust 把它当成无实参的位置参数（`1 positional argument in format string, but no
  arguments were given`）—— 四平台同时红在同一个字符上。
- **轮4 是判据自己不合格**（四平台编译已过、门禁首次真正执行）：① B13 把「建立服务定义」的调用点钉在
  `main.rs`，而该调用本轮已随启动序列迁入 domain（`guardctl` 的共享序列 + `cli` 的无头入口），main.rs
  只剩装配 —— 判据范围随产权迁到 `crate_sources()`，与同一测试里 `spawn_daemon` 的既有口径对齐；
  ② B62 ① 用「`read_log` 起手 300 字符」取函数体，而函数体只有 6 行，窗口越进了下一个函数的文档注释
  （那里正好写着「为什么不是 `String::from_utf8_lossy`」），反向判据于是把**说明**当成**实现**判红 ——
  改与 ② 同口径：锚点定界；③ **Windows 检出是 CRLF**（`actions/checkout` 的 auto-normalize 按平台归一
  换行），带 `\n` 的跨行针脚在该腿永不匹配：正向判据变红还算次要，反向判据由此**假绿**才是代价（门禁不再
  检查它声称检查的事）。修法是在取文本的单点归一化（`bootstrap_flow` 的 `lf()` 覆盖 main_rs /
  crate_sources / platform_sources / bootstrap_html / B13 / B62，`env_toolchain` 的 `read()`+`walk()`，
  以及 `include_str!` 的内嵌文本），与各 `tests/*.rs` 已有的 `read()` 口径统一，不发明第二套做法。
- **轮5 之前一直在为 fail-fast 买单**：`cargo test` 默认在第一个红的 target 处中止，19 个 target 一轮
  只暴露一个 —— 轮2/3/4 各只推进一条信息即此因。门禁步改为 `cargo test --bins --no-fail-fast`（一条都
  不会少跑，只是把「一轮一问」变成「一轮全量」），并把 `tail -80` 放宽到 `-400`：原值与本步开头那句
  「不截断：门禁失败时断言文案就是排障依据」自相矛盾，多个 target 同时红时先出现的失败会被后面的输出冲掉。
  轮5 因此在四平台上一次性跑完 19 个 target，只剩**一条**失败：反拉黑门禁（`no_suppression_machinery_test`）
  按子串命中新增的 `ServiceAttempt`。该门禁的边界条款（2026-09-21）与先例 `f110843` 都写明：禁的是那套
  **按版本记账并抑制重试的机构**，被机构名 token 命中的普通标识符应改名而非放宽判据 —— 故
  `ServiceAttempt` → `ServiceLaunch`（它承载的是一次「建定义 → 请求启动」的阶段产物，本就不含重试语义），
  判据与 K-3 锚点同步，未删任何一条断言。

- **本机验证边界**：夹具的 HTTP 行为（`/healthz` 状态行、端口顺延与 `ports.json` 持久化）在本机
  用真 node 起服务实测过；三条 workflow 脚本经 YAML 解析 + `bash -n` 校验。Rust 编译、
  `cargo test` 与这三条 CI 腿本身仍由 CI 裁决（本机无 toolchain，也未跑任何构建/测试）。
  **剩余杠杆**：Windows P5/P6 那一格未闭环（见上）；Linux ②在 systemd-user 可用的机器上才会真的
  走服务管理器路径，runner 上多数走兜底 spawn —— 两条路径都已覆盖，但「服务管理器真的把守卫拉起来」
  这件事目前只有 Linux 侧的证据。

## [1.2.1]（2026-09-21）

### Windows 上「装了 Node 仍没有 npm」的根因修复：可用 = 探针与消费者同一条 spawn 路径，落定 = 整棵工具链校验

现场（用户 Windows 真机）：引导页装完 Node 之后 npm 仍然缺失。前两轮将 npm 拉进判定链时，
**「可用」的判据本身在两处失守**（取证与推演见 `docs/ENV-TOOLCHAIN-INSTALL-STANDARD.md` §1 第三轮）：

- **探针与消费者走了两条不同的 spawn 路径**。`probe_npm` 只判 `is_file()`，Windows 上第一个命中的候选是
  `npm.cmd`；而 CreateProcessW 执行不了 `.cmd`（那是 cmd.exe 的脚本）。旧探针在平台层用 `cmd /C` 包装跑
  `--version`，于是探针报「可用」、真正的消费者（`npm install -g` / `npm prefix -g`，`Command::new` 直接 spawn）必失败。
  包装是胶水：它把一个「本平台不可直接执行」的事实藏成了探针成功，所以**删掉包装**而不是给消费者也套一层。
  现由 `Platform::is_directly_spawnable`（新增 trait 方法）在**选择程序**时就排除这类垫片，Windows 上 npm
  一律以 `node.exe + node_modules/npm/bin/npm-cli.js` 形态交出；探针因此不再需要任何平台分支（G1）。
- **解包成功被当成载荷完整**。PowerShell 5.1 的 `Expand-Archive` 走 .NET Framework 的 `ZipFile`，超 260 字符的
  条目被**静默丢弃且退出码为 0**，而 npm 的依赖树必然超深；`commit_user_node` 当时只校验 `node.exe`，
  于是残缺树被落定为「Node 已就绪」。现首选 Windows 10 1803+ 自带的 `tar.exe`（bsdtar，宽字符路径），
  `Expand-Archive` 只作兜底，且**每条解包路都以工具链校验为准**（node 且 `probe_npm` 命中）才落定，
  不通过则换下一条 —— 既有安装不会被半成品覆盖。
- 面板不再只显示「缺少 npm」：`probe_npm_usable` 改为返回 `Result<_, String>`，失败原因经 `node_status` 的
  `npmWhy` 落到 npm 分支文案（T-1d）。三类成因（载荷缺失 / 垫片拉不起来 / 执行报错）处置不同，原因不上屏就是把排障推给用户。
- 新增门禁：G-9（`probe_npm` 必过 `is_directly_spawnable`，且 `platform/` 之外不得出现 `cmd` 包装）、
  G-10（`commit_user_node` 函数体必须校验 npm）、G-11（真实归档的 CI 步骤必须按全路径命中并断言跑了 1 个），
  G-1 收紧到「`npmWhy` 上屏 + `probe_npm_usable` 带原因」，
  G-5 增加「npm 分支必须回显 `st.npmWhy`」；Windows 侧行为面在 `platform/windows.rs::toolchain_tests`。
- **整条 zip 到 npm 的链第一次在 CI 上真跑**：`build.yml` 新增 Windows 步骤，以 `--ignored` 执行
  `official_artifact_installs_usable_npm`（下载官方归档、走生产 `install_node`、读回 npm 版本）。
  静态门禁只能证明代码里写了判据，证明不了本平台解出来确实有 npm —— 这正是它此前从未被执行过的代价。
  该步骤按**全路径** `--exact` 指定测试并断言「恰好 1 passed」：`--exact` 配短名是零命中且退出码 0，
  「加了实测步骤」与「步骤什么都没跑」在 CI 上长得一模一样（门禁 G-11 钉住这一点）。

### Linux 支持面收窄为「Ubuntu + deb 一种形态」，旁路产物从产线源头停掉

上面第一条缺口不是靠补文案收口的，而是把「多形态」这条旁路整个拆掉：

- 产线源头（`build.yml` 的 linux 臂 `bundles`）与 `tauri.conf.json` 的 `bundle.targets` 都去掉 `rpm`，
  安装程序只产 `.deb`；上传产物与 GitHub Release 资产清单同步删掉 rpm 一项，装系统依赖的步骤不再装 `rpm` 包。
  起点停掉才算止血 —— 只改文档而矩阵照产，下一版又会产出一个没人支持、也没人测的形态。
- 诊断层跟着收窄：`update.rs` 的 `install_kind()` 去掉 `Rpm`/`AppImage` 分支，
  这两类落到「未知形态 → 不自称能自更新」；`self_update_capable()` 对 deb 仍要看有没有提权通道。
- 验收层：`updater_artifacts.rs` 不再找 rpm 产物；R-3 一致性门禁加**反向**断言（workflow 与 `targets`
  里出现 `rpm` 即红）。反向断言是必需的 —— `deb` 是 `deb,rpm` 的子串，只断言「有 deb」时 rpm 回潮照样绿。
- 连带改动（**必须同批**，否则所有 PR 卡住）：矩阵参数进到了 required status check 的名字里，
  job 名从 `build (ubuntu-22.04, linux-x64, deb,rpm, 2.35)` 变成 `build (ubuntu-22.04, linux-x64, deb, 2.35)`，
  分支保护的必需上下文要在这轮里一起改。
- 基座与支持面是两件事，文档里此前混着说：`ubuntu-22.04` 定的是 glibc **下限**（产物能在更新的发行版上跑），
  它不构成「我们支持 Debian / Fedora」的承诺。支持面现在只有一个名字：**Ubuntu**。
  以后要扩（rpm 槽位、AppImage、按包形态分流）得先改更新清单的每平台单槽位设计，见
  `docs/RELEASE-STANDARD.md` §2 与 `docs/SHELL-UPDATE-CHANNEL-VERIFICATION.md` §九。

### 安装包换源：Windows 不再只剩 unpkg 一条路，且候选源全部按实测挑选

上面第二条缺口的定案：**清单可达 ≠ 安装包可分发**，此前所有「多 CDN 冗余」的说法都是把前者当成后者。

- 实测（本机对 1.2.0 真实产物逐源取包，明细表在 `docs/SHELL-UPDATE-CHANNEL-VERIFICATION.md` §九）：
  jsdelivr 家族按扩展名屏蔽 `.exe`（deb / app.tar.gz / 清单照给 206）；npmmirror 把整个 `@dsh-sup` scope
  挡在 raw-file 路由外（与扩展名无关）；tencent / aliyun / huawei 的 npm 镜像没有 raw-file 路由（404）；
  unpkg.net 回 503 USAGE_EXCEEDED；gh-proxy.com / ghfast.top 能取但是未审计的第三方 ⇒ **一律不放进代码里的候选**。
- 落地的只有实测拿得到的：新增 `mirror::artifact_candidates()`，把清单声明的那一条 payload URL
  展开成有序候选（声明源永远第一，去重），npm CDN 家族换 base 同路径复用，
  再补一条 GitHub Release 同名资产作为独立来源回退。
- 换源之所以安全：Tauri 的 `endpoints` 只覆盖**取清单**这一步，`Update::download_url` 是公开可改字段，
  壳在 `download()` 前重写它；验签发生在 `download()` 内部、按下载到的字节对着 `tauri.conf.json` 内置公钥验，
  所以换的是来源、不是内容，签不过照样拒。
- 只在网络/传输类错误上换下一个源（`worth_next_source`），验签与安装类错误立刻返回、不重试；
  全部候选都拿不到才报错，错误里逐项带来源与原因。判据由内联单测钉在**插件的错误形态**上：
  `Update::download` 对非 2xx 统一返回 `Error::Network`，所以 jsdelivr 屏蔽 `.exe` 的那个 403 确实会落到下一个源，
  而不是把整次更新判死 —— 这条不是推理，是钉住的契约。
- 一个真实陷阱写进门禁：GitHub Release 的 mac 资产名不带 arch token，跨架构同名 ⇒
  按架构名匹配才生成 Release 候选，否则会把 arm64 的包递到 x64 客户端上，表现为一句看不懂的验签失败。
  另有单测断言候选集**只含**实测源（对未审计镜像按名字拒绝），防止有人再往里填假镜像。
- `plugins.updater.endpoints` 不改：两条端点都发得出清单，改它解决不了 payload 单点，只会让清单也单点。

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
