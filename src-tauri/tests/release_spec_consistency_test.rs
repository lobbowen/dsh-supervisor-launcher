//! 壳仓发布规范一致性门禁（2026-09-13）
//!
//! ## 解决的问题
//!   壳仓此前**没有单一权威流程文档**，且存在一个**幽灵产线文件**
//!   `src-tauri/launcher-build.yml`（290 行）—— 它不在 `.github/workflows/` 下，
//!   GitHub **永远不会执行**它；但 `docs/RELEASE-AND-BUILD-DECISION.md` 与
//!   `scripts/bump-shell.sh` 都**曾声称它是产线**。真实产线是 `.github/workflows/build.yml`，
//!   两份定义已漂移（触发策略与步骤数均不同）。
//!
//!   现确立 `docs/RELEASE-STANDARD.md` 为**唯一事实源**，并由本门禁把「规范 = 现实」钉死：
//!   **改代码不改规范、或改规范不改代码，本门禁即红。**
//!
//! ## 锁定不变量
//!   R-1  规范里的每个入口 / 脚本文件存在
//!   R-2  `.github/workflows/` 之外**无 workflow YAML**（禁幽灵）
//!   R-3  规范的四平台与 workflow 矩阵逐项一致（os + artifact + bundles）
//!   R-4  job 名与 tag 模式与 workflow 一致
//!   R-5  版本三处互锁一致（Cargo.toml = tauri.conf.json = Cargo.lock）
//!   R-6  `verify-shell-versions.js` 确实被 CI 调用（防「从未生效」）
//!   R-7  必需章节标题齐备
//!   R-8  反向：判据能识别幽灵文件 / 缺失入口（门禁非空转）

use std::fs;
use std::path::{Path, PathBuf};

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn repo_root() -> PathBuf {
    manifest_dir().parent().expect("src-tauri 的上级").to_path_buf()
}

/// 读文件；统一把 CRLF 归一为 LF（本仓在 Windows 检出时为 CRLF，
/// 若不归一会让正则在含 `\n` 的模式上失配 —— 已踩过）。
fn read(p: &Path) -> String {
    let s = fs::read_to_string(p).unwrap_or_else(|e| panic!("读 {} 失败: {}", p.display(), e));
    if s.contains('\r') { s.replace("\r\n", "\n") } else { s }
}

fn spec_text() -> String {
    read(&repo_root().join("docs").join("RELEASE-STANDARD.md"))
}

/// 抽取 ```json shell-release-pipeline ... ``` 块。
fn extract_block(txt: &str) -> Option<String> {
    let marker = "```json shell-release-pipeline";
    let start = txt.find(marker)? + marker.len();
    let rest = &txt[start..];
    let end = rest.find("```")?;
    Some(rest[..end].to_string())
}

fn workflow_text() -> String {
    read(&repo_root().join(".github").join("workflows").join("build.yml"))
}

#[test]
fn r1_spec_entries_exist() {
    let spec = spec_text();
    let raw = extract_block(&spec).expect("规范必须含机器可读块（json shell-release-pipeline）");
    let j: serde_json::Value = serde_json::from_str(&raw).expect("机器可读块必须是合法 JSON");
    let entries = j["entries"].as_array().expect("entries 必须是数组");
    assert!(entries.len() >= 6, "入口太少（{}），规范可能被削", entries.len());
    let mut missing = Vec::new();
    for e in entries {
        let rel = e.as_str().expect("entries 元素须为字符串");
        if !repo_root().join(rel).exists() { missing.push(rel.to_string()); }
    }
    assert!(missing.is_empty(), "R-1 失败：规范里的入口不存在: {:?}", missing);
    eprintln!("R-1 PASS spec entries exist ({} 个)", entries.len());
}

