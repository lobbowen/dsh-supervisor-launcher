// 内核（dsh-supervisor）版本治理 —— 引导期的强制更新。
//
// "强制"的准确含义（2026-09-16 随发布通道契约修正）：
//   · **正常路径**：只要目标版本严格高于本机，就必须更新（不跳过、不忽略）；
//   · **非降级**：正常路径**不因版本比较**而降级（否则会被陈旧 tag 带回旧版）；
//   · **回退是显式通道**：只有运维设了 dist-tags.rollback 才回退（契约 §3 第 ① 步、RC-2）。
// 故本文件不再自称"无回退"——回退是**契约的一部分**，只是它必须由显式信号触发。
//   ⚠ 2026-09-16 已收口：回退目标低于当前版本时，build_plan 依**通道信号**（via=rollback）
//     判为需动手（action=upgrade）——纯版本比较无法识别回退，故必须把选版依据一并下传
//     （latest_pick → build_plan → latestVia/isRollback）。执行体 core_apply 不做版本比较，
//     升级与回退共用同一写入路径（刻意如此：避免"升级/回退两套逻辑"分叉）。

// 跨平台规范性（本模块的全部设计依据）：
//   1) 包名按 os/arch 映射：@dsh-sup/dsh-core-<linux|darwin|win>-<x64|arm64>。
//   2) npm 可执行文件：Windows = npm.cmd，其余 = npm。
//   3) 全局前缀从「已定位内核的真实路径」反推——**绝不用 npm prefix -g**：
//      nvm/自定义 prefix 下 npm prefix -g 与内核实际安装位置可能不一致（2026-09 实证：
//      prefix 报 nvm 路径，内核却在 ~/.npm-global），直接用 npm i -g 会装到别处、
//      旧内核继续遮蔽新内核（「更新了却没生效」）。
//   4) 镜像顺序尊重内核 registry.json（mode=manual 用 manualOrigin；否则 origins；缺失用内建默认）。
//   5) **选版遵循发布通道契约**（RELEASE-CHANNEL-CONTRACT §3，见 release_channel.rs）——
//      优先信 dist-tags.rollback / canary / latest，仅在 latest 缺失时兜底取 versions 最高。
//
//      ⚠ 本文件此前写的是「取全量最高（与内核 fetchNpmLatest 同语义）……只信 latest 会导致
//        强制更新变强制降级」。**该结论已作废**，且与契约 §3 直接冲突：
//        · 「取全量最高」会**绕过通道控制**——BETA 的数字可能压过 RC/正式，正式用户被装上测试版（RC-1）；
//        · 那段注释举的实证（latest=0.1.1-BETA.1 而 max=0.1.5-BETA.7）事后查明是
//          **latest 陈旧未更新**（SEA 时代遗留），不是「有人在回退」——恰是契约 §2 第 1 条
//          「语义歧义」的活证据：latest 低于 max 有两种成因，机器不可分；
//        · 紧急回退因此改用**独立 rollback tag**（显式信号，不依赖版本比较）。
//      留此说明是为了让后来者知道「为什么不能改回全量最高」，而不是重复踩坑。

use serde_json::Value;
// 原 `use std::io::Read;` 已移除（2026-09-12）：read_log 改用 fs::read + from_utf8_lossy，
//   不再需要 Read trait（会触发 unused_imports 警告）。
use std::path::{Path, PathBuf};

/// 内建默认镜像（与内核 config.registries 同集合；registry.json 缺失时的兜底）。
// 与 mirror.rs 的 NPM_PRESETS / 内核 config.registries 保持同一集合。
// 这只是在 mirror.rs 配置损坏时的最后兜底；正常路径由 mirror::load() 提供。
const DEFAULT_ORIGINS: [&str; 6] = [
    "https://registry.npmmirror.com",
    "https://registry.npmjs.org",
    "https://repo.huaweicloud.com/repository/npm/",
    "https://mirrors.cloud.tencent.com/npm",
    "https://npmreg.proxy.ustclug.org",
    "https://r.cnpmjs.org",
];

/// 平台 → npm 子包名（唯一真源；错误提示/安装/查询共用，杜绝散落硬编码）。
pub fn package_name() -> Result<String, String> {
    // 平台标签是**平台事实**，只在 platform 层解析（门禁 G1）；
    // 这里只负责拼包名，不得再出现 std::env::consts 的平台分支。
    let tag = crate::platform::current()
        .core_platform_tag()
        .ok_or_else(|| "当前平台/架构无对应的内核发布包".to_string())?;
    Ok(format!("@dsh-sup/dsh-core-{}", tag))
}

/// npm 可执行名（Windows 需 .cmd 后缀）—— 下沉到 trait（P2/G1）。
pub fn npm_exe() -> &'static str {
    crate::platform::current().npm_exe_name()
}

fn num_ok(s: &str) -> bool {
    if s.is_empty() { return false; }
    if s.len() > 1 && s.starts_with('0') { return false; } // 禁止前导零（对齐 semver）
    s.chars().all(|c| c.is_ascii_digit())
}

/// 版本字面量合法性：`X.Y.Z[-pre][+build]`。
///
/// 按 semver：build 段须匹配 `[0-9A-Za-z-]+(\.[0-9A-Za-z-]+)*`。
///
/// 行为规格由 `shell-release/version-vectors.json` 锁定（内核侧有同一份，
/// 两侧测试套件都按它断言）—— 跨语言无法共享代码，但可共享行为规格。
pub fn is_valid_version(v: &str) -> bool {
    let mut it = v.splitn(2, '+');
    let core = it.next().unwrap_or("");
    let build = it.next();

    // ── 主段 + 预发布段 ──
    let mut core_it = core.splitn(2, '-');
    let nums = core_it.next().unwrap_or("");
    let parts: Vec<&str> = nums.split('.').collect();
    if parts.len() != 3 || !parts.iter().all(|p| num_ok(p)) { return false; }
    if let Some(pre) = core_it.next() {
        if pre.is_empty() { return false; }
        for seg in pre.split('.') {
            if seg.is_empty() || !seg.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') { return false; }
        }
    }

    // ── build 段（**不再忽略**）：非空，且每段为 [0-9A-Za-z-]+ ──
    if let Some(b) = build {
        if b.is_empty() { return false; }
        for seg in b.split('.') {
            if seg.is_empty() || !seg.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') { return false; }
        }
    }
    true
}

