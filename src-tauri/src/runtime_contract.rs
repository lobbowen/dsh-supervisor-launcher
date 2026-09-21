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
    /// 与该 Node 配套的 npm **可执行程序**（可直接 spawn）。
    pub npm: PathBuf,
    /// npm 的前置参数：npm 仅有包内 JS（`node_modules/npm/bin/npm-cli.js`）时，
    ///   `npm`=node、`npm_prefix`=[npm-cli.js]；常规 npm 垫片时为空。
    pub npm_prefix: Vec<String>,
    /// Node 版本（形如 `v22.12.0`）。
    pub version: String,
    /// npm 版本（形如 `10.9.2`，来自真实执行 `npm --version`）。
    /// `None` = 本轮只解析了路径、没执行过 npm（见 `derive`）—— 不得用空串冒充「已知版本」，
    ///   否则下游（面板播报、install_done）会把「未知」显示成一个可念出去的版本号。
    pub npm_version: Option<String>,
}

impl NodeRuntime {
    /// 播报用的 npm 版本号。缺失时如实说「未回读」，**绝不**回落到 Node 版本 ——
    ///   拿 Node 版本当 npm 版本念出去正是这条链的根因；旧壳写的契约没有这个键，读回即为 None。
    pub fn npm_version_label(&self) -> String {
        self.npm_version.clone().unwrap_or_else(|| "版本未回读".into())
    }
}

/// 契约文件路径（与内核读取路径一致：<产品状态根>/supervisor/runtime.json —— 该路径由 env::supervisor_dir() 解析）。
pub fn path() -> PathBuf {
    crate::env::supervisor_dir().join("runtime.json")
}

/// npm 的候选**垫片**路径（按优先级）。与 `probe_npm` 共用，错误文案因此不可能与实际查找脱节。
/// 去重按「已出现即跳过」而不是 `Vec::dedup()`：本平台首选名（Windows 的 `npm.cmd`）在
///   候选表里未必与同名的固定项相邻，`dedup()` 只消相邻重复，会让同一条路径在文案里出现两次。
pub fn npm_shim_candidates(bin_dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = Vec::new();
    for name in [
        crate::platform::current().npm_exe_name(),
        "npm",
        "npm.cmd",
        "npm.exe",
    ] {
        let p = bin_dir.join(name);
        if !v.contains(&p) {
            v.push(p);
        }
    }
    v
}

/// npm 包内 JS 入口（Node 官方分发包同款布局）：垫片不可用时的承载方式。
pub fn npm_cli_js(bin_dir: &Path) -> PathBuf {
    bin_dir
        .join("node_modules")
        .join("npm")
        .join("bin")
        .join("npm-cli.js")
}

/// 解析 npm 可执行（**工具链契约的一部分**）。
///
/// 返回 `(program, prefix_args)`：program 可直接 spawn；npm 仅有包内 JS（或垫片在本平台
///   根本拉不起来）时 program=node、prefix=[npm-cli.js]。
/// **找不到返回 None** —— 绝不伪造一个不存在的路径（旧实现恒拼 `bin_dir/npm[.cmd]`，
///   于是「环境就绪」可以指向一个不存在的 npm）。
///
/// 为什么垫片还要过 `is_directly_spawnable`（Windows 实测根因，2026-09-21）：
///   本函数给出的 program 会被 `Command::new` 直接执行（安装内核、`npm prefix -g`），
///   而 Windows 的 CreateProcessW **执行不了 .cmd/.bat**（那是 cmd.exe 的脚本，不是 PE 程序），
///   连无扩展名的 `npm`（POSIX sh 脚本）也一样。旧实现只判 `is_file()`，探针却经
///   `cmd /C` 包装 —— 「探针能跑、真装必挂」的不对称就此形成，面板显示 npm 缺失。
///   现由同一判据收口：契约里的程序必须与消费者用**同一条 spawn 路径**。
pub fn probe_npm(node: &Path, bin_dir: &Path) -> Option<(PathBuf, Vec<String>)> {
    let plat = crate::platform::current();
    for p in npm_shim_candidates(bin_dir) {
        if p.is_file() && plat.is_directly_spawnable(&p) {
            return Some((p, vec![]));
        }
    }
    let cli = npm_cli_js(bin_dir);
    if cli.is_file() {
        return Some((node.to_path_buf(), vec![cli.display().to_string()]));
    }
    None
}

