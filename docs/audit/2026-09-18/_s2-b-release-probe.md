# 壳仓审计 · 只读 · 子分片 S2-B-rel（热路径 / 正确性 / 稳定性）

- 审计对象：**已确认基线提交 0930884**（分支 release/shell-1.1.8，commit 0930884335595186b7c65afb57124a87ad31338a）；
  审计时工作树与该提交一致（`git status --porcelain` 为空），故下方行号即该提交内的行号。仓库 = dsh-supervisor-launcher。
- 审计文件：
  - \`src-tauri/src/release_channel.rs\`（未跟踪新模块，761 行）
  - \`src-tauri/src/mirror.rs\`（471 行）
  - \`src-tauri/src/nodeprobe.rs\`（662 行）
  - \`src-tauri/src/runtime_contract.rs\`（245 行）
  - \`src-tauri/src/update.rs\`（233 行）
- 只读参考：\`docs/RELEASE-STANDARD.md\`、\`docs/KERNEL-LAUNCH-STANDARD.md\`、\`docs/DESIGN-BOUNDARY.md\`、
  \`docs/DESIGN-COMPLETE.md\`、\`docs/DESIGN-SHELL-ARCHITECTURE.md\`；内核对照仓 plus 的
  \`RELEASE-CHANNEL-CONTRACT.md\`、\`EXECUTION-CONTRACT.md\`、\`src/platform/distribution/release.js\`、
  \`src/platform/contract/registry.js\`、\`src/platform/service/install-id.js\`。
- 方法：纯静态阅读 + 全文 grep + 只读查看依赖源码（ureq 2.12.1）。未运行任何测试/构建，未做任何 git 写。
- 约束遵守说明：部分结论引用仓外依赖源码（ureq），仅用于佐证时间边界，不作为问题证据主体；
  所有问题证据均给仓库相对路径:行号。

## 结论摘要

- 未发现 P0（无「必然数据损坏 / 必然永久卡死 / 必然越权」的确定性缺陷）。
- P1 共 2 条：镜像并行探测的「等全部线程」使之不满足有界承诺（DNS 阶段不受 ureq deadline 约束）；
  灰度名单包拉取按镜像数串行放大，与代码注释声称的「最多一次 PROBE_TIMEOUT」矛盾。
- P2 共 18 条，集中在：回收/孤儿计数竞态、缓存与落盘的 read-modify-write 竞态、
  错误吞没（\`let _ =\` / \`ok()\` / \`unwrap_or_default\`）、跨仓格式/编码口径分叉。
- 选版算法与 \`RELEASE-CHANNEL-CONTRACT\` §3 的五步（rollback > canary > latest > versions）**逐条一致**，
  且与内核 \`pickReleaseVersion\` 语义一致 —— 已核，无问题（见「已核项」）。

---

## P1

### S2B-MIR-01（P1）\| 并行探测对「最慢线程」无上界，DNS 阶段不受超时约束
- 证据：\`src-tauri/src/mirror.rs:314\`（\`std::thread::scope\`）、\`src-tauri/src/mirror.rs:317\`（每源一条线程）、
  \`src-tauri/src/mirror.rs:330\`（\`ureq::get(...).timeout(PROBE_TIMEOUT).call()\`）、\`src-tauri/src/mirror.rs:347\`（scope 结束 = 等全部线程）。
- 问题：\`std::thread::scope\` 必须 join 全部子线程才返回；\`ureq\` 的 request deadline 只覆盖连接/读写，
  其源码明确留有 \`// TODO: Find a way to apply deadline to DNS lookup.\`（ureq 2.12.1 \`src/stream.rs:364\`）。
  即：任一镜像的 DNS 解析挂起，\`probe_all\` 就整体挂起，\`PROBE_TIMEOUT\` 形同虚设；\`release_channel.rs\` 的
  版本对齐、\`core.rs\` 的选版、\`mirror_status\`、\`warmup_async\` 全部经由它。
- 最小修法：给 \`probe_all\` 增加「整体截止时间」——各工作线程只把结果写入共享槽并**不依赖 join 才能返回**；
  或把 DNS 解析改为在子线程内、由主线程到点后直接返回已收集结果（放弃未返回线程的计数，另设孤儿计数与上限），
  保证 \`probe_all\` 的墙钟上界 ≈ \`PROBE_TIMEOUT + 少量余量\`。
- 影响：一台 DNS/网络异常的镜像可让「检查内核版本 / 下载 Node / 导出镜像契约」整条链路无限期挂起，
  与文件头「整体耗时约为其中最慢者而非累加」的不变量相反。

### S2B-RC-01（P1）\| 灰度名单包拉取按镜像数串行放大，热路径可阻塞约 N×PROBE_TIMEOUT
- 证据：\`src-tauri/src/release_channel.rs:376\`（\`let doc = fetch(CANARY_ALLOWLIST_PKG)?;\`）、
  \`src-tauri/src/release_channel.rs:357\`（\`canary_machine_with\` 约定 fetch 为单次读取）、
  \`src-tauri/src/core.rs:361\`（\`fetch_pkg_meta\` 对每个 origin 串行 \`probe_all(one, &path)\`，逐个 8s 超时）、
  \`src-tauri/src/core.rs:338\`（注释声称「最多 PROBE_TIMEOUT，不会随镜像数量放大」）。
- 问题：opt-in 名单机在 \`latest_pick\` / \`decision_channel\` / \`core_apply\` 热路径上会执行该闭包；
  名单包未发布/不可达时每个源都要走到超时，最坏 ≈ 6×8s = 48s（源码注释与实现相反）。
- 最小修法：\`fetch_pkg_meta\` 用**单一总截止时间**贯穿所有 origin；或改成一次 \`probe_all(&origins, path)\`
  并发取回、取首个合法 JSON；并在 \`release_channel\` 契约里注明 fetch 的预算上限。
- 影响：显式开启 \`canaryAllowlist\` 的机器在镜像不可达时，检查更新/安装内核会长时间无响应，
  引导页表现为「卡住但无结论」。

---

## P2

### release_channel.rs

| ID | 证据 | 问题 | 最小修法 | 影响 |
|---|---|---|---|---|
| S2B-RC-02 | \`src-tauri/src/release_channel.rs:228\`；对照 plus \`src/platform/service/install-id.js:49\` | 壳读 \`install-id\` 只取首行 trim，**不校验 UUID v4、不 lower**；内核读同一文件时校验 \`UUID_RE\` 并把非法内容当「无标识」。 | 在 \`read_install_id_file\` 增加 UUID v4 校验（不匹配返回 None），与内核同口径。 | 文件被写坏/内容非法时壳仍当成有效 installId，灰度**静默不命中**且不告警；内核则明确告警。跨仓 RC-G7 口径分叉。 |
| S2B-RC-03 | \`src-tauri/src/release_channel.rs:229\` | \`read_to_string(...).ok()?\` 把「不可读（权限/IO）」与「不存在」合并为 None；调用方 \`:390\` 只打印「内核尚未启动过？」。 | 区分 \`NotFound\` 与其他 Err，后者单独留痕。 | 权限/IO 故障被误诊为「内核没启动」，排障方向错。 |
| S2B-RC-04 | \`src-tauri/src/release_channel.rs:245\` | hostname 兜底只读 \`HOSTNAME\`/\`COMPUTERNAME\` 环境变量；GUI 直接启动的 Linux 壳通常无 \`HOSTNAME\`（它是 shell 变量）。 | 用平台层提供 \`gethostname\` 等价实现，或对 GUI 场景明确留痕「无法取主机名」。 | §5.3 的 hostname 兜底在桌面 GUI 环境常为空，CI/容器外几乎不生效（契约允许，但需可见）。 |

### mirror.rs

| ID | 证据 | 问题 | 最小修法 | 影响 |
|---|---|---|---|---|
| S2B-MIR-02 | \`src-tauri/src/mirror.rs:334\` | \`resp.into_reader().read_to_end(&mut buf)\` 无响应体大小上限，超时内可读入任意大 body。 | 设最大字节数（元数据 ≪ 1MB），超限即视为探测失败。 | 恶意/异常镜像可在 8s 内造成内存膨胀。 |
| S2B-MIR-03 | \`src-tauri/src/mirror.rs:414\`、\`src-tauri/src/mirror.rs:417\`、\`src-tauri/src/mirror.rs:469\` | \`WARMING\` 先置 true 再 \`let _ = Builder::spawn(...)\`；spawn 失败或闭包 panic 时 \`WARMING.store(false)\` 永不执行。 | 用 RAII guard 复位 \`WARMING\`；spawn 失败立即复位并留痕。 | 线程创建失败/一次 panic 后，后台预热**永久失效**（\`mirror_cached\` 永远 ready=false）。 |
| S2B-MIR-04 | \`src-tauri/src/mirror.rs:420\`（load）、\`src-tauri/src/mirror.rs:457\`-\`465\`（改 selected_npm 后整体 \`save\`）；对照 \`src-tauri/src/commands/mod.rs:577\`-\`601\` | 预热是「载入→计算→整体落盘」的无锁 read-modify-write；与用户 \`mirror_set\`（把 \`selected_npm\` 置 None 并保存）或 Node 安装保存 \`selected_node\` 并发时，预热会用旧快照覆盖用户刚保存的候选集/选择。 | 落盘前重新 \`load()\` 并只合并本次探测字段；或全仓单一 \`Mirrors\` 写锁。 | 用户手动改镜像后可能被后台预热悄悄回滚，表现为「设置不生效」。 |
| S2B-MIR-05 | \`src-tauri/src/mirror.rs:113\`-\`152\`（load 不限条目/校验）、\`src-tauri/src/mirror.rs:317\`（每源一条线程）；对照 \`src-tauri/src/commands/mod.rs:576\`-\`590\` | \`mirrors.json\` 与 \`mirror_set\` 对列表长度无上限、load 不过滤纯空白；\`probe_all\` 每源起一条线程。 | load/set 时上限（如 ≤64）并 trim/校验 http(s)；超限截断或报错。 | 被篡改/超大的配置可一次拉起成百上千线程（栈耗尽/DoS）。 |
| S2B-MIR-06 | \`src-tauri/src/mirror.rs:293\`-\`296\`（\`npm_probe_path\` 返回**未编码**的 \`@dsh-sup/dsh-core-<tag>\`）、\`src-tauri/src/mirror.rs:246\`(\`pathTemplate\`)、\`src-tauri/src/core.rs:191\`(\`encode_pkg\` 把 \`/\` 编成 \`%2F\`)；对照 \`docs/DESIGN-BOUNDARY.md:174\`（\`@dsh-sup%2Fdsh-core-{platform}\`） | 同一 npm 探测存在三条路径口径：warmup/\`mirror_status\` 用未编码包名；\`core::latest_pick\` 用 \`%2F\`；导出的 \`probe.pathTemplate\` 是**已解析的具体平台包名**且无 \`{platform}\` 占位符（内核 \`resolveProbe\` 的 replace 成为空操作）。 | 统一到一个编码函数并导出平台中立的 \`{platform}\` 模板（或明确“导出即本机具体值”并让内核不做 replace）。 | 不同镜像对未编码 \`/\` 的处理不完全一致时，两侧/两路径可能选到不同可达集合，违背 C4「同一答案」；契约示例与实现漂移。 |

### nodeprobe.rs

| ID | 证据 | 问题 | 最小修法 | 影响 |
|---|---|---|---|---|
| S2B-NP-01 | \`src-tauri/src/nodeprobe.rs:255\`（\`recv_timeout(budget)\`）、\`src-tauri/src/nodeprobe.rs:267\`（超时后才比较 hard_deadline）；调用方 \`src-tauri/src/main.rs:436\`（45s）、\`src-tauri/src/domain/cli.rs:53\`（60s） | 当 caller 的 budget > 25s 硬上限时，实际阻塞 = budget，硬上限只在 recv 返回后才被检查，未被强制。 | 等待时长取 \`min(budget, hard_deadline.saturating_sub(elapsed))\`；或把 hard_deadline 作为 recv 的绝对截止。 | 规则二「硬上限」名不副实：探测可阻塞到 45s/60s，与文件自身承诺不符。 |
| S2B-NP-02 | \`src-tauri/src/nodeprobe.rs:240\`-\`250\`（取走 rx）、\`src-tauri/src/nodeprobe.rs:205\`-\`212\`（stale 时重开并写入新 rx）、\`src-tauri/src/nodeprobe.rs:288\`-\`290\`（把旧 rx 写回 slot） | 并发两次 status 时，A 取走 rx，B 判定 stale 重开 worker 并把新 rx 写入；A 超时后把**自己的旧 rx 覆盖**回去，新 worker 的结论被丢弃（send 失败且静默）。 | 回写 slot 前校验「代际/slot 仍为 None」；把 rx 与 generation 一起持有，或用单一 \`WorkerHandle\`。 | 探测结论丢失、重复起线程；诊断与实际 worker 不一致。 |
| S2B-NP-03 | \`src-tauri/src/nodeprobe.rs:334\`-\`337\`（abandon 自增 ORPHANS）、\`src-tauri/src/nodeprobe.rs:355\`-\`362\`（worker 退出时 saturating_sub）、\`src-tauri/src/nodeprobe.rs:147\`-\`156\`（invalidate 清零 ORPHANS） | ① 若 worker 恰在 recv_timeout 返回 Timeout 与 abandon 之间完成，它按旧代际正常退出而**不递减**，ORPHANS 永久 ≥1 → Idle 分支永远拒绝新建，探测只能靠 invalidate 复位。② invalidate 清零后旧孤儿退出会 saturating_sub，可能**扣掉新孤儿的计数**，使 Idle 分支误判为可新建，重新堆积线程。 | 用「该 worker 是否被作废」的每 worker 标志决定递减（而非全局代际）；ORPHANS 用带代际的计数或避免 reset 与 in-flight 混算。 | 探测可能永久失败直到下次安装成功；或线程/计数缓慢泄漏。 |
| S2B-NP-04 | \`src-tauri/src/nodeprobe.rs:64\`-\`81\`（\`lock().ok()\` 静默）、\`src-tauri/src/nodeprobe.rs:92\`（\`lock().ok()?\`），对照 \`src-tauri/src/nodeprobe.rs:83\`-\`88\`（snapshot 从 poison 恢复） | \`stage/finish/set_summary\` 在锁中毒后**变成静默 no-op**，而 snapshot 仍能恢复：诊断串从此只出不进、\`current_stuck\` 返回 None。 | 与 snapshot 同口径：中毒取回内部值继续写入。 | 一旦 panic 污染 live 锁，卡住阶段的唯一线索全部消失（正是本模块要消除的故障）。 |
| S2B-NP-05 | \`src-tauri/src/nodeprobe.rs:346\`（\`let _ = spawn\`） | 线程创建失败被丢弃，真实原因不落日志；状态层只能给出「探测线程异常退出」。 | 记录 spawn 错误并在结论里带上原因。 | 排障时无法区分「spawn 失败」与「worker panic/断开」。 |

### runtime_contract.rs

| ID | 证据 | 问题 | 最小修法 | 影响 |
|---|---|---|---|---|
| S2B-RTC-01 | \`src-tauri/src/runtime_contract.rs:139\`-\`143\`（\`npmPath\` 缺失时 \`unwrap_or_else\` 拼 \`bin/npm[.cmd]\`）；对照同文件 \`:52\`-\`:56\` 的「绝不伪造」承诺；调用点 \`src-tauri/src/core.rs:439\`（\`npm_global_prefix\` 不查 \`is_file\`） | \`read_node()\` 会在契约缺 \`npmPath\`（旧 schema）时**伪造**一个 npm 路径并作为 \`Some\` 返回，违背模块承诺；\`ensure()\` 与 \`core.rs:601\` 有 \`is_file\` 兜底，但 \`npm_global_prefix\` 直接用伪造路径去 spawn。 | \`npmPath\` 缺失/为空 → 返回 None（或单独返回 node-only），由调用方决定回退。 | 「环境就绪」可再次指向不存在的 npm（旧缺陷回归风险）；\`npm prefix -g\` 静默失败为 None。 |
| S2B-RTC-02 | \`src-tauri/src/runtime_contract.rs:95\`、\`120\`-\`126\` | \`write()\` 对 \`create_dir_all\`、\`write\`、\`rename\` 的错误全部 \`let _ =\`；失败后仍返回 \`()\`，\`ensure()\` 照常返回 \`Some(rt)\`。 | 返回 \`Result\`；写失败至少落 shell.log，并让调用方知道契约未持久化。 | 契约写盘静默失败 → 内核读到旧 \`runtime.json\`（node/npm 分叉），下次启动才可能自愈。 |
| S2B-RTC-03 | \`src-tauri/src/runtime_contract.rs:130\`-\`151\` | \`read_node()\` 完全不校验 \`schema\`（写了 \`SCHEMA=2\` 却从不比对）。DESIGN-SHELL-ARCHITECTURE C2 要求 schema 不匹配时明确拒绝。 | 读取时校验 \`schema\`，未来版本明确拒绝并留痕。 | 新旧契约格式静默混读，可能解析出错误 node/npm。 |
| S2B-RTC-04 | \`src-tauri/src/runtime_contract.rs:188\`-\`190\` | \`std::env::join_paths(...).unwrap_or_default()\` 在某个 PATH 目录含分隔符（如 Unix 文件名带 \`:\`）时返回**空 PATH**。 | join 失败时退回原始 PATH 或跳过非法条目。 | spawn 出的 npm/guard 得到空 PATH，运行时找不到依赖。 |
| S2B-RTC-05 | \`src-tauri/src/runtime_contract.rs:163\`；\`src-tauri/src/node.rs:311\`-\`314\`；调用点 \`src-tauri/src/commands/mod.rs:281\` | \`ensure()\` 慢路径 = \`probe_system_node()\`(≤20s) + \`probe_after()\` 再次 \`probe_system_node()\`(≤20s) + \`node_version\`(≤5s)，最坏 ≈45s；\`core_apply_inner\` 在 async 命令体内**直接同步调用**（未 spawn_blocking）。 | \`ensure()\` 内只探一次；调用方一律 \`spawn_blocking\`。 | 版本对齐命令在 Node 未就绪时阻塞 tokio 工作线程数十秒，拖慢整个 IPC。 |

### update.rs

| ID | 证据 | 问题 | 最小修法 | 影响 |
|---|---|---|---|---|
| S2B-UPD-01 | \`src-tauri/src/update.rs:118\`-\`132\`（read-modify-write）、\`src-tauri/src/update.rs:99\`（固定 temp 路径 \`identity.json.tmp\`）、\`src-tauri/src/update.rs:101\`-\`103\`（错误吞没） | \`init_identity\`/\`set_phase\` 是无锁 read-modify-write，且并发写共享同一 tmp 文件：可能丢字段、rename 失败被静默忽略。 | 加进程内 mutex 串行化；temp 名带 pid/线程唯一后缀。 | 并发 \`shell_update_check\`/\`shell_identity\` 时 \`version\`/\`exe\` 等运行时字段可能被旧快照覆盖，内核看护读到的身份不完整。 |
| S2B-UPD-02 | \`src-tauri/src/update.rs:43\`-\`59\` | 日志滚动是「metadata→read_to_string→整体 write」的 read-modify-write，且与 append 无锁；多线程并发日志会丢行/交错。 | 滚动与 append 用同一把锁（或单写者线程/队列）。 | 最关键的崩溃/更新日志可能被并发写吞掉，排障证据缺失。 |
| S2B-UPD-03 | \`src-tauri/src/update.rs:89\`-\`94\`（\`read_json\` 任何错误→\`{}\`）、\`:118\`-\`132\` | \`identity.json\` 损坏（截断/半写）时 \`read_json\` 静默返回 \`{}\`，下一次 \`set_phase\` 就以「只有 phase」的整体覆盖写回，抹掉 \`version/exe/pid\`。 | 解析失败时保留/备份原文件并明确留痕，或拒绝覆盖。 | 身份文件损坏被「自愈」成更严重的缺失，内核 shell 看护失去 \`exe\`。 |

---

## 已核，无问题

- **选版算法与契约 §3 一致（④）**：\`release_channel::select\`（\`src-tauri/src/release_channel.rs:131\`-\`152\`）
  严格按 ① rollback ② （灰度机）canary ③ latest ④ versions 最高 ⑤ 明确 Err 执行，
  与 \`RELEASE-CHANNEL-CONTRACT.md\` §3 逐条对应，也与内核 \`src/platform/distribution/release.js\` 的
  \`pickReleaseVersion\`（isOurs 分支）语义一致。RC-1/RC-2/RC-4/RC-5 均有实现。
- **beta/rc 通道**：契约 §3 冻结算法未包含 beta/rc 步骤，壳与内核同样不实现 —— 两仓一致，
  非漏改（§2 的 beta/rc 是「显式选择」语义，未进入选版自动算法）。
- **非法 tag 处理**：\`tag()\`（\`:72\`-\`79\`）对空串/占位符/非版本一律视为不存在并继续下一步，符合注释与 RC-5。
- **versions 兜底**：\`highest_version\`（\`:82\`-\`98\`）只对通过 \`is_valid_version\` 的键做 \`semver_cmp\`，
  与内核 \`highestVersion\` + \`semverCompare\` 语义一致（release>prerelease、数字段<字符串段、忽略 build）。
- **release_channel 自身不触网**：该模块为纯函数 + 注入闭包（\`:131\`、\`:357\`），网络全部在 \`core.rs\`；
  非测试代码无 \`unwrap/expect/panic\`。
- **镜像 URL 拼接**：\`probe_all\`（\`src-tauri/src/mirror.rs:318\`-\`322\`）两侧 \`trim_*_matches("/")\`，
  无重复斜杠；\`mirror_set\`（\`commands/mod.rs:586\`-\`590\`）要求 \`http(s)://\` 前缀。
- **镜像原子写**：\`mirror.rs:170\`-\`172\`、\`:258\`-\`260\` 均为 tmp+rename。
- **ureq 超时语义**：\`Request::timeout\` 为 overall deadline 且传入响应流（ureq 2.12.1 \`request.rs:59,122\`、
  \`stream.rs:54\`-\`86\`），body 读取受时间约束 —— 故 S2B-MIR-01 的问题在「scope 等最慢线程 + DNS 未覆盖」，
  不在时间维度的基本实现缺失。
- **nodeprobe 正确性机制**：worker \`catch_unwind\`（\`:352\`）、代际校验防旧 worker 污染诊断（\`:355\`）、
  \`ORPHANS\` 防重复堆积（\`:219\`-\`225\`）、枚举阶段交错 + PATH 上限 64（\`:516\`）均已实现；
  S2B-NP-01/02/03 是边界与并发竞态，不是主机制缺失。
- **runtime_contract 往返契约**：\`write\` 保留旧键 \`nodePath/nodeVersion\`（\`:109\`-\`115\`），
  \`read_node\` 容忍缺失并可由 \`node\` 派生 bin（\`:134\`-\`:138\`）；\`probe_npm\` 对「无 npm」返回 None（\`:57\`-\`76\`）。
- **update.rs panic 路径**：\`write_identity_for\` 的 \`expect\`（\`:123\`）之前已用 \`is_object\` 归一（\`:120\`-\`:122\`），
  非可达 panic；\`install_kind\`/\`self_update_capable\` 无 panic。

## 备注

- 本报告未修改仓库任何文件；未执行 cargo/npm/构建/测试；未做 git 写操作。
- 所有行号基于审计时工作树快照。