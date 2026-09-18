//! 开发运行时安全门禁（壳侧）—— SSOT: docs/DEVELOPMENT-TRACK.md §1（铁律 R-1/R-2/R-3）。
//!
//! ## 事故（本门禁的由来，2026-09-16）
//!   清理临时文件时执行 `rm -rf /tmp/dsh-*`，而 **DSH 自身正在用
//!   `/tmp/dsh-subprocess-<随机>/` 存放子进程输出** —— 目录被删后 DSH 写日志
//!   `ENOENT`，**崩溃退出（code=1）**，约 6 分钟后才由系统守卫重新拉起。
//!   源码开发**绝不应影响**系统正在运行的 DSH 与已安装的 supervisor。
//!
//! ## 断言
//!   R-G1  本仓脚本/构建配置不得对 /tmp 使用**通配/前缀**删除
//!   R-G2  本仓脚本/构建配置不得对**系统运行时路径**做破坏性操作
//!   R-G3  反向：判据能识别真实违规样本（门禁非空转）
//!
//! 说明：本文件只**读**文件做静态分析，不执行被测脚本。

use std::fs;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// 递归收集扫描目标：scripts/（构建脚本）与 .github/（CI）+ Cargo.toml。
fn scanned_files() -> Vec<String> {
    let base = root();
    let mut out: Vec<String> = Vec::new();
    for dir in ["scripts", ".github", "ci"] {
        let d = base.join(dir);
        if !d.is_dir() {
            continue;
        }
        fn walk(dir: &Path, base: &Path, out: &mut Vec<String>) {
            let rd = match fs::read_dir(dir) {
                Ok(v) => v,
                Err(_) => return,
            };
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, base, out);
                } else if let Ok(rel) = p.strip_prefix(base) {
                    out.push(rel.to_string_lossy().replace('\\', "/"));
                }
            }
        }
        walk(&d, &base, &mut out);
    }
    out
}

/// 去掉整行注释（注释里举例说明禁令不构成违规）。
fn strip_comments(src: &str) -> String {
    src.lines()
        .filter(|l| {
            let t = l.trim();
            !(t.starts_with("//") || t.starts_with('#') || t.starts_with('*'))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// R-G1：/tmp 通配/前缀删除的判据（返回命中描述）。
fn tmp_hits(code: &str) -> Vec<&'static str> {
    let mut out = Vec::new();
    for line in code.lines() {
        let l = line.trim();
        if l.is_empty() {
            continue;
        }
        // shell 形态：rm ... /tmp/<glob>   或   rm ... /tmp/dsh-
        let is_rm = l.split_whitespace().any(|t| t == "rm" || t == "rmdir")
            || l.contains("rm -rf")
            || l.contains("rm -r ");
        if is_rm {
            if l.contains("/tmp/dsh-") {
                out.push("删除 /tmp/dsh-*（DSH 运行时目录）");
                continue;
            }
            if let Some(i) = l.find("/tmp/") {
                let tail = &l[i..];
                if tail.contains('*') || tail.contains('?') || tail.contains('[') {
                    out.push("shell 通配删除 /tmp/*");
                    continue;
                }
            }
        }
        // Rust/JS 形态：remove_dir_all/remove_file/rmSync 指向 /tmp 通配
        let destructive = l.contains("remove_dir_all") || l.contains("remove_file")
            || l.contains("rmSync") || l.contains("rmdirSync") || l.contains("unlinkSync");
        if destructive {
            if let Some(i) = l.find("/tmp/") {
                let tail = &l[i..];
                if tail.contains('*') || tail.contains('?') {
                    out.push("通配删除 /tmp/*");
                    continue;
                }
                if tail.starts_with("/tmp/dsh-") {
                    out.push("删除 /tmp/dsh-*（DSH 运行时目录）");
                }
            }
        }
    }
    out
}

/// R-G2：系统运行时路径的破坏性操作（同一行既有破坏性动词又有系统路径）。
fn sys_hits(code: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in code.lines() {
        let l = line.trim();
        if l.is_empty() {
            continue;
        }
        // 声明/只读诊断放行
        if l.starts_with("import ") || l.starts_with("use ") || l.starts_with("const ") || l.starts_with("let ") {
            continue;
        }
        let destructive = l.contains("rm -rf")
            || l.contains("rm -r ")
            || l.contains("remove_dir_all")
            || l.contains("remove_file")
            || l.contains("unlinkSync")
            || l.contains("rmSync");
        if !destructive {
            continue;
        }
        let sys_paths = [
            ".local/state/dsh-supervisor",
            "/.dsh",
            "node_modules/@dsh-sup",
        ];
        for p in sys_paths {
            if l.contains(p) {
                out.push(format!("{} :: {}", p, &l[..l.len().min(60)]));
            }
        }
    }
    out
}

#[test]
fn r_g1_no_tmp_glob_deletion() {
    let mut hits: Vec<String> = Vec::new();
    let mut scanned = 0usize;
    for rel in scanned_files() {
        let p = root().join(&rel);
        let src = match fs::read_to_string(&p) {
            Ok(v) => v,
            Err(_) => continue,
        };
        scanned += 1;
        for d in tmp_hits(&strip_comments(&src)) {
            hits.push(format!("{} :: {}", rel, d));
        }
    }
    assert!(
        hits.is_empty(),
        "R-G1 FAIL 仓内脚本对 /tmp 使用通配或前缀删除（会打崩正在运行的 DSH）：{:?}",
        hits
    );
    eprintln!("R-G1 PASS 扫描 {} 个文件，零命中", scanned);
}

#[test]
fn r_g2_no_destructive_system_path_ops() {
    let mut hits: Vec<String> = Vec::new();
    for rel in scanned_files() {
        let p = root().join(&rel);
        let src = match fs::read_to_string(&p) {
            Ok(v) => v,
            Err(_) => continue,
        };
        for d in sys_hits(&strip_comments(&src)) {
            hits.push(format!("{} :: {}", rel, d));
        }
    }
    assert!(
        hits.is_empty(),
        "R-G2 FAIL 仓内脚本对系统运行时路径做破坏性操作：{:?}",
        hits
    );
    eprintln!("R-G2 PASS 零命中");
}

#[test]
fn r_g3_detector_is_not_vacuous() {
    // 反向：判据必须能识别真实违规样本
    let bad_tmp = [
        "rm -rf /tmp/dsh-*",
        "    rm -rf /tmp/*.log",
        "remove_dir_all(\"/tmp/dsh-spill-x\")",
    ];
    for s in bad_tmp {
        assert!(
            !tmp_hits(&strip_comments(s)).is_empty(),
            "R-G3a FAIL 判据未识别违规样本: {}",
            s
        );
    }
    // 且不误伤具名/仓内路径
    let good_tmp = [
        "rm -rf ./target/gen",
        "remove_dir_all(tmp.join(\"xplat-1234\"))",
    ];
    for s in good_tmp {
        assert!(
            tmp_hits(&strip_comments(s)).is_empty(),
            "R-G3b FAIL 判据误伤合规路径: {}",
            s
        );
    }
    // 系统路径：破坏性识别、只读放行
    assert!(
        !sys_hits(&strip_comments("rm -rf ~/.local/state/dsh-supervisor/supervisor")).is_empty(),
        "R-G3c FAIL 判据未识别系统路径破坏性操作"
    );
    assert!(
        sys_hits(&strip_comments(
            "const p = \"~/.local/state/dsh-supervisor\";"
        ))
        .is_empty(),
        "R-G3d FAIL 判据误伤只读引用"
    );
    eprintln!("R-G3 PASS 判据非空转");
}