/// semver 比较（与内核 semverCompare 同语义）：数值段优先；release > prerelease；
/// 预发布内「数字段 < 字符串段」；build metadata 不参与。返回 -1/0/1。
pub fn semver_cmp(a: &str, b: &str) -> i32 {
    let parse = |v: &str| -> (Vec<i64>, String) {
        let clean = v.split('+').next().unwrap_or("").to_string();
        let mut it = clean.splitn(2, '-');
        let core = it.next().unwrap_or("").to_string();
        let pre = it.next().unwrap_or("").to_string();
        (core.split('.').map(|x| x.parse::<i64>().unwrap_or(0)).collect(), pre)
    };
    let (an, ap) = parse(a);
    let (bn, bp) = parse(b);
    for i in 0..3 {
        let x = an.get(i).copied().unwrap_or(0);
        let y = bn.get(i).copied().unwrap_or(0);
        if x != y { return if x > y { 1 } else { -1 }; }
    }
    if ap == bp { return 0; }
    if ap.is_empty() { return 1; }  // release > prerelease
    if bp.is_empty() { return -1; }
    let av: Vec<&str> = ap.split('.').collect();
    let bv: Vec<&str> = bp.split('.').collect();
    let n = av.len().max(bv.len());
    for i in 0..n {
        match (av.get(i), bv.get(i)) {
            (None, _) => return -1,
            (_, None) => return 1,
            (Some(x), Some(y)) => {
                let xn = x.chars().all(|c| c.is_ascii_digit());
                let yn = y.chars().all(|c| c.is_ascii_digit());
                if xn && yn {
                    let xi: i64 = x.parse().unwrap_or(0);
                    let yi: i64 = y.parse().unwrap_or(0);
                    if xi != yi { return if xi > yi { 1 } else { -1 }; }
                } else if xn != yn {
                    return if xn { -1 } else { 1 }; // 数字段 < 字符串段
                } else if x != y {
                    return if x < y { -1 } else { 1 };
                }
            }
        }
    }
    0
}

/// 镜像候选集合（2026-09-11 重写）：**壳自持配置优先**，其次内核 registry.json，最后内建默认。
///
/// 为什么壳配置优先：装机时**没有内核**（registry.json 尚不存在），壳必须自带镜像能力；
/// 而壳在引导阶段选出的最快源若能被内核继承，就不必让内核再盲选一次。
/// 若内核已进入 manual 模式（用户在面板里手动锁定），则**尊重内核的选择**。
pub fn registry_origins() -> Vec<String> {
    let path = crate::env::supervisor_dir().join("registry.json");
    let mut kernel_manual: Option<String> = None;
    let mut kernel_list: Option<Vec<String>> = None;
    if let Ok(s) = std::fs::read_to_string(&path) {
        if let Ok(v) = serde_json::from_str::<Value>(&s) {
            let mode = v.get("mode").and_then(|x| x.as_str()).unwrap_or("auto");
            if mode == "manual" {
                if let Some(m) = v.get("manualOrigin").and_then(|x| x.as_str()) {
                    if !m.is_empty() { kernel_manual = Some(m.to_string()); }
                }
            }
            if let Some(arr) = v.get("origins").and_then(|x| x.as_array()) {
                let list: Vec<String> = arr.iter().filter_map(|x| x.as_str())
                    .map(|s| s.to_string()).filter(|s| !s.is_empty()).collect();
                if !list.is_empty() { kernel_list = Some(list); }
            }
        }
    }
    // 1) 内核手动模式：最高优先（用户显式选择）
    if let Some(m) = kernel_manual {
        return vec![m];
    }
    // 2) 壳自持配置（引导阶段已测速选择）
    let m = crate::mirror::load();
    if !m.npm.is_empty() {
        return m.npm;
    }
    // 3) 内核 registry.json 的 auto 列表
    if let Some(l) = kernel_list {
        return l;
    }
    // 4) 内建默认
    DEFAULT_ORIGINS.iter().map(|s| s.to_string()).collect()
}

/// 包名 URL 编码：scope 的 / 编码为 %2F（npm registry 两种写法均可，编码更稳）。
fn encode_pkg(pkg: &str) -> String {
    pkg.chars().map(|c| if c == '/' { "%2F".to_string() } else { c.to_string() }).collect()
}

/// 目标版本：**并行**探测全部镜像，按发布通道契约 §3 选版；返回 (version, 命中镜像)。
///
/// ## 为什么"并行 + 跨源取最高"本身还是对的
///
/// 镜像**同步存在延迟**：同一个包在不同源上可能停在不同的时刻。若"首个成功的源即采信"，
/// 一个滞后的源会把新版本掩盖成旧版本。故仍然**并行探测全部源**，再看它们各自的元数据。
///
/// ## 变的是**选版判据**（2026-09-16，契约 §3 冻结算法）
///
/// 旧实现：跨全部源取 `dist-tags` + `versions` 的**全量最高**。
/// 那是"挑数字最大的版本"，会**绕过通道控制**（RC-1）——BETA 的数字可能压过正式版。
/// 现在每个源的元数据都交给 `release_channel::select`（rollback / canary / latest / versions 兜底），
/// 再在**同一通道**内跨源取最高，最后选提供该版本的最快源。
///
/// 为什么"直接按通道决策"要先于"跨源合并"：**通道是全局事实，元数据是逐源的**。
///   若先把各源的 dist-tags 并起来（例如 A 源的 rollback 与 B 源的 latest 混在一起），
///   就等于**发明了一个并不存在的发布状态** —— 回退必须由真实存在的那个 tag 决定。
///   故本函数对每个源独立决策，再比较决策结果。
///
/// ## 不变量
///
/// · 每个源各自走完整 §3 五步，**绝不**跨源拼接 dist-tags；
/// · 回退（rollback）优先于灰度（canary）优先于正式（latest）—— 详见 release_channel；
/// · 同通道内多源给出不同版本时，取**较高**者（与旧行为一致；这不改变通道，只解决镜像滞后）；
/// · 全部源都失败 → 原样回传**每个源**的失败原因（RC-5：绝不静默，也不谎报"已是最新"）。
pub fn latest_pick(pkg: &str) -> Result<LatestPick, String> {
    if pkg.is_empty() { return Err("包名为空".into()); }
    let path = encode_pkg(pkg);
    let origins = registry_origins();
    let probes = crate::mirror::probe_all(&origins, &path);

    // §5 灰度判定**只做一次**（不要放进循环：那意味着每个源都去查一次名单包）。
    //   本函数只在这一处调用；普通机器在此返回 false 且**零额外请求**（见 release_channel）。
    let canary = canary_here();

    let mut cands: Vec<Candidate> = Vec::new();
    let mut reachable = 0usize;
    for p in &probes {
        if !p.ok { continue; }
        reachable += 1;
        let Some(body) = &p.body else { continue };
        let Ok(j) = serde_json::from_str::<Value>(body) else { continue };
        // 该源独立走 §3 五步 —— 一个源坏掉/缺 tag 不影响其它源的决策。
        let Ok(pick) = crate::release_channel::select(&j, canary) else { continue };
        cands.push((pick.version, p.latency_ms, p.source.clone(), pick.via));
    }

    match pick_best(cands) {
        Some((version, _, source, via)) => Ok(LatestPick { version, origin: source, via: via.to_string() }),
        None => {
            let detail = probes
                .iter()
                .map(|p| format!("{}:{}", p.source, if p.ok { format!("{}ms", p.latency_ms) } else { "不可达".into() }))
                .collect::<Vec<_>>()
                .join(", ");
            Err(format!("全部镜像不可用或均无该包（可达 {} 个；{}）", reachable, detail))
        }
    }
}

