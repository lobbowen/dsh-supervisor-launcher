# 桌面壳自更新签名密钥（minisign）管理手册

> **本文档不含私钥内容**（私钥绝不入库）。
> 现用密钥生成于 2026-09-21（自 1.2.0 起生效）；2026-09-11 那把自 1.2.0 起不再使用，见 §〇 与 §六。

## 〇、当前状态（2026-09-21 实测，先读这段）

| 事实 | 证据 |
|---|---|
| **签名产物在线上、且是在用的更新通道**：清单 `@dsh-sup/shell-release@latest` 现指 `1.2.1`（2026-09-21 实测，`pub_date` 2026-09-21T09:32Z；换钥后首版是 `1.2.0`），四平台各带一份 minisign 签名，**key id 逐条实测均为新钥 `54A15461E39C8AEF`**。现值随每次发布变化，重核：`npm view @dsh-sup/shell-release version` | `GET https://unpkg.com/@dsh-sup/shell-release@latest/shell-manifest.json`（`@latest` 会 302 到具体版本，须跟随），按 `docs/RELEASE-STANDARD.md` §5 的解法逐条取 key id |
| **1.1.11 及更早版本共用一把钥匙**：抽验 1.0.1 / 1.0.5 / 1.1.0 / 1.1.5 / 1.1.9 / 1.1.10 / 1.1.11，key id 均为 `96DE3EF26F389F70`，与该轮 `tauri.conf.json` 内置公钥逐字一致 → 存量客户端只认这一把。**1.2.0 起换成新钥**，两把不通用 | 同上取各版本 manifest；签名 blob 第 2..10 字节（小端转 hex）即 key id |
| **通道不是 GitHub Release**：该轮取证时 `/repos/…/releases` 为 0 条，而清单与产物一直托管在 npm（unpkg + jsdelivr 两个端点写在 `tauri.conf.json`）。**本节此前写「壳仓从未产出过签名产物，也从未发布过 GitHub Release」是错误引导** —— 后半句在当时真、前半句假，且它足以诱导「换钥无害」的结论。**现状**：`v1.2.0` 起 Release 有资产，既是手动下载点，**也进来了自动更新的候选源**（清单每平台只写一条 payload URL，拿不到时壳按实测候选换源，其中含 Release 同名资产） | `releases/tags/v1.2.0` 返回 12 项资产；`npm view @dsh-sup/shell-release versions` 含 `1.2.0`；换源口径与逐源实测见 `docs/SHELL-UPDATE-CHANNEL-VERIFICATION.md` §九 |
| **旧钥判不可得，用户 2026-09-21 裁决：轮换而非继续等待** —— 新钥对已生成、新公钥已内置 `tauri.conf.json`、两枚签名 secret 已配置 | 新 minisign key id `54A15461E39C8AEF`；私钥与口令按 §四 布局落在 `~/.tauri/`；`GET /repos/lobbowen/dsh-supervisor-launcher/actions/secrets` 现列 `NPM_TOKEN` + `TAURI_SIGNING_PRIVATE_KEY` + `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` |
| **§〇 此前写的「冻结」已解除**：不再等待找回旧钥，也不再禁止建钥 —— 但旧钥的取证结论仍然成立，是本次换钥代价的来源 | 上表前三行（清单在产线、28 条签名同 key id、通道是 npm + CDN）为 2026-09-21 实测 |

**换钥的代价（已确认并承担，不是待议风险）**

- ≤1.1.11 的存量客户端内置的是旧公钥，**永不接受**新钥签的清单：Tauri 强制验签、无降级路径，
  对这些版本自动更新等于失效，**必须手动重装 1.2.0 一次**（下载点见 `README.md`）。1.2.0 起恢复正常自动更新。
- 旧钥（`96DE3EF26F389F70`）自此**不再使用**：`@latest` 上若再出现旧钥签名的清单，1.2.0 客户端会验签失败。
  两套钥匙不能混用，也没有「先兼容旧客户端」的过渡形态可造。
- **内核自更新与此无关**：壳装内核走 `npm install -g @dsh-sup/dsh-core-<platform>-<arch>`（`src-tauri/src/core.rs`），
  完整性由 npm registry 的 `integrity` 保障，**不经 minisign**。

