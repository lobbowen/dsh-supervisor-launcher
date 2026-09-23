//! 稳定入口结构门禁（2026-09-15 架构修正）。
//!
//! 服务定义**不得**固化 node/guard 路径：三平台定义只写 `<壳> --run-guard`。
//! 行为面（路径规范化 \\?\\ + .cmd 垫片→JS、版本仲裁）在 `src/domain/coreloc.rs`
//! 的内置单元测试里直接驱动；本文件守**跨平台结构不变量**。

use std::fs;
use std::path::PathBuf;

fn read(rel: &str) -> String {
    fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel))
        .unwrap_or_else(|e| panic!("读取 {} 失败: {}", rel, e))
}

/// read_code 的纯函数内核（夹具直接喂字符串，不必落盘）。
fn read_code_from(src: &str) -> String {
    src.lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// 剥掉整行 `//` 注释后的代码。本文件的判据是**接线判据**：注释里写一遍 `--run-guard`
/// 不等于入口存在，也不等于平台定义里还残留 `spec.node`。读原文会让两类断言都失真
/// （正向恒真、反向被说明文字误判红），故统一读剥后的代码。
fn read_code(rel: &str) -> String {
    read_code_from(&read(rel))
}

/// 反空转：剥注释必须真的改变判定结果，否则 read_code 与 read 等价、上面的判据仍是空转。
#[test]
fn positive_asserts_are_backed_by_code_not_comments() {
    let only_in_comment = "// 这里只是说明：cli_run_guard 会 exec_guard --run-guard\nfn noop() {}\n";
    assert!(only_in_comment.contains("cli_run_guard"), "夹具自身失效：样本未包含 needle");
    assert!(!read_code_from(only_in_comment).contains("cli_run_guard"), "剥注释未生效：说明文字仍算接线");
}

#[test]
fn three_platforms_do_not_embed_node_guard_paths() {
    for f in ["linux.rs", "macos.rs", "windows.rs"] {
        let src = read_code(&format!("src/platform/{}", f));
        assert!(src.contains("service_command()"), "{} 未用统一稳定入口 service_command()", f);
        assert!(!src.contains("spec.node"), "{} 服务定义仍含 spec.node（应运行时检测）", f);
        assert!(!src.contains("spec.guard"), "{} 服务定义仍含 spec.guard（应运行时检测）", f);
    }
}

#[test]
fn run_guard_entry_is_wired() {
    let main = read_code("src/main.rs");
    assert!(main.contains("--run-guard"), "main 未注册 --run-guard");
    assert!(main.contains("cli_run_guard"), "main 未调用 cli_run_guard");
    let cli = read_code("src/domain/cli.rs");
    assert!(cli.contains("fn cli_run_guard"), "cli 缺 cli_run_guard");
    assert!(cli.contains("resolve_local"), "cli_run_guard 未做运行时检测");
    assert!(cli.contains("exec_guard"), "cli_run_guard 未执行 exec_guard");
}

#[test]
fn spawn_daemon_is_uniform_not_per_platform() {
    let svc = read_code("src/platform/service.rs");
    assert!(svc.contains("spec.shell") && svc.contains("--run-guard"), "spawn 兜底未统一到稳定入口");
    for f in ["linux.rs", "macos.rs", "windows.rs"] {
        let src = read_code(&format!("src/platform/{}", f));
        assert!(!src.contains("fn spawn_daemon"), "{} 仍有平台专属 spawn_daemon（应统一）", f);
    }
}

/// 工具链契约（2026-09-16）：环境就绪 = node **且** npm。
/// 干净 Windows 上「node 在、npm 缺」必须被检出并触发修复，而不是判「就绪」后用不存在的 npm 去装内核。
#[test]
fn toolchain_gate_requires_npm() {
    let rt = read_code("src/runtime_contract.rs");
    assert!(rt.contains("fn probe_npm"), "工具链契约缺 probe_npm（npm 与 node 同等必需）");
    assert!(rt.contains("npm_prefix"), "npm 仅包内 JS 时缺前缀参数承载（跨平台）");
    let cmd = read_code("src/commands/mod.rs");
    assert!(cmd.contains("npmOk"), "node_status 未回传 npmOk");
    let env_js = read_code("bootstrap/js/20-env.js");
    assert!(env_js.contains("npmOk"), "前端环境门未消费 npmOk（npm 缺失不会被修复）");
    let core = read_code("src/core.rs");
    assert!(core.contains("npm_prefix"), "npm 调用未带前缀参数（包内 JS 会 ENOENT）");
}