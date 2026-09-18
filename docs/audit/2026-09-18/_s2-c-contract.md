# S2-C 契约一致性审计（壳仓工作树 → release_channel + 跨仓契约 + 签名）

> 审计对象：**壳仓本地克隆**（remote=`wasi7mglns/dsh-supervisor-launcher`）的**待发布候选项**。
> 只读；未跑任何测试/构建（禁 cargo test/build、禁 npm test、禁 tauri build）。证据均为**仓库相对路径:行号**。
> 对照 SSOT：内核仓 `RELEASE-CHANNEL-CONTRACT.md`（§3/§5/§6）、`EXECUTION-CONTRACT.md`；
> 壳仓 `docs/KERNEL-LAUNCH-STANDARD.md`、`docs/DESIGN-BOUNDARY.md`、`docs/UPDATER-SIGNING-KEY.md`。

## 0. 审计对象状态（重要：审计中 HEAD 前移）

- 审计开始时：分支 `fix/toolchain-npm-1.1.7` @ `00f872c` + 27 修改 + 6 未跟踪（含新模块 `src-tauri/src/release_channel.rs`）。
- 审计过程中该工作树被**提交**：现 HEAD = `0930884`（分支 `release/shell-1.1.8`，
  提交信息「feat(shell): 内核选版契约落地（release_channel）+ npm 复探与工具链加固」），工作树干净。
- 本报告审计的**内容即该 `0930884` 快照**（`release_channel.rs` 已入库：`git ls-files` 可见）。
- 版本仍 `1.1.7`（`src-tauri/Cargo.toml`、`src-tauri/tauri.conf.json`），`CHANGELOG.md` 顶部仍是 `[未发布]` —— **尚未 bump/发布**。

## 1. 结论摘要

| 面 | 判定 |
|---|---|
| §1 `release_channel.rs` 实现 §3 冻结算法 | **符合**（五步逐条一致，RC-1..RC-5 满足） |
| §1 §5 灰度（installId/schema/短路顺序） | **符合**（仅认 `schema:1` + `entries[].installId` + `hostnames[]`；本地开关短路） |
| §2 内核启动契约（KERNEL-LAUNCH-STANDARD） | **符合**（H4 对齐门经通道选版；core.json schema 1） |
| §2 single-writer 内核更新 | **符合**（壳是唯一安装方；不调内核 `/self-update/apply`） |
| §3 跨仓契约文件字段/方向 | registry.json / identity.json / update-journal.json **方向符合**；发现 **1 处内核侧双写者** |
| §4 自更新签名/通道 | **符合**（pubkey 与手册一致；CI 签名 + 同源验签） |
| 门禁覆盖 | **发现 2 处缺口**（内核交叉门禁读错文件；壳侧无显式契约门禁与 SSOT 引用） |

**契约偏差/缺口共 3 处需跟进（D-1/D-2/D-3）+ 4 处信息项（D-4..D-7）。**

## 2. `release_channel.rs` 对 §3 的逐条对照

| 契约 §3 步骤 | 契约要求 | 壳实现（`src-tauri/src/release_channel.rs`） | 判定 |
|---|---|---|---|
| ① | `dist-tags.rollback` 合法 → 返回 | `:133-135` `if let Some(v)=tag(meta,CH_ROLLBACK) return via=rollback` | ✅ |
| ② | 本机在灰度名单 且 `dist-tags.canary` 合法 → 返回 | `:137-141` `if canary_machine { if let Some(v)=tag(meta,CH_CANARY) ... }` | ✅ |
| ③ | `dist-tags.latest` 合法 → 返回（RC-1 优先信 tag） | `:143-145` | ✅ |
| ④ | 否则取 `versions` 最高合法版本 | `:147-149` → `highest_version` `:82-98`（用 `core::semver_cmp`） | ✅ |
| ⑤ | 皆无 → 明确失败（不猜） | `:151` `Err(empty_error(...))`，错误带现场（`:102-118`） | ✅ |
| RC-2 | rollback 高于一切（含灰度） | 步骤①在②之前，无例外 | ✅ |
| RC-3 | 解除回退不依赖版本比较 | 实现只认 tag 存在性，无版本比较 | ✅ |
| RC-4 | 灰度定向（名单判定） | 非名单机 `canary=false`，不看 canary tag | ✅ |
| RC-5 | 失败如实返回，不静默降级 | `:151` + `empty_error` 带 dist-tags/versions 现场 | ✅ |
| 合法性 | 非法 tag 当「不存在」继续 | `:72-79` `tag()` 经 `core::is_valid_version`，非法返回 None | ✅ |