**放弃找回的两条路径（记录，避免下一轮重复怀疑）**：① 本机/备份介质/其它机器均无 2026-09-11 那次
`signer generate` 的输出（`find /home/bowen -maxdepth 6` 无 `*.key`、`git log --all -S"minisign secret key"` 无命中）；
② 迁仓前账号 `wasi7mglns/dsh-supervisor-launcher` 的 secret 现 PAT 取之 **403**（无 admin），连是否残存都无法核实。

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
| 私钥（签名用，口令加密） | `~/.tauri/dsh-supervisor.key` | `600` | **绝不** |
| 私钥口令（**没有它私钥等于不存在**，与私钥同级保护） | `~/.tauri/dsh-supervisor.keypass` | `600` | **绝不** |
| 公钥（验签用） | `~/.tauri/dsh-supervisor.key.pub` | `644` | 可（已在 `tauri.conf.json` 公开） |
| 本机备份（私钥 + 口令 + 说明） | `~/.tauri/backup/dsh-supervisor.{key,keypass}.<时间戳>`、`backup/README.txt` | `600` | **绝不** |

> 上表 2026-09-21 起为**实际存在**的状态（备份时间戳 `20260921T030035Z`，副本与原件 `cmp` 一致）。

**指纹**（用于校验备份一致性；可安全记录）：

| 文件 | sha256（前 16 位） | 可核性 |
|---|---|---|
| 私钥 | `e756d012b7e48396` | 2026-09-21 本机已核 |
| 公钥（= `tauri.conf.json` 的 pubkey 整串，无换行） | `9084f65cdc138bea` | 2026-09-21 已核 |

现用 minisign key id（公钥解码后首行注释）：`54A15461E39C8AEF`。核对某份产物是否出自现用钥匙：
把 `platforms.*.signature` **外层 base64 解成文本，取其第二行**（签名 blob）再 base64 解码，
取该 blob 的第 2..10 字节按小端转 hex，应等于该 id —— 只解外层会得到 `"trusted "` 这类 ASCII，
解到最后一行会得到 global signature，两种错法的输出都像 hex 且**每平台都不同**，不足以定性。

**现用公钥**（公开信息，与 `tauri.conf.json` 的 `plugins.updater.pubkey` 一致，152 字符）：

```
dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDU0QTE1NDYxRTM5QzhBRUYKUldUdmlwempZVlNoVkNPUFBXQVo0UHpNL1QvVGEwWWR0WFk2bjVNMHpTSlR4eUx1MnZaWDNCV2gK
```

**历史：1.1.11 及更早的公钥**（`96DE3EF26F389F70` / sha256 前缀 `d5ffd60103af390a`，私钥不可得）——
留着只为识别旧清单，**不得**再用它签任何东西：

```
dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDk2REUzRUYyNkYzODlGNzAKUldSd256aHY4ajdlbGhsQnpRNGo2bldrVG1WYkllTnc1aGlkdHZHSGJybUllZzZ6eXFwS1dJZEcK
```

## 三、CI 配置（壳仓 GitHub Secrets）

| Secret | 值 | 说明 |
|---|---|---|
| `TAURI_SIGNING_PRIVATE_KEY` | 私钥**内容**或文件路径 | 构建时签名 —— 2026-09-21 已配置（现用钥 §二） |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | 密码 | **必须设置**（即使为空也要提供）—— 2026-09-21 已配置 |

> **纠错（2026-09-20 实测）**：原文把 `failed to decode secret key: incorrect updater private key password`
> 归因成「密钥已加密但漏填密码」，这是错误引导。同一个报错在**密钥根本没配置**时也会出现：
> secret 缺失时 Actions 把变量展开成空字符串，Tauri v2 CLI 于是拿空串去解码（`Missing comment in secret key`）。
> 正确的做法分两种：
> 1. 确实没有密钥（fork 或 secret 未配置的仓；**本仓 2026-09-21 起主干与 PR 都已配**，此路只为将来备用）：
>    CI 必须让该变量**不存在**而非为空（`build.yml` 的打包步骤已 `unset`），
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

**本机布局（2026-09-21 起为实际状态；口令文件是这次新增的一项，旧档漏了它 —— 加密私钥没有口令不可用）**：

| 位置 | 内容 | 权限 |
|---|---|---|
| `~/.tauri/dsh-supervisor.key` | 在用私钥（口令加密） | `600` |
| `~/.tauri/dsh-supervisor.keypass` | 解密口令 | `600` |
| `~/.tauri/backup/dsh-supervisor.key.<时间戳>` | 本机备份副本（含同后缀的 `.keypass`） | `600` |
| `~/.tauri/backup/README.txt` | 指纹与说明（不含私钥） | `600` |

