# 壳仓 S2-D 分片审计：测试 / 门禁覆盖（只读）

> 执行者：子代理（**只读**；未做任何 git 写、未跑任何测试/构建/包管理器）。
> 仓库：壳仓（remote 'wasi7mglns/dsh-supervisor-launcher'）。
> 对照仓库：内核仓（remote 'advgyxqamf/dsh-supervisor-core'），下文以 '内核/' 前缀标注。
> 路径一律为**仓库相对路径**；不记录操作者绝对路径。

---

## 0. 审计对象状态校正（重要）

委托描述为「HEAD e41c1d8 + 6 个未跟踪文件（其中 3 个新测试）」，与实测**不一致**：

| 项 | 委托假设 | 实测 |
|---|---|---|
| HEAD | 'e41c1d8' | '0930884'（'release/shell-1.1.8'，= 5df08aa(main) + e41c1d8 + 本轮 33 文件） |
| 工作树 | 6 个未跟踪文件 | **clean，0 未跟踪** |
| 3 个新测试 | 未跟踪 | 已由 '0930884' **提交跟踪** |

'git diff --name-status e41c1d8..HEAD' 证实 3 个新测试为 'A'（新增）：
'src-tauri/tests/dev_runtime_safety_test.rs'、'src-tauri/tests/env_toolchain_standard_test.rs'、'src-tauri/tests/no_console_window_test.rs'。
**本报告按当前工作树（0930884, clean）审计**；S1 分片报告中的「对齐」已实际发生。

---

## 1. 全量 src-tauri/tests/*.rs 清单与 CI 执行判定

### 1.1 CI 是否真的跑测试

'.github/workflows/build.yml' **有** 'cargo test'，且为**自动枚举全部 test target**：