**§5 灰度对照**：
- 5.1 主依据 installId / 主机名兜底：`:293-338` `allowlist_match` 先 `entries[].installId`（大小写不敏感）后 `hostnames[]`。✅
- 5.2 installId 由**内核**生成、壳只读：`:214-222` `install_id()`，无任何生成分支；读 `<supervisorDir>/install-id` 首行（`:228-236`）。✅
  - 环境变量覆盖 `DSH_CANARY_ID`（`:55`）与内核 `ENV_OVERRIDE` **同口径**（内核 `内核仓:src/platform/service/install-id.js:17`）。✅
- 5.3 名单格式：`:301-303` `schema != 1 → None`；`note` 只取日志不参与匹配（`:311-316`）；旧 7 形状显式拒绝（测试 `:670-689`）。✅
- 5.4 本地开关：`:157-169` `config.canary` 或 `DSH_CANARY=1`；`canary_in_config` 经 `crate::env::config_flag`。✅
- 5.5 判定顺序：`:357-378` `canary_machine_with`：local→直接命中（不查包）；未 opt-in→None（零请求）；opt-in→读包。✅
- 5.6/§4 运维：实现只读，不写 tag；无自动设置 rollback/canary 的代码。✅

## 3. 内核启动契约 + single-writer

- **H4 对齐门**：`src-tauri/src/domain/guardctl.rs:120 ensure_guard` → `resolve_aligned_with`（`:49`）
  经 `crate::core::latest_version`（`src-tauri/src/core.rs:268` → `latest_pick` `:220`）
  → 每源 `crate::release_channel::select`（`:238`）。未对齐返回 `KERNEL_NOT_ALIGNED`（`guardctl.rs:144`）。✅（不是旧「磁盘最高」）
- **H2/H3 契约单一来源**：runtime.json（`runtime_contract`）、core.json（`core_contract.rs`）均由壳写；core.json schema 1
  字段 `schema/writtenBy/bin/prefix/version/source/installedAt`（`core_contract.rs:22,46-54`）与
  `docs/KERNEL-LAUNCH-STANDARD.md:70-79` 一致。✅
- **single-writer**：`src-tauri/src/commands/mod.rs:279-349 core_apply_inner`（唯一安装实现）+
  `:359-392 kernel_update_apply`（安装→停守卫→等端口→重拉）。壳**自装内核**，不调用内核 `/self-update/apply`
  （内核该端点 410）；全仓 `src-tauri/src` 无 `/self-update` 调用点。✅
- **跨源仲裁**（`core.rs:285-296 better_candidate`）：同通道内跨镜像取较高版本、再比延迟。属壳的**多镜像扩展**，
  契约 §3 只定义单份 registry 元数据（见 D-6）。

## 4. 跨仓契约文件（字段/方向）

| 文件 | 契约（壳 DESIGN-BOUNDARY §4.1/§4.3） | 壳实现 | 判定 |
|---|---|---|---|
| `~/.dsh/supervisor/registry.json` | **壳写、内核读**；schema 2：`mode/manualOrigin/catalog/selected/probe` | `src-tauri/src/mirror.rs:203-262` `export_to_kernel_with` 写 schema 2（`CONTRACT_SCHEMA=2` `:187`），且 manual 时不覆盖（`:212-218`） | ✅ 方向+字段 |
| 内核读侧兼容 | v1/v2 均接受，schema 更新则拒 | `内核仓:src/platform/contract/registry.js:6,61-77`（`SCHEMA_NEWER`、v2 优先 catalog） | ✅ |
| `~/.dsh/shell/identity.json` | **壳写、内核读**；`version/phase/pid/exe/lastSeenAt` | `src-tauri/src/update.rs:118-159` 写 `version/platform/arch/installKind/selfUpdateCapable/phase/pid/startedAt/lastSeenAt/exe` | ✅ 方向+字段 |
| 内核读侧 | 只读 version/phase/exe/lastSeenAt | `内核仓:src/domains/shell/journal.js:39` | ✅ |
| `~/.dsh/shell/update-journal.json` | **内核内部**（to/confirmed；不含回退/拉黑） | 壳全仓无写入/读取；内核 `journal.js:42-50` | ✅ |
| `shell-release/version-vectors.json` ↔ 内核 `shared/version-vectors.json` | 双向共享测试向量 | `diff` **完全一致** | ✅ |

## 5. 自更新签名 / 更新通道

- **公钥一致**：`src-tauri/tauri.conf.json:52` 的 `plugins.updater.pubkey`（152 字符）与
  `docs/UPDATER-SIGNING-KEY.md:35` **逐字相同**（已用 JSON 解析比对）。✅
- **更新产物**：`tauri.conf.json:48 createUpdaterArtifacts: true`；endpoints 为 unpkg/jsdelivr 的
  `@dsh-sup/shell-release@latest/shell-manifest.json`（`:55-58`）。✅
