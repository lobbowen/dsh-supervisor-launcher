# 桌面壳真机验收（手工清单）

> 本清单属**壳仓**（原误置于内核仓，2026-09-16 移交）。
> 前置：验收对象是 **CI 产出的安装包**（四平台矩阵见 §5）；本机的任何 `cargo build` / `cargo test`
> **都不构成验收依据**，且按 §1 的口径一律不跑。
>
> ⚠ `RELEASE-STANDARD.md` §0 硬标准：**所有平台构建、测试与发布必须经 GitHub CI 完成，本地不得产生任何发布产物**。

## 1. 构建产物从哪里来

**验收对象只有 CI 的产物**（§5）。本清单**不给任何本机产包/构建/测试命令**：

- `npx @tauri-apps/cli build`、`cargo build`、`cargo test` 在本机跑一次就违反 `RELEASE-STANDARD.md` §0
  （构建与测试都由 CI 裁决；本地不得产生任何发布产物）；
- 本机允许的上限是**纯静态**检查：`bash -n`、`node --check`、`cargo fmt --check`、读文件；
- 桌面手工项（本清单 §2–§4）用的必须是 **CI artifact 里的安装程序**，不是 `target/debug/` 下的裸二进制 ——
  后者绕过了打包、bundle 资源、服务定义与签名链路，跑出来的「能用」不代表安装包能用。

内核产物与发布在**内核仓**（`build:launcher` + `publish:core`，同样只在 CI 内），不在本仓。

## 2. 场景 A：全新环境（无 Node.js）
1. 安装产物（Linux：取 CI `build (ubuntu-22.04, …)` 的 artifact，解出的
   `dsh-supervisor_*_amd64.deb` 用 `sudo dpkg -i` 安装；`dist/` 里**没有** deb，
   那是 npm 组装目录），随后 `dsh-supervisor-gui` 启动壳；
2. 预期：壳窗口出现**引导页**「检测到缺少 Node.js · 官方最新 LTS <v>」+「一键安装 Node.js LTS」按钮（无 Node 也能跑，证明不携带运行时）；
3. 点「一键安装」→ **不弹任何系统授权**（Node 安装是用户级零权限，见
   `ENV-TOOLCHAIN-INSTALL-STANDARD.md` §4bis）→ 文字状态（统一安装事件，**禁进度条**）→ Node 装到 `<状态根>/node`；
4. 预期：自动拉起守卫 → 窗口切到面板（URL 由 `config.json` 的 `apiPort` 决定，默认 `127.0.0.1:36360`，被占自动顺延）；
   **注意**：Node 装在 `<状态根>/node`，不在系统 PATH 里，所以「任意终端 `node --version`」不是本步的判据 ——
   判据是面板/契约 `runtime.json` 记录的 node 路径可执行且版本为最新 LTS；
5. 面板「设置 → 环境与自更新」卡：Node（系统+运行时）、npm、DSH 状态、内核更新（桌面壳执行；未配置包名 → 明确提示）。

## 3. 场景 B：已有旧版 Node
1. 预期：Node **达标**（>= `minNode`，当前 v22.12）→ 壳直接进面板，不弹引导页；
   Node **缺失或低于 `minNode`** → 弹引导页，文案是「低于最低要求（v22.12）· 正在升级」并走同一条安装链
   （`bootstrap/js/20-env.js` 用后端回传的 `minOk` / `minRequired` 判定）；
2. `/env/status`：`node.detected` 为旧版。

> **已知缺口（与 §6 同源，只在这里写一次）**：Node **已达标但低于官方最新 LTS** 时，面板与引导页都
> **没有**升级入口 —— 引导页只认 `minNode`，不认「最新 LTS」。升级这种情形只能手动换 Node。
> 此前本节与 §6 对这件事**各写了一套互相矛盾的说法**（一边说「已实现：显示『升级到官方最新 LTS』
> +『跳过并使用现有版本』按钮」，一边说「记录为已知缺口」）。前端全仓 grep 不到那两个文案，
> 也没有对应分支 —— **「已实现」那一侧是假的**，已删。

## 4. 场景 C：内核更新（单写入者 = 桌面壳；2026-09-15）

> **本仓没有可从守卫侧触发的内核更新端点。** 内核 npm 包的安装/升级只由桌面壳执行：
> `POST /self-update/apply`、`POST /self-update/restart-guard` 已下架（返回 `410 KERNEL_UPDATE_SINGLE_WRITER`）；
> `selfUpdateManifestUrl` / `selfUpdateDir` 与旧 manifest 执行器 `src/domains/dist/self-update.js` 已删除。
>
> 验收方式：在**桌面壳**面板「关于」中点「更新」（壳经 `kernel_update_apply` 装内核，
> 并经服务管理器重启守卫），或走壳启动门 2。守卫侧只可 `GET /self-update/status` 读状态。

## 5. 构建矩阵（唯一事实源 = `RELEASE-STANDARD.md` §2）

| runner | artifact | 安装程序（= 本清单的验收对象）|
|---|---|---|
| `ubuntu-22.04` | `linux-x64` | `deb`（Linux 支持面 = **Ubuntu + deb 一种形态**，AppImage 与其它发行版形态都不在验收范围）|
| `macos-latest` | `darwin-arm64` | `app` + `dmg` |
| `macos-15-intel` | `darwin-x64` | `app` + `dmg` |
| `windows-latest` | `win-x64` | `nsis` + `msi` |

命令与 bundles 参数由 CI 矩阵注入（`npx --yes "@tauri-apps/cli@${TAURI_CLI_VERSION}" build --bundles "<matrix.bundles>"`），
本清单不重复、也不得据此在本机执行。

Node 安装矩阵（壳内实现，`platform/{linux,macos,windows}.rs` 的 `install_node`）：
三平台统一**用户级解包到 `<状态根>/node`、零权限** —— Windows `win-{arch}.zip`、
macOS / Linux `*-arch.tar.gz`（Linux 用 gzip 不再依赖 xz）。
**本行此前写的是重写前的旧模型**（「Windows `msiexec /qn`（UAC）/ macOS `installer -pkg`（管理员）/
Linux `pkexec tar 到 /usr/local`（tar.xz）」），那套形态 2026-09-18 已整体废除，
且有门禁 `a2_user_scope_install_needs_no_privilege` 反向钉住「`install_node` 不含 pkexec/sudo」。
提权通道（`PRIVILEGE_COMMANDS`）现在**只服务壳自更新**，与 Node 安装无关。

## 6. 已知缺口（验收记录用）
- Node **达标但低于官方最新 LTS**：无升级入口（判定只看 `minNode`，见 §3 的缺口说明）；
- 内核更新由桌面壳执行（守卫不自更新）：面板「更新」经壳 `kernel_update_apply` 装内核并由所有者重启守卫；
- GUI 真机手动项（引导页交互/托盘/关窗隐藏）需桌面环境逐个核对，**且只能装 CI 产物来核**（§1）；
- ~~AppImage 打包需可达 GitHub 下载 extern 工具~~：**AppImage 已废弃**，Linux 只按 `deb` 一种形态验收（`RELEASE-STANDARD.md` §2）。