/// 「垫片存在但本平台拉不起来」的**如实**说明（供安装/面板文案，不含猜测）。
pub fn npm_unspawnable_hint() -> &'static str {
    "npm 垫片在本平台不可直接执行（CreateProcessW 只认可执行程序，.cmd/.bat/sh 脚本除外）——\
     需随 Node 一起提供 node_modules/npm/bin/npm-cli.js"
}

/// 由 Node 路径 + 版本推导 npm 路径与 bin 目录（npm 与 node 同目录）。
/// npm 缺失 → None（环境不就绪，由壳安装/修复，绝不伪造）。
///
/// 只做**路径解析**、不执行 npm：本函数服务启动路径（`ensure`），在那里执行外部进程一旦挂住
///   就把「拉起守卫」变成不可恢复的停顿。代价是 npm_version 只能留 None（不猜版本号）。
pub fn derive(node: &Path, version: &str) -> Option<NodeRuntime> {
    let node_bin_dir = node.parent()?.to_path_buf();
    let (npm, npm_prefix) = probe_npm(node, &node_bin_dir)?;
    Some(NodeRuntime {
        node: node.to_path_buf(),
        node_bin_dir,
        npm,
        npm_prefix,
        version: version.to_string(),
        npm_version: None,
    })
}

/// 「找不到可用 npm」的**如实**说明：列出真正查过的每一条路径，并区分「不存在」与
///   「在磁盘上但本平台拉不起来」。
/// 为什么必须分开：这两句话指向完全不同的处置（补装 Node / 换 node + npm-cli.js 承载），
///   把它们合成一句「npm 缺失」正是本轮 Windows 现场无从定位的原因。
pub fn npm_search_summary(bin_dir: &Path) -> String {
    let searched: Vec<String> = {
        let mut v: Vec<String> = npm_shim_candidates(bin_dir)
            .iter()
            .map(|p| p.display().to_string())
            .collect();
        v.push(npm_cli_js(bin_dir).display().to_string());
        v
    };
    let on_disk = npm_shim_candidates(bin_dir)
        .into_iter()
        .filter(|p| p.is_file())
        .count();
    // 能走到这里说明 probe_npm 已判过：有垫片在磁盘上却全部未被选中，只可能是不可直接执行。
    let why = if on_disk > 0 {
        npm_unspawnable_hint().to_string()
    } else {
        "以上路径均不存在（npm 未随本机 Node 一起提供）".to_string()
    };
    format!("已查找 {}；{}", searched.join("、"), why)
}

/// npm 的**可用性**结论（路径 + 前置参数 + 真实执行得到的版本）。
pub struct NpmUsable {
    pub path: PathBuf,
    pub args: Vec<String>,
    pub version: String,
}

/// 解析并**真实执行** npm（--version）—— 「文件存在」不等于「可用」。
///
/// 不变量 T-1b：npmOk 只有在本函数返回 Ok 时才可为 true。旧实现只 is_file()，
///   一个 0 字节 / 损坏 / 被安全软件拦截的 npm 会让 npmOk 恒 true，随后内核 npm install 必失败，
///   而用户看到的是「环境已就绪」。
///
/// 失败必须**带出原因**（Err 文案）：面板只显示「npm 缺失」时，用户与排障者都无法区分
///   「归档解残缺」「垫片拉不起来」「npm 执行报错」三类问题，而这三类的处置完全不同。
pub fn probe_npm_usable(node: &Path, bin_dir: &Path) -> Result<NpmUsable, String> {
    let (path, args) = match probe_npm(node, bin_dir) {
        Some(x) => x,
        None => return Err(npm_search_summary(bin_dir)),
    };
    let version = run_version_probe(&path, &args)
        .map_err(|why| format!("{} 执行 --version 未成功：{}", path.display(), why))?;
    Ok(NpmUsable { path, args, version })
}

/// npm 探针的时间上限（首启冷启动也够用）；超时即判不可用，绝不无限等。
const NPM_PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(8);

