# S2-D 报告：死代码 / 文档漂移 / 门禁覆盖（壳仓，只读审计）

> 审计对象：壳仓 `(壳仓工作区)`（独立仓），**HEAD = `0930884`**
> （分支 `release/shell-1.1.8`；= 已发布 `origin/main 5df08aa` + 文档移交 + 未提交工作于本次审计前落库）。
> 工作树 clean。方法：只读 `read/grep/wc/node --check/只读 git`；**未跑任何测试/构建、未做任何 git 写**。
> 证据一律**仓库相对路径:行号**；全文不含操作者绝对路径。
> 分片：本报告由主代理 S2-D 汇总自身结论 + 两个下级分片 `_s2-d-docs.md`（docs 漂移）/ `_s2-d-gates.md`（门禁覆盖）。

---

## 0. 结论摘要

| 级别 | # | 主题 | 谁来证 |
|---|---|---|---|
| **P0** | S2D-P0-1 | `src-tauri/tests/dev_runtime_safety_test.rs` 扫描 **0 个文件**、恒 PASS（**假绿**），对全仓脚本不设防 | 主控已独立复验 |
| **P0** | S2D-P0-2 | 多份设计文档的状态根路径**全面过时**（仍写 `~/.dsh/...`），代码已迁到产品状态根；K-8 门禁反而禁止 `.dsh` | 主控已独立复验 |
| P1 | S2D-P1-1 | `no_console_window_test.rs` 弱于内核 SSOT（白名单 7 vs 3；无 K-W1 行为断言；无反向样本） | gates 分片 |
| P1 | S2D-P1-2 | `env_toolchain_standard_test.rs` 是弱化移植，且**内核仓不存在**其对照 SSOT/门禁；多处判据可被注释/表单满足 | gates 分片 |
| P1 | S2D-P1-3 | 文档引用的 G8 门禁**不存在**，且已积累悬空引用（`checkEnvironment` 等） | docs 分片 |
| P1 | S2D-P1-4 | `KERNEL-LAUNCH-STANDARD.md` §4 的 `INSTALL_FAILED`/`SERVICE_DEFINE_FAILED` 代码中不存在 | docs 分片 |
| P1 | S2D-P1-5 | G3 行数阈值文档 150/200 vs 实际门禁 550；K 编号文档 K-1..K-6 vs 实现 K-1..K-10 | docs 分片 |
| P1 | S2D-P1-6 | `docs/DEVELOPMENT-TRACK.md:19` 硬编码操作者绝对路径（壳仓无 X-2 类门禁） | 主控已独立复验 |
| P2 | S2D-P2-1 | bootstrap `NS.` 命名空间 3 处死导出（`NS.failOnMissingNpm`/`NS.showUpdRetry`/`NS.skipAction`） | 主控已独立复验 |
| P2 | S2D-P2-2 | `release_channel.rs` 一批 `pub` 项仅模块内使用（可见性过宽，非死代码） | 主控已独立复验 |
| P2 | S2D-P2-3 | `RELEASE-STANDARD.md` §8 未登记 **R-10**；§1 H3/H4 命令与 CI 不一致 | 主控已独立复验 |
| P2 | S2D-P2-4 | `ci_gate_coverage_test.rs` 判据可被绕过（C-c 被 updater_artifacts 步骤满足；C-d 只认 5 个固定 job 名） | gates 分片 |
| P2 | 其余 ~10 条 | 文档 P2（R-10/step 名/编号重载/目录树/H3H4 等），见 `_s2-d-docs.md` | docs 分片 |

---

## 1. P0（主控已独立复验）

### S2D-P0-1 `dev_runtime_safety_test.rs` 扫描 0 文件、恒 PASS（假绿）

- 判据入口 `src-tauri/tests/dev_runtime_safety_test.rs:19-21` `root() = env!("CARGO_MANIFEST_DIR")`
  ⇒ 测试运行时为 `<仓根>/src-tauri`（bin crate 的 manifest dir）。
- `:24-49` `scanned_files()` 在 `root()` 下拼 `["scripts", ".github", "ci"]`；
  实测 `src-tauri/scripts`、`src-tauri/.github`、`src-tauri/ci` **三者均不存在**（真实目录在**仓根** `scripts/ .github/ ci/`），
  且 `:29-31` 对不存在目录是 `continue`（静默跳过）。
- ⇒ `scanned_files()` 恒为空；`r_g1_no_tmp_glob_deletion`(`:141-162`) 只断言 `hits.is_empty()`，
  **没有 `scanned > 0` 断言**（`scanned` 仅用于 eprintln `:161`）；`r_g2`(`:164-183`) 连 `scanned` 都没有。
