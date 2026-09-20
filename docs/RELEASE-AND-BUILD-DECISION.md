# 发布与构建决策（壳仓视角）

> **流程以 `RELEASE-STANDARD.md` 为准。**
> 本文件只讲**决策背景与理由**，不再重复流程细节。


> 本文是**壳仓**侧的发布规范。**跨仓时序与契约**的权威定义在内核仓
> `release/README.md` §0（两仓构建决策）与 §0.2（跨仓发布时序）—— 本文与之保持一致，冲突时以内核仓为准。
> 最近更新：2026-09-21。
>
> **仓库地址现状**：两仓现均在账号 `lobbowen` 下（内核 `lobbowen/dsh-supervisor-core`、
> 壳 `lobbowen/dsh-supervisor-launcher`）。旧账号仓 `advgyxqamf/dsh-supervisor-core` 与
> `wasi7mglns/dsh-supervisor-launcher` 仍可公开访问但已停更（最后 push 2026-09-18），
> 不要向它们推送或以其内容为准。**服务器端配置不随仓迁移**：迁仓后两仓主干一度无保护，
> 2026-09-21 才在 `lobbowen` 两仓重新写入（现值见本文「把 CI 设为合并门禁」附录）。

## 1. 为什么是两个仓（**必须分开，不是历史包袱**）

| | 内核仓 `lobbowen/dsh-supervisor-core`（公开） | 壳仓 `lobbowen/dsh-supervisor-launcher`（公开，本仓） |
|---|---|---|
| 职责 | 产品逻辑 + 守护：API/路由/relay/实例/插件/端口/更新编排 | **仅**桌面体验：引导页、托盘、安装程序、原生能力 |
| 技术栈 | JS（CommonJS），运行时依赖 **0**、原生扩展 **0** | Rust（Tauri 2）+ 纯 HTML/CSS/JS 引导页（无构建步骤）|
| 产物 | npm 平台子包 `@dsh-sup/dsh-core-*`（4 平台） | 安装程序 `deb/rpm`、`dmg/app`、`msi/nsis` + `@dsh-sup/shell-*` |
| 分发 | npm registry | GitHub Release + npm（自更新产物） |
| 节奏 | 高频、可单独 hotfix | 低频（安装程序） |
| 构建负担 | 轻（纯 JS，一次构建派生四平台） | 重（Rust + 各平台系统库） |

**四个具体好处**：① 更新节奏解耦（内核热修不必重发安装程序）；
② 用户更新成本（内核走 npm 增量/可静默，壳走安装程序/需重启）；
③ 构建负担隔离（Rust/Tauri 依赖不污染内核的「零依赖纯 JS」）；
④ **为后期决策留空间**（两仓可独立决定开源策略、节奏、商业形态）。

**代价与对策**：不共享代码 → 契约只能靠**文件**传递（`registry.json` / `identity.json` /
`update-journal.json`）。字段**新增**须向后兼容；
字段**移除或改语义**须**内核先行**，并允许两侧版本错配运行一个发布周期。

## 2. 壳的构建与发布 SOP

### 2.1 版本（三处互锁，单源校验）

```bash
bash scripts/bump-shell.sh <ver>        # 同号写入 Cargo.toml / tauri.conf.json / Cargo.lock
node scripts/verify-shell-versions.js   # 自洽校验（不一致即失败）
```

### 2.2 本机自查上限（**纯静态**，不构成验收依据）

```bash
bash -n scripts/bump-shell.sh           # shell 语法
node --check scripts/verify-shell-versions.js
cargo fmt --check                       # 格式（不编译、不产 target/）
```

> 本机的上限到此为止。**`cargo test` / `cargo check --all-targets` / `cargo build` 一律不在本机跑**
> （`RELEASE-STANDARD.md` §0 硬标准：构建与测试都由 CI 裁决）。
> 理由不是「怕慢」，而是**结论不可移植**：单平台、单 glibc、单 HOME 下跑绿证明不了另三个平台，
> 跑红也可能是本机环境（缺系统依赖 / 别的会话并行改动）造成的假信号，
> 而 `src-tauri/target/` 会留下 GB 级中间产物污染工作区。
> 门禁是否真的执行由 CI 的**自动枚举**保证（新增 `tests/*.rs` 自动入 CI），不靠本机复述。