/// 执行 `<prog> [args...] --version` 并取首个非空行；失败原因如实返回（供面板与安装文案）。
///
/// 关键不变量（Windows 实测根因，2026-09-21）：**探针与消费者必须是同一条 spawn 路径**。
///   旧实现在平台层把 `.cmd` 经 `cmd /C` 包装后执行，于是探针报「npm 可用」，
///   而真正的消费者（core.rs 的 `npm install -g`、`npm prefix -g`）用 `Command::new(prog)`
///   直接拉起同一个 `.cmd` —— CreateProcessW 认不了它，安装内核那一步必失败。
///   包装本身是胶水：它把一个「本平台不可直接执行」的事实藏成了探针成功。
///   现由 `Platform::is_directly_spawnable` 在**选择程序**时就排除这类垫片，
///   探针因此不再需要任何平台分支（G1：平台知识只在 platform/）。
fn run_version_probe(prog: &Path, args: &[String]) -> Result<String, String> {
    let mut cmd = std::process::Command::new(prog);
    cmd.args(args).arg("--version");
    let out = crate::bounded::run(&mut cmd, NPM_PROBE_TIMEOUT).map_err(|e| format!("启动失败 {}", e))?;
    if !out.success {
        return Err(format!(
            "退出码 {}；{}",
            out.code.unwrap_or_else(|| "超时被终止".into()),
            if out.stderr.trim().is_empty() { "无 stderr" } else { out.stderr.trim() }
        ));
    }
    out.stdout
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(String::from)
        .ok_or_else(|| "执行成功但没有版本输出".to_string())
}

/// 由 Node 路径 + 版本推导**可用**的 NodeRuntime（npm 必须真实可执行，否则 None）。
pub fn derive_usable(node: &Path, version: &str) -> Option<NodeRuntime> {
    let bin_dir = node.parent()?.to_path_buf();
    let u = probe_npm_usable(node, &bin_dir).ok()?;
    Some(NodeRuntime {
        node: node.to_path_buf(),
        node_bin_dir: bin_dir,
        npm: u.path,
        npm_prefix: u.args,
        version: version.to_string(),
        npm_version: Some(u.version),
    })
}

/// 契约的 JSON 形态。**写与读共用这一处键映射**：两处各列一遍键名，历史上就出现过
///   「写了 npmArgs、读回只看 npm」那类不对称，加字段时必然漏一侧。
fn meta(rt: &NodeRuntime) -> serde_json::Value {
    let node_s = rt.node.display().to_string();
    let bin_s = rt.node_bin_dir.display().to_string();
    let npm_s = rt.npm.display().to_string();
    serde_json::json!({
        "schema": SCHEMA,
        "writtenBy": format!("dsh-supervisor-gui@{}", env!("CARGO_PKG_VERSION")),
        // ── 新键（本契约消费面）──
        "nodeBinDir": bin_s,
        "npmPath": npm_s,
        // 当 npm 只有包内 JS 时，npmPath=node、npmArgs=[npm-cli.js]（消费者必须带上 args）。
        "npmArgs": rt.npm_prefix,
        "node": { "path": node_s, "binDir": bin_s, "version": rt.version },
        // npm 版本只有在真实执行过 npm 时才有；未执行为 null（与空串严格区分）。
        "npm": { "path": npm_s, "args": rt.npm_prefix, "version": rt.npm_version },
        // ── 旧键（内核 env-catalog 已在读；不得删除）──
        "nodePath": node_s,
        "nodeVersion": rt.version,
        "minNode": crate::node::MIN_NODE,
        "source": "official-lts",
        "installedAt": crate::node::now_iso(),
        "updatedAt": crate::node::now_iso(),
    })
}

