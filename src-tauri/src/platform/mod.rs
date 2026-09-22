//! 平台适配层 —— **全仓唯一的平台分支所在地**（2026-09-11）
//!
//! 平台分支全部收敛到本层：新增平台只改本层，平台 bug 的定位不再跨 8 个文件。
//!
//! == 关键的分层违规（本层存在的直接动因）==
//!
//! 「服务定义」曾在顶层 `service.rs`（`ensure_defined`），而「服务启停」
//! (`start_guard_service` / `stop_guard_service`) 曾在 `main.rs` —— **同一个概念的 per-OS 知识分居两层**，
//! 各自带 4 份 `#[cfg]`。加一个平台要改两处**不同层**，且很容易只改一处。
//!
//! 现合并进同一个 [`service::ServiceControl`]：**定义与启停永远是同一个对象**。
//!
//!
//! ```text
//! commands ──▶ domain ──▶ platform ──▶ infra
//!                │                    ▲
//!                └────────────────────┘
//! ```
//!
//! · `platform` 可以依赖 `infra`（如 [`crate::bounded`]）；**不可**依赖 `domain`/`commands`。
//! · 平台分支只允许出现在**本目录内**（门禁 G1）。
//! · 每项能力**要么实现、要么显式 `Unsupported`**，绝不静默成功（门禁 G4）。

pub mod service;

/// 平台实现的共用超时（服务管理命令都很小，短超时足够）。
pub const SVC_QUICK: std::time::Duration = std::time::Duration::from_secs(8);
/// 服务管理命令的常规超时。
pub const SVC_NORMAL: std::time::Duration = std::time::Duration::from_secs(10);

/// 家目录（Windows 用 USERPROFILE，Unix 用 HOME）。
///
/// 放在平台层而非业务层：它是**平台事实**（环境变量名不同），
///   不是业务选择。原实现散在已删除的 `service.rs` 与 `env.rs` 各一份。
pub fn home_dir() -> std::path::PathBuf {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
}

/// 当前用户名（Windows 用 USERNAME）。
pub fn user_name() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "user".into())
}

/// 系统代理 URL（环境变量之外的**第二来源**）。
///
/// 为什么需要：Windows 用户常用 Clash / v2ray 的**系统代理**（只写 WinINET 注册表，
///   不设 HTTP_PROXY）；macOS 的「网络 → 代理」同理只写 SystemConfiguration。
///   ureq 不读这些位置，于是出现「浏览器能上网，壳却全部镜像不可用」。
/// 返回 http://host:port；无系统代理返回 None。
pub fn system_proxy() -> Option<String> {
    #[cfg(target_os = "windows")]
    let v = windows_system_proxy();
    #[cfg(target_os = "macos")]
    let v = macos_system_proxy();
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let v: Option<String> = None;
    v
}

#[cfg(target_os = "windows")]
fn windows_system_proxy() -> Option<String> {
    use std::process::Command;
    let query = |name: &str| -> Option<String> {
        // 必须经 bounded::run（B32：任何外部命令不得裸 .output()/status() 无界阻塞）。
        let mut cmd = Command::new("reg");
        cmd.args([
            "query",
            "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings",
            "/v",
            name,
        ]);
        let out = crate::bounded::run(&mut cmd, SVC_QUICK).ok()?;
        let line = out.stdout.lines().find(|l| l.contains(name))?;
        line.split_whitespace().last().map(|x| x.to_string())
    };
    let enabled = query("ProxyEnable").map(|v| v.ends_with('1')).unwrap_or(false);
    if !enabled {
        return None;
    }
    let server = query("ProxyServer")?;
    // ProxyServer 可能是 "host:port"，也可能是 "http=host:port;https=host:port"。
    let hostport = if server.contains('=') {
        server
            .split(';')
            .find_map(|p| p.split_once('='))
            .filter(|(k, _)| k.eq_ignore_ascii_case("https") || k.eq_ignore_ascii_case("http"))
            .map(|(_, v)| v.to_string())?
    } else {
        server
    };
    if hostport.trim().is_empty() {
        return None;
    }
    Some(if hostport.contains("://") { hostport } else { format!("http://{}", hostport) })
}

#[cfg(target_os = "macos")]
fn macos_system_proxy() -> Option<String> {
    use std::process::Command;
    let mut cmd = Command::new("scutil");
    cmd.arg("--proxy");
    let out = crate::bounded::run(&mut cmd, SVC_QUICK).ok()?;
    let s = out.stdout;
    let field = |k: &str| -> Option<String> {
        s.lines()
            .find(|l| l.trim_start().starts_with(k))
            .and_then(|l| l.split(':').nth(1))
            .map(|v| v.trim().to_string())
    };
    for (enable, host, port) in [
        ("HTTPSEnable", "HTTPSProxy", "HTTPSPort"),
        ("HTTPEnable", "HTTPProxy", "HTTPPort"),
    ] {
        if field(enable).as_deref() == Some("1") {
            if let Some(h) = field(host) {
                let p = field(port).unwrap_or_else(|| "80".into());
                return Some(format!("http://{}:{}", h, p));
            }
        }
    }
    None
}

