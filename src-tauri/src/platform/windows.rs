//! Windows 平台实现（计划任务 schtasks）。
//!
//! 本文件是 Windows 的**全部**平台知识（门禁 G1）。

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::service::ServiceControl;
use super::{home_dir, Capabilities, LaunchSpec, Platform, SVC_NORMAL, SVC_QUICK};

pub const NAME: &str = "windows";
/// 计划任务名（**定义由本文件建立**；内核不再管理，见 D6）。
pub const GUARD_TASK: &str = "DSH-Supervisor";
/// 崩溃自拉的保活任务（**由壳建立**；停止守卫时须先停它）。
/// 2026-09-15：所有者从内核 `autostart.js` 收归壳（KERNEL-DAEMON-CONTRACT D6）。
pub const WATCHDOG_TASK: &str = "DSH-Supervisor-Watchdog";

/// Windows 看护脚本（PowerShell，纯文本免 cmd 转义）。
/// 语义与内核旧实现一致：API 不可达且无 dsh-supervisor 进程 → 拉起 daemon；
/// GUI 壳缺失则**独立**拉起（两个判断必须相互独立，否则「壳崩、守卫活」时壳永远不回来）。
fn watchdog_script(spec: &LaunchSpec) -> String {
    let port = crate::env::api_port();
    let node = ps_quote(&spec.node.display().to_string());
    let guard = ps_quote(&spec.guard.display().to_string());
    let gui = std::env::current_exe()
        .map(|p| ps_quote(&p.display().to_string()))
        .unwrap_or_else(|_| "''".into());
    [
        "$ErrorActionPreference = \"SilentlyContinue\"".to_string(),
        format!("$port = {};", port),
        format!("$node = {};", node),
        format!("$guard = {};", guard),
        format!("$gui = {};", gui),
        "$up = Test-NetConnection -ComputerName 127.0.0.1 -Port $port -InformationLevel Quiet -WarningAction SilentlyContinue".to_string(),
        "if (-not $up) {".to_string(),
        "  $p = @(Get-Process -Name dsh-supervisor -ErrorAction SilentlyContinue)".to_string(),
        "  if (-not $p) { Start-Process -FilePath $node -ArgumentList $guard, 'daemon' -WindowStyle Hidden }".to_string(),
        "}".to_string(),
        "$g = @(Get-Process -Name dsh-supervisor-gui -ErrorAction SilentlyContinue)".to_string(),
        "if (-not $g -and (Test-Path $gui)) { Start-Process -FilePath $gui -WindowStyle Hidden }".to_string(),
        "exit 0".to_string(),
    ].join("\r\n")
}

/// 守卫进程的可执行命令行（**不含** cmd /C 外层引号）。
///
/// Windows 上守卫是 npm 包内**无扩展名的 Node 脚本**；cmd 不能执行无扩展名文件、也不认 shebang，
///   故必须显式用契约里的 node：`"<node>" "<guard>" daemon`。
///   仅当守卫**确是** .cmd/.bat 垫片时才直接执行它（node 无法解析 cmd 脚本）。
fn guard_argv(spec: &LaunchSpec) -> String {
    // 用**原始字面量路径**：本函数只用于 `cmd /C <line>`（下方 spawn_daemon）——创建进程时命令行
    //   是 **Unicode**，cmd 不按码页解码命令行，故含空格/非 ASCII 的路径也安全。
    //   原先的 %ENV% 改写只为 `.cmd` **正文**服务（正文才受码页影响）；包装脚本已改 PowerShell BOM，不再需要。
    let is_cmd = spec
        .guard
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| { let e = e.to_ascii_lowercase(); e == "cmd" || e == "bat" })
        .unwrap_or(false);
    if is_cmd {
        format!("\"{}\" daemon", spec.guard.display())
    } else {
        format!("\"{}\" \"{}\" daemon", spec.node.display(), spec.guard.display())
    }
}

/// PowerShell 单引号字符串（内部单引号翻倍；反斜杠为字面量，无需转义）。
fn ps_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// 安装命令超时（15 分钟：下载 + msiexec + UAC 授权）。
const INSTALL_CMD_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15 * 60);

