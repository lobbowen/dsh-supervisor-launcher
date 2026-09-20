# 壳更新通道验证报告（实测，2026-09-11）

> 回答用户问题：**「更新路径通过什么？是通过 GitHub？」**
> 结论：**不能是 GitHub Release 直连**。以下全部为实测 / 源码级证据。
>
> ## 读前必看：本文是**当时的验证记录**，三项建议的最新状态如下
>
> | 建议 | 后来怎样 | 现在的事实源 |
> |---|---|---|
> | **N1 通道 = npm CDN（unpkg/jsdelivr）** | **已采纳**：`tauri.conf.json` 的 `plugins.updater.endpoints` 就是这两条直链，清单由 CI 的 `shell-release/make-manifest.js` 生成。**冗余度先被打折、后按实测重建**：jsdelivr 对 `.exe` 返 403（清单却拿得到），所以「两条端点」从来不等于「两条下载源」—— 现在安装包由壳按候选源换源（`mirror::artifact_candidates`，见 §九） | `docs/RELEASE-STANDARD.md` §1 H6/H8；本文 §九 |
> | **N2 Linux 主形态 = deb（放弃 AppImage）** | **采纳 deb，但不采纳「扩展」**：矩阵曾同时产 `deb,rpm`，现已**收窄为只产 `deb`** —— 支持面 = Ubuntu 一种形态，其它发行版不产不测不承诺（清单每平台只有一个槽位，多产一种形态就会让那种客户端的自动更新拿到别的包）。**AppImage 全仓已废弃**，任何文档再出现它都是残留 | `docs/RELEASE-STANDARD.md` §2 矩阵；`README.md` 的 Linux 那条 |
> | **N3 内核本地预取 + 缓存加速** | **未采纳，且方向被推翻**：壳自更新链**没有预取、没有缓存、没有隐式回退**，账本只有 `pending -> confirmed` 两态。§六「加速侧」那段（含 `~/.dsh/shell/cache/` 路径）**从未落地，不要照它实现** | 内核 `src/domains/shell/journal.js`；紧急回退走 `rollback` dist-tag（内核仓 `RELEASE-CHANNEL-CONTRACT.md` RC-2 优先级 / RC-7 反降级下限）|
>
> §一 的实测网速是 2026-09-11 在**当时的开发机与当时的网络出口**上取样的，只用于解释「为什么不直连 GitHub」，
> **不是当前带宽结论**。本仓的 GitHub Release 在 `1.2.0` 之前确实为空（签名密钥不可得，tag 构建被 workflow
> 判红 —— 见 `docs/UPDATER-SIGNING-KEY.md` §〇），所以本文当时量的是旧账号仓库的 v0.1.0 资产、如今在本仓不可复现；
> `v1.2.0` 起 Release 已有资产。它**仍不是主通道**（清单与产物第一顺位都在 npm CDN），
> 但按 §九 的实测它够格当**安装包的回退源** —— 这一条是 §一 那次取样给不出的结论。

---

## 一、决定性实测：GitHub Release 直连不可用

用**本项目真实产物**测试（壳仓 v0.1.0 的 Release 资产）：

| 目标 | 结果 |
|---|---|
| `objects.githubusercontent.com`（Release 资产 CDN） | **15s 完全超时** |
| GitHub Release 资产下载（3.8MB deb） | **14.8 KB/s**（15.6s 仅下 231KB） |
| GitHub Release 资产下载（3.1MB dmg） | **28 KB/s**（20s 仅下 561KB） |
| `api.github.com`（仅列元数据） | 0.39s（快，但不能下产物） |
| `github.com` 主站 | 8.0s（很慢） |

**推论**：77MB 的 AppImage 在 15–28 KB/s 下需 **约 45 分钟**。
更致命的是——**Tauri updater 的传输超时会先触发，更新永远失败**，而不是只是慢。

> 这条判据只对**主通道**成立（AppImage 早已不在支持面内）。09-21 在同一条出口上复测：
> `v1.2.0` 的 Release 资产直连可取到全量字节、约 116KB/s，但会间歇性连不上 ——
> 够格当回退源、不够格当主力。数字与判据见 §九。

### 为什么这恰好印证了内核既有机制的正确性