/// 用户级 Node 安装的**原子落定**（三平台共用）。
///
/// 约定：调用方先把官方归档解到 `staging`。两种布局都由本函数统一处理：
///   Unix 用 `tar --strip-components=1`，staging 下直接是 bin/lib/...；
///   Windows 解包不带 strip，staging 下多一层 `node-vX-win-.../`。
/// 落定前必须验过整棵工具链（node **且** npm），落定后返回 node 可执行路径。
/// 失败不留下半装状态（root 只在工具链完整后才替换）。
pub fn commit_user_node(
    staging: &std::path::Path,
    root: &std::path::Path,
    node_rel: &[&str],
) -> Result<std::path::PathBuf, String> {
    let probe = |b: &std::path::Path| -> std::path::PathBuf {
        node_rel.iter().fold(b.to_path_buf(), |p, s| p.join(*s))
    };
    let mut base = staging.to_path_buf();
    if !probe(&base).is_file() {
        // Windows：压缩包内多一层版本目录 → 若 staging 下只有一个目录且其中含 node，以此为准。
        if let Ok(entries) = std::fs::read_dir(&base) {
            let dirs: Vec<std::path::PathBuf> = entries
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.is_dir())
                .collect();
            if dirs.len() == 1 {
                base = dirs[0].clone();
            }
        }
    }
    let found = probe(&base);
    if !found.is_file() {
        return Err(format!("解包后未找到 Node 可执行（{}）", found.display()));
    }
    // 工具链完整性：**node 在 ≠ 工具链在**。npm 必须与 node 出自同一棵解包树，判据直接取
    //   运行期契约的解析口（probe_npm）—— 这里再列一份候选就必然与契约分叉。
    //   Windows 实测根因（2026-09-21）：PowerShell 5.1 的 Expand-Archive 不处理长路径，
    //   深层 `node_modules/npm/**` 被截断而浅层的 node.exe 完好，旧实现只看 node.exe，
    //   于是「Node 已就绪」指向一棵残缺的树，面板随后如实报出 npm 缺失。
    let bin_dir = found.parent().unwrap_or(base.as_path()).to_path_buf();
    if crate::runtime_contract::probe_npm(&found, &bin_dir).is_none() {
        return Err(format!(
            "解包后的归档不含可用 npm（{}）。拒绝落定半成品。",
            crate::runtime_contract::npm_search_summary(&bin_dir)
        ));
    }
    if let Some(parent) = root.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let _ = std::fs::remove_dir_all(root);
    if let Err(e) = std::fs::rename(&base, root) {
        return Err(format!("落定 {} 失败: {}", root.display(), e));
    }
    let _ = std::fs::remove_dir_all(staging); // base != staging 时清掉剩余空壳
    let installed = probe(root);
    if !installed.is_file() {
        return Err(format!("{} 未就位", installed.display()));
    }
    Ok(installed)
}

/// 平台能力声明（供 `--platform-matrix` 自检与诊断，**不参与业务逻辑**）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct Capabilities {
    pub platform: &'static str,
    /// 是否有**原生**服务管理器（systemd / launchd / 计划任务）。
    pub native_service: bool,
    /// 是否有提权通道（**仅壳自更新/可选系统安装**需要；Node 安装已用户级、零权限）。
    pub privilege_channel: bool,
    /// Node 制品形态（`zip` / `tar.gz`，均为用户级零权限归档）。
    pub node_artifact: &'static str,
}

/// Node 官方制品的描述（**平台层解析**，业务层只消费）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeArtifact {
    /// `index.json` 的 `files[]` 标签（如 `linux-x64` / `osx-arm64-tar` / `win-x64-zip`）。
    pub tag: &'static str,
    /// 发布文件名（含版本号），如 `node-v22.12.0-linux-x64.tar.gz`。
    pub file: String,
}

/// 把**要交给外部工具的路径**规范成它们能接受的形式 —— 全仓唯一实现。
///
/// 目前唯一的规则：剥掉 Windows 的 verbatim（`\\?\`、`\\?\UNC\`）与 device（`\\.\`）
/// 命名空间前缀。`std::env::current_exe()` 与 `std::fs::canonicalize()` 都会返回这种
/// 形式，而**外部工具不一定接受**：npm 侧已实测栽在这种形态上（`--prefix` 带前缀 →
/// arborist `realpathCached` 栈溢出、以及 1.1.5 的 `EISDIR`，见
/// `tests/kernel_install_evidence_test.rs` 首部）。计划任务的动作串（`schtasks /TR`）
/// 来自同一个源头，因此它也是 Windows 启动失败的**第一个要排除项**；在收口之前，这类
/// 失败连「写进定义的路径长什么样」都看不到。
///
/// ## 为什么必须是「一处实现」（2026-09-21 B2 合并）
/// 此前有**两份规则不同的实现**：`domain/coreloc.rs`（Path 版，`UNC` 段大小写敏感、
/// 无 `\\.\`）与 `core.rs`（str 版，大小写不敏感、含 `\\.\`）。同一个 `canonicalize()`
/// 结果经两条路径可得到两个不同字符串，而「外部工具能否执行它」恰好取决于这个字符串。
/// 现取两者规则的**并集**，落在平台层（路径形态属平台事实，不是业务选择）。
///
/// ## 为什么写成**不带 `#[cfg]`** 的纯函数
/// 若写成 `#[cfg(windows)]`，POSIX CI 既编译不到也测不到这段规则 —— 那正是两份分叉
/// 实现能长期存活的原因。非 Windows 路径在这里恒等，于是三平台的 CI 都跑同一份实现。
pub fn external_path(p: &std::path::Path) -> std::path::PathBuf {
    // concat! 拼出「以反斜杠结尾」的字面量（raw string 不能以反斜杠结尾）。
    const VERBATIM: &str = concat!(r"\\?", "\\");
    const VERBATIM_UNC: &str = concat!(r"\\?\UNC", "\\");
    const DEVICE: &str = concat!(r"\\.", "\\");
    let s = p.to_string_lossy();
    // UNC 必须先判：`\\?\UNC\...` 也以 `\\?\` 开头，顺序反了会把服务器名一起剥掉。
    if s.len() >= VERBATIM_UNC.len()
        && s.is_char_boundary(VERBATIM_UNC.len())
        && s[..VERBATIM_UNC.len()].eq_ignore_ascii_case(VERBATIM_UNC)
    {
        return std::path::PathBuf::from(format!(r"\\{}", &s[VERBATIM_UNC.len()..]));
    }
    for prefix in [VERBATIM, DEVICE] {
        if let Some(rest) = s.strip_prefix(prefix) {
            return std::path::PathBuf::from(rest);
        }
    }
    p.to_path_buf()
}

