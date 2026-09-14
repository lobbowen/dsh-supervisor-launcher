//! 桌面壳身份与日志。
//!
//! ## 执行边界（重要）
//!
//! · 本文件只负责**桌面壳自身**的身份落盘与日志；
//! · 壳的更新是**强制**的：本文件不含任何「跳过 / 暂停 / 冷却 / 按版本拉黑 / 回退」逻辑；
//! · identity.json 由壳写；内核只读其中的运行时字段（version / phase / exe / lastSeenAt 等）。
//!
//! ## 与内核的分工
//!
//! · 内核 domains/shell/watchdog 用 identity.json 的 exe 在壳崩溃后把它拉起 —— 必须保留。

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// 壳状态目录（与内核状态目录物理隔离：内核用 ~/.dsh/supervisor）。
pub fn state_dir() -> PathBuf {
    if let Some(p) = test_state_dir_override() {
        return p;
    }
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(home).join(".dsh").join("shell")
}

/// 状态目录的测试注入点（行为级测试必须能改写到临时目录）。
#[cfg(test)]
static TEST_STATE_DIR: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);

#[cfg(test)]
fn test_state_dir_override() -> Option<PathBuf> {
    TEST_STATE_DIR.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

#[cfg(not(test))]
fn test_state_dir_override() -> Option<PathBuf> {
    None
}

fn identity_path() -> PathBuf { state_dir().join("identity.json") }
fn log_path() -> PathBuf { state_dir().join("shell.log") }

/// 落盘日志（滚动：超过 1MB 时保留后半部分）。绝不 panic、绝不阻塞启动。
pub fn log(line: &str) {
    let dir = state_dir();
    let _ = fs::create_dir_all(&dir);
    let p = log_path();
    if let Ok(md) = fs::metadata(&p) {
        if md.len() > 1024 * 1024 {
            if let Ok(s) = fs::read_to_string(&p) {
                let n = s.chars().count();
                let keep: String = s.chars().skip(n.saturating_sub(512 * 1024)).collect();
                let _ = fs::write(&p, keep);
            }
        }
    }
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(&p) {
        let _ = writeln!(f, "[{}] {}", now_secs(), line);
    }
}

/// 运行时安装形态（判断「能否自更新」与诊断用）。
pub fn install_kind() -> String {
    match tauri::utils::platform::bundle_type() {
        Some(tauri::utils::config::BundleType::Deb) => "deb",
        Some(tauri::utils::config::BundleType::Rpm) => "rpm",
        Some(tauri::utils::config::BundleType::AppImage) => "appimage",
        Some(tauri::utils::config::BundleType::Msi) => "msi",
        Some(tauri::utils::config::BundleType::Nsis) => "nsis",
        Some(tauri::utils::config::BundleType::App) => "app",
        _ => "unknown",
    }
    .to_string()
}

/// 是否存在可用的提权通道（Linux deb/rpm 更新需要）。只探测，不执行。
fn has_privilege_channel() -> bool {
    crate::platform::current().has_privilege_channel()
}

/// 能否自更新：形态受支持 且（Linux）有提权通道。
pub fn self_update_capable() -> bool {
    match install_kind().as_str() {
        "deb" | "rpm" => has_privilege_channel(),
        "appimage" | "nsis" | "msi" | "app" => true,
        _ => false,
    }
}

fn read_json(p: &Path) -> serde_json::Value {
    fs::read_to_string(p)
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .unwrap_or_else(|| serde_json::json!({}))
}

fn write_json(p: &Path, v: &serde_json::Value) {
    let dir = p.parent().unwrap_or(Path::new("."));
    let _ = fs::create_dir_all(dir);
    let tmp = p.with_extension("json.tmp");
    let body = serde_json::to_string_pretty(v).unwrap_or_default();
    if fs::write(&tmp, body).is_ok() {
        let _ = fs::rename(&tmp, p);
    }
}

/// 当前 Unix 时间（秒）。
fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 写 identity.json —— 身份文件的唯一写入点。
///
/// runtime_fields 描述本次调用的运行时上下文；其余字段从现有文件保留。
/// 本文件不写任何护栏/回退字段（拉黑、冷却、回退账本均已废除）。
fn write_identity_for(runtime_fields: &serde_json::Value) -> serde_json::Value {
    let mut v = read_json(&identity_path());
    if !v.is_object() {
        v = serde_json::json!({});
    }
    let map = v.as_object_mut().expect("identity.json must be an object");
    if let Some(rf) = runtime_fields.as_object() {
        for (k, val) in rf {
            map.insert(k.clone(), val.clone());
        }
    }
    let out = serde_json::Value::Object(map.clone());
    write_json(&identity_path(), &out);
    out
}

/// 启动时调用：写壳身份文件（仅运行时字段）。
///
/// 壳的更新强制且不可回退，故不保留任何「尝试计数 / 待确认版本」状态。
pub fn init_identity(version: &str) -> serde_json::Value {
    let kind = install_kind();
    let capable = self_update_capable();
    let now = now_secs();
    let runtime = serde_json::json!({
        "version": version,
        "platform": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "installKind": kind,
        "selfUpdateCapable": capable,
        "phase": "boot",
        "pid": std::process::id(),
        "startedAt": now,
        "lastSeenAt": now,
        "exe": std::env::current_exe().ok().map(|p| p.display().to_string()),
    });
    let id = write_identity_for(&runtime);
    log(&format!(
        "壳启动 v{} kind={} 可自更新={}",
        version, kind, capable
    ));
    id
}

/// 更新阶段上报（引导页各步骤调用）。
pub fn set_phase(phase: &str) {
    write_identity_for(&serde_json::json!({ "phase": phase }));
    log(&format!("阶段 → {}", phase));
}

pub fn identity_snapshot() -> serde_json::Value {
    read_json(&identity_path())
}

#[cfg(test)]
mod tests {
    use super::*;

    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    struct Env {
        _g: std::sync::MutexGuard<'static, ()>,
        dir: PathBuf,
    }
    impl Env {
        fn new(tag: &str) -> Self {
            let g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
            let dir = std::env::temp_dir().join(format!("dsh-shell-{}-{}", tag, std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).expect("create temp dir");
            *TEST_STATE_DIR.lock().unwrap_or_else(|e| e.into_inner()) = Some(dir.clone());
            Env { _g: g, dir }
        }
    }
    impl Drop for Env {
        fn drop(&mut self) {
            if let Ok(mut m) = TEST_STATE_DIR.lock() {
                *m = None;
            }
            let _ = fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn t1_runtime_fields_are_written() {
        let env = Env::new("runtime");
        let id = init_identity("1.2.3");
        assert_eq!(id["version"], serde_json::json!("1.2.3"));
        assert_eq!(id["phase"], serde_json::json!("boot"));
        assert!(id.get("exe").is_some(), "exe 必须存在（内核看护依赖）");
        // 回退机构已废除：护栏字段不得再写入（用 concat 避免本断言自匹配）
        assert!(id.get(concat!("at", "tempt")).is_none(), "不得存在尝试计数");
        assert!(id.get(concat!("pending", "Version")).is_none(), "不得存在待确认版本");
        assert!(env.dir.join("identity.json").exists());
    }

    #[test]
    fn t4_set_phase_preserves_runtime_fields() {
        let _env = Env::new("phase");
        init_identity("1.2.3");
        set_phase("shell-update-check");
        let id = identity_snapshot();
        assert_eq!(id["phase"], serde_json::json!("shell-update-check"));
        assert_eq!(id["version"], serde_json::json!("1.2.3"), "set_phase 不得抹掉 version");
    }

    #[test]
    fn t5_no_suppression_machinery_in_source() {
        let src = include_str!("update.rs");
        assert!(!src.contains(concat!("pin", "ned")), "不得存在按版本拉黑");
        assert!(!src.contains(concat!("cool", "down")), "不得存在冷却抑制");
        assert!(!src.contains(concat!("should_", "check")), "不得存在跳过判定");
        assert!(!src.contains(concat!("reset_", "guard")), "不得存在手动恢复旁路");
        assert!(!src.contains(concat!("at", "tempt")), "不得存在回退计数");
        assert!(!src.contains(concat!("pending", "Version")), "不得存在待确认版本字段");
    }
}