内核 / DSH 的更新**早就因为这个原因走了 npm 镜像**：

| 通道 | 实测速度 |
|---|---|
| npmmirror（内核在用） | **1.44 MB/s** |
| npm 官方源 | 160 KB/s |

壳的更新通道**不该例外**。

---

## 二、Tauri 源码级验证：url 接受任意 HTTPS URL

`tauri-plugin-updater/src/updater.rs`（v2 分支）原文：

```rust
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ReleaseManifestPlatform {
    /// Download URL for the platform
    pub url: Url,          // url::Url 通用类型，无域名白名单
    pub signature: String,
}
```

下载与校验逻辑：

```rust
verify_signature(&buffer, &self.signature, &self.context.config.pubkey)?;
```

**结论**：
- `url` 字段是**通用 URL**，可指向任意 HTTPS 位置（不限于 GitHub）
- **签名校验独立于托管位置**（minisign + pubkey），换 CDN 不影响安全性
- 源码中**未发现 host 白名单 / allowlist**（仅 `dangerousInsecureTransportProtocol` 控制 HTTP/HTTPS）

---

## 三、npm CDN 可直链包内文件（实测）

`unpkg` / `jsdelivr` 可直接提供 **npm 包内任意单个文件**（**无需**下载整个 tarball）：

| CDN | 小文件（855KB） | **大文件（10MB）** |
|---|---|---|
| **unpkg** | 268 KB/s | **1.71 MB/s** |
| **jsdelivr** | 337 KB/s | 735 KB/s |
| fastly.jsdelivr | 28 KB/s | — |
| gcore.jsdelivr | 216 KB/s | — |
| npmmirror 的 /files/ 路径 | **403**（不提供文件级访问） | — |

**关键洞察**：**小文件时 TLS/延迟开销占比大，大文件时吞吐上来** ——
所以对真实产物（几 MB 至几十 MB），unpkg 达 **1.71 MB/s**，与内核查镜像同级。

### npm 承载大二进制有先例

| 包 | unpackedSize |
|---|---|
| `@napi-rs/canvas-linux-x64-gnu` | **33 MB** |
| `@esbuild/linux-x64` | 11 MB |

说明 npm 完全可承载壳级别（3–77MB）的二进制产物。

---

## 四、重大修正：Tauri 能自更新 deb（**我们只用 deb**）

**我之前判断「Linux 只有 AppImage 是更新产物、deb 用户出局」是错的。** 源码证据（插件本身也认 rpm，
但 rpm 不在支持面内，见 `RELEASE-STANDARD.md` §2 —— 下面这段只是能力取证，不是我们的产物清单）：

```rust
fn install_inner(&self, bytes: &[u8]) -> Result<()> {
    match installer_for_bundle_type(bundle_type()) {
        Some(Installer::Deb) => self.install_deb(bytes),
        Some(Installer::Rpm) => self.install_rpm(bytes),
        _ => self.install_appimage(bytes),
    }
}

fn install_deb(&self, bytes: &[u8]) -> Result<()> {
    if !infer::archive::is_deb(bytes) { return Err(Error::InvalidUpdaterFormat); }
    self.try_tmp_locations(bytes, "dpkg", "-i", "deb")
}

// 提权路径：pkexec（图形 sudo 提示）→ zenity/kdialog 图形密码 → sudo
```

### 体积 / 速度对比（本项目真实产物）

| 平台产物 | 体积 | unpkg @1.71MB/s | 是否需提权 |
|---|---|---|---|
| Linux **deb** | **3.8 MB** | **约 2.2 秒** | 需（pkexec 弹窗） |
| Linux AppImage | **77 MB** | 约 45 秒 | 不需 |
| macOS dmg | 3.1 MB | 约 1.8 秒 | 不需 |
| Windows msi | 3.7 MB | 约 2.2 秒 | 视安装模式 |

**deb 比 AppImage 小 20 倍**。这是之前未被考虑的选项。

> 上面的「待实测：Tauri 是否为 deb 自动生成 `.sig`」**已确证，不必再等 Rust 环境**：
> 线上清单 `@dsh-sup/shell-release@1.2.0` 的 `linux-x86_64` 条目 URL 与其签名的 trusted comment
> 都是 `dsh-supervisor_1.2.0_amd64.deb` —— Tauri 自己为 deb 产出了 `.sig` 并写进了清单。
> 复核口径见 `RELEASE-STANDARD.md` §5（含钥匙 id 的两层 base64 解法）。