- **CI 签名**：`.github/workflows/build.yml:156-166` 注入 `TAURI_SIGNING_PRIVATE_KEY(_PASSWORD)`；
  `:180-186` 组装器「缺 `.sig` 直接失败」；`:193-200` 用 Tauri 同源 `updater_artifacts` 测试验签。✅
- **插件强制验签**：`src-tauri/src/main.rs:49-50,402-404`（tauri-plugin-updater，minisign 不可关闭）。✅

## 6. 契约偏差 / 缺口清单

| ID | 严重度 | 偏差 | 证据 | 建议 |
|---|---|---|---|---|
| **D-1** | 中 | **内核 RC-G1/G2 交叉门禁读错文件**：算法已迁至 `release_channel.rs`，门禁仍只读 `core.rs`。`hasRollback` 被 `core.rs:687 Some("rollback")` 平凡命中；`isUnionMax` 恒 false ⇒ 门禁绿但**不再校验算法存在与优先级** | `内核仓:test/release-channel-gate-test.js:224,231,234`；`src-tauri/src/core.rs:238` | RC-G1/G2 改读 `src-tauri/src/release_channel.rs`（或同时读两文件），并按符号断言 `select` 五步顺序 |
| **D-2** | 中 | **壳侧无显式发布通道契约门禁/SSOT 引用**：`src-tauri/tests/` 与 `ci/` 对 `RELEASE-CHANNEL`/`dist-tags`/`rollback` 零引用；覆盖仅 `release_channel.rs` 内联单测（由 `cargo test --bins` 跑到，`.github/workflows/build.yml:141-143`）。`ci_gate_coverage_test.rs` 只保证 tests 枚举，无「契约→门禁」映射；`release_spec_consistency_test.rs` 只钉 `docs/RELEASE-STANDARD.md` | `内核仓:RELEASE-CHANNEL-CONTRACT.md:207-215`（RC-G1/G2）；`src-tauri/tests/ci_gate_coverage_test.rs:23-28` | 新增壳侧命名门禁（如 `tests/release_channel_contract_test.rs`）并在 `docs/RELEASE-STANDARD.md` 登记 SSOT 引用 |
| **D-3** | 低-中 | **内核侧双写 `identity.json`（违反 C1「只有一个写入方」）**：壳写全量运行时字段，内核 `health()` 又兜底写 `phase/version/lastSeenAt` | 壳 `src-tauri/src/update.rs:118-159`；内核 `内核仓:src/domains/shell/journal.js:82-89`；契约 `docs/DESIGN-BOUNDARY.md:202` | 或内核只读不写（壳上报即已写），或把「内核兜底写」写入契约并声明优先级 |
| D-4 | 低 | `docs/SHELL-UPDATE-CHANNEL-VERIFICATION.md` 仍含未决项（deb `.sig` 待实测；N1/N2/N3 待用户决策），日期 2026-09-11 | 同文件 `:120-122,199-205` | 发布前更新为已定案，或标注为历史记录 |
| D-5 | 低 | CI 未配置签名密钥时仅 `::warning::` 后继续构建；未签名发布实际由后续组装器失败拦住 | `.github/workflows/build.yml:159-166` vs `:180-186` | tag 构建下把「缺密钥」升为 `::error::` 早失败 |
| D-6 | 信息 | 跨镜像「同通道取较高版本」是壳扩展，契约 §3 未定义多源合并 | `src-tauri/src/core.rs:285-296` | 在内核 `RELEASE-CHANNEL-CONTRACT` 补「多镜像同通道仲裁」条款，或注明为实现细节 |
| D-7 | 信息 | 候选提交 `0930884` 版本仍 `1.1.7`，CHANGELOG 顶部 `[未发布]`；分支名 `release/shell-1.1.8` | `src-tauri/Cargo.toml`；`src-tauri/tauri.conf.json`；`CHANGELOG.md:5` | 发布前走 `bump-shell.sh`（三处互锁，`release_spec_consistency_test.rs R-5`） |

## 7. 未核验 / 边界（诚实声明）

- 未在本机执行任何 `cargo test/build`、`npm test` —— `release_channel.rs` 的 20+ 内联单测**未实际运行**，本报告只做**源码级**对照。
- `release_channel.rs` 的内联单测覆盖：①回退压倒一切、②canary 仅名单机、③latest 压过 versions、④versions 兜底、⑤明确失败、
  §5.3 唯一格式/旧形状拒绝/note 不参与匹配/installId 大小写不敏感/schema≠1 作废、installId 文件读取容错 —— 与契约 §3/§5 **逐条对应**（`:441-760`），
  但「能编译且测试真跑绿」需 CI 裁决。
- `docs/DESKTOP-ACCEPTANCE.md` 未在本片范围（属 S2 其他片）。
- 版本向量一致性已 `diff`；`glibc`/打包/清单格式属其他片。
