# 壳仓 S2-D 分片：docs 漂移审计（只读）

> 审计对象：壳仓当前工作树（`(壳仓工作区)`，remote=wasi7mglns/dsh-supervisor-launcher）。
> 基线说明：任务书写「HEAD e41c1d8 + 27 未提交 + 6 未跟踪」，实测工作树**干净**，HEAD=`0930884`（分支 `release/shell-1.1.8`，领先 origin/main 2）。`e41c1d8`（文档移交）与 `0930884`（release_channel 落地）已在本地提交，即原「未提交/未跟踪」内容已入库。本报告按**当前工作树**审计。
> 约束遵守：未做任何 git 写、未跑测试/构建；未修改被审计仓任何文件。报告内一律仓库相对路径。
> 审计文档：README.md、docs/README.md、KERNEL-LAUNCH-STANDARD.md、DESIGN-BOUNDARY.md、DESIGN-COMPLETE.md、DESIGN-SHELL-ARCHITECTURE.md、DESKTOP-ACCEPTANCE.md、RELEASE-STANDARD.md、DEVELOPMENT-TRACK.md、ENV-TOOLCHAIN-INSTALL-STANDARD.md。

---

## 结论摘要

- 文档体系与代码总体方向一致，发布/构建（RELEASE-STANDARD）、门禁体系（K-*、G-*、R-*、R-G*）大多真实存在；release_channel.rs 与内核 `RELEASE-CHANNEL-CONTRACT.md` §3/§5 高度一致。
- 但存在一处**体系性过时路径**（P0）与多处「规范声称的门禁/符号/计数与实现不符」（P1/P2）。
- 最危险的模式：规范声称「有 G8 门禁保证注释引用路径存在」，但该门禁**不存在**，且实际已积累多条悬空引用——即「仓库对自己说谎」这一被规范点名的问题正在发生。

---

## P0

### D-P0-1 状态根路径全面过时：文档仍写 `~/.dsh/...`，代码已迁到 XDG/Application Support/LOCALAPPDATA
**声称**（SSOT 文档把 `~/.dsh` 下的 supervisor/shell 目录描述为契约落点）：
- `docs/KERNEL-LAUNCH-STANDARD.md:31`：P0 `runtime.json` = `~/.dsh/supervisor/runtime.json`
- `docs/KERNEL-LAUNCH-STANDARD.md:68`：`core.json` 路径 = `~/.dsh/supervisor/core.json`
- `docs/DESIGN-SHELL-ARCHITECTURE.md:166,184`：`~/.dsh/supervisor/registry.json`、`~/.dsh/supervisor/runtime.json`
- `docs/DESIGN-BOUNDARY.md:101,155,194,195`：`~/.dsh/supervisor/registry.json`、`~/.dsh/shell/identity.json`、`~/.dsh/shell/update-journal.json`
- `docs/DESIGN-COMPLETE.md:139-143`：config.json/runtime.json/registry.json/identity.json/update-journal.json 全部 `~/.dsh/...`

**实现证据**（状态根已独立于 DSH）：
- `src-tauri/src/env.rs:176-193`：`state_root()`（`DSH_SUPERVISOR_HOME` 覆盖）→ `supervisor_dir()=<状态根>/supervisor`、`shell_dir()=<状态根>/shell`；:221-226 `config.json` 读 `supervisor_dir()`。
- `src-tauri/src/platform/mod.rs:264-271`：Linux 默认 `$XDG_STATE_HOME/dsh-supervisor` 或 `~/.local/state/dsh-supervisor`。
- `src-tauri/src/platform/macos.rs:149-151`：`~/Library/Application Support/dsh-supervisor`。
- `src-tauri/src/platform/windows.rs:237-244`：`%LOCALAPPDATA%\dsh-supervisor`。
- 门禁 `src-tauri/tests/kernel_launch_standard_test.rs:167-202`（K-8）断言 `supervisor_dir` 函数体不得再拼 `.dsh`，并含反向判据。

**影响**：运维/用户按「唯一事实源」去 `~/.dsh` 找 core.json/runtime.json/registry.json/identity.json 会全部落空（老用户仅靠 `env.rs:197-219 migrate_legacy` 前向自愈迁移）。这是 SSOT 文档级的错误，非笔误。
**最小修法**：把上述文档的 `~/.dsh/{supervisor,shell}` 统一改为「产品状态根/`supervisor`（或 `shell`）」并给出三平台默认与 `DSH_SUPERVISOR_HOME` 覆盖；同步修 `src-tauri/src/main.rs:345`、`src-tauri/src/mirror.rs:12,80` 里残留的 `~/.dsh/...` 注释。