/// 契约 JSON → `NodeRuntime`（`meta` 的读回侧）。必需项缺失 → None，绝不猜路径。
fn from_meta(v: &serde_json::Value) -> Option<NodeRuntime> {
    let node = v.get("nodePath").and_then(|x| x.as_str()).map(PathBuf::from)?;
    let node_bin_dir = v
        .get("nodeBinDir")
        .and_then(|x| x.as_str())
        .map(PathBuf::from)
        .or_else(|| node.parent().map(|p| p.to_path_buf()))?;
    let npm = v
        .get("npmPath")
        .and_then(|x| x.as_str())
        .map(PathBuf::from)
        .unwrap_or_else(|| node_bin_dir.join(crate::platform::current().npm_exe_name()));
    let npm_prefix: Vec<String> = v
        .get("npmArgs")
        .and_then(|x| x.as_array())
        .map(|a| a.iter().filter_map(|s| s.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default();
    let version = v.get("nodeVersion").and_then(|x| x.as_str()).unwrap_or("").to_string();
    let npm_version = v
        .get("npm")
        .and_then(|n| n.get("version"))
        .and_then(|x| x.as_str())
        .map(String::from);
    Some(NodeRuntime {
        node,
        node_bin_dir,
        npm,
        npm_prefix,
        version,
        npm_version,
    })
}

/// 原子写契约（tmp + rename）。保留旧键供内核兼容读取。
/// 权限：契约只有路径、无机密，且所在目录已 0700 —— 顶层模块因此不做平台权限分支（G1）。
pub fn write(rt: &NodeRuntime) {
    let dir = crate::env::supervisor_dir();
    let _ = std::fs::create_dir_all(&dir);
    let p = path();
    let body = serde_json::to_string_pretty(&meta(rt)).unwrap_or_default();
    let tmp = p.with_extension("json.tmp");
    if std::fs::write(&tmp, body + "\n").is_ok() {
        // 契约不含机密（只有路径），且目录已是 0700 —— 不再做平台权限分支
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
    from_meta(&v)
}

/// 确保契约存在且指向**可执行**的 Node：先读；缺失/失效则经 nodeprobe 解析并写入。
///
/// 返回 `None` = 本机 Node 未就绪 —— 调用方如实报错，绝不猜路径（契约的意义就在于此）。
pub fn ensure() -> Option<NodeRuntime> {
    if let Some(rt) = read_node() {
        // 工具链契约：node **与** npm 都必须真实存在（旧实现只查 node → 「就绪」可指向不存在的 npm）。
        if rt.node.is_file() && rt.npm.is_file() {
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

#[cfg(test)]
mod toolchain_tests {
    //! 工具链契约行为门禁（2026-09-16）：环境就绪 = node **且** npm 真实存在；
    //!   缺失必须如实为 None，绝不伪造路径（旧实现恒拼 `bin_dir/npm[.cmd]`）。
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("dsh-npmprobe-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// 本平台**可直接 spawn** 的垫片候选名（Windows 为 `npm.exe`，其余为 `npm`）。
    ///
    /// 夹具必须按平台事实构造：写死 `npm` 或 `npm.cmd` 会让同一条断言在另一个平台
    ///   表达相反的含义（Linux 上 `npm.cmd` 是合法候选，Windows 上它拉不起来）。
    fn spawnable_shim_name() -> String {
        let name = npm_shim_candidates(Path::new("."))
            .into_iter()
            .find(|p| crate::platform::current().is_directly_spawnable(p))
            .and_then(|p| p.file_name().map(|s| s.to_string_lossy().into_owned()))
            .expect("每个平台至少要有一个可直接 spawn 的垫片候选名");
        name
    }

    #[test]
    fn probe_npm_reports_missing_instead_of_fabricating() {
        let d = tmp("missing");
        let node = d.join("node");
        std::fs::write(&node, b"").unwrap();
        assert!(probe_npm(&node, &d).is_none(), "npm 缺失时必须返回 None，绝不伪造路径");
        // 只加包内 npm-cli.js → 用同一 node 承载。
        let cli = npm_cli_js(&d);
        std::fs::create_dir_all(cli.parent().unwrap()).unwrap();
        std::fs::write(&cli, b"").unwrap();
        let got = probe_npm(&node, &d).expect("npm-cli.js 存在时应命中");
        assert_eq!(got.0, node);
        assert_eq!(got.1, vec![cli.display().to_string()]);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn probe_npm_prefers_executable_shim() {
        let d = tmp("shim");
        let node = d.join("node");
        let npm = d.join(spawnable_shim_name());
        std::fs::write(&node, b"").unwrap();
        std::fs::write(&npm, b"").unwrap();
        let got = probe_npm(&node, &d).expect("npm 垫片存在时应命中");
        assert_eq!(got.0, npm);
        assert!(got.1.is_empty());
        let _ = std::fs::remove_dir_all(&d);
    }

    /// 契约的硬不变量：`probe_npm` 给出的程序**永远**能被 `Command::new` 直接拉起。
    /// 三种布局（只垫片 / 垫片+包内 JS / 只包内 JS）逐一过一遍判据。
    /// node 夹具用本平台真实文件名：以 node 承载 npm-cli.js 时，那个 node 路径会被交出去，
    ///   写成裸 `node` 就等于在 Windows 上断言一个不存在于生产形态的名字。
    #[test]
    fn probe_npm_result_is_always_directly_spawnable() {
        let layouts = ["shim", "shim+cli", "cli"];
        for layout in layouts {
            let d = tmp(layout);
            let node = d.join(crate::platform::current().node_exe_name());
            std::fs::write(&node, b"").unwrap();
            let shim = d.join(spawnable_shim_name());
            let cli = npm_cli_js(&d);
            std::fs::create_dir_all(cli.parent().unwrap()).unwrap();
            if layout != "cli" {
                std::fs::write(&shim, b"").unwrap();
            }
            if layout != "shim" {
                std::fs::write(&cli, b"").unwrap();
            }
            let (prog, args) = probe_npm(&node, &d).unwrap_or_else(|| panic!("{} 布局应可解析出 npm", layout));
            assert!(
                crate::platform::current().is_directly_spawnable(&prog),
                "{} 布局解析出的程序不可直接 spawn: {}",
                layout,
                prog.display()
            );
            // 前置参数只允许挂在「同一目录的 node」上：args 非空却交出别的程序，
            //   消费者（npm install -g / npm prefix -g）就会把 npm-cli.js 当参数喂给错的东西。
            assert_eq!(
                !args.is_empty(),
                prog == node,
                "{} 布局：args 与程序归属不一致（{:?} / {}）",
                layout,
                args,
                prog.display()
            );
            let _ = std::fs::remove_dir_all(&d);
        }
    }

    #[test]
    fn derive_requires_npm() {
        let d = tmp("derive");
        let node = d.join("node");
        std::fs::write(&node, b"").unwrap();
        assert!(derive(&node, "v22.12.0").is_none(), "无 npm → 环境不就绪");
        std::fs::write(d.join(spawnable_shim_name()), b"").unwrap();
        assert!(derive(&node, "v22.12.0").is_some(), "有 npm → 就绪");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn probe_npm_usable_rejects_non_executable() {
        // 不变量 T-1b：文件存在 != 可用。空/不可执行的 npm 必须判为不可用。
        let d = tmp("usable");
        let node = d.join("node");
        std::fs::write(&node, b"").unwrap();
        let npm = d.join(spawnable_shim_name());
        std::fs::write(&npm, b"").unwrap();
        let why = probe_npm_usable(&node, &d).err().expect("空/不可执行的 npm 不得判为可用（T-1b）");
        assert!(why.contains("npm"), "失败原因要说清缺的是 npm：{}", why);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn contract_roundtrip_carries_npm_version() {
        // npm 版本必须能写出也能读回：它一旦在往返中丢失，安装管线就只能拿 node 版本冒充 npm 版本。
        let d = tmp("roundtrip");
        let rt = NodeRuntime {
            node: d.join("node"),
            node_bin_dir: d.clone(),
            npm: d.join(crate::platform::current().npm_exe_name()),
            npm_prefix: vec!["/x/npm-cli.js".into()],
            version: "v22.12.0".into(),
            npm_version: Some("10.9.2".into()),
        };
        let back = from_meta(&meta(&rt)).expect("契约写读必须对称");
        assert_eq!(back.npm_version.as_deref(), Some("10.9.2"));
        assert_eq!(back.npm, rt.npm);
        assert_eq!(back.npm_prefix, rt.npm_prefix);
        assert_eq!(back.node_bin_dir, rt.node_bin_dir);
        assert_eq!(back.version, rt.version);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn derive_does_not_fabricate_npm_version() {
        // 只解析路径时版本必须是 null：空串会被下游当作「已知版本号」念给用户。
        let d = tmp("noexec");
        let node = d.join("node");
        std::fs::write(&node, b"").unwrap();
        let npm = d.join(spawnable_shim_name());
        std::fs::write(&npm, b"").unwrap();
        let rt = derive(&node, "v22.12.0").expect("node 与 npm 路径齐全 → derive 成功");
        assert!(rt.npm_version.is_none(), "未执行 npm 探测不得给出版本");
        assert!(meta(&rt)["npm"]["version"].is_null(), "落盘必须是 null 而非 \"\"");
        assert!(from_meta(&meta(&rt)).unwrap().npm_version.is_none());
        let _ = std::fs::remove_dir_all(&d);
    }
}