/// 壳自身可执行文件的**规范路径**（全仓唯一入口）。
///
/// 为什么必须单点：三个调用点曾各自 `std::env::current_exe()`，对失败的处理**各不相同** ——
/// `LaunchSpec::from_runtime` 用 `unwrap_or_default()`，取不到时得到**空路径**并照原样写进
/// 服务定义（`"" --run-guard`），随后 schtasks `/Run` 以退出码 1 失败且无处说明原因；
/// 另两处把同一个失败咽成 `null`。真机上 Windows 的 `current_exe()` 还返回 **verbatim
/// 形式**（`\\?\C:\...`），计划任务的动作串就是这个字符串 —— `/Run` 报「找不到指定的
/// 文件」时它是第一个要排除的对象。取不到**必须报错**、取到**必须规范**：两类静默降级
/// 一次消除，`--service-plan` 也因此能把「将要写进定义的那一行」如实打出来给人看。
pub fn self_exe() -> Result<std::path::PathBuf, String> {
    let p = std::env::current_exe().map_err(|e| format!("无法取得壳自身可执行路径：{}", e))?;
    Ok(external_path(&p))
}

/// 守卫子进程的标准流：**stdin 关死，输出汇入守卫日志**。
///
/// 为什么不能再 `Stdio::null()`（`--run-guard` 兜底 spawn 与 `exec_guard` 共 3 处）：
/// 内核守卫是 node 进程，它**启动崩溃的原因只写在自己的 stderr 上**。全部丢弃后，
/// `READY_TIMEOUT` 只能报「超时」，现场没有任何正文 —— 与 B1 修掉的「丢真话」同类。
/// 日志打不开时退回 null：诊断通道不得反过来阻断启动（可用性优先，见 trait 文档）。
pub fn guard_stdio(cmd: &mut std::process::Command) {
    use std::process::Stdio;
    let (out, err) = guard_stdio_streams(crate::update::guard_log_file());
    cmd.stdin(Stdio::null()).stdout(out).stderr(err);
}

/// 「日志句柄 → 三条标准流」的**纯映射**（行为门禁的落点，避免测试依赖真实状态根）。
fn guard_stdio_streams(
    log: Option<std::fs::File>,
) -> (std::process::Stdio, std::process::Stdio) {
    use std::process::Stdio;
    match log {
        // 两条流各持一个句柄（O_APPEND 下单次写原子，两进程交叉写不会互相截断）。
        Some(f) => match f.try_clone() {
            Ok(out) => (Stdio::from(out), Stdio::from(f)),
            // 克隆失败时**保住 stderr**：崩溃原因在那条流上，stdout 只是噪声。
            Err(_) => (Stdio::null(), Stdio::from(f)),
        },
        None => (Stdio::null(), Stdio::null()),
    }
}

/// 守卫启动所需的**已解析运行期事实**（来自 `runtime_contract`，见其头部的根因说明）。
///
/// 为什么单独成类型：守卫是 `#!/usr/bin/env node` 脚本；服务管理器与 spawn 的 ambient PATH
///   经常不含"壳解析出的 Node"（nvm/fnm/volta、GUI 最小 PATH）。把 node/guard/PATH 作为
///   **显式入参**交给平台实现，服务定义不再假设 ambient 环境。
#[derive(Clone, Debug)]
pub struct LaunchSpec {
    /// Node 可执行绝对路径。
    pub node: std::path::PathBuf,
    /// 内核守卫可执行文件。
    pub guard: std::path::PathBuf,
    /// 子进程应继承的 PATH（nodeBinDir 必在首位）。
    pub env_path: String,
    /// 产品状态根：**必须**注入服务/spawn 环境（`DSH_SUPERVISOR_HOME`），
    ///   否则壳与内核各自推导状态根（XDG 环境差异）→ 契约/端口写到两个目录 → 永远拉不起来。
    pub state_root: std::path::PathBuf,
    /// 壳自身可执行文件 —— 服务定义**唯一**指向的稳定入口（--run-guard）。
    ///
    /// 架构（2026-09-15 二次修正）：服务定义**不再固化 node/guard 路径**。
    ///   定义只写 `<壳> --run-guard`；node/guard 由 --run-guard 在**每次启动时**重新检测
    ///   （node 迁移 / 内核升级自动适配）。旧做法把检测结果写进每台机器现生成的脚本
    ///   （.cmd/.ps1/unit），必然过期，且被编码/前缀/垫片细节反复咬。
    pub shell: std::path::PathBuf,
}