/// 选版结果 + **选版依据**（契约 §3 的通道）。
///
/// 为什么必须把 `via` 带回上层：`rollback` 生效时目标版本**低于**当前版本 ——
///   若上层只看"目标 vs 已装"的版本比较，回退会被判成"无需动手"（RC-2 端到端断裂）。
///   通道信息是"这是不是一个回退指令"的**唯一可靠依据**。
pub struct LatestPick {
    pub version: String,
    pub origin: String,
    /// rollback | canary | latest | versions（见 release_channel::select）
    pub via: String,
}

/// 兼容封装：只要「版本, 源」的调用方（诊断输出等）用这个；需要判回退的用 `latest_pick`。
pub fn latest_version(pkg: &str) -> Result<(String, String), String> {
    latest_pick(pkg).map(|p| (p.version, p.origin))
}

/// 一个镜像给出的候选：(版本, 延迟ms, 源, 通道)。
type Candidate = (String, u128, String, &'static str);

/// 候选 a 是否**优于**候选 b（跨源仲裁的唯一判据）。
///
/// 顺序（**契约 §3 的优先级**，不是数字大小）：
///   ① 通道优先级：rollback > canary > latest > versions；
///   ② 同通道内：版本更高者胜（解决**镜像同步滞后**——同一通道不同源可能停在不同版本）；
///   ③ 版本相同：延迟更低者胜（与旧行为一致，让"最快源"仍被优先命中）。
///
/// 为什么 ① 必须压过 ②：**回退目标是低于 latest 的**（这正是它的用途）。
///   若按数字大小比较，一个尚未同步 rollback 的源会用更大的 latest 把它压过去，
///   紧急回退就无法全量生效（RC-2 被违反）。
fn better_candidate(a: &Candidate, b: &Candidate) -> bool {
    let (av, alat, _, avia) = a;
    let (bv, blat, _, bvia) = b;
    match channel_rank(avia).cmp(&channel_rank(bvia)) {
        std::cmp::Ordering::Less => true,
        std::cmp::Ordering::Greater => false,
        std::cmp::Ordering::Equal => {
            let c = semver_cmp(av, bv);
            c > 0 || (c == 0 && *alat < *blat)
        }
    }
}

/// 从全部候选里挑出唯一目标（纯函数 —— 跨源仲裁因此可被单元测试直接覆盖）。
///
/// 按输入顺序**一次遍历**：只有**严格更优**才替换，故并列时保留先到者
///   （`probe_all` 已按"可达优先 + 延迟升序"排好，先到即最快）。
fn pick_best(cands: Vec<Candidate>) -> Option<Candidate> {
    let mut best: Option<Candidate> = None;
    for c in cands {
        match &best {
            Some(b) if !better_candidate(&c, b) => {}
            _ => best = Some(c),
        }
    }
    best
}

/// 通道优先级（数值越小越优先）——**契约 §3 的顺序**，不是版本高低。
///
/// 为什么必须单列：跨源比较时，"哪个源的决策更该被采纳"取决于**通道**。
///   若退回"比版本号大小"，回退目标（通常低于 latest）会被别的源的 latest 压过去，
///   而这正是契约 §2 三条硬缺陷要根除的推断方式（RC-2）。
fn channel_rank(via: &str) -> u8 {
    match via {
        crate::release_channel::CH_ROLLBACK => 0,
        crate::release_channel::CH_CANARY => 1,
        crate::release_channel::CH_LATEST => 2,
        _ => 3, // versions 兜底
    }
}

/// 本机是否在**灰度名单**内（契约 §5）。
///
/// 短路顺序见 `release_channel::canary_machine_with`（契约 §5.5，**顺序未变**）：
///   ① 本机配置 `canary: true`（或 `DSH_CANARY=1`）→ 命中，**不查包**；
///   ② 未标记且未 opt-in（`canaryAllowlist: true` / `DSH_CANARY_ALLOWLIST=1`）→ false，**零额外请求**；
///   ③ 只有**显式进入候选**的机器才读 `@dsh-sup/canary-allowlist`（可选包，读不到只留日志）。
///      名单只认 §5.3 唯一格式（`schema:1` + `entries[].installId` 主依据 + `hostnames[]` 兜底）；
///      本机 installId 取自内核写的 `<supervisorDir>/install-id`（**壳不生成**，见 release_channel）。
///
/// ③ 的代价是"多一次网络请求"，故它**绝不能**发生在普通机器上 —— 这正是 ①② 短路的理由。
///   也正因如此，这一次请求发生在**探测循环之外**：多源探测已经并行跑完，
///   这里是全程串行的一次（最多 PROBE_TIMEOUT），不会随镜像数量放大。
fn canary_here() -> bool {
    let local = crate::release_channel::local_canary_hit();
    // 普通机器（未标记、未 opt-in）在此**直接返回 false**，连 fetch 闭包都不会被调用
    //   —— 于是"多一次网络请求"这件事对绝大多数用户根本不存在。
    let opt_in = crate::release_channel::allowlist_opt_in();
    if !local && !opt_in {
        return false;
    }
    let origins = registry_origins();
    crate::release_channel::canary_machine(local, opt_in, move |pkg| fetch_pkg_meta(&origins, pkg))
}

/// 按镜像顺序**串行**取一份包元数据，首个成功即返回。
///
/// ## 为什么复用 `probe_all` 的**单个源**而不是调用它整个
///
/// `probe_all` 的并行是为"测速"服务的：它要打满全部源才能选出最快者。
///   而这里要的是"谁能给我这份可选内容"——第一个能回答的源就够了，
///   打满全部源只会把这个**可选**查询的成本放大到 N 倍。
/// 但也**不另写一套 HTTP**：仍经 `probe_all`（单源）取回响应体 ——
///   URL 编码、超时（`mirror::PROBE_TIMEOUT`）、响应体读取全仓只有那一份实现，
///   避免此处成为"第二份 registry 客户端"。单源时它内部只 spawn 一个线程，无额外代价。
fn fetch_pkg_meta(origins: &[String], pkg: &str) -> Result<Value, String> {
    let path = encode_pkg(pkg);
    let mut last = String::from("无可用镜像");
    for o in origins {
        let one = std::slice::from_ref(o);
        match crate::mirror::probe_all(one, &path).into_iter().next() {
            Some(p) if p.ok => match p.body.as_deref() {
                Some(body) => match serde_json::from_str::<Value>(body) {
                    Ok(j) => return Ok(j),
                    Err(_) => last = format!("{}: 响应不是合法 JSON", o),
                },
                None => last = format!("{}: 空响应", o),
            },
            Some(_) => last = format!("{}: 不可达", o),
            None => last = format!("{}: 探测未返回", o),
        }
    }
    Err(last)
}

/// 无 GUI 场景下定位内核可执行文件（与 main.rs 的 locate_core 同一候选集）。
/// 供 --service-plan 等 CLI 自检使用：它们没有 AppHandle。
pub fn locate_core_for_cli() -> Option<std::path::PathBuf> {
    // CLI 无 AppHandle → 不提供资源目录兜底（那是 GUI 形态的最后一层）
    crate::domain::coreloc::locate_core_candidates(None)
        .into_iter()
        .find(|p| p.is_file())
}

/// 内核包目录（<pkg>/bin/<exe> → <pkg>）。
fn package_dir_of(bin: &Path) -> Option<PathBuf> {
    let bin_dir = bin.parent()?;              // <pkg>/bin
    if bin_dir.file_name().and_then(|s| s.to_str()) != Some("bin") { return None; }
    bin_dir.parent().map(|p| p.to_path_buf()) // <pkg>
}

/// 已安装版本：优先读包内 package.json（无进程开销、跨平台一致）；兜底执行 --version。
pub fn installed_version(bin: &Path) -> Option<String> {
    if let Some(dir) = package_dir_of(bin) {
        if let Ok(s) = std::fs::read_to_string(dir.join("package.json")) {
            if let Ok(v) = serde_json::from_str::<Value>(&s) {
                if let Some(ver) = v.get("version").and_then(|x| x.as_str()) {
                    if is_valid_version(ver) { return Some(ver.to_string()); }
                }
            }
        }
    }
    // 兜底：执行 --version。
    // 必须有界（2026-09-11 修复，与「检测环境卡死」同一类缺陷）：
    //   原实现用 Command::output() **无限阻塞**，且 locate_core 会对**每个候选**都调用一次；
    //   一旦某个候选不可执行（损坏的 shim、被安全软件拦截、架构不符），
    //   引导页就会永久停在「正在检查内核版本」。
    let mut cmd = std::process::Command::new(bin);
    cmd.arg("--version");
    match run_command_bounded(cmd, VERSION_PROBE_TIMEOUT) {
        Ok(o) if o.success => parse_version_output(&o.stdout),
        _ => None,
    }
}

/// 内核 `--version` 探测的时间上限。
const VERSION_PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// 从 --version 输出解析版本（形如 "dsh-supervisor v0.1.2-BETA.7"）。
pub fn parse_version_output(s: &str) -> Option<String> {
    for tok in s.split_whitespace() {
        let t = tok.trim().trim_start_matches('v');
        if is_valid_version(t) { return Some(t.to_string()); }
    }
    None
}

/// 读 **npm 自己**的全局 prefix（`<contract npm> prefix -g`）。
///
/// 用途**仅限**「安装刚完成后回读 npm 把包装到了哪」——用于 P2 记录 core.json。
/// **不得**用它来选择安装前缀（选择仍用 global_prefix_for，理由见文件头注释：
///   既有内核位置与 npm prefix -g 可能不一致，用它选前缀会把新内核装到别处）。
pub fn npm_global_prefix() -> Option<PathBuf> {
    let rt = crate::runtime_contract::read_node()?;
    let mut cmd = std::process::Command::new(&rt.npm);
    cmd.args(&rt.npm_prefix);
    cmd.args(["prefix", "-g"]);
    let o = run_command_bounded(cmd, VERSION_PROBE_TIMEOUT).ok()?;
    if !o.success { return None; }
    let p = o.stdout.trim();
    if p.is_empty() { None } else { Some(PathBuf::from(p)) }
}

/// 从内核真实路径反推 npm 全局前缀（跨平台布局差异见下）。
///   Unix    : <prefix>/lib/node_modules/@scope/pkg/bin/exe  → <prefix>
///   Windows : <prefix>/node_modules/@scope/pkg/bin/exe      → <prefix>
pub fn global_prefix_for(bin: &Path) -> Option<PathBuf> {
    let comps: Vec<std::path::Component> = bin.components().collect();
    for i in 0..comps.len() {
        if comps[i].as_os_str() == std::ffi::OsStr::new("node_modules") {
            let is_lib = i >= 1 && comps[i - 1].as_os_str() == std::ffi::OsStr::new("lib");
            let cut = if is_lib { i - 1 } else { i };
            let mut p = PathBuf::new();
            for c in &comps[..cut] { p.push(c.as_os_str()); }
            if p.as_os_str().is_empty() { return None; }
            return Some(simplify(p));
        }
    }
    // Windows npm 垫片兜底（2026-09-11 修复）：
    //   路径形如 `%APPDATA%\npm\dsh-supervisor.cmd` —— **不含 node_modules 段**，
    //   故上面的循环返回 None，进而 install_version 丢失 --prefix，
    //   可能装到 npm 默认前缀而非内核当前所在前缀（旧内核遮蔽新内核）。
    //   判据：该目录直接含 node_modules 时，它本身就是 npm 全局前缀。
    if let Some(dir) = bin.parent() {
        if dir.join("node_modules").is_dir() {
            return Some(simplify(dir.to_path_buf()));
        }
    }
    None
}

fn tail(s: &str, n: usize) -> String {
    let t = s.trim();
    if t.chars().count() <= n { return t.to_string(); }
    t.chars().skip(t.chars().count() - n).collect()
}

/// 安装/升级到指定版本（npm install -g [--prefix] pkg@version）。
/// 显式 --prefix 保证装回「内核当前所在前缀」，避免 npm 默认前缀不一致导致旧内核遮蔽新内核。
/// 返回 npm 输出（成功）或含退出码与 stderr 的错误（失败——供引导页如实呈现，不再吞错）。
///
/// 不变量：
///   · 首次失败且**快失败**时，用全新临时缓存目录重试一次（隔离损坏的 npm 缓存）；
///     仅在 FAST_FAIL_RETRY 窗口内重试，避免突破引导页预算。
///   · 失败时**必须回传证据**（命令 / prefix / 源 / 两次尝试输出）。
pub fn install_version(pkg: &str, version: &str, prefix: Option<&Path>, registry: Option<&str>) -> Result<String, String> {
    if !is_valid_version(version) { return Err(format!("非法目标版本: {}", version)); }
    let spec = format!("{}@{}", pkg, version);

    let t0 = std::time::Instant::now();
    let first = run_npm_install(&spec, prefix, registry, None);
    match &first {
        Ok(out) if out.success => return Ok(tail(&out.stdout, 500)),
        Err(_) => {} // 连启动都失败（npm 不存在等）—— 直接回报，不重试
        Ok(_) => {}
    }
    let first_out = first.as_ref().ok();

    // 只在**快失败**时做缓存隔离重试（见上方说明）
    let mut second: Option<crate::bounded::Output> = None;
    if first_out.is_some() && t0.elapsed() <= FAST_FAIL_RETRY {
        if let Some(dir) = fresh_cache_dir() {
            let s = run_npm_install(&spec, prefix, registry, Some(&dir));
            let ok = matches!(&s, Ok(o) if o.success);
            if !ok { second = s.ok(); }
            let _ = std::fs::remove_dir_all(&dir); // 尽力清理，失败不报错
            if ok {
                return Ok("（默认缓存首次失败，改用隔离缓存重试后成功）"
                    .to_string()
                    + &tail(first_out.map(|o| o.stdout.as_str()).unwrap_or(""), 200));
            }
        }
    }

    // 组织**完整证据**（现场定位所需：命令 / prefix / 源 / 两次输出）
    let mut ev = String::new();
    ev.push_str(&format!("cmd: {} install -g --no-audit --no-fund {}", npm_exe(), spec));
    if let Some(p) = prefix { ev.push_str(&format!(" --prefix {}", p.display())); }
    if let Some(r) = registry { if !r.is_empty() { ev.push_str(&format!(" [registry {}]", r)); } }
    let fmt = |o: &crate::bounded::Output| {
        let code = o.code.clone().unwrap_or_else(|| "killed".into());
        format!("退出码 {}：{}", code, tail(&o.stderr, 700))
    };
    match (first_out, second.as_ref()) {
        (Some(a), Some(b)) => Err(format!("{}；缓存隔离重试仍失败：{}", fmt(a), fmt(b))),
        (Some(a), None) => Err(format!("{}{}", fmt(a), FAST_SKIP_NOTE)),
        _ => Err(first.err().unwrap_or_else(|| "npm 未能启动".into())),
    }
    .map_err(|e| format!("{}\n  [{}]", e, ev))
}

/// prefix 是否为 **Node 安装目录**（含 node_modules/npm）。
///
/// 为什么单独判定：把包 --prefix 装进 Node 自身，通常需要管理员权限，
/// 且会让 npm 遍历自身庞大的依赖树；现场那条栈溢出无法本地复现，
/// 故此处**只回传证据**、不擅自改变语义（丢弃 prefix 可能装到别的前缀，
/// 反而制造「装了但检测不到」）。
pub fn is_node_install_prefix(p: &Path) -> bool {
    p.join("node_modules").join("npm").is_dir()
}
/// 剥掉 Windows verbatim / device 命名空间前缀 —— **交给外部工具（npm / node）前必须做**。
///
/// 剥除 Windows verbatim/device 命名空间前缀（交给外部工具 npm/node 前必须做）。
///
/// ## 规则
///   \\?\UNC\server\share -> \\server\share（UNC 段大小写不敏感）
///   \\?\C:\x             -> C:\x
///   \.\C:\x             -> C:\x
///   其它                             原样返回（非 Windows 路径不受影响）
pub fn strip_verbatim(s: &str) -> String {
    // concat! 拼出「以反斜杠结尾」的字面量（raw string 不能以反斜杠结尾）
    const V: &str = concat!(r"\\?", "\\");
    const VU: &str = concat!(r"\\?\UNC", "\\");
    const D: &str = concat!(r"\\.", "\\");
    if s.len() >= VU.len() && s.is_char_boundary(VU.len()) && s[..VU.len()].eq_ignore_ascii_case(VU) {
        return format!("{}{}", r"\\", &s[VU.len()..]);
    }
    if let Some(r) = s.strip_prefix(V) {
        return r.to_string();
    }
    if let Some(r) = s.strip_prefix(D) {
        return r.to_string();
    }
    s.to_string()
}

/// 把 PathBuf 中的 verbatim 前缀剥掉（无前缀时原样返回，不做多余分配）。
/// 单一实现：global_prefix_for 与 npm 参数构造都调它，避免两份规则分叉。
fn simplify(p: PathBuf) -> PathBuf {
    let s = p.to_string_lossy();
    let cleaned = strip_verbatim(&s);
    if cleaned == s { p } else { PathBuf::from(cleaned) }
}

/// 缓存隔离重试窗口：首次失败耗时不超过此值才重试（避免突破引导页 17 分钟预算）。
const FAST_FAIL_RETRY: std::time::Duration = std::time::Duration::from_secs(120);
/// 未重试时的说明——让现场知道「为什么没有第二次尝试」。
const FAST_SKIP_NOTE: &str = "（首次失败耗时较长，未做缓存隔离重试）";

/// 构造一个**全新的**临时缓存目录（用于隔离损坏的 npm 缓存）。
fn fresh_cache_dir() -> Option<std::path::PathBuf> {
    let d = std::env::temp_dir().join(format!("dsh-npmcache-{}", std::process::id()));
    std::fs::create_dir_all(&d).ok()?;
    Some(d)
}

/// 执行一次 npm install（cache 为 Some 时使用隔离缓存）。
fn run_npm_install(
    spec: &str,
    prefix: Option<&Path>,
    registry: Option<&str>,
    cache: Option<&Path>,
) -> Result<crate::bounded::Output, String> {
    // 单一事实源：优先用运行期契约里的**绝对 npm** 与 PATH（不再依赖 ambient PATH 的裸名）。
    //   根因同守卫拉起：GUI/服务环境的 PATH 常不含 nvm/fnm 的 npm。
    let (npm_bin, npm_prefix, env_path) = match crate::runtime_contract::read_node() {
        Some(rt) if rt.npm.is_file() => (
            rt.npm,
            rt.npm_prefix,
            Some(crate::runtime_contract::env_path(&rt.node_bin_dir)),
        ),
        _ => (std::path::PathBuf::from(npm_exe()), Vec::new(), None),
    };
    let mut cmd = std::process::Command::new(&npm_bin);
    if let Some(p) = &env_path {
        cmd.env("PATH", p);
    }
    // npm 仅有包内 JS 时：npmBin=node、npmPrefix=[npm-cli.js]（带上才能调用）。
    cmd.args(&npm_prefix);
    cmd.args(["install", "-g", "--no-audit", "--no-fund"]).arg(spec);
    if let Some(p) = prefix { cmd.arg("--prefix").arg(simplify(p.to_path_buf())); }
    if let Some(r) = registry { if !r.is_empty() { cmd.env("npm_config_registry", r); } }
    if let Some(c) = cache { cmd.env("npm_config_cache", c); }
    // CREATE_NO_WINDOW：GUI 进程调 npm 不弹控制台。
    // 经 bounded::prepare（**infra 原语**，与 bounded::run 同一处实现）——
    // 本文件因此不再需要平台分支（门禁 G1）。
    crate::bounded::prepare(&mut cmd);
    // 必须有界（2026-09-11 修复，与引导页「网络步骤无超时 → 永久卡住」属同一类缺陷）：
    //   原实现用 `cmd.output()` **无限阻塞** —— npm 因网络停滞/registry 无响应而挂起时，
    //   引导页会永久停在「正在安装内核…」，用户除了杀进程别无选择。
    //   实现要点：输出重定向到**临时文件**而非管道 —— 若用 Stdio::piped() 且不读取，
    //   冗长的 npm 输出（npm 会打印大量进度）填满 OS 管道缓冲区（约 64KB）后子进程会阻塞，
    //   反而制造死锁。临时文件无此问题，且便于超时后保留现场。
    run_command_bounded(cmd, NPM_INSTALL_TIMEOUT)
}
/// npm install 的时间上限。npm 在慢网下确实可能耗时数分钟，故给足预算；
/// 但绝不无限等待 —— 超时即杀进程并如实报错（引导页据此给出重试/回退）。
const NPM_INSTALL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15 * 60);