---

## 五、通道方案对比（基于实测）

| 方案 | 速度 | 新增基础设施 | 供应链风险 | 保留官方安装机制 |
|---|---|---|---|---|
| A. GitHub Release 直连 | 15–28 KB/s | 无 | 无 | 是 |
| B. GitHub + 第三方代理（ghfast 659KB/s） | 中 | 无 | **有**（代理可替换产物，虽有验签兜底） | 是 |
| **C. npm CDN（unpkg/jsdelivr）** | **1.71 MB/s** | **无**（复用已有 npm 发布流程） | 无 | 是 |
| D. 自建 CDN / OSS | 快 | **需成本** | 无 | 是 |

### 推荐：C（npm CDN）

理由：
1. **速度最优**（1.71 MB/s，与内核在用镜像同级）
2. **零新增基础设施**——我们**已经在用 npm 发布内核子包**，壳产物复用同一套流程与账号
3. **无第三方代理的供应链风险**（B 的问题）
4. **零成本**（D 的问题）
5. 冷启动可用（壳直连，不依赖内核）

---

## 六、修正后的更新通道架构

```
【发布侧】
  壳产物（deb / .app.tar.gz / -setup.exe —— Linux 只 deb 一种形态，见开头 N2 行）
      -> 发布为 npm 包：@dsh-sup/shell-<os>-<arch>@<version>
      -> 清单也作为包内文件：shell-manifest.json

【获取侧】
  壳启动 -> 读清单（unpkg / jsdelivr 直链，端点回退只覆盖这一步）
          -> 取 platforms[<os>-<arch>] = { url, signature }
          -> url 指向 unpkg 上的产物文件；该 URL 拿不到时由壳按实测候选换源（§九）
          -> tauri-plugin-updater：下载 -> minisign 验签 -> 平台安装

【加速侧（可选）】                        ← 未采纳，从未落地（见开头 N3 行）
  内核（若在运行）预取并缓存到 ~/.dsh/shell/cache/
  -> 壳优先从本机内核取（本地，秒级），失败再直连公网
```

### 与 Tauri 配置的对应关系

```json
{
  "bundle": { "createUpdaterArtifacts": true },
  "plugins": {
    "updater": {
      "pubkey": "<minisign 公钥>",
      "endpoints": [
        "https://unpkg.com/@dsh-sup/shell-release@latest/shell-manifest.json"
      ]
    }
  }
}
```

- **HTTPS 原生满足** → **不需要** `dangerousInsecureTransportProtocol`（去掉一个安全妥协）
- 清单格式用 Tauri 的**静态 JSON** 语义：`{ version, platforms: { "<os>-<arch>": { url, signature } } }`

---

## 七、对执行方案的修改点

| 章节 | 原内容 | 修正后 |
|---|---|---|
| 第 1 节 通道 | 「公网发布通道（GitHub Release / CDN）」未定 | **定为 npm CDN（unpkg / jsdelivr 直链）** |
| 第 2.5 节 Tauri 配置 | endpoints 未定 | `endpoints = [unpkg 上的 shell-manifest.json]` |
| P4 发布链 | CI 产出更新产物 + 挂 GitHub Release | **产物发布为 npm 包**（复用已有 npm 发布流程）；GitHub Release 仅作人工下载 / 备用 |
| P5 分发形态 | Linux「AppImage 主通道 / deb 由 apt」 | **新增选项：deb 可自更新（3.8MB，pkexec 提权）**；体积差 20 倍，需重新决策 |
| 风险 K3 | 回环 HTTP vs TLS 强制 | 已消除（HTTPS 原生满足） |
| 新增 | — | **K13：npm CDN 可用性**（unpkg / jsdelivr 均为第三方）。该条设想的两个缓解**都打了折扣**：多 CDN 回退对 Windows 不成立（jsdelivr 屏蔽 `.exe`，见 `CHANGELOG.md` `[1.2.0]`），内核本地缓存**从未落地**（N3 已被推翻）。风险仍在，只是不再有「已经 mitigated」的假象 |