impl LaunchSpec {
    /// 由运行期契约 + 已定位守卫组装（PATH 经 runtime_contract 单一实现）。
    /// 状态根取壳进程解析值（单一事实源），随服务定义与 spawn 注入内核。
    ///
    /// **构造即规范化**：四个路径字段全部过 [`external_path`]（壳路径经 [`self_exe`]）。
    /// 归一必须发生在这里、而不是各平台实现里 —— 三平台的服务定义与 spawn 都从本类型
    /// 取路径，只要有一条带 verbatim 前缀漏过去，那条路径在该平台上就**永久拉不起来**，
    /// 且事后回读不到当时写进去的值（2026-09-21 Windows 真机的启动链正卡在这一段）。
    ///
    /// 返回 `Result`：壳自身路径取不到时**必须报错**，不得退化成空路径写进服务定义。
    pub fn from_runtime(
        rt: &crate::runtime_contract::NodeRuntime,
        guard: std::path::PathBuf,
    ) -> Result<Self, String> {
        Ok(LaunchSpec {
            node: external_path(&rt.node),
            guard: external_path(&guard),
            env_path: crate::runtime_contract::env_path(&rt.node_bin_dir),
            state_root: external_path(&crate::env::state_root()),
            shell: self_exe()?,
        })
    }

    /// 服务定义应执行的**稳定入口**：壳自身 + --run-guard。
    ///
    /// 不变量（门禁锁定）：三平台服务定义**不得**出现 spec.node / spec.guard。
    pub fn service_command(&self) -> (&std::path::Path, &'static [&'static str]) {
        (self.shell.as_path(), &["--run-guard"])
    }
}

/// 把稳定入口组装成一行**服务定义命令**（值内部自带引号）。
///
/// systemd 的 `ExecStart`、launchd 的 `ProgramArguments`、schtasks 的 `/TR`
///   三处对引号/数组的要求不同，但「命令 = 壳 + --run-guard」这一事实必须单源。
pub fn service_exec_line(shell: &std::path::Path, args: &[&str]) -> String {
    let mut s = format!("\"{}\"", shell.display());
    for a in args {
        s.push(' ');
        s.push_str(a);
    }
    s
}

/// 以守卫身份运行：`--run-guard` 解析出 node/guard 后调用。
///
/// · Unix：`execvp` **替换**当前进程 —— systemd/launchd 直接追踪真实 node；
/// · Windows：分离启动（不等待）—— 计划任务实例即可结束，保活由看护任务按端口负责。
///
/// 平台分支只允许在本层（门禁 G1）。
#[cfg(unix)]
pub fn exec_guard(spec: &LaunchSpec) -> Result<(), String> {
    use std::os::unix::process::CommandExt;
    let mut cmd = std::process::Command::new(&spec.node);
    cmd.arg(&spec.guard)
        .arg("daemon")
        .env("PATH", &spec.env_path)
        .env("DSH_SUPERVISOR_HOME", &spec.state_root);
    // exec 前接管标准流：日志文件句柄随 exec 保留，node 的输出即落在守卫日志里。
    guard_stdio(&mut cmd);
    Err(format!("exec 守卫失败: {}", cmd.exec()))
}

#[cfg(windows)]
pub fn exec_guard(spec: &LaunchSpec) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let is_shim = spec
        .guard
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| {
            let e = e.to_ascii_lowercase();
            e == "cmd" || e == "bat"
        })
        .unwrap_or(false);
    let mut cmd = if is_shim {
        // 规范化后的包内 JS 入口不存在时的兜底：垫片必须由 cmd 执行。
        let mut c = std::process::Command::new("cmd");
        c.arg("/C").arg(&spec.guard).arg("daemon");
        c
    } else {
        let mut c = std::process::Command::new(&spec.node);
        c.arg(&spec.guard).arg("daemon");
        c
    };
    cmd.env("PATH", &spec.env_path)
        .env("DSH_SUPERVISOR_HOME", &spec.state_root);
    // 计划任务实例退出后 node 仍在跑，其 stderr 是**唯一**的崩溃证据 → 落盘而非丢弃。
    guard_stdio(&mut cmd);
    // DETACHED_PROCESS | CREATE_NO_WINDOW：分离运行，父进程（计划任务实例）即可退出。
    cmd.creation_flags(0x0000_0008 | 0x0800_0000);
    cmd.spawn().map_err(|e| format!("启动守卫失败: {}", e))?;
    Ok(())
}

/// 平台契约。
///
/// **不变量 P1**：每个能力要么实现，要么显式 `Unsupported`（见 [`service`]）。
pub trait Platform: Send + Sync {
    /// 平台标识（`linux` / `macos` / `windows` / `unsupported`）。
    fn name(&self) -> &'static str;
    /// 服务控制（**定义 + 启停 + spawn 兜底**，同一对象）。
    fn service(&self) -> &'static dyn service::ServiceControl;
    /// 能力声明。
    fn capabilities(&self) -> Capabilities;