/// 有界执行子进程 —— **委托给 `bounded.rs` 的统一实现**（P2 去重，2026-09-12）。
///
/// 此处原有 `struct BoundedOutput` + 一份 `run_command_bounded` 的**完整复制**：
///   字段与 `bounded::Output` 逐一相同，逻辑也几乎逐行相同，但**行为已经分叉**：
///     · 漏 `cmd.stdin(Stdio::null())` —— 子进程会继承 GUI 进程的 stdin；
///     · 曾用 `as_millis()` 做临时名（并发撞名）而 bounded 一直用 nanos；
///     · `prepare()`（Windows CREATE_NO_WINDOW）只在 npm 路径手动调过，物探路径漏了 → 闪控制台。
///   这正是 `bounded.rs` 顶部「所有外部命令一律经它执行」被违反的又一例。
///
///   现统一走 `crate::bounded::run`：stdin/prepare/nanos/超时杀进程全部一致，
///   返回类型直接用 `bounded::Output`（字段本就相同，无需再定义一份）。
fn run_command_bounded(
    mut cmd: std::process::Command,
    timeout: std::time::Duration,
) -> Result<crate::bounded::Output, String> {
    crate::bounded::run(&mut cmd, timeout)
}


// ── 注：本文件原有的 `read_log` 已删除（2026-09-12 去重）。
//    它随 `run_command_bounded` 的复制体一起存在；统一到 `bounded.rs` 后成为死代码。
//    GBK/非 UTF-8 的容忍现由 `bounded.rs::read_log` 单点负责（B62 锁定）。