---

## 八、需用户决策的新增项

| # | 决策 | 选项 | 我的建议 |
|---|---|---|---|
| **N1** | **更新通道** | A GitHub直连 / B 代理 / **C npm CDN** / D 自建 | **C**（速度最优 + 零基础设施 + 无供应链风险） |
| **N2** | **Linux 更新主形态** | AppImage（77MB，免密码）/ **deb（3.8MB，需一次密码）** | 倾向 **deb**：体积差 20 倍、速度差 20 倍；提权是一次性成本 |
| **N3** | 是否需要内核本地缓存加速 | 需要 / 不需要 | **需要**（热路径从 45 秒降到秒级；且是离线降级路径） |


---

## 九、复测：把「拿得到清单」与「拿得到安装包」分开量（2026-09-21）

前八节量的都是**清单**能不能取到。但自更新真正要下载的是清单里那条**绝对产物 URL** 指向的安装包，
而 Tauri 的 `Update::download_url` 在插件下载阶段**不会换源**（`endpoints` 的回退只覆盖清单请求）。
所以「某镜像清单可达」推不出「该镜像能分发安装包」—— 这正是此前把 jsdelivr 记成
「Windows 第二条 CDN」的出处。

本轮唯一判据：**该源能否把本平台安装包的完整字节取回**（HTTP 200 + 全量）。
清单 JSON、`.sig` 这类小包能取到不算数。取样 = 本机出口，直连与经代理各一遍，两遍判定一致；
吞吐只作参考，**不代表用户网络**。产物取线上 `1.2.0`（win `.exe` = 3,272,190 字节）。

| 候选源 | win `.exe` | linux `.deb` | mac `.app.tar.gz` | 清单 `.json` | 判定 |
|---|---|---|---|---|---|
| `unpkg.com/@dsh-sup/<pkg>@<ver>/artifact/…` | 200 全量 | 200 | 200 | 200 | **唯一四形态全通的公共 CDN** |
| `cdn.jsdelivr.net/npm/…` | **403 Forbidden** | 200 | 200 | 200 | 安装包侧只覆盖 mac / linux；清单侧可用 |
| `fastly` / `gcore` / `testingcf.jsdelivr.net`、`cdn.jsdmirror.com` | 403 | 200 | 200 | — | 同族同策略，换入口域名无效 |
| `registry.npmmirror.com/<pkg>/<ver>/files/…` | 403 | 403 | 403 | 403 | 403 文案 = `"@dsh-sup/…" is not allow to unpkg files`：**按包**关闭 raw-file 路由，与扩展名无关 |
| `unpkg.npmmirror.com` | DNS 不解析 | — | — | — | 主机不存在 |
| `mirrors.cloud.tencent.com/npm`、`mirrors.huaweicloud.com/repository/npm` | 404 | 404 | 404 | 404 | 只有 registry 协议：`…/-/<pkg>-<ver>.tgz` **200 且快**（3,179KB），但 tgz 不是安装程序 |
| `mirrors.aliyun.com/npm` | 404 | 404 | — | — | 连包元数据路径都非标准 |
| `github.com/…/releases/download/v<ver>/<file>` | 200 全量 | 200 | 200（有同名坑，见下）| 无该资产 | 可作回退；直连约 116KB/s、经代理约 412KB/s，且**会间歇性连不上** |
| `unpkg.net` | 503 `USAGE_EXCEEDED` | — | — | — | 不是镜像，是 unpkg 备用域且已限流 |
| `gh-proxy.com`、`ghfast.top` | 200 | 200 | — | — | 能取到字节，但由**未审计第三方**中转 → 不进默认源表，只作排障手工出口 |
| `mirror.ghproxy.com`、`hub.gitmirror.com` | 连接失败 | — | — | — | — |

三条由此定案的事实：

1. **国内 npm 镜像永远给不出 Windows 安装程序**：npmmirror 对整个 `@dsh-sup` scope 关闭 raw-file 路由，
   腾讯 / 华为只有 registry 协议（tgz）。此前「npm CDN 多镜像」的想象，在 `.exe` 这一格是空的。
2. **jsdelivr 家族按扩展名屏蔽 `.exe`**（不是我们包的问题，也不是某个入口域名的偶发），
   因此任何把 jsdelivr 写成「Windows 的第二条 CDN」的表述都是错的，已全部清理。