    /// 内核 npm 子包的**平台标签**（`linux-x64` / `darwin-arm64` / `win-x64` …）。
    ///
    /// 这是**平台事实**（OS × ARCH → 标签），必须在平台层解析；
    /// `core.rs::package_name()` 只负责拼 `@dsh-sup/dsh-core-<tag>`。
    /// 返回 `None` = 本平台/架构无对应组合（调用方如实报错，不得猜一个）。
    fn core_platform_tag(&self) -> Option<&'static str>;

    /// **Node 官方制品**：`index.json` 的 files 标签 + 发布文件名。
    ///
    /// 这是纯**制品解析**（平台 → 文件名映射），不含下载/安装逻辑 ——
    /// 正因为它「只回答平台问题」，才有资格进平台层。
    ///
    /// `version` 不带前导 `v`（如 `22.12.0`）。返回 `None` 表示本平台无可用制品。
    fn node_artifact(&self, version: &str) -> Option<NodeArtifact>;

    /// 该目录是否位于**本地固定盘**（Windows 需排除网络盘/可移动盘）。
    ///
    /// 为什么必须问平台：`Path::is_file()` / `canonicalize()` 在断开的映射盘或
    /// UNC 路径上**会触网并阻塞数十秒**，而调用点在引导的关键路径上。
    /// 判定本身（`GetDriveTypeW`）不触网。Unix 恒为 true。
    fn is_local_fixed_dir(&self, dir: &std::path::Path) -> bool;

    /// **Node 可执行文件的已知落点**（按优先级；含版本管理器布局）。
    ///
    /// 供环境探测枚举候选。业务层只消费这个列表，不关心它是 `Program Files`
    /// 还是 `/opt/homebrew` —— 那正是平台知识。
    fn node_candidate_paths(&self) -> Vec<std::path::PathBuf>;

    /// 内核可执行文件的**平台额外候选**（PATH 之外）。
    ///
    /// · Windows —— `%APPDATA%\npm`（npm 全局 bin）下的 `.cmd` 垫片，
    ///   以及包内真实脚本 `node_modules/<pkg>/bin/`；
    ///   后者不可省：`.cmd` 垫片无法被 `package_dir_of` 解析（父目录不是 `bin/`），
    ///   直接给出包内路径既能正确读 `package.json` 取版本，也能让前缀推导正常。
    /// · macOS   —— `/opt/homebrew/bin`（Apple Silicon）、`/usr/local/bin`（Intel）
    /// · Linux   —— 空（PATH 与 ~/.local/bin 已覆盖）
    ///
    /// `pkg` 为内核包名（如 `@dsh-sup/dsh-core-linux-x64`），用于 Windows 包内路径。
    fn core_extra_candidates(&self, names: &[&str], pkg: Option<&str>) -> Vec<std::path::PathBuf>;

    /// 给定 npm 全局 prefix，返回内核在该 prefix 下的候选 bin 路径（P2「安装后记录位置」用）。
    ///
    /// 默认（Unix）：`<prefix>/bin/<name>`。
    /// Windows 覆写：npm 垫片直接在 prefix 下（`<prefix>\<name>.cmd`），
    ///   以及包内真实脚本 `<prefix>\node_modules\<pkg>\bin\<name>`。
    ///
    /// 为什么需要：内核由 npm 装进 **npm 全局 prefix**（nvm/volta/fnm/自定义），
    ///   「装完立刻回读确切位置并写入 core.json」必须有跨平台的路径推导，
    ///   不能在业务层写平台分支（门禁 G1）。
    fn core_bin_candidates_in_prefix(
        &self,
        prefix: &std::path::Path,
        names: &[&str],
        _pkg: Option<&str>,
    ) -> Vec<std::path::PathBuf> {
        names.iter().map(|n| prefix.join("bin").join(n)).collect()
    }

    /// **安装后** Node 可执行文件应出现的位置（用于校验安装成功）。
    fn node_bin_after_install(&self) -> std::path::PathBuf;

    /// 该文件是否是**可用的可执行候选**。
    ///
    /// Windows 必须过滤两类伪可执行：
    ///   · `\WindowsApps\` 下的**应用执行别名存根**（执行它会挂起或唤起 Store）；
    ///   · 0 字节文件。
    /// 其它平台只判存在性。
    fn is_usable_executable(&self, cand: &std::path::Path) -> bool;

    /// 产品状态根的**平台默认**（不含 `DSH_SUPERVISOR_HOME` 覆盖；env.rs 负责覆盖）。
    ///   Linux/Unix：`$XDG_STATE_HOME/dsh-supervisor` 或 `~/.local/state/dsh-supervisor`
    ///   macOS：`~/Library/Application Support/dsh-supervisor`
    ///   Windows：`%LOCALAPPDATA%\dsh-supervisor`
    /// **独立于 DSH 的 ~/.dsh** —— 我们是管控 DSH 的产品，状态不得寄在其目录下。
    fn state_root_default(&self) -> std::path::PathBuf {
        if let Some(x) = std::env::var_os("XDG_STATE_HOME") {
            if !x.is_empty() {
                return std::path::Path::new(&x).join("dsh-supervisor");
            }
        }
        home_dir().join(".local").join("state").join("dsh-supervisor")
    }