- **后果**：这道为「`rm -rf /tmp/dsh-*` 打崩正在运行的 DSH」事故（见文件头 `:3-7`）而建的门禁，
  对**全仓脚本零覆盖**；任何脚本里出现同类删除都不会被拦。`r_g3` 只测判据函数对合成样本有效，通过不代表覆盖面。
- **最小修法**：`root()` 改为仓根（`env!("CARGO_MANIFEST_DIR").parent()` 或加 `CARGO_WORKSPACE_DIR`），
  或扫描列表改为 `../scripts`/`../.github`/`../ci`；并加 `assert!(scanned > 0)`（R-G1/R-G2 各一）。

### S2D-P0-2 设计文档状态根路径全面过时（与代码/门禁矛盾）

- 文档仍称状态在 `~/.dsh/...`：`docs/KERNEL-LAUNCH-STANDARD.md:30,68,117`、
  `docs/DESIGN-SHELL-ARCHITECTURE.md:166,167,184`、`docs/DESIGN-BOUNDARY.md:101,155,194,195`、
  `docs/DESIGN-COMPLETE.md:139-143`。
- 代码已迁到**产品状态根**：`src-tauri/src/env.rs:176-193` `state_root()` 取 `DSH_SUPERVISOR_HOME`
  或 `platform::current().state_root_default()`，再派生 `<状态根>/supervisor`、`<状态根>/shell`；
  平台默认可证：`src-tauri/src/platform/mod.rs` `fn state_root_default` + `macos.rs`/`windows.rs` 覆写。
- 且有 K-8 门禁**明确禁止再拼 `.dsh`**：`src-tauri/tests/kernel_launch_standard_test.rs:167-202`
  （`state_root_is_independent` 检查 `supervisor_dir` 函数体不得含 `.dsh`）。
- **后果**：运维按这些「设计 SSOT」会去 `~/.dsh/...` 找 `runtime.json/core.json/registry.json/identity.json`，
  实际不存在 → 排障与迁移会走错目录。属**文档与实现的 P0 级矛盾**（不是代码 bug）。
- **最小修法**：把上述文档的 `~/.dsh/{supervisor,shell}` 统一改为「产品状态根 `<state_root>/{supervisor,shell}`
  （Linux `$XDG_STATE_HOME/dsh-supervisor` 或 `~/.local/state/dsh-supervisor`；macOS/Windows 见 platform 覆写）」，
  或删除具体路径只指向 `env.rs::state_root`。

---

## 2. P1

### S2D-P1-6 操作者绝对路径落入提交文档（主控已独立复验）

- `docs/DEVELOPMENT-TRACK.md:19` 表格单元含 `/home/<操作者>/develop/dsh-supervisor-launcher`（本机绝对路径）。
- 内核仓有 `test/no-dev-path-test.js`（X-2）拦这类；壳仓**无同名门禁**，故未被拦。
- **最小修法**：改为 `本仓（仓库相对路径）`；并考虑在壳仓加等价 X-2 静态门禁（扫描 `*.md/*.rs` 的操作者 home 模式）。

### S2D-P1-1 `no_console_window_test.rs` 弱于内核 SSOT（gates 分片）

- 白名单 7 项，比内核 `NO-CONSOLE-WINDOW-STANDARD.md:81` 的 S-W1（3 项）多放行 4 个平台文件；
- 无 K-W1 等价断言（无人验证 `bounded::prepare` 真加 `CREATE_NO_WINDOW`，见 `src-tauri/src/bounded.rs:49-53`）；
- 无 K-W3 反向合成样本；非白名单只做文件级 `contains("prepare(")`（非调用点级）。
- **最小修法**：补 K-W1 对 `bounded.rs` 的结构断言 + K-W3 合成反向样本；白名单收窄或逐项注明理由。

### S2D-P1-2 `env_toolchain_standard_test.rs` 弱化移植 + 对照 SSOT 缺失（gates 分片）

- **内核仓不存在** `ENV-TOOLCHAIN-INSTALL-STANDARD.md` 及同名门禁（内核全仓无 `npmOk/probe_npm/20-env` 引用）
  ⇒ 任务书「与内核同名门禁等价」的**前提不成立**（它是壳仓自建）。
- G-2 可被 `src-tauri/src/main.rs:186` **注释**里的 `probe_npm` 字样满足（未剥注释、未查 `checkEnvironment`/`reinstall_for_npm`）；
- G-4/G-3 只扫 `src/bootstrap` 而非「全仓」；G-5 放行的恰是 SSOT §1 根因形态 `start_node_install`
  （`src-tauri/bootstrap/js/20-env.js:110`）；G-6 只查 `10-ui.js` 导出。
- **最小修法**：G-2 去注释后再判且查真实函数；G-3/G-4 扫描面扩到全仓；G-5 明确区分「npm 修复」与「node 安装」文案。

