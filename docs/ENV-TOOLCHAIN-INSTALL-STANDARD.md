# 环境工具链检测与安装规范（ENV-TOOLCHAIN-INSTALL-STANDARD）

> **本文件是「壳的环境检测 / 安装 / 下载」的唯一事实源（SSOT）**，2026-09-16 立。
> 目标：Node 与 npm **并行同权**检测与安装；全平台同一逻辑；引导页所有安装/下载**同一 UI 规范**。

---

## 1. 根因（为什么立此规范）

真机：干净机器上 npm **没有被安装**。取证：

- `src-tauri/src/main.rs::run_install` 只做 `node::install` + `node::probe_after()`（**只校验 node 版本**），
  安装完成后即报成功并 `emit env_done` —— **npm 缺失也被判定为「环境就绪」**；
- 前端 `20-env.js` 的 npm 分支（`st.npmOk === false`）再次调用**同一个** `start_node_install`，
  而该管线仍只装 node → **永远补不上 npm**（死循环式重试）；
- 各步骤（Node / 内核 / 桌面壳）**各自持有自己的进度样式**：`showProgress/hideProgress` + `#prog/#progBar`
  与 `shell_update_progress` 的纯文字风格并存 → 四分五裂。

**架构结论**：node 与 npm 必须是**同一条工具链管线**里并列的必需项，检测与安装都不得只看 node。

---

## 2. 后端契约（Rust，冻结）

### 2.1 状态查询 `node_status`（唯一环境状态读取口）

```jsonc
{
  "installed": "v22.12.0" | null,   // node 版本
  "minOk": true | false,              // node 是否达 MIN_NODE
  "minRequired": "v22.12.0",
  "npmOk": true | false,              // npm 是否可用（**与 node 同权**）
  "npmPath": "/abs/npm" | null,
  "nodePath": "/abs/node" | null,
  "busy": true | false,
  "status": "文字（供 UI 直接显示）",
  "progress": 0.0,                    // 0..1；UI 仅用于文字提示，不再画进度条
  "probeError": "..." | null,
  "stuck": {...} | null, "trace": [...]
}
```

**不变量 T-1**：`npmOk === true` 当且仅当按 `platform::npm_exe_name()`（或包内 `node_modules/npm/bin/npm-cli.js`）
能真实解析到一个**存在的** npm 可执行；不得伪造。
**不变量 T-2**：`busy === true` 期间 `installed/minOk/npmOk` 允许为中间态，UI 必须只依赖 `busy/status` 展示。

### 2.2 安装 `start_node_install`（唯一环境安装口）

**职责（顺序执行，缺一不可）**：
1. 解析并安装/修复 **node**（现有 `latest_lts → download_verified → install` 不变）；
2. 安装后**校验 npm**；若缺失 → **修复 npm**（见 §2.3）；
3. 两者都就绪才成功；任一失败 → 如实失败（绝不 emit done）。

**不变量 T-3**：成功返回时，后续 `node_status` 必须给出 `installed != null && minOk && npmOk`。
**不变量 T-4**：失败必须 `emit` 错误事件并保留可操作文案；不得静默。

### 2.3 npm 修复策略（按优先级，跨平台）

1. 官方分发包自带 npm：node 的 MSI/pkg/tar.xz 安装后，npm 通常已就位（先探测，命中即止）；
2. 未命中且存在 `<nodeBinDir>/node_modules/npm/bin/npm-cli.js` → 以 `node <npm-cli.js>` 形态可用（契约已支持 `npmArgs`）；
3. 包内 CLI 也不存在（裁剪分发/解包不完整）→ **重新执行官方安装**（幂等）后复探；
4. 仍失败 → 如实失败，文案给出「手动安装 Node 官方分发包」的指引。

**禁止**：`npm config set`、写用户 `~/.npmrc`、改全局 registry（凭据与用户环境不得被污染）。

### 2.4 统一安装事件（**三平台同一形态**）

所有安装/下载类动作（node / npm / kernel / shell）统一发**同一组事件**：