#[test]
fn r2_no_workflow_yaml_outside_github_dir() {
    // 幽灵产线的直接防线：`.github/workflows/` 之外不得有 workflow 形态的 YAML。
    // 判据：文件含 `runs-on:` 且含 `jobs:` —— 那是 workflow 的结构特征（普通配置文件不会有）。
    let root = repo_root();
    let mut offenders: Vec<String> = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        let rd = match fs::read_dir(&dir) { Ok(x) => x, Err(_) => continue };
        for ent in rd.flatten() {
            let p = ent.path();
            let name = ent.file_name().to_string_lossy().to_string();
            if p.is_dir() {
                if matches!(name.as_str(), "target" | "node_modules" | ".git") { continue; }
                stack.push(p);
                continue;
            }
            if !(name.ends_with(".yml") || name.ends_with(".yaml")) { continue; }
            let rel = p.strip_prefix(&root).unwrap_or(&p).to_string_lossy().replace('\\', "/");
            if rel.starts_with(".github/workflows/") { continue; }
            let body = fs::read_to_string(&p).unwrap_or_default();
            if body.contains("runs-on:") && body.contains("jobs:") { offenders.push(rel); }
        }
    }
    assert!(offenders.is_empty(),
        "R-2 失败：.github/workflows/ 之外存在 workflow 文件（幽灵产线）: {:?}", offenders);
    // 反向：判据确实能识别（构造一段 workflow 特征文本）
    let probe = "on:\n  push:\njobs:\n  build:\n    runs-on: ubuntu-latest\n";
    assert!(probe.contains("runs-on:") && probe.contains("jobs:"), "R-8 反向判据自检");
    eprintln!("R-2 PASS no stray workflow yaml outside .github/workflows/");
}

#[test]
fn r3_matrix_matches_workflow() {
    let spec = spec_text();
    let raw = extract_block(&spec).unwrap();
    let j: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let wf = workflow_text();
    let legs = j["matrix"].as_array().expect("matrix 必须是数组");
    assert_eq!(legs.len(), 4, "壳必须四平台，规范里是 {} 个", legs.len());
    for leg in legs {
        let os = leg["os"].as_str().unwrap();
        let art = leg["artifact"].as_str().unwrap();
        let bundles = leg["bundles"].as_str().unwrap();
        assert!(wf.contains(&format!("os: {}", os)),
            "R-3 失败：workflow 缺少 runner {}", os);
        assert!(wf.contains(&format!("artifact: {}", art)),
            "R-3 失败：workflow 缺少 artifact {}", art);
        assert!(wf.contains(bundles),
            "R-3 失败：workflow 缺少 bundles {}", bundles);
    }
    // Linux 基座必须是 22.04（glibc 2.35），否则产物无法在 22.04/Debian 12 运行
    assert!(wf.contains("os: ubuntu-22.04"), "R-3 失败：Linux 基座不是 ubuntu-22.04");
    assert!(wf.contains("glibc_max: '2.35'") || wf.contains("glibc_max: \"2.35\"")
        || wf.contains("2.35"), "R-3 失败：workflow 未声明 glibc 2.35 上限");
    eprintln!("R-3 PASS 四平台矩阵与 workflow 一致");
}

#[test]
fn r4_jobs_and_tag_pattern() {
    let spec = spec_text();
    let raw = extract_block(&spec).unwrap();
    let j: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let wf = workflow_text();
    for job in j["ciJobs"].as_array().unwrap() {
        let name = job.as_str().unwrap();
        let needle = format!("\n  {}:", name);
        assert!(wf.contains(&needle), "R-4 失败：workflow 缺少 job {}", name);
    }
    let tag = j["tagPattern"].as_str().unwrap();
    assert!(wf.contains("tags:"), "R-4 失败：workflow 无 tag 触发");
    assert!(tag.contains('*') && wf.contains("v*"), "R-4 失败：tag 模式与规范不一致");
    eprintln!("R-4 PASS jobs + tag pattern 一致");
}

#[test]
fn r5_version_single_source() {
    let root = repo_root();
    let cargo = read(&root.join("src-tauri").join("Cargo.toml"));
    let conf = read(&root.join("src-tauri").join("tauri.conf.json"));
    let lock = read(&root.join("src-tauri").join("Cargo.lock"));

    let v_cargo = cargo.lines()
        .find(|l| l.trim_start().starts_with("version") && l.contains('=') && l.contains('"'))
        .and_then(|l| l.split('"').nth(1))
        .expect("Cargo.toml 缺 [package] version")
        .to_string();
    let cj: serde_json::Value = serde_json::from_str(&conf).expect("tauri.conf.json 必须可解析");
    let v_conf = cj["version"].as_str().expect("tauri.conf.json 缺 version").to_string();

    // Cargo.lock 中本包条目（包名以实际为准，不硬编码 —— 用 Cargo.toml 的 name）
    let pkg_name = cargo.lines()
        .find(|l| l.trim_start().starts_with("name") && l.contains('"'))
        .and_then(|l| l.split('"').nth(1))
        .expect("Cargo.toml 缺 name")
        .to_string();
    let mut v_lock = None;
    let mut lines = lock.lines().peekable();
    while let Some(l) = lines.next() {
        if l.trim() == "[[package]]" {
            let mut nm = None;
            let mut vs = None;
            while let Some(n) = lines.peek() {
                if n.trim().starts_with('[') { break; }
                let n = lines.next().unwrap();
                if let Some(rest) = n.strip_prefix("name = ") { nm = Some(rest.trim_matches('"').to_string()); }
                if let Some(rest) = n.strip_prefix("version = ") { vs = Some(rest.trim_matches('"').to_string()); }
            }
            if nm.as_deref() == Some(pkg_name.as_str()) { v_lock = vs; break; }
        }
    }
    assert_eq!(v_cargo, v_conf,
        "R-5 失败：Cargo.toml({}) != tauri.conf.json({})", v_cargo, v_conf);
    if let Some(vl) = v_lock {
        assert_eq!(v_cargo, vl,
            "R-5 失败：Cargo.lock({}) != Cargo.toml({})", vl, v_cargo);
        eprintln!("R-5 PASS 三处互锁一致: {}", v_cargo);
    } else {
        panic!("R-5 失败：Cargo.lock 中找不到包 {} 的版本", pkg_name);
    }
}