---

## P1

### D-P1-1 KERNEL-LAUNCH §4 的两个失败 code 在代码中不存在
**声称**：`docs/KERNEL-LAUNCH-STANDARD.md:96` `INSTALL_FAILED`、:98 `SERVICE_DEFINE_FAILED`。
**实现证据**：全仓 grep 这两个字符串**只命中该文档**。代码实际产出的结构化 code 为 `RUNTIME_MISSING`（`src-tauri/src/domain/guardctl.rs:131`）、`ALIGN_RESOLVE_FAILED`（:139）、`KERNEL_NOT_ALIGNED`（:144）、`SERVICE_START_FAILED`（:178）、`READY_TIMEOUT`（:190 / `src-tauri/src/commands/mod.rs:436`）。P4 服务定义失败甚至**不是失败**：`guardctl.rs:158-161` 只写日志并继续走 spawn 兜底。
**最小修法**：在 `core.rs`/IPC 边界补 `INSTALL_FAILED`，并给 P4 定义失败补 `SERVICE_DEFINE_FAILED` 事件；或把 §4 表改为实际 code 集合并注明「P4 失败非致命、自动走 spawn 兜底」。

### D-P1-2 G8 门禁（注释引用仓内路径必须存在）不存在，且悬空引用已积累
**声称**：`docs/DESIGN-SHELL-ARCHITECTURE.md:292` 与 `docs/DESIGN-COMPLETE.md:578` 把 G8 列为「会失败的测试」，理由是「已发现 30+ 悬空引用」。
**实现证据**：全仓 grep `G8` **仅命中上述文档**（无任何测试）。实际已存在的悬空/错位引用：
- `docs/ENV-TOOLCHAIN-INSTALL-STANDARD.md:143`（G-2）要求 `run_install` 校验 `checkEnvironment`；全仓 0 处 `checkEnvironment`。真实门禁断言的是 `probe_npm/npmOk/npm_exe_name`（`src-tauri/tests/env_toolchain_standard_test.rs:85-94`，函数体见 `src-tauri/src/main.rs:156-198`）。
- `src-tauri/src/release_channel.rs:178` 指内核 `src/platform/install-id.js`；契约 `RELEASE-CHANNEL-CONTRACT.md:117` 与内核实际文件均为 `src/platform/service/install-id.js`（缺 `/service/`）。同类：`src-tauri/src/env.rs:169` 指 `src/platform/state-root.js`，实际为 `src/platform/service/state-root.js`。
- `docs/DESKTOP-ACCEPTANCE.md:18` 写 `sudo dpkg -i dist/*.deb`；Tauri 产物在 `src-tauri/target/release/bundle/deb/*.deb`（workflow `.github/workflows/build.yml:210,225`），`dist/` 仅 CI 组装 npm-shell 且被 `.gitignore:20` 忽略。该路径不存在。

**最小修法**：新增一条 G8 测试（扫描 `src/**` 注释与指定 docs 中的仓内路径 token，断言 `repo_root.join(path).exists()`，含反向判据），并逐条修复上述引用；否则把 G8 从规范删除。

### D-P1-3 G3 的文档阈值（150/200 行）与门禁实现（550 行）不一致
**声称**：`docs/DESIGN-COMPLETE.md:573`「main.rs ≤ 150 行」；`docs/DESIGN-SHELL-ARCHITECTURE.md:287`「main.rs ≤ 200 行」；两文件的验收标准（`DESIGN-COMPLETE.md:587`、`DESIGN-SHELL-ARCHITECTURE.md:367`）沿用。
**实现证据**：`src-tauri/tests/bootstrap_flow.rs:1656-1667`（G3）实际上限为 **550 行**，并附理由「不设 150 是因为 setup() 窗口/托盘尚未拆，设当下即红的数字会被 ignore」；当前 `src-tauri/src/main.rs` = **548 行**（临界通过）。
**最小修法**：规范把「目标 ≤150/200」与「当前门禁 ≤550」分开表述，或直接把规范阈值改为 550 并记录目标值。