pub struct Impl;
static IMPL: Impl = Impl;

pub fn platform() -> &'static dyn Platform {
    &IMPL
}

pub fn service() -> &'static dyn ServiceControl {
    &IMPL
}

impl Platform for Impl {
    fn name(&self) -> &'static str {
        NAME
    }
    fn service(&self) -> &'static dyn ServiceControl {
        &IMPL
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            platform: NAME,
            native_service: true, // 计划任务
            privilege_channel: true, // UAC / msiexec
            node_artifact: "msi",
        }
    }

    fn core_platform_tag(&self) -> Option<&'static str> {
        match std::env::consts::ARCH {
            "x86_64" => Some("win-x64"),
            "aarch64" => Some("win-arm64"),
            _ => None,
        }
    }

    fn node_artifact(&self, version: &str) -> Option<super::NodeArtifact> {
        // 官方**没有** win-arm64-msi（files[] 只有 win-arm64-7z / win-arm64-zip）。
        // 故 arm64 Windows 也取 x64 msi —— 依赖系统的 x64 模拟执行。
        // 这是**有意为之的折中**（原生 arm64 需改用 zip 解包，当前未实现），
        // 并在本方法的文档与 --platform-matrix 输出中如实记录。
        Some(super::NodeArtifact {
            tag: "win-x64-msi",
            file: format!("node-v{}-x64.msi", version),
        })
    }

    fn node_candidate_paths(&self) -> Vec<PathBuf> {
        // 不得硬编码 C:\Program Files（2026-09-11 审计）：
        //   真实路径随**系统盘符**与**系统语言**变化（中文系统是本地化目录名），
        //   也可能装在 Program Files (x86)。故一律经环境变量推导。
        let exe = "node.exe";
        let mut v: Vec<PathBuf> = Vec::new();
        let env_dir = |var: &str, rest: &[&str]| -> Option<PathBuf> {
            std::env::var(var).ok().map(|base| {
                let mut p = PathBuf::from(base);
                for seg in rest {
                    p = p.join(seg);
                }
                p
            })
        };
        for var in ["ProgramFiles", "ProgramFiles(x86)"] {
            if let Some(p) = env_dir(var, &["nodejs", exe]) {
                v.push(p);
            }
        }
        if let Some(p) = env_dir("ProgramData", &["chocolatey", "bin", exe]) {
            v.push(p);
        }
        if let Some(p) = env_dir("LOCALAPPDATA", &["Programs", "nodejs", exe]) {
            v.push(p);
        }
        if let Some(p) = env_dir("LOCALAPPDATA", &["Volta", "bin", exe]) {
            v.push(p);
        }
        if let Some(p) = env_dir("USERPROFILE", &["scoop", "apps", "nodejs", "current", exe]) {
            v.push(p);
        }
        // nvm-windows 有三种布局：LOCALAPPDATA\nvm、APPDATA\nvm、%NVM_HOME%
        for base in ["LOCALAPPDATA", "APPDATA"] {
            if let Ok(b) = std::env::var(base) {
                if let Some(p) = super::latest_versioned_node(&PathBuf::from(&b).join("nvm"), &[exe]) {
                    v.push(p);
                }
            }
        }
        if let Ok(nh) = std::env::var("NVM_HOME") {
            if let Some(p) = super::latest_versioned_node(&PathBuf::from(&nh), &[exe]) {
                v.push(p);
            }
            v.push(PathBuf::from(&nh).join(exe));
        }
        if let Ok(link) = std::env::var("NVM_SYMLINK") {
            v.push(PathBuf::from(&link).join(exe));
        }
        v
    }

    fn node_bin_after_install(&self) -> PathBuf {
        std::env::var("ProgramFiles")
            .map(|b| PathBuf::from(b).join("nodejs").join("node.exe"))
            .unwrap_or_else(|_| PathBuf::from("C:\\Program Files\\nodejs\\node.exe"))
    }

    fn is_usable_executable(&self, cand: &Path) -> bool {
        // 过滤两类**伪可执行**：
        //   · \WindowsApps\ 下的应用执行别名存根 —— 执行它会挂起或唤起 Store；
        //   · 0 字节文件。
        if !cand.is_file() {
            return false;
        }
        let low = cand.to_string_lossy().to_ascii_lowercase();
        if low.contains("\\windowsapps\\") {
            return false;
        }
        !std::fs::metadata(cand).map(|m| m.len() == 0).unwrap_or(true)
    }

    fn install_node(&self, file: &Path) -> Result<PathBuf, String> {
        let abs = file.canonicalize().map_err(|e| e.to_string())?;
        let esc = abs.display().to_string().replace('"', "");
        let ps = format!(
            "Start-Process -FilePath msiexec -ArgumentList '/i','{}','/qn','/norestart' -Verb RunAs -Wait",
            esc
        );
        let out = crate::bounded::run(
            Command::new("powershell").args(["-NoProfile", "-Command", &ps]),
            INSTALL_CMD_TIMEOUT,
        )
        .map_err(|e| format!("无法启动 powershell: {}", e))?;
        if !out.success {
            return Err(format!(
                "Windows 安装失败（用户取消 UAC 或 msiexec 报错）: {}",
                out.stderr.trim()
            ));
        }
        Ok(self.node_bin_after_install())
    }

    fn core_extra_candidates(&self, names: &[&str], pkg: Option<&str>) -> Vec<PathBuf> {
        let mut v: Vec<PathBuf> = Vec::new();
        let Ok(appdata) = std::env::var("APPDATA") else {
            return v;
        };
        let npm_root = PathBuf::from(&appdata).join("npm");
        for name in names {
            // npm 生成的 .cmd 垫片（在 npm 根目录下）
            v.push(npm_root.join(name));
        }
        // 真实包内脚本：.cmd 垫片无法被 package_dir_of 解析（父目录不是 bin/），
        // 且执行它取版本在部分环境下会失败。直接给出包内真实路径优先命中，
        // 既能正确读 package.json 取版本，也能让 global_prefix_for 正常推导前缀。
        if let Some(p) = pkg {
            v.push(
                npm_root
                    .join("node_modules")
                    .join(p)
                    .join("bin")
                    .join("dsh-supervisor"),
            );
        }
        v
    }

    /// Windows：npm 全局垫片直接在 prefix 下（`<name>.cmd`），真实脚本在
    ///   `<prefix>\node_modules\<pkg>\bin\<name>`（后者可被 package_dir_of 正确解析版本）。
    fn core_bin_candidates_in_prefix(
        &self,
        prefix: &std::path::Path,
        names: &[&str],
        pkg: Option<&str>,
    ) -> Vec<std::path::PathBuf> {
        let mut v: Vec<std::path::PathBuf> = Vec::new();
        for name in names {
            v.push(prefix.join(format!("{}.cmd", name)));
            v.push(prefix.join(name));
        }
        if let Some(p) = pkg {
            for name in names {
                v.push(prefix.join("node_modules").join(p).join("bin").join(name));
            }
        }
        v
    }

    /// Windows 状态根惯例：%LOCALAPPDATA%\dsh-supervisor。
    fn state_root_default(&self) -> PathBuf {
        let base = std::env::var("LOCALAPPDATA")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| home_dir().join("AppData").join("Local"));
        base.join("dsh-supervisor")
    }

    fn is_local_fixed_dir(&self, dir: &Path) -> bool {
        use std::os::windows::ffi::OsStrExt;
// 先做本地固定盘判定（不触网），再访问文件系统；按盘符缓存，每盘只查一次。
        let w: Vec<u16> = dir.as_os_str().encode_wide().collect();
        // UNC（以两个反斜杠开头，ASCII 92）→ 跳过（纯字面判定，不触网）
        if w.len() >= 2 && w[0] == 92 && w[1] == 92 {
            return false;
        }
        // 无盘符（相对路径等）→ 保守放行
        if w.len() < 2 || w[1] != 58 {
            return true;
        }
        drive_is_fixed(w[0])
    }

    fn has_privilege_channel(&self) -> bool {
        // Windows 的 UAC 提权**恒可用**（msiexec -Verb RunAs）。
        true
    }

    // ── 可执行文件名的平台差异（P2/G1：原为平台层之外的 cfg!() 宏）──
    fn node_exe_name(&self) -> &'static str { "node.exe" }
    /// Windows 上 npm 是 `.cmd`；Node 的 spawn/execFileSync **不做 PATHEXT 解析** ——
    /// 与内核侧 `platform/os/exec-path.js::npmBin()` 同一事实（P1-C）。
    fn npm_exe_name(&self) -> &'static str { "npm.cmd" }
    /// Windows 内核候选：`.cmd` 垫片必须在内 —— PATH 解析只认扩展名形态。
    fn core_exe_names(&self) -> &'static [&'static str] {
        &["dsh-supervisor.exe", "dsh-supervisor.cmd", "dsh-supervisor"]
    }
}