/// 计算规划：只有目标版本**严格大于**已装版本才需动手（正常路径不因版本比较而降级）。
///
/// ## 与契约 §3 的边界（**已知缺口，需上层收口**）
///
/// 选版（`latest_version`）回答"**目标是谁**"：`rollback` tag 生效时它会给出
///   **低于当前**的目标（这正是回退的用途）。本函数回答"**要不要动手**"，
///   而它只按 `semver_cmp(目标, 已装) > 0` 判 `upgrade` ——
///   于是**回退目标在这里被判成 `action=none`**，前端据此显示"已是最新"，
///   回退到不了用户（RC-2 端到端未闭合）。
///
/// 为什么不在此处就地修：
///   · 判"回退也要动手"必须知道**选版依据（via）**，而入参只有 `(version, origin)`；
///     把 via 透传下来要改 `latest_version` 的返回形态与 `commands/mod.rs` 的调用点
///     （本文件之外），并同步前端 `bootstrap/js/50-kernel.js`（它只认 install/upgrade/none）；
///   · 只改这里的输出会让"后端说回退、前端不执行"变成**静默失效**——比现状更糟。
/// 故此处**如实标注**，由上层一次性收口（选版依据 → action → 前端执行）。
/// 注意 `core_apply` 本身**不做版本比较**：它直接安装选出的目标版本，回退机制本身是可用的。
pub fn build_plan(installed: Option<String>, latest: Result<LatestPick, String>) -> Value {
    let (latest_v, origin, err, via) = match latest {
        Ok(p) => (Some(p.version), Some(p.origin), None, Some(p.via)),
        Err(e) => (None, None, Some(e), None),
    };
    // 回退判定（2026-09-16 收口 RC-2 端到端）：
    //   `rollback` 通道生效时，目标版本**低于**当前版本 —— 这正是回退的目的，
    //   但纯版本比较会把它判成"无需动手"（旧缺口：前端显示"已是最新"，回退到不了用户）。
    //   故：**通道 = rollback 时，目标与已装不同即视为需动手**（action 仍用升级词，
    //   因为前端的执行路径只有 install/upgrade —— 执行体 `core_apply` 不做版本比较，
    //   直接装选出的目标版本，故"回退"与"升级"共用同一执行路径是正确且刻意的）。
    let is_rollback = via.as_deref() == Some("rollback");
    let action = match (&installed, &latest_v) {
        (None, Some(_)) => "install",
        (Some(i), Some(l)) => {
            let cmp = semver_cmp(l, i);
            if cmp > 0 { "upgrade" }
            else if is_rollback && cmp != 0 { "upgrade" }   // ← 回退：显式通道信号，非版本比较
            else { "none" }
        }
        _ => "unknown",
    };
    // 统一更新决策形状（与桌面自更新同一组键）；保留原字段向后兼容前端。
    let origin_out = origin.clone();
    let mut extra = serde_json::Map::new();
    extra.insert("installed".into(), serde_json::json!(installed.clone()));
    extra.insert("action".into(), serde_json::json!(action));
    extra.insert("updateAvailable".into(), serde_json::json!(action == "upgrade"));
    extra.insert("registry".into(), serde_json::json!(origin));
    // 选版依据（rollback/canary/latest/versions）——前端与运维据此识别"当前是否回退中"。
    extra.insert("latestVia".into(), serde_json::json!(via));
    extra.insert("isRollback".into(), serde_json::json!(is_rollback));
    crate::update_plan::unified(
        "kernel",
        installed,
        latest_v,
        action == "upgrade" || action == "install",
        origin_out,
        err,
        extra,
    )
}