### 2.3 发布

```bash
git add -A && git commit -m "release: v<ver>" && git tag v<ver>
git push origin main && git push origin v<ver>
```

→ tag 触发 `.github/workflows/build.yml`：**四平台完整构建**
（`ubuntu-22.04` / `windows-latest` / `macos-latest` / `macos-15-intel`）
→ Tauri bundle + 壳 npm 包 + `shell-manifest.json` + 验签
→ `publish` job **仅 tag 触发**（挂 GitHub Release + `npm publish`）。

## 3. CI 触发策略（2026-09-13 定稿）

| 事件 | 行为 |
|---|---|
| **push `main`** | 跑**完整构建矩阵**（四平台 bundle + 全部门禁测试）。不打包发布、不发布。 |
| **push tag `v*`** | 同上 + `publish`（Release + npm）。 |
| `workflow_dispatch` | 同 push main（可手动触发验证）。 |

> **为什么 push 也跑完整构建**：只做编译校验（`cargo check`）发现不了
> **打包 / 签名 / 产物装配**阶段的问题，而本仓恰在那些阶段踩过坑；轻量检查会给出
> 「绿了」的假象、反而掩盖问题。公开仓 Actions 免费，故一律跑完整构建。
> 实证：该策略第一次运行（run 34737146315）就抓到 `macos.rs` 的语法错误
> （嵌套未转义双引号 → macOS 无法出包），该错误在 Linux 上因 `#[cfg(target_os)]`
> 完全不可见。

> **门禁必须自动枚举**：CI 的门禁步骤用 `ls tests/*.rs` 自动列出全部 test target
> （仅排除需打包产物的 `updater_artifacts`）。曾经的硬编码 `--test` 名单导致
> **新增门禁被静默排除在 CI 之外**（实测漏 4 个）—— `tests/ci_gate_coverage_test.rs`
> 现在锁死「不得硬编码 + 必须有 mac/win 构建」。

## 4. 跨仓发布时序（规范）

```text
1) 内核先发（契约变更方）
2) 壳后发（消费方），至少在「内核那一版已发布」之后
3) 交叉验证：
     · 壳=最新 / 内核=上一版  → 验证降级路径
     · 壳=上一版 / 内核=最新  → 验证向后兼容
```

## 5. 平台矩阵要点

| 平台 | runner | 产物 |
|---|---|---|
| linux-x64 | `ubuntu-22.04`（**基座固定**，glibc 2.35）| `deb,rpm` |
| darwin-arm64 | `macos-latest` | `app,dmg` |
| darwin-x64 | `macos-15-intel`（**macos-14 已弃用**）| `app,dmg` |
| win-x64 | `windows-latest` | `nsis,msi` |

> Linux **必须**用 ubuntu-22.04 基座：在 24.04（glibc 2.39）构建的产物**无法**在
> 22.04 / Debian 12 运行（Rust std 对 `pidfd_spawnp`/`pidfd_getpid` 的弱引用
> 在 2.39 主机会被解析成硬性 `verneed`）。

---

## 附：把 CI 设为**合并门禁**（required status checks）

### 现状（条目按 2026-09-21 读回校准）

| 项 | 状态 |
|---|---|
| `pull_request` 触发器 | **已加**：此前 PR **完全不跑 CI**，若直接设 required 会让 PR 永远等不到状态 |
| 分支保护 | **已设**（2026-09-21 `PUT` 后 `GET` 读回，见下表现值）。此前的记录是 `404 Branch not protected`，原因是迁仓不迁服务端配置，而不是「令牌没有 admin 权限」—— 现用 PAT 属 `lobbowen` 本人、对两仓都带 `Administration: Read and write`。**换账号 / 迁仓后必须重新写入**，任何文档里的保护描述都不等于服务端事实，以 `GET .../branches/main/protection` 为准 |

### 现值（version + 4 平台，共 5 个语境；PUT 即按此恢复）

`build` 是**无条件矩阵**（4 平台每次必跑），故可作为 required；`publish` 只在 tag 时跑、
在 PR 事件上 `skipped`，**不可**设 —— required 里只要出现一个永不落地的语境，所有 PR 就永久阻塞。

