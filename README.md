# dsh-supervisor-launcher

**DSH supervisor — DeepSeek Harness 的桌面守卫**（桌面壳 + 环境引导器）

给你的 DeepSeek Harness 配一个自带托盘图标的桌面管家：一键装官方 Node LTS，自动拉起自愈守卫，
浏览器/面板随时看状态，局域网设备安全访问，版本更新有日志可查。

> 壳（本仓库）**开源（MIT）**；守卫内核为**闭源构建物**（npm 分发，见下）。

## 功能

- **环境引导**：探测系统 Node.js → 缺失/过旧时内嵌引导页一键安装官方最新 LTS（下载 + SHA256 校验 + **用户级零权限解包到 `<状态根>/node`，不弹系统授权**）
- **守卫拉起**：自动定位已安装内核并启动守护进程，控制面板就绪后直达（端口由内核配置 `apiPort` 决定，动态分配）
- **生命周期守卫**（内核提供）：进程保活、故障自动重启、崩溃退避、期望状态语义（启动/停止可控）
- **局域网安全访问**：`0.0.0.0:3088 → 127.0.0.1:3080` 反向代理，DSH 官方生态同款回环呈现，`remoteToken` 可选
- **运维面板**：实时状态 / 事件时间线 / 配置中心 / 更新日志 / 一键安装 DSH
- **托盘常驻**：关窗=隐藏；菜单直发 启动/停止/重启

## 架构

```
systemd user unit → dsh-supervisor（守卫内核，闭源）→ dsh web (127.0.0.1:3080)
                          │
                          └─ Tauri 壳（本仓库）→ 内容区加载「守卫托管的控制面板」
```

壳与内核通过本地 HTTP API 通信（端口由内核 `config.json` 的 `apiPort` 决定）；
壳源码全量开源，可审计、可贡献。

## 仓库结构（本仓自持）

本仓库**独立可构建、可发布**，不依赖内核仓：

```
├── src-tauri/                  壳源码（Rust + 内嵌引导页）
│   ├── bootstrap/              引导页（纯 HTML/CSS/JS，无构建步骤）
│   ├── src/                    main.rs / env.rs / node.rs / core.rs / update.rs
│   ├── tests/                  引导流程回归 + 更新产物验收（用 Tauri 同源依赖）
│   ├── tauri.conf.json         窗口 / 更新通道（npm CDN 静态清单）配置
│   └── tauri.windows.conf.json Windows 平台覆盖（Tauri 平台配置合并）
├── shell-release/              npm 壳包组装 + shell-manifest.json 生成
├── scripts/                    版本提升（bump-shell.sh）/ 版本自洽校验
├── ci/                         glibc 基座门禁（防「只能在新发行版运行」）
├── docs/                       设计 / 审计文档
└── .github/workflows/          四平台构建 + 产物验收 + npm 发布（tag 触发）
```


## 文档索引

| 文档 | 性质 | 说明 |
|---|---|---|
| [docs/README.md](docs/README.md) | **索引** | 文档角色表 + 产品硬规则 |
| [DEVELOPMENT-TRACK.md](docs/DEVELOPMENT-TRACK.md) | **规范（SSOT）** | **壳仓改代码规则**：运行时禁区（源码开发绝不触碰系统安装版）|
| [RELEASE-STANDARD.md](docs/RELEASE-STANDARD.md) | **规范（SSOT）** | 发布/构建流程的唯一事实源 |
| [ENV-TOOLCHAIN-INSTALL-STANDARD.md](docs/ENV-TOOLCHAIN-INSTALL-STANDARD.md) | **规范（SSOT）** | **环境工具链检测/安装/下载的唯一事实源**（node 与 npm 并行同权；统一安装事件与 UI 规范；禁进度条）|
| [RELEASE-AND-BUILD-DECISION.md](docs/RELEASE-AND-BUILD-DECISION.md) | 决策依据 | 为什么这样发布/构建 |
| [DESIGN-COMPLETE.md](docs/DESIGN-COMPLETE.md) | **总纲（权威）** | 完整架构理解与抽取决策 |
| [DESIGN-SHELL-ARCHITECTURE.md](docs/DESIGN-SHELL-ARCHITECTURE.md) | **规范（权威）** | 壳工程架构：分层 / 平台适配层 / 错误模型 / 契约层 / 门禁 |
| [DESIGN-BOUNDARY.md](docs/DESIGN-BOUNDARY.md) | **规范（权威）** | 内核↔壳职责边界与抽取审计 |
| [KERNEL-LAUNCH-STANDARD.md](docs/KERNEL-LAUNCH-STANDARD.md) | **规范（SSOT）** | 内核启动的唯一事实源（跨平台 P0–P6 流水线 / 平台矩阵 / core.json）|
| [SHELL-UPDATE-CHANNEL-VERIFICATION.md](docs/SHELL-UPDATE-CHANNEL-VERIFICATION.md) | 实测记录（2026-09-11） | 壳更新通道为什么定为 npm CDN（unpkg/jsdelivr）；其 §六/§八 的三项建议此后被推翻，见该文件头部的现状块 |
| [UPDATER-SIGNING-KEY.md](docs/UPDATER-SIGNING-KEY.md) | 运维手册 | minisign 自更新签名密钥的保管/备份/验证/轮换（**不含私钥**）|
| [DESKTOP-ACCEPTANCE.md](docs/DESKTOP-ACCEPTANCE.md) | 验收清单 | 桌面壳真机验收（引导页 / 服务定义 / 自更新，GUI 场景）|
| 内核仓 `RELEASE-CHANNEL-CONTRACT.md` | **规范（SSOT，跨仓）** | **发布通道/选版唯一事实源**（canary/beta/rc/latest/rollback）；壳侧实现 `src-tauri/src/release_channel.rs` |
| 内核仓 `NO-CONSOLE-WINDOW-STANDARD.md` | **规范（SSOT，两仓共遵）** | 壳启动内核全链路不得弹终端；壳侧门禁 `src-tauri/tests/no_console_window_test.rs` |
| 内核仓 `DSH-TOKEN-CONTRACT.md` | 规范（契约，跨仓） | 令牌分类与铁律；壳不持有 DSH 令牌 |