/// Windows 看护任务的固有实现（**不属于** ServiceControl 契约：它是壳的私有辅助，
/// 由 `ensure_defined` 调用；放进 trait impl 内会触发 E0407）。
impl Impl {
    /// 壳拥有的 Windows 看护任务（D6/H5）：写 watchdog.ps1 + schtasks MINUTE。
    /// 幂等：每次 ensure_defined 都重写脚本并 `/Create /F`（覆盖语义），
    ///   故不会因守卫任务「已是最新」而被跳过。
    fn watchdog_status(&self, spec: &LaunchSpec) -> String {
        match self.ensure_watchdog(spec) {
            Ok(s) => format!("；{}", s),
            Err(e) => format!("；看护未建立（{}）", e),
        }
    }

    fn ensure_watchdog(&self, spec: &LaunchSpec) -> Result<String, String> {
        let dir = crate::env::supervisor_dir();
        std::fs::create_dir_all(&dir).map_err(|e| format!("创建状态目录失败: {}", e))?;
        let ps1 = dir.join("watchdog.ps1");
        let body = watchdog_script(spec);
        let tmp = ps1.with_extension("ps1.tmp");
        std::fs::write(&tmp, body).map_err(|e| format!("写看护脚本失败: {}", e))?;
        std::fs::rename(&tmp, &ps1).map_err(|e| format!("落盘看护脚本失败: {}", e))?;
        let tr = format!(
            "powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -File \"{}\"",
            ps1.display()
        );
        let r = crate::bounded::run(
            Command::new("schtasks").args([
                "/Create", "/TN", WATCHDOG_TASK, "/SC", "MINUTE", "/MO", "5", "/RL", "HIGHEST", "/F", "/TR", &tr,
            ]),
            SVC_NORMAL,
        )?;
        if r.success {
            Ok(format!("看护任务 {} 已建立", WATCHDOG_TASK))
        } else {
            Err(format!("schtasks 看护任务创建失败: {}", r.stderr.trim()))
        }
    }

}