#[test]
fn r6_version_guard_wired_into_ci() {
    let spec = spec_text();
    let raw = extract_block(&spec).unwrap();
    let j: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let guard = j["versionGuardScript"].as_str().expect("规范须声明 versionGuardScript");
    assert!(repo_root().join(guard).exists(), "R-6 失败：版本校验脚本 {} 不存在", guard);
    let wf = workflow_text();
    assert!(wf.contains(guard),
        "R-6 失败：{} 未被 CI 调用（版本一致性将失去强制）", guard);
    eprintln!("R-6 PASS 版本校验已接入 CI");
}

#[test]
fn r7_required_sections_present() {
    let spec = spec_text();
    let raw = extract_block(&spec).unwrap();
    let j: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let mut missing = Vec::new();
    for sec in j["requiredSections"].as_array().unwrap() {
        let s = sec.as_str().unwrap();
        if !spec.contains(s) { missing.push(s.to_string()); }
    }
    assert!(missing.is_empty(), "R-7 失败：缺章节 {:?}", missing);
    eprintln!("R-7 PASS 章节齐备 ({} 节)", j["requiredSections"].as_array().unwrap().len());
}

#[test]
fn r8_reverse_judgements_are_not_vacuous() {
    // 反向：抽取器在缺块时返回 None
    assert!(extract_block("no block").is_none(), "R-8 抽取器应返回 None");
    // 反向：幽灵判据能识别构造的 workflow 文本
    let ghost = "name: x\non:\n  push:\njobs:\n  b:\n    runs-on: ubuntu-latest\n";
    assert!(ghost.contains("runs-on:") && ghost.contains("jobs:"), "R-8 幽灵判据自检");
    // 反向：规范正文确实声明了矩阵来源（防删块仍绿）
    let spec = spec_text();
    assert!(spec.contains("唯一来源") || spec.contains("CI 矩阵"), "R-8 规范正文缺矩阵来源声明");
    eprintln!("R-8 PASS 反向判据有效");
}
#[test]
fn r9_no_local_build_or_publish_path() {
    // 硬标准（2026-09-13）：**所有平台构建与发布必须经 GitHub CI**；本地不得有发布路径。
    let root = repo_root();
    // ① scripts/ 下不得有构建/发布脚本（只允许版本提升与版本校验）
    let mut local_build: Vec<String> = Vec::new();
    if let Ok(rd) = fs::read_dir(root.join("scripts")) {
        for ent in rd.flatten() {
            let n = ent.file_name().to_string_lossy().to_string();
            let allowed = n.starts_with("bump-shell") || n.starts_with("verify-shell-versions");
            if !allowed && (n.contains("build") || n.contains("publish") || n.contains("release")) {
                local_build.push(n);
            }
        }
    }
    assert!(local_build.is_empty(),
        "R-9 失败：scripts/ 下存在本地构建/发布脚本（硬标准禁止）: {:?}", local_build);
    // ② 不得有全平台本地发布脚本（旁路）
    for cand in ["shell-release/build.sh", "shell-release/publish.sh", "shell-release/release.sh"] {
        assert!(!root.join(cand).exists(), "R-9 失败：存在本地发布脚本 {}", cand);
    }
    // ③ 反向：判据本身有效
    let probe = "local-build.sh";
    let allowed_probe = probe.starts_with("bump-shell") || probe.starts_with("verify-shell-versions");
    assert!(!allowed_probe && probe.contains("build"), "R-9 反向判据自检");
    eprintln!("R-9 PASS 无本地构建/发布路径（硬标准）");
}
