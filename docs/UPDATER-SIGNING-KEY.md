# 桌面壳自更新签名密钥（minisign）管理手册

> **本文档不含私钥内容**（私钥绝不入库）。
> 密钥生成于 2026-09-11，用于桌面壳（Tauri）自更新产物签名。

## 〇、当前状态（2026-09-20 实测，先读这段）

| 事实 | 证据 |
|---|---|
| 两仓 GitHub Secrets 只有 `NPM_TOKEN`，没有任何 `TAURI_SIGNING_*` | REST `/repos/…/actions/secrets` 列举（core 与 launcher 各一） |
| 本机已无 `~/.tauri/`，全盘 `*.key` / `*.key.pub` 无命中 | 2026-09-20 `find /home/bowen -maxdepth 6`；本文档原§二/§四 的本机路径与备份已不可核 |
| 壳仓从未产出过签名产物，也从未发布过 GitHub Release | `/releases` 返回 0 条；main 上最近一次 run 结论 failure |
| 公钥仍在仓内并生效 | `src-tauri/tauri.conf.json` `plugins.updater.pubkey`，key id `96DE3EF26F389F70` |

**推论**：2026-09-19 的同机凭据事故（旧库 `~/.dsh/credentials`、`~/.ssh` 部署密钥全丢）很可能把
`~/.tauri/` 一并带走；若离线介质另有副本，恢复前先按 §五 比对 key id 与 sha256 前缀。
在私钥重新可用之前，**tag 发布会被 workflow 主动拦下**（这是设计，不是缺陷）；
非 tag 构建不再因缺密钥而红：CI 在同一分支撤掉空的签名变量，并用 `--config` 把
`bundle.createUpdaterArtifacts` 关掉（配置里内置了 pubkey 时，Tauri 见「有公钥无私钥」
会直接失败，只撤变量仍红）。

公钥与私钥的配对是单向可验证的：任何新公钥都要重新内置进 `tauri.conf.json`，
而旧客户端只认旧公钥，见 §六。

## 一、这是什么、为什么必需

Tauri updater 在下载更新包后，**强制用公钥验证 minisign 签名，不可关闭**。

- **没有签名** → 无法产出 `.sig` → **自动更新链路根本无法成立**
- **有签名** → 即使产物经第三方 CDN（unpkg / jsdelivr）分发，仍能保证完整性

> 与 Apple / Windows 代码签名是**两件事**：
> - minisign：自动更新的完整性校验，**免费、必需**
> - Apple Developer ID / Windows 代码签名：消除安装拦截提示，**付费、可选**

## 二、文件与位置

| 用途 | 路径 | 权限 | 是否入库 |
|---|---|---|---|
| 私钥（签名用） | `~/.tauri/dsh-supervisor.key` | `600` | **绝不** |
| 公钥（验签用） | `~/.tauri/dsh-supervisor.key.pub` | `644` | 可（已在 `tauri.conf.json` 公开） |
| 本地备份 | `~/.tauri/backup/dsh-supervisor.key.<时间戳>` | `600` | **绝不** |

> 上表是 2026-09-11 建档时的设计位置；**当前本机这三处都已不存在**（见 §〇）。恢复或重建后再按表核对。

**指纹**（用于校验备份一致性；可安全记录）：

| 文件 | sha256（前 16 位） | 2026-09-20 可核性 |
|---|---|---|
| 私钥 | `92e3ae43ed4dea58` | 不可核（本机文件已不存在，见 §〇） |
| 公钥 | `d5ffd60103af390a` | **已核**：对 `tauri.conf.json` 内 pubkey 字符串（无换行）取 sha256 前 16 位即此值 |

minisign key id（公钥解码后的注释）：`96DE3EF26F389F70`。恢复私钥副本后，先比对该 key id
是否等于公钥注释，再比对私钥 sha256 前缀，两者都过才可用于发布。

**公钥**（公开信息，与 `tauri.conf.json` 的 `plugins.updater.pubkey` 一致，152 字符）：

```
dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDk2REUzRUYyNkYzODlGNzAKUldSd256aHY4ajdlbGhsQnpRNGo2bldrVG1WYkllTnc1aGlkdHZHSGJybUllZzZ6eXFwS1dJZEcK
```

## 三、CI 配置（壳仓 GitHub Secrets）

| Secret | 值 | 说明 |
|---|---|---|
| `TAURI_SIGNING_PRIVATE_KEY` | 私钥**内容**或文件路径 | 构建时签名 |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | 密码 | **必须设置**（即使为空也要提供） |

