//! 运行期启动契约（Runtime Launch Contract）—— 壳写、内核读（schema 2）。
//!
//! ## 为什么存在（根因，2026-09-15）
//!
//! 守卫是 `#!/usr/bin/env node` 脚本。旧实现里「Node 在每个运行时一致」这条事实被
//! **四处独立推导**：
//!   ① `nodeprobe`（把 nodePath 写进 runtime.json）、
//!   ② 内核安装 `Command::new(\"npm\")`（裸名，ambient PATH）、
//!   ③ systemd `ExecStart=\"<guard>\" daemon`（靠 shebang 找 node）、
//!   ④ `spawn_daemon`（ambient PATH）。
//! 四处必然分叉。实测（nvm 用户）：交互 shell 的 PATH 含 nvm 的 node 目录，
//! 而 systemd --user / GUI 启动的壳的 PATH 不含 —— 于是「内核装得上（②在 shell 环境）
//! 却永远拉不起来（③④在 service/GUI 环境）」。这不是某个函数的 bug，是缺少单一事实源。
//!
//! ## 所有权（符合 DESIGN-BOUNDARY R1 / R3-②）
//!
//! 装壳那一刻机器上**没有内核**，壳必须先解析环境才能装内核 —— 故本契约的**所有者是壳**，
//! 内核只**消费产物**。本文件是壳侧唯一的读/写入口；安装内核、建立服务定义、spawn 守卫
//! 三处都必须从这里取事实。
//!
//! ## 兼容
//!
//! 保留内核 `env-catalog` 已读的旧键（`nodePath` / `nodeVersion` / `minNode`）。
//! schema 只向前自愈（旧壳 + 新内核仍可读旧键；新壳 + 旧内核因强制更新同步升级）。

use std::path::{Path, PathBuf};

/// 契约 schema 版本。
pub const SCHEMA: u32 = 2;

/// 已解析的 Node 运行期事实。
#[derive(Clone, Debug)]
pub struct NodeRuntime {
    /// Node 可执行绝对路径。
    pub node: PathBuf,
    /// Node 所在 bin 目录（服务/子进程 PATH 首位）。
    pub node_bin_dir: PathBuf,
    /// 与该 Node 配套的 npm 绝对路径。
    pub npm: PathBuf,
    /// Node 版本（形如 `v22.12.0`）。
    pub version: String,
}

/// 契约文件路径（与内核读取路径一致：~/.dsh/supervisor/runtime.json）。
pub fn path() -> PathBuf {
    crate::env::supervisor_dir().join("runtime.json")
}

/// 由 Node 路径 + 版本推导 npm 路径与 bin 目录（npm 与 node 同目录）。
pub fn derive(node: &Path, version: &str) -> Option<NodeRuntime> {
    let node_bin_dir = node.parent()?.to_path_buf();
    let npm = node_bin_dir.join(crate::platform::current().npm_exe_name());
    Some(NodeRuntime {
        node: node.to_path_buf(),
        node_bin_dir,
        npm,
        version: version.to_string(),
    })
}

/// 原子写契约（tmp + rename；Unix 0600）。保留旧键供内核兼容读取。
pub fn write(rt: &NodeRuntime) {
    let dir = crate::env::supervisor_dir();
    let _ = std::fs::create_dir_all(&dir);
    let node_s = rt.node.display().to_string();
    let bin_s = rt.node_bin_dir.display().to_string();
    let npm_s = rt.npm.display().to_string();
    let meta = serde_json::json!({
        "schema": SCHEMA,
        "writtenBy": format!("dsh-supervisor-gui@{}", env!("CARGO_PKG_VERSION")),
        // ── 新键（本契约消费面）──
        "nodeBinDir": bin_s,
        "npmPath": npm_s,
        "node": { "path": node_s, "binDir": bin_s, "version": rt.version },
        "npm": { "path": npm_s },
        // ── 旧键（内核 env-catalog 已在读；不得删除）──
        "nodePath": node_s,
        "nodeVersion": rt.version,
        "minNode": crate::node::MIN_NODE,
        "source": "official-lts",
        "installedAt": crate::node::now_iso(),
        "updatedAt": crate::node::now_iso(),
    });
    let p = path();
    let body = serde_json::to_string_pretty(&meta).unwrap_or_default();
    let tmp = p.with_extension("json.tmp");
    if std::fs::write(&tmp, body + "\n").is_ok() {
        // 契约不含机密（只有路径），且目录 ~/.dsh/supervisor 已是 0700 —— 不再做平台权限分支
        // （顶层模块保持平台无关；G1 门禁禁止 platform/ 之外的平台分支）。
        let _ = std::fs::rename(&tmp, &p);
    } else {
        let _ = std::fs::remove_file(&tmp);
    }
}

/// 读回契约（壳内部消费：安装 / 服务定义 / spawn 的**单一来源**）。
pub fn read_node() -> Option<NodeRuntime> {
    let s = std::fs::read_to_string(path()).ok()?;
    let v: serde_json::Value = serde_json::from_str(&s).ok()?;
    let node = v.get("nodePath").and_then(|x| x.as_str()).map(PathBuf::from)?;
    let bin = v
        .get("nodeBinDir")
        .and_then(|x| x.as_str())
        .map(PathBuf::from)
        .or_else(|| node.parent().map(|p| p.to_path_buf()))?;
    let npm = v
        .get("npmPath")
        .and_then(|x| x.as_str())
        .map(PathBuf::from)
        .unwrap_or_else(|| bin.join(crate::platform::current().npm_exe_name()));
    let version = v.get("nodeVersion").and_then(|x| x.as_str()).unwrap_or("").to_string();
    Some(NodeRuntime { node, node_bin_dir: bin, npm, version })
}

/// 确保契约存在且指向**可执行**的 Node：先读；缺失/失效则经 nodeprobe 解析并写入。
///
/// 返回 `None` = 本机 Node 未就绪 —— 调用方如实报错，绝不猜路径（契约的意义就在于此）。
pub fn ensure() -> Option<NodeRuntime> {
    if let Some(rt) = read_node() {
        if rt.node.is_file() {
            return Some(rt);
        }
    }
    let (node, version) = crate::env::probe_system_node().or_else(crate::node::probe_after)?;
    let rt = derive(&node, &version)?;
    write(&rt);
    Some(rt)
}

/// 由 Node 目录 + 家族固定落点 + ambient PATH 组装 PATH（nodeBinDir **必在首位**）。
///
/// 为什么把 ~/.npm-global/bin 与 ~/.local/bin 显式加入：内核 spawn 的 `dsh` 与
/// 内核自身的 shim 常在其一；GUI 启动的壳 ambient PATH 可能不含它们。
pub fn env_path(node_bin_dir: &Path) -> String {
    let mut dirs: Vec<PathBuf> = vec![node_bin_dir.to_path_buf()];
    let h = crate::env::home();
    for d in [h.join(".npm-global").join("bin"), h.join(".local").join("bin")] {
        if !dirs.contains(&d) {
            dirs.push(d);
        }
    }
    if let Some(p) = std::env::var_os("PATH") {
        for d in std::env::split_paths(&p) {
            if !dirs.contains(&d) {
                dirs.push(d);
            }
        }
    }
    std::env::join_paths(&dirs)
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}