### S2D-P1-3 文档引用的 G8 门禁不存在 + 悬空引用（docs 分片）

- `G8` 仅在文档出现（`DESIGN-SHELL-ARCHITECTURE.md:292`、`DESIGN-COMPLETE.md:578`），**无实现**；
- 已积累悬空引用：`ENV-TOOLCHAIN-INSTALL-STANDARD.md:143` 的 `checkEnvironment` 全仓 0 命中
  （真实门禁为 `probe_npm/npmOk`，`env_toolchain_standard_test.rs:85-94`）；
  `release_channel.rs:178` / `env.rs:169` 内核路径缺 `/service/`；`DESKTOP-ACCEPTANCE.md:18` 的 `dist/*.deb` 不存在
  （实际在 `src-tauri/target/release/bundle/deb`）。
- **最小修法**：要么实现 G8（文档路径引用存在性门禁），要么把 G8 从文档删除并修正上述引用。

### S2D-P1-4 `KERNEL-LAUNCH-STANDARD.md` §4 错误码不存在（docs 分片）

- `INSTALL_FAILED`/`SERVICE_DEFINE_FAILED` 只在 `KERNEL-LAUNCH-STANDARD.md:96,98` 出现，代码无此 code；
  P4 定义失败在 `src-tauri/src/domain/guardctl.rs:158-161` 只是日志 + spawn 兜底，非结构化失败码。
- **最小修法**：文档改为如实描述（日志+兜底），或代码补结构化错误码。

### S2D-P1-5 阈值/编号漂移（docs 分片）

- G3 行数：`DESIGN-COMPLETE.md:573` 称 main.rs ≤150、`DESIGN-SHELL-ARCHITECTURE.md:287` 称 ≤200，
  实际门禁 ≤550（`bootstrap_flow.rs:1656-1667`，当前 548 行）；
- `KERNEL-LAUNCH-STANDARD.md` §6 只列 K-1..K-6，实现已有 K-1..K-10；前端模块实际 9 个、多处写 8 个。
- **最小修法**：文档阈值/编号与门禁常量对齐（或反向：把门禁常量改为文档值并修代码，需评估）。

---

## 3. P2

### S2D-P2-1 bootstrap `NS.` 命名空间死导出（主控已独立复验）

| # | 位置 | 证据 | 最小修法 |
|---|---|---|---|
| a | `src-tauri/bootstrap/js/20-env.js:171` `NS.failOnMissingNpm` | `NS.failOnMissingNpm` 全 bootstrap 出现 1 次（仅赋值）；函数内以裸名调用（`:130/:144/:145`），跨模块无人读 | 删该导出行（函数保留） |
| b | `src-tauri/bootstrap/js/40-shell-update.js:56` `NS.showUpdRetry` | 与 `:55 NS.showUpdChoice = showUpdRetry;` 同函数双名导出；`NS.showUpdRetry` 无消费者（`NS.showUpdChoice` 被 `80-init.js:8` 消费） | 删 `:56` 重复导出 |
| c | `src-tauri/bootstrap/js/00-runtime.js:15` `NS.skipAction = null;` | 全仓零引用（bootstrap JS + html + Rust） | 删该行或补消费者 |

### S2D-P2-2 `release_channel.rs` 可见性过宽（主控已独立复验，非死代码）

- 模块外零引用、仅模块内使用的 `pub` 项：`AllowlistHit`(:196)、`allowlist_match`(:293)、
  `canary_in_config`(:157)/`canary_in_env`(:162)、`install_id`(:214)、`hostnames`(:245)、
  `canary_machine_with`(:357)、`CANARY_ALLOWLIST_PKG`(:43)、`MATCH_LOCAL/INSTALL_ID/HOSTNAME`(:185/:187/:189)。
- 均为**模块内有真实调用**（≥2 次出现）；建议收窄为 `pub(crate)`/私有，使对外契约清晰。

### S2D-P2-3 `RELEASE-STANDARD.md` 与实现小漂移（主控已独立复验）

- §8 表列 R-1..R-9，但 `src-tauri/tests/release_spec_consistency_test.rs:263` 有 **R-10**
  （assembler 覆盖 CI 矩阵 artifact）未登记；
- §1 H3 称 `cargo run -- --node-plan`（`:54`），实际 `.github/workflows/build.yml:151-152` 用 `cargo build` + 直接跑二进制；
  §1 H4 称 `cargo tauri build`（`:55`），实际 `:166` 用 `npx @tauri-apps/cli@2 build`。
- 功能等价；`release_spec_consistency_test` 不校验命令文本（R-1 只校验文件存在），故不会被门禁发现。