```jsonc
// 进度（文字为主，progress 仅辅助）
event "install_progress": { "kind": "node"|"npm"|"kernel"|"shell", "status": "文字", "progress": 0.0 }
event "install_done":     { "kind": ..., "version": "vX" }
event "install_error":    { "kind": ..., "error": "文字" }
```

- `kind` 是**枚举**，UI 据此决定文案前缀（不改变样式）；
- 旧事件 `env_progress` / `env_done` / `env_error` / `shell_update_progress` **一律删除**（无兼容层）。

---

## 3. 前端契约（引导页，冻结）

### 3.1 检测与安装顺序（并行同权，`20-env.js`）

```text
stepEnv 轮询 node_status
   ├─ busy            → 等待（只显示 status 文字）
   ├─ !installed      → 安装（kind=node）
   ├─ minOk === false → 安装/升级（kind=node）
   ├─ npmOk === false → 安装/修复（kind=npm）← **必须与 node 并列，不得复用 node 分支文案**
   └─ 全部通过        → stepNodeDone
```

**不变量 T-5**：npm 分支失败时**不得**继续进入内核步骤（否则会用不存在的 npm 装内核）。

### 3.2 统一 UI 规范（唯一实现，参照壳更新）

以**壳更新（`40-shell-update.js`）的纯文字风格**为基准：

- 结构：沿用既有 `steps` 步骤条 + `status` 单行文字；
- **删除进度条**：`#prog` / `#progBar` 元素与 `NS.showProgress` / `NS.hideProgress` **全部移除**；
- 所有安装/下载（node / npm / kernel / shell）**只显示文字**，形态统一为：
  `正在下载 <目标> … <进度文字>` → `正在安装 <目标> …` → `<目标> 已就绪`；
- 统一入口（`10-ui.js` 导出，**唯一实现**）：

```js
NS.install.begin(kind, text)   // 显示安装态并写文字（无进度条）
NS.install.text(kind, text)    // 仅更新文字
NS.install.done(kind, text)    // 完成文字
NS.install.fail(kind, text)    // 失败文字（走既有 fail 面板）
```

**不变量 T-6**：任何模块**不得**自行拼装下载/安装样式；一律调用 `NS.install.*`。
**不变量 T-7**：全仓不得再出现 `showProgress` / `hideProgress` / `progBar` / `#prog`（门禁强制）。

### 3.3 事件消费（`80-init.js`，唯一入口）

- `install_progress` → `NS.install.text(p.kind, p.status)`；
- `install_done` → `NS.install.done(p.kind, ...)`；
- `install_error` → `NS.install.fail(p.kind, p.error)`；
- 删除对旧事件的监听。

---

## 4. 跨平台不变量

| 项 | Linux | macOS | Windows |
|---|---|---|---|
| node 可执行名 | `node` | `node` | `node.exe` |
| npm 可执行名 | `npm` | `npm` | `npm.cmd` |
| 官方分发包 | `tar.xz` → `/usr/local` | `.pkg` → `/usr/local` | `.msi` |
| npm 兜底 | `node_modules/npm/bin/npm-cli.js` | 同左 | 同左 |
| 事件形态 | 统一 `install_*` | 同左 | 同左 |

---

## 5. 门禁

| 门禁 | 断言 |
|---|---|
| G-1 | `node_status` 含 `npmOk`/`npmPath`，且来源是真实探测（`probe_npm`） |
| G-2 | `run_install` 启动前校验 `checkEnvironment`（node + npm）/启动后校验 npm |
| G-3 | 全仓无 `showProgress`/`hideProgress`/`progBar`/`id="prog"` |
| G-4 | 全仓无旧事件名（`env_progress`/`env_done`/`env_error`/`shell_update_progress`） |
| G-5 | `20-env.js` 存在独立的 `npmOk === false` 分支且**不得**与 node 分支共用安装文案 |
| G-6 | 前端所有 install 文案经 `NS.install.*`（无自行拼装） |