```json
{
  "required_status_checks": {
    "strict": true,
    "contexts": [
      "version",
      "build (ubuntu-22.04, linux-x64, deb,rpm, 2.35)",
      "build (windows-latest, win-x64, nsis,msi)",
      "build (macos-latest, darwin-arm64, app,dmg)",
      "build (macos-15-intel, darwin-x64, app,dmg)"
    ]
  },
  "required_pull_request_reviews": {
    "required_approving_review_count": 0,
    "dismiss_stale_reviews": false,
    "require_code_owner_reviews": false
  },
  "enforce_admins": true,
  "required_conversation_resolution": true,
  "restrictions": null,
  "allow_force_pushes": false,
  "allow_deletions": false,
  "required_linear_history": false
}
```

`required_pull_request_reviews` 必须**存在但审批数为 0**：这一条只是关掉「不经 PR 的直推」，
不要求第二个人点头 —— 单人仓里审批数 ≥1 会让「CI 绿后合入」变成推不动的死锁。
（早前本文写的是 `required_pull_request_reviews: null`，即允许直推主干，与「CI 是唯一放行裁决者」矛盾。）

重设步骤（保护被误删或误改时，按上表原样恢复；2026-09-21 首次写入即用此调用）。
**不要把令牌拼进命令行**（会落进 shell 历史与进程参数），也不要用 `$HOME` 直接推路径
（沙箱会重定向 `$HOME`）—— 路径由内核仓的凭据入口解析，值在进程内读：

```bash
# 在壳仓根执行；CRED_FILE 只放路径不放值
export CRED_FILE="$(bash <内核仓>/release/scripts/cred.sh path github-pat)"
node -e '
const fs=require("fs");
const tok=fs.readFileSync(process.env.CRED_FILE,"utf8").trim();
fetch("https://api.github.com/repos/lobbowen/dsh-supervisor-launcher/branches/main/protection",{
  method:"PUT",
  headers:{authorization:"Bearer "+tok,accept:"application/vnd.github+json","content-type":"application/json"},
  body:fs.readFileSync("protection.json")}).then(async(r)=>console.log(r.status, await r.text()));
'
```

> 走不开命令行时，等价操作是 GitHub UI 的 Settings → Branches → branch protection。

> **PUT 200 之后必须 GET 读回**，本文「现值」即读回值。两处坑：`PUT` 的 body 与 `GET` 的返回结构
> 不同形（`GET` 把各字段包成 `{"enabled": …}`，`required_status_checks.contexts` 是**字符串数组**而非对象数组，
> 按 `c.context` 取会全为 `null`）；`PUT` 是**整体覆盖**，漏写字段等于把它改成默认值。

> **context 字符串必须与 job 名逐字一致**（含括号内矩阵参数）。
> 取法：跑一次 CI 后查 `GET /repos/{owner}/{repo}/actions/runs/{run_id}/jobs` 的 `jobs[].name`。

### 一个必须知道的陷阱

**required 的 context 一旦不再产生，PR 会被永久阻塞**。
本仓 4 条 `build (...)` 的语境**内嵌矩阵参数** —— 若将来增删平台或改 arch 组合，
旧语境会变成「预期但永不出现」-> **所有 PR 合不进去**。
故：**改平台矩阵时必须同步更新分支保护的 contexts**。

### 与内核仓的差异

两仓的 `build` **都是无条件矩阵**（内核仓 2026-09-14 起移除了 job 级 `if:`，
`need_build` 只门控 `release` job 与 `--publish` 步骤），所以两侧都已把 `build` 设为 required。
差别只在 required 集合：内核 `master` = `precheck` + `test` + 4 条 `build (...)`，
壳 `main` = `version` + 4 条 `build (...)`（见内核仓 `DEVELOPMENT-TRACK.md` §7）——
这正是「跨平台构建能力不被业务开发破坏」的服务器端保障。其余字段（strict / enforce_admins /
必须走 PR 且审批数 0 / conversation resolution / 禁 force push 与删除）两仓逐字相同，
2026-09-21 同批写入并读回。
