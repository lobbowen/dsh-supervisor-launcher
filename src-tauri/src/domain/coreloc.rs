//! 内核（`dsh-supervisor`）的定位：候选枚举 + 版本仲裁。
//!
//! 内核是 npm 全局包，落点随安装方式而异（PATH / %APPDATA%\\npm / Homebrew /
//! ~/.local/bin 软链 / 资源目录内嵌）。故**枚举全部候选**再按版本取最高 ——
//! 只认一个路径会在「装了却找不到」或「装了新版却用旧版」时出错。
//!
//! 平台差异（Windows %APPDATA% / macOS Homebrew）已下沉到 platform 层；
//!   本模块是**平台无关**的（门禁 G1）。
//!
//! 不变量 B1（有界）：is_file/canonicalize 在断开的映射盘或 UNC 上会触网 ——
//!   故先问 `platform::is_local_fixed_dir`（GetDriveTypeW 不触网）再访问文件系统。
use tauri::Manager;

use std::path::PathBuf;

/// 内核可执行候选名 —— 下沉到 trait（P2/G1：原为 `cfg!()` 宏）。
pub(crate) fn core_exe_names() -> &'static [&'static str] {
    crate::platform::current().core_exe_names()
}

/// 收集全部内核候选（去重 + 解析符号链接），供「按版本最高仲裁」使用。
/// 跨平台路径规范：
///   - PATH（crate::env::find_in_path，Windows 走 PATHEXT）
///   - Windows: %APPDATA%\npm（npm 全局 bin 目录）+ 包内真实脚本
///   - macOS:   /opt/homebrew/bin（Apple Silicon）、/usr/local/bin（Intel）
///   - Unix:    ~/.npm-global/bin、~/.local/bin（内核 install 写入的软链）
///   - 资源目录内嵌兜底（旧版过渡）
///
/// `resource_dir` 为 None 时跳过「资源目录内嵌兜底」——CLI 自检（无 AppHandle）走这条。
pub(crate) fn locate_core_candidates(resource_dir: Option<PathBuf>) -> Vec<PathBuf> {
    let home = crate::env::home();
    let mut out: Vec<PathBuf> = Vec::new();
    // 与 env.rs 的 PATH 探测同一类防护（2026-09-11 架构修复）：
    //   is_file() / canonicalize() 底层会触网 —— 在断开的映射盘或 UNC 路径上
    //   可能阻塞数十秒，而本函数在**内核定位的关键路径**上（引导页每一步都要用）。
    //   故先做「本地固定盘」判定（GetDriveTypeW 自身不触网），再访问文件系统。
    let add = |p: PathBuf, out: &mut Vec<PathBuf>| {
        if let Some(dir) = p.parent() {
            if !crate::env::is_local_fixed_dir(dir) { return; }
        }
        if !p.is_file() { return; }
        let real = std::fs::canonicalize(&p).unwrap_or(p); // 解析 ~/.local/bin 软链到包内真实路径
        if !out.contains(&real) { out.push(real); }
    };
    // ① 位置契约优先（core.json.bin）—— 安装成功后壳写入的**确切位置**。
    //    为什么必须最先：npm 全局 prefix 可能是 nvm/volta/fnm 的 node 目录或任何自定义目录，
    //    PATH 与下面两个硬编码目录都不含它；契约是唯一可靠的事实源。
    let names_owned: Vec<&str> = crate::domain::coreloc::core_exe_names().to_vec();
    if let Some(c) = crate::core_contract::read() {
        add(c.bin.clone(), &mut out);
    }
    // ② 运行期契约派生：内核由 npm 装到 node 所在 prefix，其 bin 就在 nodeBinDir。
    if let Some(rt) = crate::runtime_contract::read_node() {
        for name in &names_owned {
            add(rt.node_bin_dir.join(name), &mut out);
        }
    }
    // ③ 启发式：壳进程 PATH。
    for name in crate::domain::coreloc::core_exe_names().iter().copied() {
        if let Some(p) = crate::env::find_in_path(name) { add(p, &mut out); }
    }
    // 平台额外候选（Windows 的 %APPDATA%\npm 与包内真实脚本；macOS 的 Homebrew 落点）
    // —— 已下沉到 platform 层（2026-09-11），本文件不再出现平台分支。
    {
        let names: Vec<&str> = crate::domain::coreloc::core_exe_names().to_vec();
        let pkg = crate::core::package_name().ok();
        for p in crate::platform::current().core_extra_candidates(&names, pkg.as_deref()) {
            add(p, &mut out);
        }
    }
    for name in crate::domain::coreloc::core_exe_names().iter().copied() {
        add(home.join(".npm-global").join("bin").join(name), &mut out);
        add(home.join(".local").join("bin").join(name), &mut out);
    }
    if let Some(res) = resource_dir {
        for name in crate::domain::coreloc::core_exe_names().iter().copied() { add(res.join("bin").join(name), &mut out); }
    }
    out
}

/// 在候选集（含指定 prefix 的平台候选）中找**恰好等于 `version`** 的内核。
///
/// 用途：P2「安装成功后回读确切位置并记录 core.json」。找不到 → None：
///   调用方必须**如实报**「已安装但定位不到目标版本（安装前缀不一致）」，绝不假装成功。
pub(crate) fn locate_core_at_version(
    app: &tauri::AppHandle,
    version: &str,
    prefix: Option<&std::path::Path>,
) -> Option<PathBuf> {
    let mut cands = locate_core_candidates(app.path().resource_dir().ok());
    if let Some(p) = prefix {
        let names: Vec<&str> = core_exe_names().to_vec();
        let pkg = crate::core::package_name().ok();
        for c in crate::platform::current().core_bin_candidates_in_prefix(p, &names, pkg.as_deref()) {
            cands.push(c);
        }
    }
    cands
        .into_iter()
        .find(|c| crate::core::installed_version(c).as_deref() == Some(version))
}

/// 定位已安装内核：多候选**按版本最高**仲裁（K5 修复）——旧内核不得遮蔽新内核。
pub(crate) fn locate_core(app: &tauri::AppHandle) -> Option<PathBuf> {
    locate_core_with_version(app).map(|(p, _)| p)
}

/// 定位内核并**一并返回其版本**（避免调用方再执行一次二进制取版本）。
///
/// 仲裁规则（K5）：多候选中**按版本最高**选取 —— 旧内核不得遮蔽新内核。
/// 每个候选的版本探测都经有界执行器（10 秒上限），单个坏候选不会拖死定位。
pub(crate) fn locate_core_with_version(app: &tauri::AppHandle) -> Option<(PathBuf, String)> {
    let cands = crate::domain::coreloc::locate_core_candidates(app.path().resource_dir().ok());
    if cands.is_empty() { return None; }
    let mut best: Option<(PathBuf, String)> = None;
    for c in &cands {
        let v = crate::core::installed_version(c).unwrap_or_else(|| "0.0.0".into());
        let better = best.as_ref().map(|(_, bv)| crate::core::semver_cmp(&v, bv) > 0).unwrap_or(true);
        if better { best = Some((c.clone(), v)); }
    }
    best.or_else(|| cands.into_iter().next().map(|p| (p, "0.0.0".into())))
}