### S2D-P2-4 `ci_gate_coverage_test.rs` 判据可绕过（gates 分片；主控已确认它确为「真读」）

- **确为真读**：`:37-40 fs::read_to_string(.github/workflows/build.yml)`，缺文件即 panic；先剥 YAML 注释(`:42-45`)；
  C-a..C-d 正向 + C-e 反向非空转自证 ⇒ **不存在「只断言文件存在」的空转**。
- 但：C-c 的 `build.contains("cargo test")` 会被 `build.yml:200` 的 updater_artifacts 步骤满足（不保证「门禁步骤」本身跑了 cargo test）；
  `has_hardcoded_test_targets` 会漏掉「含 updater_artifacts 的硬编码行」；C-d 只认 5 个固定 job 名。
- **最小修法**：C-c 改为对 `job_block("build")` 的**门禁步骤**断言（如同时含 `TARGETS` 与 `cargo test --bins`）；
  C-d 改为「任何 job 只 cargo check 不 build」的全 job 扫描。

---

## 4. 死代码（主控，负向结论有证据）

- **无 `#[allow(dead_code)]`/`#[allow(unused*)]`**：`grep -rn 'allow(dead_code)|allow(unused' src-tauri/src` 零命中。
- **无死函数**：全 `src-tauri/src` 的 `fn NAME` 扫描中，单次出现者**全部是 `#[test]` 函数**（harness 调用，grep 不可见）⇒ 非死函数。bootstrap 全部 `function NAME` 均有 ≥2 次出现。
- **无只写不读字段**：`release_channel::Selected.via` 被 `core.rs:239/678/757` 读；`AllowlistHit.note` 在 `release_channel.rs:401` 读。
- **无悬空符号**：本提交「删除」的 `latest_version`/`build_plan`/`run_install` 均改址后仍在（`core.rs:268/676`、`main.rs:156`）；
  `env_status`/`shell_update_progress` 仅存于测试禁用串与注释。
- **无死广播**：Rust 发 `install_progress`(`main.rs:142,commands/mod.rs:728`)/`install_done`(`commands/mod.rs:188`)/
  `install_error`(`commands/mod.rs:197`)/`guard_progress`(`domain/guardctl.rs:124`)，bootstrap `80-init.js:58/69/73/77` 逐一对上。
- **进度条移除彻底**：删 `#prog/#progBar` 与 `showProgress/hideProgress` 后全 bootstrap 零残留引用。

---

## 5. 门禁覆盖总览

- CI **确有** `cargo test` 且**自动枚举** `tests/*.rs`：`.github/workflows/build.yml:141` 枚举、`:143 cargo test --bins`、
  `:200` 单独跑 `updater_artifacts` ⇒ **20 个测试文件（含 3 个新增）全部会被 CI 执行**。问题在**判据/扫描面**（P0-1/P1-1/P1-2），非「未执行」。
- `ci_gate_coverage_test.rs`：真读 workflow、非空转（见 P2-4）。
- `release_spec_consistency_test.rs`：R-1..R-10 真读规范/矩阵/脚本，非空转（R-8 反向自证）。

---

## 6. `.github/workflows/build.yml` 放行条件 vs `RELEASE-STANDARD.md` §4（主控）

| §4 声明 | workflow 证据 | 一致 |
|---|---|---|
| 触发 push(main)/push(v* tag)/PR(main)/dispatch | `:7-16` | ✅ |
| `version` 总是 + 三处互锁校验 | `:24-41`（`verify-shell-versions.js` `:41`） | ✅ |
| `build` 总是 + 4 平台矩阵 | `:43-78` | ✅ |
| build 步骤序 + tag 时挂 Release | `:120-232`（tag 守卫 `:222`） | ✅ |
| `publish` 仅 tag `v*` | `:236-238` | ✅ |
| dist-tag：BETA→beta；RC→latest + 补 rc | `:316-329` | ✅ 与内核 `RELEASE-CHANNEL-CONTRACT.md` §2/§4 一致 |
| 分支保护 = version + 4×build，strict+enforce_admins | **服务器端配置，仓内不可验证**（§4 `:106-109` 已自述需人工同步） | ⚠ 不可证 |

**唯一可改进**：分支保护是「只在文档里、无机器校验」的放行条件；建议加只读 `gh api` 核对或显式标为人工项。

---

## 7. 方法与局限

- 全程只读；**未编译**（禁 `cargo`）⇒ 无编译器 unused-import/field 警告清单，死代码结论基于静态 grep + 赋值/读取计数，已给判据与反向自查。
- 两个分片的逐条证据在其各自文件 `_s2-d-docs.md` / `_s2-d-gates.md`；本报告已把 P0/P1 逐条落到「仓库相对路径:行号 + 最小修法」。