    /// **安装 Node**（**用户级，零权限**；2026-09-18 权限模型重写）。
    ///
    /// 三平台统一：把官方归档解到 `<状态根>/node`，再原子替换。
    /// · Linux/macOS `tar -xzf <archive> -C <staging> --strip-components=1`
    /// · Windows     `powershell Expand-Archive`
    ///
    /// 为什么不再提权：系统级安装（MSI/pkg//usr/local）需要 UAC/pkexec/sudo，
    ///   而 UAC 提升到管理员账户后常读不到当前用户 profile 下的安装包（msiexec 1619）、
    ///   容器/WSL/SSH 常无可用 polkit agent 或 sudo。用户级解包在**任何**权限下都能成功。
    /// 提权只与**壳自更新**（替换安装程序）有关，由各平台自身通道完成。
    fn install_node(&self, file: &std::path::Path) -> Result<std::path::PathBuf, String>;

    /// 是否存在可用的**提权通道**（用于「不可自更新」的提前判定）。
    ///
    /// **不主动执行提权**，只探测命令存在性。
    fn has_privilege_channel(&self) -> bool;

    // ── 可执行**文件名**的平台差异（P2/G1 修复，2026-09-12）──
    //
    // 为什么这三个要进 trait：它们此前以 `cfg!()` 形式散落在 env.rs / core.rs /
    //   domain/coreloc.rs —— 而门禁 G1 只拦 `#[cfg(` **属性**，看不见 `cfg!()` **宏**，
    //   于是「平台分支只在 platform/」这条约束被绕过（三处生产代码在 platform/ 之外）。
    //
    // 语义差别也重要：`cfg!` **不做条件编译裁剪**，两个分支都参与类型检查，
    //   平台知识会随调用点扩散；`#[cfg]` 属性则按目标平台裁剪。
    //   进 trait 后由各平台文件实现，语义与 G1 的意图一致。

    /// Node 可执行**文件名**（Windows `node.exe` / 其余 `node`）。
    fn node_exe_name(&self) -> &'static str;

    /// npm 可执行**文件名**（Windows `npm.cmd` / 其余 `npm`）。
    ///
    /// 与内核侧 `platform/os/exec-path.js::npmBin()` 是**同一事实的两端**：
    ///   Windows 上 npm 是 `.cmd`，Node 的 spawn 不做 PATHEXT 解析（P1-C 已修）。
    fn npm_exe_name(&self) -> &'static str;

    /// 内核可执行文件的**候选名**（Windows 含 `.exe`/`.cmd` 垫片）。
    fn core_exe_names(&self) -> &'static [&'static str];

    /// 该路径能否作为 `Command::new(prog)` 的**程序**被直接拉起（不经任何 shell）。
    ///
    /// 为什么是平台知识：Windows 的 CreateProcessW 只执行可执行程序，`.cmd`/`.bat` 是
    ///   cmd.exe 的脚本、无扩展名的 `npm` 是 POSIX sh 脚本 —— 两者都**存在但拉不起来**
    ///   （ERROR_BAD_EXE_FORMAT）。运行期契约承诺「program 可直接 spawn」，
    ///   判据必须在这里给出，否则探针与消费者会走两条不同的 spawn 路径而分叉。
    fn is_directly_spawnable(&self, prog: &std::path::Path) -> bool;
}

// ── 平台实现的选择：**本文件是全仓唯一出现平台分支的地方**（门禁 G1）──
//
// 每个平台一个独立文件（而非同一个文件里的 #[cfg] 块）：
//   · 加一个平台 = 加一个文件 + 在下面加一行，**不碰任何既有代码**；
//   · 平台代码无法互相污染（各自的 use / 私有助手完全隔离）；
//   · 未知平台显式落到 `unsupported`，而不是编译失败或静默成功。

#[cfg(target_os = "linux")]
pub(crate) mod linux;
#[cfg(target_os = "linux")]
pub(crate) use linux as imp;

#[cfg(target_os = "macos")]
pub(crate) mod macos;
#[cfg(target_os = "macos")]
pub(crate) use macos as imp;

#[cfg(target_os = "windows")]
pub(crate) mod windows;
#[cfg(target_os = "windows")]
pub(crate) use windows as imp;

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
pub(crate) mod unsupported;
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
pub(crate) use unsupported as imp;

/// 当前平台。
pub fn current() -> &'static dyn Platform {
    imp::platform()
}

/// 当前平台的服务控制（**定义与启停的统一入口**）。
pub fn service() -> &'static dyn service::ServiceControl {
    imp::service()
}

// （`name()` 自由函数已移除：经 `current().name()` 使用 trait 方法即可，
//  保留一层多余包装只会成为 dead_code 噪声。）

/// 当前平台能力声明。
pub fn capabilities() -> Capabilities {
    current().capabilities()
}