- 'build.yml:141'：TARGETS=$(ls tests/*.rs | sed 去掉 tests/ 前缀与 .rs 后缀 | grep -v '^updater_artifacts$' | sed 加 --test 前缀)
- 'build.yml:143'：'cargo test --bins $TARGETS 2>&1 | tail -80'（'set -o pipefail'，失败必红）
- 'build.yml:200'：'cargo test --test updater_artifacts -- ...'（需打包产物，单独在 bundle 后跑）
- 触发：'build.yml:11/15' 'branches: [main]' + PR + tag；矩阵含 'build.yml:66 windows-latest'、'build.yml:70 macos-latest'。

**结论**：20 个 'tests/*.rs' 中 19 个由门禁步骤跑，'updater_artifacts.rs' 由单独步骤跑 → 全部会被 CI 执行；新增 'tests/*.rs' 自动纳入，无静默漏跑。
（本机硬约束禁跑测试，以上为静态判定。）

### 1.2 逐文件清单

| 文件（tests/ 下） | 测什么 / 覆盖对象 | CI |
|---|---|---|
| 'bootstrap_flow.rs' | 引导流程回归 B1–B63：步骤顺序、前后端有界性、镜像、平台层归属、托盘、Windows 配置 | 是 |
| 'ci_gate_coverage_test.rs' | CI 覆盖性元门禁 C-a..C-e（自动枚举、排除面、完整矩阵、无轻量替代） | 是 |
| 'dev_runtime_safety_test.rs' | R-G1/R-G2/R-G3：/tmp 通配删除、系统运行时路径破坏性操作（**新增**） | 是 |
| 'env_toolchain_standard_test.rs' | G-1..G-6：npm 与 node 同权、run_install 校验、进度条残留、旧事件名、NS.install（**新增**） | 是 |
| 'guard_resolution_test.rs' | 稳定入口结构：服务定义不固化 node/guard 路径、工具链门禁要求 npm | 是 |
| 'kernel_install_evidence_test.rs' | K-1..K-8：内核安装失败可定位、verbatim 前缀剥除、重试有界 | 是 |
| 'kernel_launch_standard_test.rs' | K-1..K-10：内核定位/对齐/就绪/平台统一/状态根 | 是 |
| 'kernel_update_single_writer_test.rs' | SW1–SW6：内核更新单写入者=壳 | 是 |
| 'mirror_env_wiring_test.rs' | M-a..e：registry selected、延迟同源、统一 install_progress | 是 |
| 'no_console_window_test.rs' | S-W1/S-W2：src 下 spawn 白名单、env::node_version 经 prepare（**新增**） | 是 |
| 'no_suppression_machinery_test.rs' | 反回归：不重新引入回退/拉黑/冷却/跳过机构 | 是 |
| 'platform_launch_contract_test.rs' | L1–L5：三平台稳定入口、runtime 契约 schema、端口发现 | 是 |
| 'platform_shared_items_test.rs' | 平台文件共享项导入（父模块项） | 是 |
| 'platform_unsupported_structure_test.rs' | unsupported.rs 的 trait 方法归属 | 是 |
| 'release_spec_consistency_test.rs' | R1–R10：发布规范一致性、矩阵、版本单源、CI 接线 | 是 |
| 'round13_p3_batch_test.rs' | P-a..e：平台事实判定/前端告知/死广播清理 | 是 |
| 'service_self_heal_test.rs' | B61：服务定义自愈行为级 | 是 |
| 'update_guard_test.rs' | G6-i/j/k：probe 超时单源、SHA 失败回退、退出握手 | 是 |
| 'update_pipeline_test.rs' | U1–U3：壳/内核统一更新决策形状 | 是 |
| 'updater_artifacts.rs' | V1–V6：Tauri 同源验签 + 清单契约（需产物） | 是（独立步骤） |

---

## 2. 三个新增测试 vs 内核同名门禁：判据条目对照

### 2.1 dev_runtime_safety_test.rs ↔ 内核/test/dev-runtime-safety-gate-test.js

| 内核检查项 | 壳等价项 | 判定 |
|---|---|---|
| R-G1 /tmp 通配/前缀删除 | 'r_g1_no_tmp_glob_deletion'（dev_runtime_safety_test.rs:141-162） | **弱化→空转**：扫描根错误，见下 |
| R-G2 系统运行时路径破坏性操作 | 'r_g2_no_destructive_system_path_ops'（:164-183） | **弱化→空转** |
| R-G3 反向非空转 | 'r_g3_detector_is_not_vacuous'（:185-224） | 判据函数非空转，但**未断言扫描数>0**，救不了空转 |
| （内核独有）R-G4 strip 顺序自检 | 无 | 缺失（P2，本仓行式剥离无 glob 吞代码风险） |
| 扫描面：内核 SCAN_DIRS=['src','test','release','bin','ci','.github']、扩展名白名单 | 壳 ['scripts','.github','ci']（:27） | **缺失**：不含 Rust 源码 src-tauri/src、bootstrap、tests，而破坏性代码恰在这些目录 |
| R-G2 破坏性动词：内核含 rm、mv、rmSync(、unlinkSync( 与绝对系统路径正则 | 壳仅 rm -rf / rm -r / remove_dir_all / remove_file / unlinkSync / rmSync（:118-123），无 mv、无绝对路径 | 弱化 |

**致命点（P0）**：壳测试 root()=env!("CARGO_MANIFEST_DIR")，即 src-tauri/（dev_runtime_safety_test.rs:19-21），而 scanned_files() 在其下找 scripts/.github/ci（:27）——实测 'src-tauri/scripts'、'src-tauri/.github'、'src-tauri/ci' **三者皆不存在**（真实目录在仓根）。于是循环 if !d.is_dir() { continue; } 全部跳过，**扫描 0 个文件**，R-G1/R-G2 恒 PASS。该门禁当前对仓内任何脚本均**不设防**。

**最小修法**：scanned_files() 改用仓根为基准（root().join("..")），扫描 ['scripts','.github','ci','src-tauri/src','src-tauri/bootstrap','src-tauri/tests']；并在 r_g1/r_g2 里加 assert!(scanned > 0, ...)。

### 2.2 env_toolchain_standard_test.rs ↔ 内核 ENV-TOOLCHAIN-INSTALL-STANDARD.md 及其门禁

**前提不成立**：内核仓**不存在** ENV-TOOLCHAIN-INSTALL-STANDARD.md，也**不存在**任何引用 npmOk/probe_npm/20-env 的门禁测试（全仓 grep 仅 CHANGELOG.md/CROSS-PLATFORM-BUILD-AND-UPDATE.md 提「工具链」）。该文档唯一副本在**壳仓** 'docs/ENV-TOOLCHAIN-INSTALL-STANDARD.md'（0930884 从内核移交的壳资产，见 S1 报告）。

因此不存在「内核同名门禁」可对照；正确对照物是**壳仓 SSOT §5 门禁表**（docs/ENV-TOOLCHAIN-INSTALL-STANDARD.md:140-146）：

| SSOT §5 门禁 | 壳测试实现 | 判定 |
|---|---|---|
| G-1 node_status 含 npmOk/npmPath 且来源 probe_npm | 'g1'（:72-80）检查 commands/mod.rs 含三词 + runtime_contract.rs 含 fn probe_npm | 等价（未剥注释，靠真实代码命中） |
| G-2 run_install 启动前校验 checkEnvironment(node+npm)/启动后校验 npm | 'g2'（:84-94）仅查 fn run_install 函数体含 probe_npm / npmOk / npm_exe_name 之一 | **弱化**：不查 checkEnvironment；不剥注释，main.rs:186 注释含 probe_npm，即使删掉 :188 真调用与 :195 reinstall_for_npm 仍可过 |
| G-3 全仓无 showProgress/hideProgress/progBar/id="prog" | 'g3'（:98-114）只 walk("bootstrap") | 范围收窄（未覆盖 src/） |
| G-4 全仓无旧事件名 | 'g4'（:116-133）只 walk("src") | **弱化**：前端 JS（bootstrap/）不扫，旧事件名残留不会被本门禁拦住 |
| G-5 独立 npmOk === false 分支、不共用安装文案 | 'g5'（:135-159）取 700 字符窗口，要求含 start_node_install **或** start_npm_install + 一句带 npm 的 status | **弱化**：放行的恰是 SSOT §1 根因形态 NS.core.invoke('start_node_install')（20-env.js:110）；且不验证后端真执行 npm 修复 |
| G-6 前端所有 install 文案经 NS.install.* | 'g6'（:161-178）只查 10-ui.js 600 字符窗口含 begin/text/done/fail | **弱化**：不扫描其他模块是否自行拼装 |
| （SSOT §2.4/§3.3）统一事件三件套与 80-init.js 消费 | 本文件无 | 缺失（其余测试片段覆盖，非本门禁） |
| （SSOT §2.3）reinstall_for_npm 修复路径 | 本文件无 | 缺失 |

**最小修法**：g2 先剥离注释再取 fn_body，并要求同时出现 probe_npm_after/npmOk 与 reinstall_for_npm；g3/g4 扫描根扩到 src-tauri（含 bootstrap）；g5 去掉对 start_node_install 的放行（只认 start_npm_install，或在后端 run_install 内新增行为门禁以保证修复）；g6 增扫 bootstrap/js 全部模块不得直接操作进度/安装样式。

### 2.3 no_console_window_test.rs ↔ 内核/test/no-console-window-gate-test.js + 内核/NO-CONSOLE-WINDOW-STANDARD.md §4

| 内核项 | 壳等价项 | 判定 |
|---|---|---|
| K-W1 spawn.js 三入口 options 含 windowsHide: true（含 carrier 间接） | 无 | **缺失**：壳无「统一执行器真的加了隐藏标志」的断言 |
| K-W2 src/** 裸 spawn = 0（spawn.js 豁免） | 's_w1_spawn_sites_are_bounded'（:159-195） | 结构对应，但见下三条弱化 |
| K-W3 反向能识别旧形态（合成样本） | S-W1 仅有 !hits.is_empty() + 白名单条目存在（:162-175） | **弱化**：无「非白名单 spawn 且无 prepare 会被判违规」的合成反例，判据失效不会报警 |
| SSOT §4 S-W1：仅 bounded.rs、platform/mod.rs、platform/service.rs | 白名单 7 项：+platform/unsupported.rs、linux.rs、macos.rs、windows.rs（:136-157） | **多余白名单**：比 SSOT 放宽 4 个平台文件（当前这些文件无 spawn，属潜在放行） |
| SSOT §4 S-W2：env::node_version 经统一执行器或显式加标志 | 's_w2'（:197-215）：函数体含 prepare 且无 creation_flags | 等价 |

补充：S-W1 对非白名单文件只做**文件级** contains("prepare(")，未定位到具体 spawn 调用点；env.rs:102 的 prepare 恰好覆盖 env.rs:103 的 cmd.spawn()，当前可通过，但存在「同文件另一个函数裸 spawn 也被放行」的漏洞。

**最小修法**：白名单收敛为 SSOT 的 3 项；S-W1 改为按函数体/相邻调用点判定 prepare；新增 K-W1 等价断言（读取 bounded.rs::prepare 函数体必须含 creation_flags/CREATE_NO_WINDOW）；为 S-W1 加合成反例（spawn 无 prepare 必须命中）。

---

## 3. ci_gate_coverage_test.rs 是否真读 build.yml

**是真读文件内容，不是只断言存在。** ci_gate_coverage_test.rs:37-40 用 fs::read_to_string(...build.yml)，读取失败直接 panic；随后对**正文**做断言（:77-194），并用 strip_yaml_comments/job_block 解析结构。c_e 还自带正/反样本证明判据非空转。

但存在可绕过的判据（非「空转」而是「够不到」），列 P2：

- c_c 第③条 build.contains("cargo test")（:141-144）可被 build.yml:200 的 updater_artifacts 步骤满足——即使门禁枚举步骤不存在，C-c 仍过（C-a 会兜住，但 C-c 判据本身弱）。
- has_hardcoded_test_targets（:48-54）把含 updater_artifacts 的整行排除：若有人写 'cargo test --bins --test bootstrap_flow --test updater_artifacts'，该硬编码行**不被识别**。
- c_d（:148-168）只枚举 5 个固定 job 名，换名的轻量 job 可绕过。
- 判据依赖精确字面量 'ls tests/*.rs'（:81），改写枚举实现（如 find）会假红——属脆弱而非空转。

**最小修法**：C-c 只认门禁步骤对应的 cargo test --bins 行；has_hardcoded_test_targets 改为「排除自动枚举行本身」而非「排除含 updater_artifacts 的行」；C-d 改为「任一 job 含 cargo check 且无 cargo build/tauri build 即违规」的全 job 扫描。

---

## 4. 结论清单（P0/P1/P2 + 证据 + 最小修法）

### P0

1. **dev_runtime_safety_test.rs 扫描 0 文件、恒 PASS（假绿门禁）。**
   - 证据：src-tauri/tests/dev_runtime_safety_test.rs:19-49（root=src-tauri，扫描 src-tauri/scripts|.github|ci）——三个目录实测不存在（真实位于仓根）；无 scanned>0 断言。
   - 修法：基准改仓根；扫描面加入 src-tauri/src、src-tauri/bootstrap、src-tauri/tests；加非空断言。

### P1

2. **no_console_window_test.rs 白名单比 内核/NO-CONSOLE-WINDOW-STANDARD.md:81 的 S-W1 宽 4 个平台文件。**
   - 证据：src-tauri/tests/no_console_window_test.rs:136-157 vs 内核/NO-CONSOLE-WINDOW-STANDARD.md:81。
   - 修法：白名单收敛为 bounded.rs + platform/mod.rs + platform/service.rs。
3. **壳侧无 K-W1 等价断言：bounded::prepare 是否真的加 CREATE_NO_WINDOW 无人守。**
   - 证据：src-tauri/src/bounded.rs:49-53（creation_flags(0x0800_0000)）与测试 S-W1/S-W2 均只查 prepare( 引用。
   - 修法：新增测试读取 prepare 函数体，断言含 creation_flags 与 0x0800_0000（Windows 分支）。
4. **壳侧无 K-W3 等价反向样本：S-W1 判据失效不会报警。**
   - 证据：src-tauri/tests/no_console_window_test.rs:159-195（仅非空计数 + 白名单文件存在）。
   - 修法：加入 spawn 无 prepare 的合成夹具必须命中。
5. **env_toolchain_standard_test.rs G-2 可被注释满足、且不查 checkEnvironment。**
   - 证据：src-tauri/tests/env_toolchain_standard_test.rs:84-94；src-tauri/src/main.rs:186 注释含 probe_npm（真调用在 :188/:195）。
   - 修法：先剥注释再取函数体；要求 probe_npm_after 与 reinstall_for_npm 同时出现。
6. **G-4/G-3 扫描面窄于 SSOT「全仓」。**
   - 证据：src-tauri/tests/env_toolchain_standard_test.rs:98-133（只扫 bootstrap / src）vs docs/ENV-TOOLCHAIN-INSTALL-STANDARD.md:144-145（全仓）。
   - 修法：扫描根扩到 src-tauri（含 bootstrap）。
7. **G-5 放行 SSOT §1 记录的根因形态 start_node_install，且不验证后端真修复 npm。**
   - 证据：src-tauri/tests/env_toolchain_standard_test.rs:145-148 放行；src-tauri/bootstrap/js/20-env.js:105-111 复用 start_node_install；SSOT docs/ENV-TOOLCHAIN-INSTALL-STANDARD.md:12-15 将其列为根因。
   - 修法：g5 只认 start_npm_install（或在后端 run_install 增加 npm 修复的行为门禁后再保留放行）。
8. **内核仓无 env-toolchain SSOT / 门禁 → 「跨仓等价」不成立。**
   - 证据：内核全仓无 ENV-TOOLCHAIN-INSTALL-STANDARD.md，test/ 无 npmOk/probe_npm/20-env 引用。
   - 修法：明确该 SSOT 归壳仓唯一维护；或在内核仓补同名门禁，否则「同名对照」无意义。

### P2

9. **env_toolchain_standard_test.rs G-6 只查 10-ui.js 导出，不查其他模块是否自行拼装。** 证据 :161-178；修法：扫描全部 bootstrap/js。
10. **ci_gate_coverage_test.rs 三条判据可绕过**（C-c 的 cargo test 归属、硬编码检测的 updater_artifacts 排除、C-d 固定 job 名）。证据 :48-54、:141-144、:148-168；修法见 §3。
11. **dev-runtime R-G2 动词/路径面窄于内核**：壳无 mv、无裸 rm（仅 rm -rf/rm -r）、无绝对系统路径正则。证据 src-tauri/tests/dev_runtime_safety_test.rs:118-131 vs 内核/test/dev-runtime-safety-gate-test.js:94-101；修法：对齐内核 SYS_PATHS/DESTRUCTIVE。
12. **dev-runtime 无 R-G4 剥离顺序自检**（低风险，本仓为行式剥离）。证据：壳文件无对应块 vs 内核/test/dev-runtime-safety-gate-test.js:50-59；修法可选补。

### 说明（非缺陷）

- S-W1 的 !hits.is_empty() 会被 src-tauri/src/bounded.rs:230 **文档注释**里的 cmd.spawn() 计数命中；因该文件在白名单内，当前无副作用。若将来改为「统计非白名单 spawn」需先剥注释。
- 三个新测试与 ci_gate_coverage_test.rs 均会被 CI 自动枚举执行（build.yml:141/143），本报告的「假绿」均为**判据/扫描面**问题，不是「没被 CI 跑」。

---

## 5. 无法核验项（硬约束导致）

- 未运行 cargo test / cargo build / 任何包管理器，所有 PASS/FAIL 均为**静态判据**推演。
- 未运行内核 JS 门禁（node test/*.js）。
- 未做 git 写操作；HEAD/工作树状态取自只读 git status/rev-parse/diff。