**已做过的校验**：2026-09-21 备份与原件 `cmp` 一致；本机对探针文件跑 `tauri signer sign` 成功，
证明「私钥 + 口令」这一对可用（CLI 无 verify 子命令，验签由壳仓 CI 的 `updater_artifacts` 门禁承担）。
2026-09-11 那次建档做的恢复演练随旧目录一并失效，只作为「当时确实可用」的证据。

**必须遵守**：

1. **备份随开发机一同保护**（勿放在会被清理的临时目录）。
2. **绝不入库**：`.gitignore` 已覆盖 `*.key` / `*.key.pub`；导出脚本亦自检无泄漏。
3. **换机/重装前先拷走 `~/.tauri/`**——否则将永久失去签发更新的能力。

> ⚠ 风险如实说明（不改变上述决定）：本机单点备份意味着**磁盘损坏 / 误删 / 系统重装**都会导致不可逆的更新能力丧失。
> 若后续要加保险，最低成本做法是把 `~/.tauri/backup/` 复制一份到任意离线介质（U 盘即可）。

## 五、验证钥匙是否可用

**第 1 步（本机，纯静态）**：比对指纹，确认手里这份就是 §二 登记的在用钥。

```bash
# 应等于 e756d012b7e48396
sha256sum ~/.tauri/backup/dsh-supervisor.key.<时间戳> | cut -c1-16
```

**第 2 步（本机，只签探针）**：`tauri signer sign` 对一个临时文件签名，成功即证明「私钥 + 口令」匹配。
CLI 无 verify 子命令，且本机一律不得跑 `npx @tauri-apps/cli@2 build` —— 那会产出发布形态的产物，
直接违反 `RELEASE-STANDARD.md` §0 硬标准。

**第 3 步（CI，唯一能证明「产线可用 + 客户端会接受」的地方）**：

1. 两枚 secret 必须齐备：`TAURI_SIGNING_PRIVATE_KEY`（私钥内容）+ `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`（真实口令）。
   secret **只可覆盖写、不可读回**，所以写入是否成功没有 API 可自证，只能由产线产物裁决。
   私钥**不得**经命令行 `export`、不得进工作区任何文件（与内核仓 CREDENTIALS-STANDARD 铁律 2 同规）；
2. 判据（非 tag 构建即可看）：每个平台的 artifact 里出现**成对**的 `<安装程序>` + `<安装程序>.sig`，
   且 `组装 npm 包` 步骤没有 `::warning::TAURI_SIGNING_PRIVATE_KEY 未配置`；
3. 判据（tag 构建才有）：H7 的 `updater_artifacts` 同源验签过 —— 它用 `tauri-plugin-updater` 内部同一个
   minisign 实现、按 `tauri.conf.json` 的 pubkey 验，过则证明客户端会接受；清单里每条 signature
   解码后的 key id 应等于 `54A15461E39C8AEF`。

## 六、轮换

minisign **不支持密钥轮换对旧用户生效**：公钥固化在客户端内，换钥后

- 新版本用新私钥签名 + 新公钥内置 → **该版本必须由用户手动安装一次**
- 此后该用户才能继续自动更新

**原则**：非必要不换钥（私钥确定泄漏时才换，且必须公告）。

**已执行的一次：2026-09-21，随 1.2.0**。触发原因不是泄漏，而是 2026-09-11 那把私钥的副本在
本机 / git 历史 / 现账号三处均不可得（§〇），继续等待找回 = 壳自更新通道无限期停摆。
用户裁决承担代价换通道恢复：`≤1.1.11` 的存量客户端不再接受自动更新，需手动重装 1.2.0 一次；
公告与下载点写在 `README.md`，新钥匙身份见 §二。

出厂后的验收证据（线上可复算，口径见 `docs/RELEASE-STANDARD.md` §5）：tag run 的 H7 同源验签四平台全绿，
`@latest` 清单四条签名的 key id 均为 `54A15461E39C8AEF`，同期 `@1.1.11` 清单复算仍为旧 id
—— 两把钥匙各管各的版本区间，没有混用。逐条记录见 `CHANGELOG.md` 的 `[1.2.0]` 段。