/// 无头自检输出（--core-plan 用，便于发布后冒烟验证，无需 GUI）。
///
/// 输出含 `latest_via`（本次是从 **rollback / canary / latest / versions** 哪条通道选出来的）：
///   契约 §4 把"rollback tag 存在 = 回退进行中"当作**可观测性**承诺 ——
///   而运维在无 GUI 的机器上核对回退是否生效，靠的就是这个自检入口。
pub fn plan_text() -> String {
    let pkg = match package_name() { Ok(p) => p, Err(e) => return format!("pkg_error={}", e) };
    let mut lines = vec![format!("package={}", pkg)];
    lines.push(format!("origins={}", registry_origins().join(",")));
    match latest_version(&pkg) {
        Ok((v, o)) => {
            lines.push(format!("latest={}", v));
            lines.push(format!("latest_origin={}", o));
            lines.push(format!("latest_via={}", decision_channel(&pkg)));
        }
        Err(e) => lines.push(format!("latest_error={}", e)),
    }
    lines.join(" | ")
}

/// 目标版本是经哪条通道选出的（**仅供自检/日志**，不参与安装决策）。
///
/// 为什么**重新查一遍**而不是让 `latest_version` 也返回通道：
///   · `latest_version` 的签名（version, origin）被 8 处调用点依赖，改动面远大于收益；
///   · 本函数**只在 `--core-plan` 自检路径上执行**，不在引导主链路上 ——
///     多一次元数据探测的代价可接受（自检本就是"多花几秒换确定性"的入口）。
/// 失败一律回传原因，绝不编造通道（RC-5）。
fn decision_channel(pkg: &str) -> String {
    let path = encode_pkg(pkg);
    let origins = registry_origins();
    let probes = crate::mirror::probe_all(&origins, &path);
    let canary = canary_here();
    let mut cands: Vec<Candidate> = Vec::new();
    for p in &probes {
        if !p.ok { continue; }
        let Some(body) = &p.body else { continue };
        let Ok(j) = serde_json::from_str::<Value>(body) else { continue };
        if let Ok(pick) = crate::release_channel::select(&j, canary) {
            cands.push((pick.version, p.latency_ms, p.source.clone(), pick.via));
        }
    }
    pick_best(cands).map(|c| c.3.to_string()).unwrap_or_else(|| "unknown".into())
}
#[cfg(test)]
mod tests {
    use super::*;