### D-P1-4 KERNEL-LAUNCH §6 门禁表只列 K-1..K-6，实现已有 K-1..K-10
**声称**：`docs/KERNEL-LAUNCH-STANDARD.md:123-132` 仅 K-1..K-6（而同一文件 :108 又写「各有门禁（K-1..K-10）」）。
**实现证据**：`src-tauri/tests/kernel_launch_standard_test.rs` 含 K-7（:137 Windows 看护）、K-8（:165 状态根独立）、K-9（:203 状态根注入）、K-10（:221 Windows 稳定入口/杀守卫），且各带反向判据。
**最小修法**：把 K-7..K-10 补进 §6 表。

### D-P1-5 前端模块数：实现 9 个，多处文档写 8 个
**实现证据**：`src-tauri/bootstrap/js/` 实有 9 个文件：`00-runtime.js,10-ui.js,20-env.js,30-mirror.js,40-shell-update.js,50-kernel.js,60-guard.js,70-boot.js,80-init.js`。
**声称**：`docs/DESIGN-SHELL-ARCHITECTURE.md:7` 写「九个」（正确），但同文件 :368 与 `docs/DESIGN-COMPLETE.md:552,588` 写「8 个 JS 模块」。
**最小修法**：统一为 9，并让该计数随目录门禁生成/校验。

---

## P2

### D-P2-1 RELEASE-STANDARD §8 表缺 R-10，docs/README 同缺
- 实现：`src-tauri/tests/release_spec_consistency_test.rs:263-292` 有 `r10_assembler_covers_all_ci_matrix_artifacts`。
- 声称：`docs/RELEASE-STANDARD.md:146-159` 只列 R-1..R-9；`docs/README.md:47-49` 同样只列到 R-9。
- 修法：§8 与 docs/README 补 R-10（assembler PLATFORMS 覆盖 CI 全 artifact）。