> **产品硬规则**：壳与内核同一套升级逻辑 —— 有新版本即强制更新；**不得跳过、不得按版本拉黑、不得冷却抑制**。紧急回退是发布通道契约内的**显式**通道（运维打 `rollback` dist-tag，见内核 `RELEASE-CHANNEL-CONTRACT.md`），不是「按版本比较自动降级」。
> **不变量**：壳的跨平台与引导行为由测试保障（`cargo test`，含引导流程回归 B1–B63）；文字文档不构成证据。
## 安装

### 内核（闭源，npm 分发）

内核按平台发布为独立 npm 子包（Node launcher 形态，需 Node ≥18）：

```bash
npm i -g @dsh-sup/dsh-core-<platform>-<arch>   # linux-x64 / darwin-arm64 / darwin-x64 / win-x64
dsh-supervisor self-check                      # guardVersion / node / platform 三段自检
```

### 壳（本仓库）

壳有**两条**面向用户的分发路径，都由 CI 在 `v*` tag 上产出（本机一律不产发布物）：

| 通道 | 位置 | 用途 |
|---|---|---|
| GitHub Releases | 本仓 `Releases` 页（`softprops/action-gh-release` 在 tag 构建挂上四平台安装程序 + `.sig`） | 手动下载安装 |
| npm + CDN | 安装程序 `@dsh-sup/shell-<platform>@<ver>/artifact/…`（unpkg / jsdelivr 可直取）；自更新清单 `@dsh-sup/shell-release@latest/shell-manifest.json` | **自动更新走的这条** |

> 两条通道的时间线不同：npm + CDN 从 `1.0.1` 起就在出货（旧账号 CI 签名，清单一直有更新）；
> GitHub Releases 则一直是 0 条 —— 迁仓后新账号没有签名密钥，tag 构建缺 `TAURI_SIGNING_PRIVATE_KEY`
> 即报错退出（这是设计，见 `docs/UPDATER-SIGNING-KEY.md` §〇），`1.1.11`（2026-09-18）之后
> 两条通道都没有新版本。**`1.2.0`（本版）发布后两条通道同时恢复，且改用新钥匙签名。**
> 日常构建的 CI artifacts 只是未发布的中间产物，不是下载点。

> **≤1.1.11 的客户端必须手动重装 1.2.0 一次。** 签名钥匙在 1.2.0 换过：Tauri 强制验签、无降级路径，
> 旧客户端内置旧公钥，永不接受新钥签的清单 —— 对它们自动更新等于失效。1.2.0 起自动更新恢复正常。
> （内核自更新不经 minisign，不受影响。）
>
> **Linux 请装 `.deb`。** 构建矩阵同时产 `deb` 与 `rpm`，但更新清单每平台只有一个槽位、放的是 deb，
> 因此 rpm 装上的客户端在自动更新时会拿到 deb 包。该缺口已登记（`CHANGELOG.md` 的 `[1.2.0]` 段），
> 真正的修法待定案。

**没有「本机先把壳跑起来」这一步。** `docs/RELEASE-STANDARD.md` §0 的硬标准：构建、无头冒烟
（`cargo build` + `--node-plan`）与全部测试都是 CI `build` job 的步骤，本机执行既产不出可验收的包，
其结论也不构成验收依据；本机跑起来还会绕过 CI 的 glibc 基座门禁与签名步骤，跑出与用户拿到的不同的二进制。
本机上限只有纯静态检查：`bash -n`、`node --check`、`cargo fmt --check`。

## 开发 / 贡献

改代码规则见 `docs/DEVELOPMENT-TRACK.md`（SSOT）。推上去由 CI 裁决：
`cargo test --bins` + 自动枚举的 `tests/*.rs`（引导流程回归、更新产物验收、各结构门禁）
→ 无头冒烟 → 四平台打包。看 run 的 job 日志与 artifacts，而不是本机结论。

版本升级与校验（本仓自持，只改文件、不构建）：

```bash
bash scripts/bump-shell.sh <ver>        # 三处互锁同号：Cargo.toml / tauri.conf.json / Cargo.lock
node scripts/verify-shell-versions.js   # 自洽校验（只读比对，本机可跑）
```

感兴趣的方向：引导页交互、多语言、Windows 打包、自动化测试。欢迎提 Issue / PR。

## 截图

<!-- 放真实截图：引导页 / 面板概览 / 托盘菜单 / 局域网开关。仓库内暂无 screenshots/ 目录（此项为待办） -->

- 引导页：环境检测 + 一键安装
- 面板：实时状态卡 + 事件时间线
- 托盘：常驻菜单

## 许可

- **壳（本仓库）**：MIT License（见 [LICENSE](LICENSE)）
- **内核**：UNLICENSED（闭源，保留所有权利；经 npm 平台子包分发）

## 相关

- DeepSeek Harness: <https://github.com/deepseek-ai/DeepSeek-Harness>

Topics: `dsh` `deepseek` `tauri` `rust` `desktop-app` `guardian` `tray` `lan-proxy` `self-update` `nodejs`