    /// 版本语义**共享测试向量**（2026-09-11）。
    ///
    /// ## 为什么需要它
    ///
    /// 壳（Rust）与内核（JS）各自实现版本校验/比较 —— **实测 3 处分歧**：
    /// `1.0.0+` / `1.0.0+!!!` / `1.0.0+あ` 壳判合法、内核判非法
    /// （旧壳现在验证前 `split('+')` 丢弃 build 段）。
    ///
    /// 跨语言无法共享代码，故共享**行为规格**：
    /// `shell-release/version-vectors.json`（内核仓有逐字节相同的一份）。
    /// 两侧测试套件都加载它并按自己的实现断言。
    ///
    /// 任何一侧改了语义而没同步 → 本测试失败。这是防止再次分叉的唯一可靠手段。
    ///
    /// `include_str!` 是**编译期**嵌入：文件缺失或路径错误会直接编译失败，
    /// 比运行时读取的门禁更强（不会因「文件恰好不在」而静默跳过）。
    const VECTORS: &str = include_str!("../../shell-release/version-vectors.json");

    /// 从形如 `{"input": "x", "valid": true}` 的对象体里取字符串字段。
    fn str_field(body: &str, key: &str) -> Option<String> {
        let pat = format!("\"{}\":", key);
        let after = body.split(&pat).nth(1)?;
        let mut it = after.split('"');
        it.next()?;
        Some(it.next()?.to_string())
    }

    /// 取布尔字段（只认 `"key": true`）。
    fn bool_field(body: &str, key: &str) -> Option<bool> {
        let pat = format!("\"{}\":", key);
        let after = body.split(&pat).nth(1)?;
        let v = after.trim_start();
        if v.starts_with("true") { Some(true) } else { Some(false) }
    }

    /// 取整数字段。
    fn int_field(body: &str, key: &str) -> Option<i32> {
        let pat = format!("\"{}\":", key);
        let after = body.split(&pat).nth(1)?;
        let digits: String = after.trim_start()
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '-')
            .collect();
        digits.parse().ok()
    }

    /// 把 JSON 文本里的**每个**对象块（`{` … `}`）都取出来。
    ///
    /// 实现：栈记录每个 `{` 的起始位置，遇 `}` 弹出即得一个完整对象。
    /// 随后按**大小**过滤掉根对象（根对象包住整份文件，必然最长）——
    /// 只留逐条向量的小对象。
    ///
    /// 已知足够：向量文件里字符串不含花括号（数据由本仓维护）。
    fn object_bodies(raw: &str) -> Vec<String> {
        let mut all = Vec::new();
        let mut stack: Vec<usize> = Vec::new();
        for (i, c) in raw.char_indices() {
            match c {
                '{' => stack.push(i),
                '}' => {
                    if let Some(s) = stack.pop() {
                        all.push(raw[s..=i].to_string());
                    }
                }
                _ => {}
            }
        }
        // 根对象 = 唯一「包住整份文件」的那个（长度接近全文）——过滤掉。
        let limit = raw.len() / 2;
        all.into_iter().filter(|b| b.len() < limit).collect()
    }

    #[test]
    fn shared_version_vectors_hold() {
        let bodies = object_bodies(VECTORS);
        assert!(
            bodies.len() >= 20,
            "向量块数异常：{}（模板可能被破坏）",
            bodies.len()
        );

        let (mut nv, mut nc) = (0, 0);
        for b in &bodies {
            if b.contains("\"input\"") {
                let input = str_field(b, "input").expect("input 缺失");
                let valid = bool_field(b, "valid").expect("valid 缺失");
                let got = is_valid_version(&input);
                assert_eq!(
                    got, valid,
                    "版本合法性分歧：{:?} → 本实现 {}，向量期望 {}",
                    input, got, valid
                );
                nv += 1;
            } else if b.contains("\"expected\"") {
                let a = str_field(b, "a").expect("a 缺失");
                let bq = str_field(b, "b").expect("b 缺失");
                let exp = int_field(b, "expected").expect("expected 缺失");
                let got = semver_cmp(&a, &bq);
                assert_eq!(
                    got, exp,
                    "版本比较分歧：{:?} vs {:?} → 本实现 {}，向量期望 {}",
                    a, bq, got, exp
                );
                nc += 1;
            }
        }
        assert!(nv >= 15, "合法性向量过少：{}", nv);
        assert!(nc >= 8, "比较向量过少：{}", nc);
        eprintln!("版本向量通过：合法性 {} 条 / 比较 {} 条", nv, nc);
    }