> **纠错（2026-09-20 实测）**：原文把 `failed to decode secret key: incorrect updater private key password`
> 归因成「密钥已加密但漏填密码」，这是错误引导。同一个报错在**密钥根本没配置**时也会出现：
> secret 缺失时 Actions 把变量展开成空字符串，Tauri v2 CLI 于是拿空串去解码（`Missing comment in secret key`）。
> 正确的做法分两种：
> 1. 确实没有密钥（日常构建、PR）：CI 必须让该变量**不存在**而非为空（`build.yml` 的打包步骤已 `unset`），
>    且必须关掉 `bundle.createUpdaterArtifacts`（否则报 `A public key has been found, but no private key`），
>    并且组装/验收步骤对 `.sig` 的强校验只在 tag 上生效；
> 2. 有密钥（发布）：`TAURI_SIGNING_PRIVATE_KEY` 给内容或绝对路径，
>    `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` 给该密钥**真实的**口令（未加密的密钥才可给空串）。
>
> 判据提示：报错文本里的 `Missing comment in secret key` 说明解码输入根本不是合法 minisign 私钥。

`NPM_TOKEN` 亦需配置（发布 `@dsh-sup/shell-*` 与清单包 `@dsh-sup/shell-release`）。

## 四、备份要求（**2026-09-11 用户定案：本机备份即可，不做异地**）

> **私钥一旦丢失，已安装该版本的用户将永久无法再收到自动更新。**
>
> 原因：公钥已固化在用户端的程序内；换新密钥对旧用户无效——他们只能手动重装。

**设计上的本机方案**（下表三处**当前均不存在**，见 §〇；恢复或重建私钥后按此布局落位）：

| 位置 | 内容 | 权限 |
|---|---|---|
| `~/.tauri/dsh-supervisor.key` | 在用私钥 | `600` |
| `~/.tauri/backup/dsh-supervisor.key.<时间戳>` | 本机备份副本 | `600` |
| `~/.tauri/backup/README.txt` | 指纹与说明（不含私钥） | `600` |

**2026-09-11 建档时做过的校验**（记录，非当前状态）：备份与原件 `cmp` 一致；并做过一次恢复演练
（解出后指纹相同）。该演练结论随本机目录一并失效，只能作为「当时确实可用」的证据。

**必须遵守**：

1. **备份随开发机一同保护**（勿放在会被清理的临时目录）。
2. **绝不入库**：`.gitignore` 已覆盖 `*.key` / `*.key.pub`；导出脚本亦自检无泄漏。
3. **换机/重装前先拷走 `~/.tauri/`**——否则将永久失去签发更新的能力。

> ⚠ 风险如实说明（不改变上述决定）：本机单点备份意味着**磁盘损坏 / 误删 / 系统重装**都会导致不可逆的更新能力丧失。
> 若后续要加保险，最低成本做法是把 `~/.tauri/backup/` 复制一份到任意离线介质（U 盘即可）。

## 五、验证备份是否可用

> **前置**：本机已无这些文件（见 §〇）。下面两步只在**从离线介质找回副本后**执行，
> 且必须先过 §二 的 key id 比对。

**第 1 步（本机，纯静态）**：比对指纹，不签名、不构建。

```bash
# 应等于 92e3ae43ed4dea58
sha256sum ~/.tauri/backup/dsh-supervisor.key.<时间戳> | cut -c1-16
```

**第 2 步（CI，唯一能证明「密钥可用于产线」的地方）**：

1. 由用户把私钥写入仓库 secret `TAURI_SIGNING_PRIVATE_KEY`（+ 口令 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`）；
   私钥**不得**经命令行 `export`、不得进工作区任何文件（与内核仓 CREDENTIALS-STANDARD 铁律 2 同规）；
2. 触发一次**非 tag** 构建（`workflow_dispatch`）；`build` job 见 secret 非空即照常产出 updater 产物；
3. 判据：每个平台的 artifact 里出现**成对**的 `<安装程序>` + `<安装程序>.sig`，
   且 `组装 npm 包` 步骤没有 `::warning::TAURI_SIGNING_PRIVATE_KEY 未配置`。

> 非 tag 构建**不跑** H7 的 `updater_artifacts` 同源验签（该步 `if: startsWith(github.ref,'refs/tags/v')`），
> 所以上面只证明「能签出 `.sig`」，不证明「Tauri 客户端会接受」。后者要等真正的 tag 构建裁决。
> 本机一律不得跑 `npx @tauri-apps/cli@2 build` 来验证 —— 那会产出发布形态的产物，直接违反
> `RELEASE-STANDARD.md` §0 硬标准。

## 六、轮换（设计上可行，实操代价极高）

minisign **不支持密钥轮换对旧用户生效**。若确需换钥：

- 新版本用新私钥签名 + 新公钥内置 → **该版本必须由用户手动安装一次**
- 此后该用户才能继续自动更新

**结论**：除非私钥确定泄漏，否则不要换钥；泄漏时走「手动安装过渡版」的应急流程。