impl ServiceControl for Impl {
    fn kind(&self) -> &'static str {
        "schtasks"
    }

    fn definition_path(&self) -> PathBuf {
        // 计划任务不是文件；返回标识串供日志/诊断。
        // 正因如此，**不能**用 `definition_path().is_file()` 判断「定义是否存在」
        //   （恒 false，会让 --service-plan 自检误报）—— 见下面的 is_defined 覆写。
        PathBuf::from(format!("schtasks://{}", GUARD_TASK))
    }

    /// Windows 的真实判定：`schtasks /Query` 成功即计划任务存在。
    ///
/// 覆写默认判定：以 `schtasks /Query` 成功为准（标识串用 is_file() 恒 false）。
    fn is_defined(&self) -> bool {
        matches!(
            crate::bounded::run(
                Command::new("schtasks").args(["/Query", "/TN", GUARD_TASK]),
                SVC_QUICK,
            ),
            Ok(o) if o.success
        )
    }

    /// 建立计划任务（幂等，且**包装脚本过时时自愈**）。
    ///
    /// 2026-09-12（P2）：原实现「`/Query` 成功 → 直接返回」= **只创建、永不更新**。
    ///   与 Linux unit / macOS plist 同病：模板演进后老用户永远跑旧定义。
    ///
    ///   Windows 与另两平台的区别：计划任务**本身**无法直接比对内容，
    ///   但它的动作指向我们写的 `.cmd` 包装脚本（可比对）；
    ///   故判据改为「任务存在 **且** 包装脚本内容一致」才提前返回，
    ///   否则用 `/Create /F` 强制重建（`/F` 本就是覆盖语义）。
    fn ensure_defined(&self, spec: &LaunchSpec) -> Result<String, String> {
        let task_exists = matches!(
            crate::bounded::run(
                Command::new("schtasks").args(["/Query", "/TN", GUARD_TASK]),
                SVC_QUICK,
            ),
            Ok(o) if o.success
        );
        // 计划任务的 /TR 引号转义极易出错（尤其是路径含空格与 npm 垫片）。
        // 改为写一个 **PowerShell 包装脚本**（UTF-8 BOM）再指向它 —— 与内核 watchdog.ps1 同一思路。
        //
        // 硬规则（2026-09-15 二次修复）：**彻底弃用 `.cmd`**。
        //   真机证据：`.cmd` 包装脚本确实被执行（日志有 start/exit），但命令行为
        //   「系统找不到指定的路径。」且 ERRORLEVEL=3 —— cmd 只在「命令行里某**目录**不存在」时给 3
        //   （找不到命令是 9009）。两个可能根因：
        //     ① `.cmd` 正文被 cmd 按 OEM 码页解码 → 含非 ASCII 用户名的路径乱码；
        //     ② 正文里 `%APPDATA%` 等**在计划任务环境未展开**（写脚本的壳进程 env ≠ 任务 env）。
        //   两者同源：用 cmd 读文件 + 依赖任务 env。PowerShell 脚本按 **UTF-8 BOM** 解码，
        //   路径以**字面量**写入（ps_quote 单引号，不插值、不依赖 env）→ 从根上消除两类根因。
        //   看护脚本（watchdog.ps1）早已如此且稳定，此处对齐同一形态。
        let wrapper = crate::env::supervisor_dir().join("guard-task.ps1");
        if let Some(dir) = wrapper.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("创建状态目录失败: {}", e))?;
        }
        // 清理 1.1.4 及以前遗留的 .cmd 包装脚本，避免残留误导排查。
        let _ = std::fs::remove_file(crate::env::supervisor_dir().join("guard-task.cmd"));
        let node_dir = spec.node.parent().map(|p| p.to_path_buf()).unwrap_or_default();
        let shim = {
            // 全程落日志：即使 node/guard 启动失败，guard-task.log 也留下「脚本是否执行、node 报了什么」。
            let log = crate::env::supervisor_dir().join("guard-task.log");
            let lines = [
                format!("$env:DSH_SUPERVISOR_HOME = {}", ps_quote(&spec.state_root.display().to_string())),
                format!("$env:PATH = {} + ';' + $env:PATH", ps_quote(&node_dir.display().to_string())),
                format!("$log = {}", ps_quote(&log.display().to_string())),
                "function Write-GLog([string]$m) { Add-Content -LiteralPath $log -Value $m -Encoding UTF8 }".to_string(),
                "Write-GLog \"[guard-task] start $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss')\"".to_string(),
                format!("$node = {}", ps_quote(&spec.node.display().to_string())),
                format!("$guard = {}", ps_quote(&spec.guard.display().to_string())),
                "Write-GLog \"[guard-task] node=$node guard=$guard\"".to_string(),
                "if (-not (Test-Path -LiteralPath $node)) { Write-GLog '[guard-task] NODE MISSING' }".to_string(),
                "if (-not (Test-Path -LiteralPath $guard)) { Write-GLog '[guard-task] GUARD MISSING' }".to_string(),
                "& $node $guard daemon *>> $log".to_string(),
                "Write-GLog \"[guard-task] exit $LASTEXITCODE\"".to_string(),
            ];
            let body = lines.join("\r\n") + "\r\n";
            // UTF-8 BOM：Windows PowerShell 5.1 对**无 BOM** 的脚本按 ANSI 解码 → 中文路径再次乱码。
            format!("\u{feff}{}", body)
        };
        // ── 内容比对（P2 自愈）：任务在 + 包装脚本内容一致 → 才算「已是最新」。
        //    否则继续往下走 `/Create /F` 重建（覆盖语义）。
        let wrapper_current = std::fs::read_to_string(&wrapper).ok().as_deref() == Some(shim.as_str());
        if task_exists && wrapper_current {
            // 即使守卫任务已是最新，也必须确保**壳拥有的看护任务**存在（幂等）。
            let wd = self.watchdog_status(spec);
            return Ok(format!("已存在且为最新 计划任务 {}{}", GUARD_TASK, wd));
        }
        let is_update = task_exists;
        std::fs::write(&wrapper, &shim).map_err(|e| format!("写入包装脚本失败: {}", e))?;
        // 看护任务（DSH-Supervisor-Watchdog）的所有者 = 壳（D6/H5）：随服务定义一并（重）建立。
        let wd_note = self.watchdog_status(spec);