// verbatim 前缀剥除的针对性回归。
    #[test]
    fn strips_verbatim_drive_prefix() {
        assert_eq!(strip_verbatim("\\\\?\\C:\\Users\\x"), "C:\\Users\\x");
    }

    #[test]
    fn strips_verbatim_unc_prefix_case_insensitively() {
        assert_eq!(strip_verbatim("\\\\?\\UNC\\srv\\share\\x"), "\\\\srv\\share\\x");
        assert_eq!(strip_verbatim("\\\\?\\unc\\srv\\share"), "\\\\srv\\share");
    }

    #[test]
    fn strips_device_prefix() {
        assert_eq!(strip_verbatim("\\\\.\\C:\\x"), "C:\\x");
    }

    #[test]
    fn leaves_clean_paths_untouched() {
        // 反向：干净路径**不得**被改写（否则引入新的「路径变了」问题）
        for p in ["C:\\Users\\x", "/home/u/x", ""] {
            assert_eq!(strip_verbatim(p), p, "不应改写: {}", p);
        }
    }

    // ═══════════════════════════════════════════════════════════════════
    // 跨源仲裁（契约 §3 + 镜像同步滞后）—— `latest_version` 的决策内核。
    //
    // §3 五步本身由 release_channel.rs 的单元测试逐分支覆盖；
    //   此处覆盖**第二步**：多个源各自决策之后，谁的目标版本会被采纳。
    // ═══════════════════════════════════════════════════════════════════

    use crate::release_channel::{CH_CANARY, CH_LATEST, CH_ROLLBACK, CH_VERSIONS};

    fn cand(v: &str, lat: u128, src: &str, via: &'static str) -> Candidate {
        (v.to_string(), lat, src.to_string(), via)
    }

    #[test]
    fn pick_best_prefers_higher_version_within_same_channel() {
        // 同一通道内取更高版本 —— 解决"某镜像元数据滞后一版"。
        let got = pick_best(vec![
            cand("0.1.5-BETA.3", 10, "fast", CH_LATEST),
            cand("0.1.5-BETA.7", 500, "slow", CH_LATEST),
        ])
        .unwrap();
        assert_eq!(got.0, "0.1.5-BETA.7");
        assert_eq!(got.2, "slow", "更高版本胜出，即使它更慢");
    }

    #[test]
    fn pick_best_prefers_lower_latency_on_tie() {
        let got = pick_best(vec![
            cand("0.1.5", 900, "slow", CH_LATEST),
            cand("0.1.5", 12, "fast", CH_LATEST),
        ])
        .unwrap();
        assert_eq!(got.2, "fast", "同版本时保留更快源");
    }

    #[test]
    fn pick_best_rollback_beats_other_sources_latest() {
        // **RC-2 回归（跨源形态）**：A 源已同步 rollback（低版本），B 源还是 latest（高版本）。
        //   若按"数字大小"仲裁，B 会压过 A → 紧急回退无法全量生效。
        let got = pick_best(vec![
            cand("0.2.0", 5, "synced-latest", CH_LATEST),
            cand("0.1.4", 800, "synced-rollback", CH_ROLLBACK),
        ])
        .unwrap();
        assert_eq!(got.0, "0.1.4");
        assert_eq!(got.3, CH_ROLLBACK);
    }

    #[test]
    fn channel_priority_is_rollback_canary_latest_versions() {
        // 通道优先级即契约 §3 的步序；**与版本数字无关**。
        assert!(channel_rank(CH_ROLLBACK) < channel_rank(CH_CANARY));
        assert!(channel_rank(CH_CANARY) < channel_rank(CH_LATEST));
        assert!(channel_rank(CH_LATEST) < channel_rank(CH_VERSIONS));
        // 交叉验证：灰度版本低于 latest 时，灰度机仍取灰度
        let got = pick_best(vec![
            cand("0.9.9", 1, "a", CH_LATEST),
            cand("0.1.6-BETA.1", 900, "b", CH_CANARY),
        ])
        .unwrap();
        assert_eq!(got.3, CH_CANARY);
    }

    /// RC-G5 反向：判据必须能识别"取全量最高"的旧形态 —— 否则门禁是空转的。
    ///
    /// 旧形态与 §3 在**同一份元数据**上给出不同答案，故"改回旧实现"必然被测试抓到。
    #[test]
    fn pick_best_rejects_old_highest_of_all_form() {
        let meta = serde_json::json!({
            "dist-tags": { "latest": "0.1.5-BETA.3" },
            "versions": { "0.1.5-BETA.3": {}, "1.0.0-BETA.1": {} },
        });
        // 契约 §3 第 ③ 步：信 latest（BETA 数字大也不动）
        let picked = crate::release_channel::select(&meta, false).unwrap();
        assert_eq!(picked.version, "0.1.5-BETA.3");
        // 旧的"全量最高"会选 1.0.0-BETA.1 —— 两者不同，故本门禁非空转
        let old_would_pick = "1.0.0-BETA.1";
        assert_ne!(picked.version, old_would_pick, "RC-G5 反向自检失败");
    }

    #[test]
    fn pick_best_empty_is_none() {
        assert!(pick_best(vec![]).is_none(), "无候选必须返回 None（由调用方如实报错，RC-5）");
    }

    // ── 回退端到端（RC-2）：契约最关键的运维能力，必须有回归锚点 ──
    //
    // 缺口背景：回退目标**低于**当前版本，若 build_plan 只做版本比较会判 action=none
    //   → 前端显示"已是最新" → 回退到不了用户。修法是让**通道信号**（via=rollback）参与判定。
    // 本测试锁定该行为，防止将来有人"简化"回版本比较。

    fn pick(v: &str, via: &'static str) -> Result<LatestPick, String> {
        Ok(LatestPick { version: v.to_string(), origin: "test".into(), via: via.to_string() })
    }

    #[test]
    fn rollback_lower_version_still_triggers_action() {
        // 本机 0.1.5，回退目标 0.1.4（更低）→ **必须**判为需动手，而非"已是最新"
        let p = build_plan(Some("0.1.5".into()), pick("0.1.4", "rollback"));
        assert_eq!(p["action"], "upgrade", "回退目标更低时必须触发（否则 RC-2 端到端断裂）");
        assert_eq!(p["available"], true);
        assert_eq!(p["isRollback"], true);
        assert_eq!(p["latestVia"], "rollback");
        assert_eq!(p["latest"], "0.1.4");
    }

    #[test]
    fn same_version_via_rollback_is_noop() {
        // 回退目标 == 当前版本：无意义，不动手（避免无谓重装）
        let p = build_plan(Some("0.1.5".into()), pick("0.1.5", "rollback"));
        assert_eq!(p["action"], "none");
        assert_eq!(p["isRollback"], true);
    }

    #[test]
    fn non_rollback_lower_version_does_not_trigger() {
        // **反向对照**：非回退通道给出更低版本（陈旧 latest）→ 不得触发
        //   （这正是契约 §2 缺陷 1 的场景：陈旧 ≠ 回退，必须区分）
        let p = build_plan(Some("0.1.5".into()), pick("0.1.1-BETA.1", "latest"));
        assert_eq!(p["action"], "none", "陈旧 latest 不得被误判为回退");
        assert_eq!(p["isRollback"], false);
    }

    #[test]
    fn rollback_above_current_is_normal_upgrade() {
        // 回退 tag 指向更高版本（运维设错/已恢复正常）→ 按正常升级处理
        let p = build_plan(Some("0.1.5".into()), pick("0.1.6", "rollback"));
        assert_eq!(p["action"], "upgrade");
        assert_eq!(p["isRollback"], true);
    }

    #[test]
    fn not_installed_via_rollback_is_install() {
        let p = build_plan(None, pick("0.1.4", "rollback"));
        assert_eq!(p["action"], "install");
    }
}