/// 在**版本化安装目录**（形如 `v22.12.0` / `22.12.0`）中挑版本最高的一个，
/// 返回其中可执行文件的路径。
///
/// 三处版本管理器布局不同，故用 `suffix` 参数表达差异：
///   · nvm (Unix)   `<root>/<ver>/bin/node`            → suffix = ["bin", "node"]
///   · fnm (Unix)   `<root>/<ver>/installation/bin/node` → suffix = ["installation", "bin", "node"]
///   · nvm (Windows) `<root>/<ver>/node.exe`            → suffix = ["node.exe"]
///
/// 含 `read_dir`（可能落在漫游配置/慢速盘上）—— 调用方必须先 `stage()` 上报。
pub fn latest_versioned_node(root: &std::path::Path, suffix: &[&str]) -> Option<std::path::PathBuf> {
    let rd = std::fs::read_dir(root).ok()?;
    let mut best: Option<(Vec<u64>, std::path::PathBuf)> = None;
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let trimmed = name.trim_start_matches('v');
        let nums: Vec<u64> = trimmed.split('.').map_while(|x| x.parse::<u64>().ok()).collect();
        if nums.is_empty() {
            continue;
        }
        let mut full = e.path();
        for seg in suffix {
            full = full.join(seg);
        }
        let better = match &best {
            None => true,
            Some((bv, _)) => nums > *bv,
        };
        if better {
            best = Some((nums, full));
        }
    }
    best.map(|(_, p)| p)
}

/// `--platform-matrix` 自检输出（无头可跑，供三平台 CI 比对）。
///
/// 目的：把「平台矩阵」从**文档承诺**变成**可执行断言** ——
///   文档说支持某能力但代码没实现，这一输出会在 CI 上暴露。
pub fn matrix_text() -> String {
    let c = capabilities();
    // 经 trait 对象取（而非仅用自由函数）：确保 `Platform::name` / `Platform::service`
    // 这两个契约方法**真的被调用过** —— 否则它们只是「声明了但没用」，
    // 与本项目反复出现的「注释声称、代码没有」是同一类风险。
    let plat = current();
    let svc = plat.service();
    let def = svc.definition_path();
    [
        format!("platform={}", plat.name()),
        format!("platform_field={}", c.platform),
        format!("native_service={}", c.native_service),
        format!("service_kind={}", svc.kind()),
        format!("privilege_channel={}", c.privilege_channel),
        format!("node_artifact={}", c.node_artifact),
        format!("definition_path={}", def.display()),
        format!("definition_exists={}", def.is_file()),
    ]
    .join(" | ")
}

#[cfg(test)]
mod tests {
    /// 平台标签契约（2026-09-14）：`core_platform_tag()` 必须与**当前构建 target** 一致。
    /// 在 CI 的 4 个 runner（linux-x64 / win-x64 / darwin-arm64 / darwin-x64）上各跑一次，
    /// 把「OS × ARCH → 内核包标签」这条跨平台事实钉死。
    #[test]
    fn core_platform_tag_matches_build_target() {
        let got = super::current().core_platform_tag();
        let want = match (std::env::consts::OS, std::env::consts::ARCH) {
            ("linux", "x86_64") => Some("linux-x64"),
            ("linux", "aarch64") => Some("linux-arm64"),
            ("macos", "x86_64") => Some("darwin-x64"),
            ("macos", "aarch64") => Some("darwin-arm64"),
            ("windows", "x86_64") => Some("win-x64"),
            ("windows", "aarch64") => Some("win-arm64"),
            _ => None,
        };
        assert_eq!(got, want, "core_platform_tag 必须与构建 target 一致");
    }
}

#[cfg(test)]
mod launch_spec_tests {
    //! 行为门禁：服务定义只指向**稳定入口** `<壳> --run-guard`（2026-09-15 架构修正）。
    use super::*;

    fn spec_with(shell: &str) -> LaunchSpec {
        LaunchSpec {
            node: std::path::PathBuf::from("/NODE SENTINEL/node"),
            guard: std::path::PathBuf::from("/GUARD SENTINEL/guard"),
            env_path: String::new(),
            state_root: std::path::PathBuf::from("/state"),
            shell: std::path::PathBuf::from(shell),
        }
    }

    #[test]
    fn service_command_is_shell_run_guard() {
        let s = spec_with("/opt/x/dsh-supervisor");
        let (shell, args) = s.service_command();
        assert_eq!(shell, std::path::Path::new("/opt/x/dsh-supervisor"));
        assert_eq!(args, &["--run-guard"]);
    }

    #[test]
    fn service_exec_line_quotes_shell_and_has_no_volatile_paths() {
        let s = spec_with("/home/John Smith/dsh-supervisor");
        let (shell, args) = s.service_command();
        let line = service_exec_line(shell, args);
        assert_eq!(line, "\"/home/John Smith/dsh-supervisor\" --run-guard");
        assert!(!line.contains("NODE SENTINEL") && !line.contains("GUARD SENTINEL"),
            "服务定义不得含 node/guard 路径：{}", line);
    }

    // ── verbatim / device 前缀剥除：**两份分叉实现合并后的唯一用例集** ──
    //
    // 这里跑在**所有** CI 平台上（函数无 `#[cfg]`）。原来 Windows 专属的那份
    // 在 POSIX runner 上根本不参与编译，所以「规则分叉」从来没有门禁看得见。

    fn ext(s: &str) -> String {
        external_path(std::path::Path::new(s)).to_string_lossy().into_owned()
    }