3. **GitHub Release 自 1.2.0 起确实在分发安装程序**（§一 那句「GitHub 直连不可用」是 09-11
   那次出口的取样）。今天本机直连能到 116KB/s，但会间歇性连不上 —— 可作回退，不可当主力。

**同名坑**：macOS 两架构的产物文件名都是 `dsh-supervisor.app.tar.gz`，挂进同一个 Release 会互相覆盖。
换源换到错架构的包**不会装错**（清单里的签名对不上，验签必失败），但会把「直连失败」这种可懂的
报错变成一句验签错误 —— 所以候选源只在**文件名带架构标识**时才挂 Release。

### 落地

| 位置 | 改动 |
|---|---|
| `src-tauri/src/mirror.rs` | 新增 `SHELL_ARTIFACT_NPM_CDNS` / `SHELL_ARTIFACT_RELEASE_BASE` 与 `artifact_candidates()`：清单声明源永远第一，其后按 npm 包内同路径换主机，最后退到同名 Release 资产 |
| `src-tauri/src/commands/mod.rs` | `shell_update_apply` 按候选源逐个下载。**只有「这个源没把字节给全」**（`Network` / `Reqwest` / `Io` / 超时）才换源；验签类失败立即报出，换源掩盖它只会反复下载大包。全失败时把每个源的主机名与失败原因一起回显 |
| 门禁 | `mirror.rs` 内联单测钉候选推导与「假镜像不得回流」的 deny-list；`tests/bootstrap_flow.rs` B19 ③ 钉安装包链路确实接了候选源 |
| `tauri.conf.json` 与 `SHELL_PRESETS` | **不动**：两条端点对**清单**都成立（本轮复测 200），只是不能再宣称它们分发 `.exe` |

### 换源上线后按平台复算（同一天，线上 `1.2.0`，判据同上）

把 `artifact_candidates()` 的推导在真实清单上重跑一遍，逐候选请求前 1MB（带 `Range`，故成功格是
**206 + 1,048,576 字节**，不是 200；整包字节的判据见上表）：

| 平台 | 候选（按尝试顺序） | 实测 |
|---|---|---|
| `windows-x86_64` | unpkg → jsdelivr → Release 同名 `.exe` | 206 取到 → **403**（预期，落到下一个）→ 206 取到（本机直连 ~82KB/s） |
| `linux-x86_64` | unpkg → jsdelivr → Release 同名 `.deb` | 206 取到 → 206 → 206（~157KB/s） |
| `darwin-arm64` / `darwin-x86_64` | unpkg → jsdelivr | 均取到（unpkg 对 `darwin-x64` 那格忽略了 `Range`，直接 200 给完整 4,139,411 字节）—— **不挂 Release**：`v1.2.0` 的资产表里 `dsh-supervisor.app.tar.gz` 只有一份，两架构共用，正是同名坑（由资产清单本身证实，不是推测） |

Release 侧文件名与 npm `artifact/` 内的文件名逐字一致（`dsh-supervisor_1.2.0_x64-setup.exe`、
`dsh-supervisor_1.2.0_amd64.deb`），所以同名回退成立。

### 为什么不再往表里加源（「更多源」的天花板在哪）

剩下的可用镜像只剩**腾讯 / 华为 / npmmirror 的 `.tgz`**（本轮实测 200 且快）。要接它就得
下载 tgz → 解包 → 取出 `artifact/<file>` → **在壳侧自己验 minisign** —— 因为插件的验签发生在
`download()` 内，绕过它就等于把签名校验从插件里搬进壳：`Cargo.toml` 里
`minisign-verify` / `base64` 是**刻意只放 dev-dependencies**（验收门禁用的），运行时验签不是壳的职责，
`tauri-plugin-updater` 那条依赖注释写的「壳侧不写任何平台分支」也在这里成立。
所以 tgz 通道**不做**，除非哪天肯把「验签 + 解包」收进一个独立、可被同一门禁验的下载层 —— 那是
更新通道设计，与 deb/rpm 槽位、mac 产物改名（`<name>_<arch>.app.tar.gz`，改完 mac 也能挂 Release）同批定案。