// 创建「最高权限」任务可能被拒：权限不足时降级为普通权限任务（不彻底失败）。
        // `/TR` 现为**完整命令行**（powershell + -File "<ps1>"），而非单一脚本路径。
        //   引号仍写在**值内部**（`-File` 的参数自带引号），不给 Rust args 加引号；
        //   脚本路径含空格/用户名（如 "John Smith"）时不会被截断。与看护脚本同一形态。
        let tr_action = format!(
            "powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -File \"{}\"",
            wrapper.display()
        );
        let base = |rl: Option<&str>| {
            let mut c = Command::new("schtasks");
            c.args(["/Create", "/TN", GUARD_TASK, "/SC", "ONLOGON"]);
            if let Some(level) = rl {
                c.args(["/RL", level]);
            }
            c.args(["/F", "/TR", &tr_action]);
            c
        };

        let verb = if is_update { "已更新" } else { "已建立" };
        let first = crate::bounded::run(&mut base(Some("HIGHEST")), SVC_NORMAL)?;
        if first.success {
            return Ok(format!(
                "{} 计划任务 {}（最高权限）-> {}{}",
                verb,
                GUARD_TASK,
                wrapper.display(),
                wd_note
            ));
        }
        // 降级重试（去掉 /RL HIGHEST）
        let second = crate::bounded::run(&mut base(None), SVC_NORMAL)?;
        if second.success {
            return Ok(format!(
                "{} 计划任务 {}（普通权限，HIGHEST 被拒）-> {}{}",
                verb,
                GUARD_TASK,
                wrapper.display(),
                wd_note
            ));
        }
        Err(format!(
            "schtasks /Create 失败（含降级重试）: 首次={} / 降级={}",
            first.stderr.trim(),
            second.stderr.trim()
        ))
    }

    fn start(&self) -> Result<(), String> {
        crate::bounded::run_checked(
            Command::new("schtasks").args(["/Run", "/TN", GUARD_TASK]),
            SVC_NORMAL,
            "schtasks /Run",
        )
        .map(|_| ())
    }

    fn stop(&self) -> Result<(), String> {
        // Windows：先停 watchdog 保活任务，再终止守卫进程（否则 watchdog 会立刻重新拉起）。
        // 全部有界：退出流程也要能在服务管理器无响应时走完，否则用户会觉得「程序关不掉」。
        crate::bounded::run_lossy(
            Command::new("schtasks").args(["/End", "/TN", WATCHDOG_TASK]),
            SVC_NORMAL,
        );
        crate::bounded::run_lossy(
            Command::new("schtasks").args(["/End", "/TN", GUARD_TASK]),
            SVC_NORMAL,
        );
        // 守卫是 `node.exe`（**不是** dsh-supervisor.exe）——旧的按镜像名 taskkill 根本杀不掉它，
        //   会残留进程/锁，导致后续启动被 guard.lock 拒绝（「永远拉不起来」）。
        //   按**命令行**精确匹配 dsh-supervisor 的 node 进程再杀，绝不误杀 DSH 自身的 node。
        crate::bounded::run_lossy(
            Command::new("powershell").args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                "Get-CimInstance Win32_Process -Filter \"Name='node.exe'\" | Where-Object { $_.CommandLine -like '*dsh-supervisor*' } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force }",
            ]),
            SVC_NORMAL,
        );
        Ok(())
    }

    fn spawn_daemon(&self, spec: &LaunchSpec) -> Result<u32, String> {
        use std::os::windows::process::CommandExt;
        // 引号处理必须正确（2026-09-11 修复）：
        //   npm 全局安装的守卫是 `dsh-supervisor.cmd` 垫片，须经 `cmd /C` 启动。
        //   而旧写法 `.args(["/C", path, "daemon"])` 在**路径含空格**时会被 cmd 拆错 ——
        //   而 `%APPDATA%` 形如 `C:\Users\<用户名>\AppData\Roaming`，
        //   Windows 用户名**可以含空格**（如 "John Smith"），故此风险真实存在。
        //   症状是「守卫启动失败」，且错误信息难以解读（cmd 报路径语法错误）。
        //
        //   正确形态（cmd 的经典引号规则）：整个命令用**外层引号**包住，
        //   各参数自身再包一层 —— 即 `cmd /C ""<node>" "<guard>" daemon"`。
        //   用 raw_arg 直接给出该形式，避免 Rust 再次转义。
        // 2026-09-15 修复：guard_argv 对无扩展名的 Node 脚本**显式用 node 执行**（见其说明）。
        let line = format!("\"{}\"", guard_argv(spec));
        let dir = crate::env::supervisor_dir();
        let _ = std::fs::create_dir_all(&dir);
        // 兜底 spawn 的输出落 guard-spawn.log：失败时能看到 node/cmd 到底报了什么。
        let log_path = dir.join("guard-spawn.log");
        let mut cmd = Command::new("cmd");
        cmd.arg("/C")
            .raw_arg(line)
            .env("PATH", &spec.env_path)
            .env("DSH_SUPERVISOR_HOME", &spec.state_root)
            .stdin(Stdio::null());
        match std::fs::File::create(&log_path) {
            Ok(f) => {
                let err = f.try_clone();
                cmd.stdout(Stdio::from(f));
                match err {
                    Ok(e) => { cmd.stderr(Stdio::from(e)); }
                    Err(_) => { cmd.stderr(Stdio::null()); }
                }
            }
            Err(_) => {
                cmd.stdout(Stdio::null());
                cmd.stderr(Stdio::null());
            }
        }
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        let child = cmd.spawn().map_err(|e| format!("直接拉起守卫失败: {}", e))?;
        Ok(child.id())
    }
}

/// 盘符是否为固定磁盘（结果按盘符缓存，每个盘符最多查询一次）。
#[cfg(target_os = "windows")]
fn drive_is_fixed(letter: u16) -> bool {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static CACHE: OnceLock<Mutex<HashMap<u16, bool>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(m) = cache.lock() {
        if let Some(v) = m.get(&letter) {
            return *v;
        }
    }
    let mut root = [0u16; 4];
    root[0] = letter;
    root[1] = 58; // 冒号
    root[2] = 92; // 反斜杠
    root[3] = 0;
    extern "system" {
        fn GetDriveTypeW(lp_root_path_name: *const u16) -> u32;
    }
    // DRIVE_FIXED = 3；其余（REMOTE=4 / NO_ROOT_DIR=1 / UNKNOWN=0）一律跳过
    let fixed = unsafe { GetDriveTypeW(root.as_ptr()) } == 3;
    if let Ok(mut m) = cache.lock() {
        m.insert(letter, fixed);
    }
    fixed
}