    #[test]
    fn external_path_strips_verbatim_drive_and_device_prefix() {
        assert_eq!(ext(r"\\?\C:\Users\x\dsh.exe"), r"C:\Users\x\dsh.exe");
        assert_eq!(ext(r"\\.\C:\x"), r"C:\x");
    }

    #[test]
    fn external_path_strips_verbatim_unc_case_insensitively() {
        assert_eq!(ext(r"\\?\UNC\srv\share\x"), r"\\srv\share\x");
        // coreloc 版曾是大小写敏感的 —— `\\?\unc\...` 漏剥；合并后必须覆盖。
        assert_eq!(ext(r"\\?\unc\srv\share"), r"\\srv\share");
    }

    #[test]
    fn external_path_leaves_clean_and_unix_paths_untouched() {
        // 反向：干净路径**不得**被改写（否则会引入新的「路径变了」问题）。
        for p in [r"C:\Users\x", "/home/u/bin/dsh-supervisor", ""] {
            assert_eq!(ext(p), p, "不应改写: {}", p);
        }
    }

    #[test]
    fn external_path_accepts_multibyte_without_splitting_char_boundary() {
        // `is_char_boundary` 守卫的用例：前缀后紧跟多字节目录名时不得 panic
        // （原 core.rs 版用 `s.len() >= VU.len()` 已避坑，合并后必须保住这个性质）。
        let p = r"\\?\UNC\srv\中文共享\x";
        assert_eq!(ext(p), r"\\srv\中文共享\x");
        assert_eq!(ext(r"\\?\C:\用户\x"), r"C:\用户\x");
    }

    #[test]
    fn self_exe_is_normalized_and_reports_failure() {
        // 真机行为：能取到时必须是**可交给外部工具**的绝对路径（不含 verbatim 前缀）。
        let got = self_exe().expect("测试环境应能取得壳自身路径");
        assert!(got.is_absolute(), "壳路径必须是绝对路径: {}", got.display());
        assert!(
            !got.to_string_lossy().starts_with(r"\\?\"),
            "壳路径不得带 verbatim 前缀（外部工具不接受该形态，写进服务定义后失败无从归因）: {}",
            got.display()
        );
    }

    #[test]
    fn guard_streams_keep_child_output_and_fall_back_to_null() {
        // 兜底 spawn 与 exec_guard 共用 `guard_stdio` → 本函数的两条流：
        // 日志可用时 stderr **必须**指向文件，否则内核崩溃原因再次进入黑洞
        // （本轮 Windows 事故的直接后果：READY_TIMEOUT 只报「超时」，没有任何正文）。
        let dir = std::env::temp_dir().join(format!("dsh-guardio-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("guard.log"))
            .unwrap();

        // 正向按**现场**判，不按 `{:?}` 判：std 的 `Stdio` 的 Debug 恒为 `Stdio { .. }`
        //   （1.98.1 的 `impl fmt::Debug for Stdio` 用 finish_non_exhaustive），
        //   运行期从返回值里取不出「这条流最终指向 null 还是文件」——判不出来的断言就是空转。
        //   所以这里真的拉起一个子进程，看它的两条流有没有落到那个文件里。
        let (out, err) = guard_stdio_streams(Some(f));
        spawn_marker_child(out, err).wait().expect("子进程应能跑完");
        let log = std::fs::read_to_string(dir.join("guard.log")).unwrap_or_default();
        assert!(
            log.contains("dsh-mark-out") && log.contains("dsh-mark-err"),
            "日志可用时两条流都得进文件: {log:?}"
        );

        // 反向：日志不可用时**退回 null**（不是 inherit 灌进宿主，也不是让拉起整体失败）。
        //   这一半没有可观察的现场（null 与「父进程不读的管道」在小写入下都跑得完），
        //   只能钉形态：判据读的是本文件里那段实现，函数改名/分支变化都会立刻判红。
        let (out, err) = guard_stdio_streams(None);
        spawn_marker_child(out, err).wait().expect("无日志时也必须能拉起");
        let src = include_str!("mod.rs");
        let body = &src[src.find("fn guard_stdio_streams(").expect("guard_stdio_streams 已改名：同步本判据")..];
        let body = &body[..body.find("\n}\n").expect("guard_stdio_streams 边界") + 3];
        assert!(
            body.contains("None => (Stdio::null(), Stdio::null())"),
            "无日志时两条流必须退回 null（原断言用 Stdio 的 Debug 渲染判 Null，std 不渲染该字段，恒假）"
        );
        assert!(!body.contains("Stdio::inherit()"), "不得把守卫输出接到宿主进程");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 用给定的两条流拉起一个「各写一行到 stdout/stderr」的子进程。
    fn spawn_marker_child(
        out: std::process::Stdio,
        err: std::process::Stdio,
    ) -> std::process::Child {
        let mut c = std::process::Command::new(if cfg!(windows) { "cmd" } else { "/bin/sh" });
        if cfg!(windows) {
            c.args(["/C", "echo dsh-mark-out & echo dsh-mark-err>&2"]);
        } else {
            c.args(["-c", "echo dsh-mark-out; echo dsh-mark-err >&2"]);
        }
        c.stdin(std::process::Stdio::null()).stdout(out).stderr(err).spawn().expect("spawn 标记子进程")
    }
}