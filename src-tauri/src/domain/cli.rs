//! 无头自检入口（任何平台可跑，无需 GUI）。
//!
//! 这些命令是**发布后冒烟验证**与**三平台 CI 门禁**的抓手：
//! 无需图形会话即可验证镜像 / 环境 / 内核治理链路。
//!
//! 它们同时证明「壳在无 GUI 环境仍可诊断」—— 用户的机器大多没有 Xvfb，
//! 出问题时只能靠这些命令取回现场。


pub(crate) fn cli_mirror_plan() -> i32 {
    println!("== 镜像测速自检 ==");
    let m = crate::mirror::load();
    println!("Node 候选 {} 个 / npm 候选 {} 个", m.node.len(), m.npm.len());
    println!();
    println!("--- 并行测速（Node index.json）---");
    let np = crate::mirror::probe_all(&m.node, "index.json");
    for p in &np {
        println!(
            "  {:<48} {} {:>6} ms",
            p.source,
            if p.ok { "可达" } else { "不可达" },
            p.latency_ms
        );
    }
    println!();
    println!("--- 并行测速（npm registry）---");
    let pp = crate::mirror::probe_all(&m.npm, "");
    for p in &pp {
        println!(
            "  {:<48} {} {:>6} ms",
            p.source,
            if p.ok { "可达" } else { "不可达" },
            p.latency_ms
        );
    }
    println!();
    match np.iter().find(|p| p.ok) {
        Some(b) => println!("Node 选中: {} ({} ms)", b.source, b.latency_ms),
        None => println!("Node 选中: 无（全部不可达）"),
    }
    match pp.iter().find(|p| p.ok) {
        Some(b) => println!("npm  选中: {} ({} ms)", b.source, b.latency_ms),
        None => println!("npm  选中: 无（全部不可达）"),
    }
    0
}

pub(crate) fn cli_env_plan() -> i32 {
    println!("== 环境探测自检 ==");
    println!("平台          = {}", std::env::consts::OS);
    // 注意顺序：候选摘要由**探测线程**写入缓存，必须在 status() 之后再读，
    // 否则首次调用会读到空串（诊断输出出现空白，易被误读为「没有候选」）。
    let out = crate::nodeprobe::status(std::time::Duration::from_secs(60));
    println!("{}", crate::nodeprobe::candidate_summary());
    println!("完成          = {}", out.finished);
    println!("耗时          = {} ms", out.elapsed_ms);
    if let Some(e) = &out.error {
        println!("失败原因      = {}", e);
    }
    match (&out.path, &out.version) {
        (Some(p), Some(v)) => println!("node          = {} @ {}", v, p.display()),
        _ => println!("node          = （未找到）"),
    }
    if let Some((on, ms)) = crate::nodeprobe::current_stuck() {
        println!("仍在探测    = {} （已 {} ms）", on, ms);
    }
    println!("逐候选追踪:");
    print!("{}", crate::nodeprobe::render_trace(&out.trace));
    0
}

pub(crate) fn cli_plan() -> i32 {
    let out = crate::nodeprobe::status(std::time::Duration::from_secs(30));
    let sys = match (&out.path, &out.version) {
        (Some(p), Some(v)) => Some((p.clone(), v.clone())),
        _ => None,
    };
    match &sys {
        Some((p, v)) => println!("node=present {} @ {}", v, p.display()),
        None => println!("node=missing（finished={}）", out.finished),
    }
    println!("node_probe_candidates={}", crate::nodeprobe::candidate_summary());
    print!("{}", crate::nodeprobe::render_trace(&out.trace));
    match crate::node::latest_lts() {
        Ok(c) => {
            println!("latest_lts={} file={}", c.version, c.file);
            println!("mirror_selected={}", c.source);
            for (s, ok, ms) in &c.probes {
                println!("mirror_probe={} ok={} latency_ms={}", s, ok, ms);
            }
            println!("node_outdated={}", crate::node::outdated(sys.as_ref().map(|x| x.1.as_str()), &c.version));
            0
        }
        Err(e) => { eprintln!("latest_lts_error={}", e); 1 }
    }
}

/// 运行时守卫入口（`--run-guard`）：**每次启动重新检测** node + 守卫，然后执行。
///
/// 这是「服务定义只指向稳定入口」的落地：systemd/launchd/schtasks 永久指向
///   `<壳> --run-guard`，node 迁移 / 内核升级后**无需重建服务定义**。
///
/// **不触网**：只做本地检测（运行期契约 + 内核位置契约 + 候选扫描），离线也能启动；
///   线上对齐（P1）由壳在创建/启动服务前把关。
pub(crate) fn cli_run_guard() -> i32 {
    let Some((rt, guard)) = crate::domain::guardctl::resolve_local(None) else {
        eprintln!("[run-guard] 本地检测失败：未找到可用的 node 或内核守卫");
        return 1;
    };
    eprintln!("[run-guard] node={} guard={}", rt.node.display(), guard.display());
    let spec = crate::platform::LaunchSpec::from_runtime(&rt, guard);
    match crate::platform::exec_guard(&spec) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("[run-guard] {}", e);
            1
        }
    }
}