### D-P2-2 CI step 名/注释的测试区间过时（B1–B56、G1–G6）
- `.github/workflows/build.yml:118,120`：`G1–G6 + B1–B56`。
- 实际：`bootstrap_flow.rs` 编号到 **B63**（66 个 `#[test]`；`docs/README.md:70` 与 `docs/DESIGN-SHELL-ARCHITECTURE.md:327` 也写 B63）。且「G1–G6」与规范 `G1–G8`（`docs/DESIGN-SHELL-ARCHITECTURE.md:285-292`）及 `update_guard_test` 自用的 G6-i/j/k 存在编号重载。
- 修法：step 名与注释改为「全部 tests/*.rs」的稳定描述，避免硬编码区间；或改为 G1–G8 + B1–B63。

### D-P2-3 DESKTOP-ACCEPTANCE 的 deb 安装路径错误
`docs/DESKTOP-ACCEPTANCE.md:18` `dist/*.deb` → 应为 `src-tauri/target/release/bundle/deb/*.deb`（见 `.github/workflows/build.yml:210`、`.gitignore:20`）。

### D-P2-4 DESIGN-COMPLETE 自相矛盾：无头入口「4 个」却列 5 个
`docs/DESIGN-COMPLETE.md:200` 写「4 个」，紧跟列出 `--env-plan/--mirror-plan/--node-plan/--core-plan/--service-plan`；实际 `src-tauri/src/main.rs` 注册 8 个（:354,:358,:362,:366,:374,:379,:386,:392，另有 `--platform-matrix`、`--shell-update-plan`、`--run-guard`）。修法：改为实际清单与数量。

### D-P2-5 DEVELOPMENT-TRACK 硬编码操作者本机绝对路径
`docs/DEVELOPMENT-TRACK.md:19` 在「允许」列写入了一条操作者本机工作区绝对路径（家目录下 develop 工作区）。违反可移植性/脱敏，也违背本报告口径。修法：改为「本仓工作区（仓库根）」或仓库相对路径。

### D-P2-6 代码注释残留过时路径与错位内核路径
- `src-tauri/src/main.rs:345`：`~/.dsh/shell/shell.log`（实际 `<状态根>/shell`）。
- `src-tauri/src/mirror.rs:12,80`：`~/.dsh/shell/mirrors.json`（同上）。
- `src-tauri/src/release_channel.rs:178`、`src-tauri/src/env.rs:169`：内核路径缺 `/service/`（见 D-P1-2）。
修法：随 D-P0-1/D-P1-2 一并修。

### D-P2-7 RELEASE-STANDARD §1 的命令与 CI 实际不一致
- 文档 `docs/RELEASE-STANDARD.md:54` H3 `cargo run -- --node-plan` vs workflow `.github/workflows/build.yml:152` `./target/debug/dsh-supervisor-gui --node-plan`。
- 文档 :55 H4 `cargo tauri build` vs workflow :166 `npx --yes @tauri-apps/cli@2 build --bundles ...`。
两种都可行，但「唯一流程事实源」应写 CI 真实命令或明确标注二者等价。修法：对齐或加注。

### D-P2-8 目标目录树与实际结构差距大，但验收标准以既成口吻陈述
- 目标/规范：`docs/DESIGN-SHELL-ARCHITECTURE.md:36-62`、`docs/DESIGN-COMPLETE.md:322-348` 描述 `commands/{env,node,core,mirror,shell,window}.rs`、`domain/{probe,provision,mirror,update,contract}/`、`infra/`。
- 实际：`src-tauri/src/commands/mod.rs` 单文件（779 行）、`domain/{cli,coreloc,guardctl,localhttp,windowing,mod}.rs`、**无 `infra/` 目录**（`bounded.rs` 在 `src/` 根）。
- 文档已标「目标/迁移路径」，但 §八 验收标准与门禁表 G7 直接引用 `infra::bounded`——该符号不存在（实际 `crate::bounded`，B32 门禁 `bootstrap_flow.rs:769-792`）。修法：明确区分「目标 vs 现状」，并把 G7 的 `infra::bounded` 改为指向 B32/`crate::bounded`。

### D-P2-9 G4/G6/G7 门禁编号名不副实
- G4（能力×平台=实现或 Unsupported，`docs/DESIGN-SHELL-ARCHITECTURE.md:288`）：无名为 G4 的测试；实际由 `--platform-matrix`（`src-tauri/src/main.rs:374`）与结构门禁 `src-tauri/tests/platform_unsupported_structure_test.rs`（U-a..U-d）部分覆盖。
- G6（契约 schema+启动导出，:290）：无同名测试；`update_guard_test.rs:1` 的 G6 是另一件事（跨平台/回退护栏），编号冲突。
- G7（:291）→ 见 D-P2-8。
修法：门禁编号唯一化，或规范直接引用真实测试名。

### D-P2-10 release_channel 的 `DSH_CANARY_ID` 扩展未在契约文档化
`src-tauri/src/release_channel.rs:53-55,214-222` 增加 `DSH_CANARY_ID` 环境变量覆盖 installId；`RELEASE-CHANNEL-CONTRACT.md` §5.2 只定义文件来源。属壳侧扩展，建议回写契约或在壳侧注明。

---

## 内核对照（只读）

- **RELEASE-CHANNEL-CONTRACT.md**：`src-tauri/src/release_channel.rs` 忠实实现 §3 五步（① rollback 最高优先级 :131-135、② 仅灰度机取 canary :136-141、③ 优先 latest :142-145、④ versions 兜底 :146-149、⑤ 明确 Err :150-152），§5.5 短路顺序（:357-378：local → 未 opt-in 零请求 → opt-in 才读包），§5.3 `schema:1` 唯一格式并显式拒绝旧 7 形状（:293-338）。与契约 `RELEASE-CHANNEL-CONTRACT.md:44-66,173-185` 一致。壳侧对应 RC-1/RC-2/RC-4/RC-5 的门禁在单元测试中逐条覆盖（:443-562）。
- **EXECUTION-CONTRACT.md**：属 plus 仓内核域结构施工接口，**不含任何壳仓声明**；唯一交集是开发纪律（不碰 `/tmp/dsh-*`、`~/.dsh`、`~/.local/state/dsh-supervisor`），与壳 `DEVELOPMENT-TRACK.md` §1 一致，无漂移。
- **跨仓悬空文档名**：壳文档引用的内核文档（KERNEL-DAEMON-CONTRACT、PLATFORM-CAPABILITY-MATRIX、ARCHITECTURE-CONTRACT-phase0、DSH-TOKEN-CONTRACT、NO-CONSOLE-WINDOW-STANDARD、CREDENTIALS-STANDARD、内核 RELEASE-STANDARD、内核 DEVELOPMENT-TRACK、shared/version-vectors.json）在 plus 仓**全部存在**。
- **DESKTOP-ACCEPTANCE §4 的内核下架声明属实**：plus `src/domains/dist/self-update.js` 不存在；`src/api/domains/guard.js:147,156` 与 `src/api/contract.js:44-45` 确认 `/self-update/apply|restart-guard` 返回 410 `KERNEL_UPDATE_SINGLE_WRITER`。
- **待内核分片核实**：README `0.0.0.0:3088 → 127.0.0.1:3080` 在壳仓无实现（属内核 LAN 反代）；plus 仓 `src/` 未 grep 到 `3088` 字面量，仅 README/CHANGELOG/ui dist 提及。壳仓侧无需动作，但建议内核 docs 分片确认该默认端口是否仍准确。

---

## 已核实「声称=实现」的清单（抽样）

- 发布流程 `docs/RELEASE-STANDARD.md:14-21`：根目录无 package.json、`.github/workflows/` 外无 workflow YAML、`scripts/` 仅 bump+verify —— R-1..R-9 门禁（`release_spec_consistency_test.rs:58-261`）均存在。
- 四平台矩阵 `docs/RELEASE-STANDARD.md:64-71` ↔ `.github/workflows/build.yml:56-77`（ubuntu-22.04/win/macos/macos-15-intel，bundles 与 glibc 2.35）一致。
- 版本三处互锁：`scripts/bump-shell.sh:16-20`、`scripts/verify-shell-versions.js:8-11`；当前 Cargo.toml/tauri.conf.json/Cargo.lock 均 1.1.7。
- 服务定义矩阵 `docs/KERNEL-LAUNCH-STANDARD.md:45-55`：`src-tauri/src/platform/linux.rs:183-189`（systemd user unit）、`platform/macos.rs:20`+`definition_path`（LaunchAgents/com.dsh.supervisor.plist）、`platform/windows.rs:13`（schtasks DSH-Supervisor）；统一稳定入口由 `platform/mod.rs:120 service_exec_line` + 各平台 `service_command()` 组装（K-4/K-9/K-10 断言）。
- Node 安装矩阵 `docs/DESKTOP-ACCEPTANCE.md:44`：Linux `platform/linux.rs:21,116`（pkexec/sudo + tar 到 /usr/local）、Windows `platform/windows.rs:173`（msiexec /qn）、macOS `platform/macos.rs:121`（installer -pkg via osascript）。
- 默认 apiPort `docs/DESKTOP-ACCEPTANCE.md:21`（127.0.0.1:36360）= `src-tauri/src/env.rs:271 DEFAULT_API_PORT`。
- 环境工具链 §2.1 字段（installed/minOk/minRequired/npmOk/npmPath/nodePath/busy/status/progress/probeError/stuck/trace）在 `src-tauri/src/commands/mod.rs:34-94` 与 `src-tauri/src/main.rs:68-78` 均有。
- ENV §5 的 G-1..G-6 门禁真实存在于 `src-tauri/tests/env_toolchain_standard_test.rs:72-178`；进度条/旧事件已清除（:99-133）。
- DEVELOPMENT-TRACK R-1/R-2/R-3 由 `src-tauri/tests/dev_runtime_safety_test.rs`（R-G1/R-G2/R-G3，含反向判据）覆盖。
- 幽灵文件处置属实：`src-tauri/launcher-build.yml` 不存在，R-2 有反向判据（`release_spec_consistency_test.rs:74-104`）。
- 壳许可 MIT（`LICENSE:1`）；更新通道为 npm CDN 静态清单（`src-tauri/tauri.conf.json:53-56` unpkg/jsdelivr `@dsh-sup/shell-release@latest/shell-manifest.json`）。

---

## 建议的最小修复顺序

1. D-P0-1（状态根路径）——影响运维面最大，先行。
2. D-P1-1（失败 code）、D-P1-2（G8 门禁 + 悬空引用）、D-P1-3（G3 阈值）、D-P1-4（K 门禁表）、D-P1-5（模块数）。
3. P2 各条随文档例行更新一并处理；D-P2-8/D-P2-9 建议重整门禁编号与「目标 vs 现状」措辞。

> 说明：本报告所有证据均为只读静态核对（read/grep/wc/只读 git），未运行任何测试或构建。
