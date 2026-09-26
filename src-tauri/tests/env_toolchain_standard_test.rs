//! 环境工具链「检测 / 安装 / 下载」门禁（2026-09-16）。
//!
//! 事实源：docs/ENV-TOOLCHAIN-INSTALL-STANDARD.md（SSOT）。
//! 核心不变量：node 与 npm **并行同权** —— 只装 node 就判「环境就绪」，会在干净 Windows 上
//!   留下「node 在、npm 缺」，随后用不存在的 npm 去装内核，必失败。
//! 行为面（probe_npm 命中/兜底、版本仲裁）由各模块的内置单元测试驱动；
//!   本文件守**跨模块结构不变量**：契约字段、run_install 校验、旧 UI/事件残留。
//!
//! 注意：G-1/G-2/G-4 依赖改造落地，落地前**如实失败**（不得为转绿而放宽断言）。

use std::fs;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// 读源码文本。换行一律归一化为 LF：Windows 检出可能是 CRLF（actions/checkout 的
/// auto-normalize），而 G-12/G-13 的反向针脚是**跨行字面量**，带 `\r` 时永不匹配 →
/// 「已收口」的假绿。口径同各 `tests/*.rs` 的 `read()` 与 B57。
fn lf(s: String) -> String {
    if s.contains('\r') { s.replace("\r\n", "\n") } else { s }
}

fn read(rel: &str) -> String {
    lf(fs::read_to_string(root().join(rel)).unwrap_or_else(|e| panic!("读取 {} 失败: {}", rel, e)))
}

/// 递归收集目录下所有常规文件。
/// 「为什么」：引导页拆分为多文件后，样式/进度条残留可能落在 html 或尚未拆分的 js ——
///   只扫固定几个 js 就会漏网，门禁必须按目录整体收口。
fn walk_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(dir).unwrap_or_else(|e| panic!("读取目录 {} 失败: {}", dir.display(), e));
    for ent in entries {
        let p = ent.expect("目录项读取失败").path();
        if p.is_dir() {
            walk_files(&p, out);
        } else if p.is_file() {
            out.push(p);
        }
    }
}

fn walk(rel: &str) -> Vec<(PathBuf, String)> {
    let mut files = Vec::new();
    walk_files(&root().join(rel), &mut files);
    files
        .into_iter()
        .filter_map(|p| fs::read_to_string(&p).ok().map(|s| (p, lf(s))))
        .collect()
}

/// 返回 sig 所指函数的函数体（花括号配平）。
/// 「为什么」：G-2 要求断言「run_install **函数体内**」有 npm 校验，
///   纯 src.contains("...") 会被文件里任意位置的同名字符串骗过。
fn fn_body(src: &str, sig: &str) -> String {
    let start = src.find(sig).unwrap_or_else(|| panic!("未找到函数签名 {}", sig));
    let open = src[start..].find('{').map(|i| start + i).expect("函数体缺左花括号");
    let mut depth = 0i32;
    let mut end = open;
    for (i, b) in src.as_bytes().iter().enumerate().skip(open) {
        match b {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    end = i;
                    break;
                }
            }
            _ => {}
        }
    }
    src[open..=end].to_string()
}

/// 同 fn_body，但签名不存在时返回 None。
/// 为什么：G-8 要对**旧形态样本**跑判据，旧形态本来就没有 readEnv / applyToolchain，
///   辅助函数不能先把测试炸掉 —— 找不到归属函数就意味着没有任何一行是合法的。
fn fn_body_opt(src: &str, sig: &str) -> Option<String> {
    if src.contains(sig) {
        Some(fn_body(src, sig))
    } else {
        None
    }
}

/// 去掉行注释，只留代码。
/// 为什么：结构判据要在**函数体**里找证据 token，而函数体连着注释一起返回 ——
///   一段提到 `derive_usable` 的注释就能让 G-2 转绿，于是门禁校验的是「写过说明」而不是「调用了它」。
fn code_only(body: &str) -> String {
    body.lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// G-1：node_status 必须回传 npm 三字段，且 npm 可用性只出自**一条真实执行**的探针。
/// 不变量 T-1：npmOk 不得伪造 —— 字段存在还不够，必须能追溯到 probe_npm。
/// 2026-09-22（B5）：探针实现从命令层下沉到 `domain/probes.rs`，命令层只做组装 ——
///   原先命令层自己 spawn npm，于是「面板用的探针」与「契约用的探针」是两条代码路径，
///   同一次轮询里 npm 被执行两遍（第三遍在 derive_usable）。
#[test]
fn g1_node_status_exposes_real_npm_probe() {
    let cmd = read("src/commands/mod.rs");
    assert!(cmd.contains("npmOk"), "node_status 未回传 npmOk（npm 与 node 同权，缺失即漏判）");
    assert!(cmd.contains("npmPath"), "node_status 未回传 npmPath（UI/排障无法定位 npm）");
    // 判 false 时必须同时给出**为什么**：面板只有拿到原因才能区分「缺 npm」与「npm 拉不起来」。
    assert!(cmd.contains("npmWhy"), "node_status 未回传 npmWhy（不可用原因不上屏，排障只能靠猜）");
    assert!(
        cmd.contains("probes::dependents"),
        "node_status 未取用 domain/probes 的探测事实（npm 结论必须有唯一来源）"
    );
    assert!(
        !code_only(&cmd).contains("probe_npm_usable("),
        "命令层仍自行执行 npm（B5 后唯一调用点是 domain/probes.rs）—— 两条 spawn 路径会给出两个结论"
    );
    let pb = read("src/domain/probes.rs");
    assert!(
        pb.contains("probe_npm_usable("),
        "probes.rs 的 npmOk 未追溯 probe_npm_usable（禁止伪造 npm 存在，不变量 T-1b）"
    );
    let rt = read("src/runtime_contract.rs");
    assert!(rt.contains("fn probe_npm"), "runtime_contract 缺 probe_npm（npm 真实探测的唯一实现）");
    // 唯一探测口必须能**输出**失败原因；返回 Option 的版本会把三类根因合并成一个 None。
    assert!(
        rt.contains("pub fn probe_npm_usable(node: &Path, bin_dir: &Path) -> Result<NpmUsable, String>"),
        "probe_npm_usable 未以 Result<_, String> 暴露失败原因（不可用原因无法送到面板）"
    );
}

/// npm「可用」的证据 token：三者内部都会真实执行 `npm --version`（不变量 T-1b）。
/// 为什么不含 `probe_npm`：它只判断文件存在，而「存在 != 可用」正是本条链的根因形态。
const NPM_USABLE_TOKENS: [&str; 2] = ["derive_usable", "probe_npm_usable"];

fn has_npm_usable_call(body: &str) -> bool {
    NPM_USABLE_TOKENS.iter().any(|t| body.contains(t))
}

/// G-2：run_install 函数体内必须校验 npm —— 否则「安装成功」只保证 node，
/// 干净机器上 npm 仍缺失却 emit done（SSOT §1 的根因正是它）。
/// 2026-09-21（B4）：管线随安装语义一起住在 `src/domain/install.rs`。
#[test]
fn g2_run_install_validates_npm_inside_body() {
    let inst = read("src/domain/install.rs");
    let body = code_only(&fn_body(&inst, "fn run_install"));
    let final_body = code_only(&fn_body(&read("src/node.rs"), "fn finalize_install"));
    // 校验可以下沉到 node::finalize_install（G-3 要求 main.rs 只做组装），
    //   但下沉后必须在**被委派的那一侧**真实发生 —— 注释不算证据。
    assert!(
        has_npm_usable_call(&body)
            || (body.contains("finalize_install") && has_npm_usable_call(&final_body)),
        "run_install 未校验 npm（本地或经 node::finalize_install 委派）—— \
         装完 node 即报成功，npm 缺失会被误判为环境就绪"
    );
}

/// G-3：进度条只准由**真分母**驱动（SSOT §3.2 不变量 T-7）。
/// 条曾被整体删除，因为那时的分数按代码顺序编出来、与真实字节无关。后端现在只在拿到
/// Content-Length / dist.size 时才发非 null 比值，所以条可以回来 —— 但「唯一元素 + 唯一写入点 +
/// 无分母即隐藏」必须由门禁钉住：这三条一松，假条会以完全相同的形态长回来，而这次它带着合法外观。
#[test]
fn g3_progress_bar_is_denominator_driven() {
    let legacy = ["showProgress", "hideProgress", "progBar", "id=\"prog\""];
    let mut hits: Vec<String> = Vec::new();
    let mut meter_files: Vec<String> = Vec::new();
    for (p, text) in walk("bootstrap") {
        for (n, line) in text.lines().enumerate() {
            if legacy.iter().any(|b| line.contains(b)) {
                hits.push(format!("{}:{} {}", p.display(), n + 1, line.trim()));
            }
        }
        if text.contains("dlMeter") {
            meter_files.push(p.file_name().and_then(|s| s.to_str()).unwrap_or_default().to_string());
        }
    }
    assert!(hits.is_empty(), "bootstrap 仍残留旧的假进度条实现（SSOT §3.2 不变量 T-7）：\n{}", hits.join("\n"));
    meter_files.sort();
    assert_eq!(meter_files, vec!["10-ui.js".to_string(), "bootstrap.html".to_string()],
        "条的元素与写入点必须各只有一处（T-7）");
    let ui = read("bootstrap/js/10-ui.js");
    let meter = fn_body(&ui, "function installMeter");
    assert!(meter.contains("typeof ratio === 'number'"), "installMeter 没把比值是否为数当判据（T-7）");
    assert!(meter.contains("'none'"), "无分母时条必须隐藏而不是画 0%（0 会被读成「还没开始」）：\n{}", meter);
    assert!(meter.contains("el.value = r"), "条的值未经 installMeter 这一处：\n{}", meter);
    assert_eq!(ui.matches("el.value = ").count(), 1, "installMeter 之外还有第二处写条的值（T-7）");
    let init = read("bootstrap/js/80-init.js");
    assert!(init.contains("NS.install.text(p.kind, p.status, p.progress)"),
        "事件里的比值没被送进唯一渲染点（T-7）");
}

/// G-4：扫描 src 不得出现旧事件名（SSOT §2.4：改统一 install_* 且**无兼容层**）。
#[test]
fn g4_src_has_no_legacy_install_events() {
    let banned = ["env_progress", "env_done", "env_error", "shell_update_progress"];
    let mut hits: Vec<String> = Vec::new();
    for (p, text) in walk("src") {
        for (n, line) in text.lines().enumerate() {
            if banned.iter().any(|b| line.contains(b)) {
                hits.push(format!("{}:{} {}", p.display(), n + 1, line.trim()));
            }
        }
    }
    assert!(
        hits.is_empty(),
        "src 仍出现旧事件名（应统一为 install_progress/install_done/install_error）：\n{}",
        hits.join("\n")
    );
}

/// G-5：20-env.js 必须有**独立的** npm 分支，且文案自带 npm 字样。
/// 不变量 T-5：npm 缺失不得复用 node 分支（复用即「重装 node 补 npm」的死循环文案）。
#[test]
fn g5_env_js_has_standalone_npm_branch() {
    let js = read("bootstrap/js/20-env.js");
    // 2026-09-18：npmOk 改为**三态**（true/false/null=未知），只有 npmOk === true 才放行。
    //   故独立分支判据由 "npmOk === false" 收紧为 "npmOk !== true"（覆盖「缺」与「未知」）。
    let at = js
        .find("npmOk !== true")
        .expect("20-env.js 缺 npmOk !== true 独立分支（npm 缺失/未知不会被修复）");
    assert!(
        js.contains("npmOk === true"),
        "就绪判定必须要求 npmOk === true（文件存在 != 可用，不变量 T-1b）"
    );
    // 取分支起点后的窗口：足够覆盖分支体（probeMirrorThen + status 文案 + invoke）。
    let window: String = js[at..].chars().take(700).collect();
    assert!(
        window.contains("start_node_install") || window.contains("start_npm_install"),
        "npm 分支未触发任何安装/修复调用：\n{}",
        window
    );
    // 独立文案：分支窗口内必须有一句含 npm 的 status/文字，而不是照抄 node 分支。
    let mentions_npm = window
        .lines()
        .any(|l| (l.contains("status(") || l.contains("install.")) && l.to_lowercase().contains("npm"));
    assert!(
        mentions_npm,
        "npm 分支未使用独立文案（必须显式提到 npm，不得与 node 分支共用）：\n{}",
        window
    );
    // 原因上屏：npmOk=false 有三种根因（归档解残缺 / 垫片在本平台拉不起来 / npm 执行报错），
    //   处置彼此不同。只报「缺少 npm」会把排障推给用户 —— 后端已回传 npmWhy，前端必须用。
    assert!(
        window.contains("st.npmWhy"),
        "npm 分支未回显后端给出的不可用原因 npmWhy：\n{}",
        window
    );
}

/// G-6：10-ui.js 导出统一安装入口 NS.install，且含 begin/text/done/fail 四项。
/// 不变量 T-6：任何模块不得自行拼装下载/安装样式，一律经 NS.install.*。
#[test]
fn g6_ui_js_exports_unified_install_api() {
    let js = read("bootstrap/js/10-ui.js");
    let at = js
        .find("NS.install")
        .expect("10-ui.js 未导出 NS.install（安装/下载样式无唯一实现）");
    let region: String = js[at..].chars().take(600).collect();
    for key in ["begin", "text", "done", "fail"] {
        assert!(
            region.contains(key),
            "NS.install 缺 {} 方法（统一入口须含 begin/text/done/fail 四项）：\n{}",
            key,
            region
        );
    }
}

/// 抽出每个 `sig` 调用（到与之配平的右括号为止）。
/// 为什么按调用切块：kind 与 version 必须在**同一次发射里**对账。按字符窗口取会串到下一条
///   事件，于是「npm 的事件带 node 版本」这种串位永远查不出来。
/// 2026-09-21（B4）：签名不再是 `handle.emit(` —— 发射统一走 `install::done(`，
///   而「只允许一个发射点」由 G-12 单独钉。
fn call_args(src: &str, sig: &str) -> Vec<String> {
    let chars: Vec<char> = src.chars().collect();
    let sig: Vec<char> = sig.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i + sig.len() <= chars.len() {
        if chars[i..i + sig.len()] == sig[..] {
            let mut depth = 0usize;
            let mut j = i + sig.len() - 1; // 指向 sig 末尾的 '('
            while j < chars.len() {
                match chars[j] {
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
                j += 1;
            }
            if j < chars.len() {
                out.push(chars[i..=j].iter().collect());
                i = j + 1;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// run_install 签名 + 完成事件的版本归属，返回违规项（空 = 通过）。
/// 抽成纯函数是为了能对**旧形态样本**跑一遍：判据认不出旧形态就是空转。
fn g7_violations(inst: &str, cmd: &str) -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    let head = inst
        .lines()
        .find(|l| l.contains("fn run_install"))
        .unwrap_or("")
        .to_string();
    if !head.contains("NodeRuntime") {
        v.push("run_install 未返回运行期契约：node 与 npm 的事实被拆成字段子集，npm 版本无处可来".into());
    }
    let body = code_only(&fn_body(cmd, "pub fn start_node_install"));
    let done: Vec<String> = call_args(&body, "install::done(");
    if done.len() < 2 {
        v.push("install_done 未覆盖 node 与 npm 两个 kind（SSOT §2.4：完成事件按 kind 各发一条）".into());
    }
    let owns = |kind: &str, ver: &str| done.iter().any(|c| c.contains(kind) && c.contains(ver));
    if !owns("InstallKind::Node", "rt.version") {
        v.push("node 的完成事件未播报契约里的 node 版本".into());
    }
    if !owns("InstallKind::Npm", "npm_version") {
        v.push("npm 的完成事件没有 npm 自己的版本来源（只能拿 node 版本冒充）".into());
    }
    if done
        .iter()
        .any(|c| c.contains("InstallKind::Npm") && c.contains("rt.version"))
    {
        v.push("npm 的完成事件混入了 node 版本（kind 与 version 归属不一致）".into());
    }
    v
}

/// G-7：完成播报的 kind 与 version 必须归属一致。
/// 失效模式：管线只发一条 kind=npm 却带 node 的版本号 —— 引导页念出的
///   「npm 已就绪（v22.x）」从来不是 npm 的版本，而字符串拼接对类型系统完全合法，没人能发现。
#[test]
fn g7_install_done_events_carry_their_own_kind_version() {
    let inst = read("src/domain/install.rs");
    let cmd = read("src/commands/mod.rs");
    let hits = g7_violations(&inst, &cmd);
    assert!(
        hits.is_empty(),
        "G-7 失败：\n{}\n",
        hits.join("\n")
    );

    // 反向 1：旧形态（返回 (String, String)、只发一条 kind=npm 带 node 版本）必须逐条判红。
    let old_inst = "fn run_install(app: &H) -> Result<(String, String), Failure> {\n    Ok((p, v))\n}\n";
    let old_cmd = "pub fn start_node_install() -> R {\n    install::done(&handle, Npm, json!(version));\n}\n";
    let old = g7_violations(old_inst, old_cmd);
    for want in [
        "未返回运行期契约",
        "两个 kind",
        "npm 自己的版本来源",
    ] {
        assert!(
            old.iter().any(|s| s.contains(want)),
            "G-7 判据对旧形态的 {} 无反应（空转）：{:?}",
            want,
            old
        );
    }

    // 反向 2：两条事件都发、但 npm 那条串的仍是 node 版本 —— 串位必须单独被查出来。
    let mixed = "pub fn start_node_install() -> R {\n\
         install::done(&handle, InstallKind::Node, json!(rt.version));\n\
         install::done(&handle, InstallKind::Npm, json!(rt.version));\n\
         }\n";
    let mv = g7_violations("fn run_install() -> Result<NodeRuntime, E> { }\n", mixed);
    assert!(
        mv.iter().any(|s| s.contains("混入了 node 版本")),
        "G-7 对 kind/version 串位无反应：{:?}",
        mv
    );
}

/// 引导层工具链快照的结构性判据，返回违规项（空 = 通过）。
/// 同样对旧形态样本跑一遍，见 g8_toolchain_snapshot_has_single_owner 的反向断言。
fn g8_violations(files: &[(String, String)]) -> Vec<String> {
    let mut v = Vec::new();
    // 判定锚点是**归属函数的函数体**，不是行形状。
    //   为什么：旧形态同样带 withTimeout / 同样在 20-env.js 里，按形状放行等于放过它 ——
    //   判据要认的是「谁拥有这件事」，快照的拥有者是 applyToolchain，读取口的拥有者是 readEnv。
    let env_src = files
        .iter()
        .find(|(p, _)| p.ends_with("20-env.js"))
        .map(|(_, text)| text.clone())
        .unwrap_or_default();
    let owner = fn_body_opt(&env_src, "function applyToolchain").unwrap_or_default();
    let reader = fn_body_opt(&env_src, "function readEnv").unwrap_or_default();
    let mut writes = 0usize;
    let mut reads = 0usize;
    for (p, text) in files {
        for (i, line) in text.lines().enumerate() {
            let t = line.trim();
            if t.starts_with("//") {
                continue;
            }
            let tag = format!("{}:{} {}", p, i + 1, t);
            if t.contains("invoke('node_status')") {
                reads += 1;
                if !reader.contains(t) {
                    v.push(format!("node_status 的读取口不在 readEnv 内：{}", tag));
                }
            }
            if t.contains("NS.toolchain =") && !t.contains("emptyToolchain()") {
                writes += 1;
                if !owner.contains(t) {
                    v.push(format!("快照事实在 applyToolchain 之外被写入：{}", tag));
                }
            }
            if t.contains("NS.nodeVer") || t.contains("NS.npmVer") {
                v.push(format!("散装版本字段复活（应由 NS.toolchain 快照承载）：{}", tag));
            }
        }
    }
    if reads != 1 {
        v.push(format!("node_status 应有且只有一个读取口（超时预算与快照写入各写一遍即源于此），实为 {}", reads));
    }
    if writes != 1 {
        v.push(format!("快照事实应有且只有一个写入点（漏写一处不报错，只让「已就绪」少一半），实为 {}", writes));
    }
    v
}

/// G-8：工具链快照只有一个所有者、一个读取口，且就绪行同时含 node 与 npm。
/// 失效模式：版本字段散成 NS.nodeVer / NS.npmVer 两个字段、三个轮询点各读各的
///   —— 漏写一处不报错，只让「已就绪」少一半，并在重试时残留上一轮的值。
#[test]
fn g8_toolchain_snapshot_has_single_owner() {
    let files: Vec<(String, String)> = walk("bootstrap")
        .into_iter()
        .map(|(p, text)| (p.display().to_string(), text))
        .collect();
    let hits = g8_violations(&files);
    assert!(hits.is_empty(), "G-8 失败：\n{}\n", hits.join("\n"));

    let env_js = read("bootstrap/js/20-env.js");
    let apply = fn_body(&env_js, "function applyToolchain");
    assert!(
        apply.contains("st.npmVersion") && apply.contains("st.installed"),
        "applyToolchain 必须同时登记 node 与 npm 两个版本字段：\n{}",
        apply
    );
    let line = fn_body(&env_js, "function toolchainLine");
    assert!(
        line.contains("Node") && line.contains("npm")
            && line.contains("t.node") && line.contains("t.npm"),
        "就绪行必须同时念出 node 与 npm，且两者都取自快照：\n{}",
        line
    );
    // T-7b：诊断串与就绪行同源 —— 排障时「npm 探到了没有」不该再靠读代码猜。
    let diag = fn_body(&read("bootstrap/js/10-ui.js"), "function diagText");
    assert!(
        diag.contains("'node='") && diag.contains("'npm='"),
        "diagText 未同时输出 node 与 npm（不变量 T-7b）：\n{}",
        diag
    );

    // 反向：旧形态（散装字段 + 轮询点各读 node_status + 分支自行改写快照）必须逐条判红。
    let old = vec![(
        "x/20-env.js".to_string(),
        "NS.nodeVer = st.installed;\n\
         NS.withTimeout(NS.core.invoke('node_status'), 15000, 'q')\n\
         NS.toolchain = { node: st.installed };\n"
            .to_string(),
    )];
    let oh = g8_violations(&old);
    for want in ["散装版本字段", "读取口", "applyToolchain 之外"] {
        assert!(
            oh.iter().any(|s| s.contains(want)),
            "G-8 判据对旧形态的 {} 无反应（空转）：{:?}",
            want,
            oh
        );
    }
}

/// G-9：探针与消费者必须走**同一条 spawn 路径**。
/// 不变量 T-10：选择 npm 程序时按平台可执行性过滤，且平台层之外不得出现把程序包进 cmd 的调用。
/// 为什么要钉住这一点（Windows 实测根因之一）：`npm.cmd` 是 cmd.exe 的脚本，CreateProcessW 认不了它。
///   旧实现让探针经 `cmd /C` 跑通，于是面板报「npm 可用」，而真正的消费者（`npm install -g`、
///   `npm prefix -g`）用同一个路径直接 spawn 必失败 —— 包装把缺陷藏成了成功。
#[test]
fn g9_npm_probe_shares_the_consumer_spawn_path() {
    let rt = code_only(&read("src/runtime_contract.rs"));
    let body = fn_body(&rt, "pub fn probe_npm(");
    assert!(
        body.contains("is_directly_spawnable"),
        "probe_npm 未按平台可执行性筛选 npm 程序：不可执行的垫片会被直接交给消费者\n{}",
        body
    );
    let mut hits: Vec<String> = Vec::new();
    for (p, text) in walk("src") {
        let rel = p.display().to_string().replace('\\', "/");
        if rel.contains("/platform/") {
            continue;
        }
        for (n, line) in text.lines().enumerate() {
            if line.trim_start().starts_with("//") {
                continue;
            }
            let l = line.to_lowercase();
            if l.contains("cmd.exe") || l.contains("cmd /c") || (l.contains("\"cmd\"") && l.contains("\"/c\"")) {
                hits.push(format!("{}:{}", rel, n + 1));
            }
        }
    }
    assert!(
        hits.is_empty(),
        "平台层之外出现 cmd 包装（探针与消费者必须同源，否则不对称会重现）：{}",
        hits.join(", ")
    );
}

/// G-10：解包落定必须以「npm 载荷可用」为条件。
/// 不变量 T-11：解包器可以静默截断深层路径（Expand-Archive 对 >260 字符路径如此且退出码为 0），
///   只校验 node.exe 就会把半成品树报成安装成功，npm 仍然缺失。
#[test]
fn g10_commit_validates_npm_payload() {
    let plat = read("src/platform/mod.rs");
    let at = plat
        .find("fn commit_user_node")
        .expect("platform/mod.rs 缺 commit_user_node（解包落定的唯一出口）");
    let body = code_only(&fn_body(&plat[at..], "fn commit_user_node"));
    assert!(
        body.contains("probe_npm"),
        "commit_user_node 未校验 npm 载荷 —— 被截断的归档会被当作安装成功：\n{}",
        body
    );
}

/// G-11：真实归档的 Windows 实测步骤必须**真的跑到那一个测试**。
/// 不变量 T-12：`zip 解包 -> npm 可用` 这条链只由真机归档证明，静态门禁一律碰不到它。
/// 为什么要钉在 CI 步骤上：libtest 对 `--exact` 的短名是零命中且退出码 0，
///   于是「加了实测步骤」和「实测步骤什么都没测」在 CI 上长得一模一样 ——
///   而后者正是本轮问题拖到用户报障的原因（注释里的承诺不算证据，只看执行行）。
#[test]
fn g11_windows_real_artifact_step_actually_runs_the_ignored_test() {
    let y = read("../.github/workflows/build.yml");
    let full = "platform::windows::toolchain_tests::official_artifact_installs_usable_npm";
    assert!(
        y.contains(full),
        "CI 未按**全路径**执行真实归档测试：--exact 配短名会零命中且退出码 0（空转）"
    );
    assert!(
        y.contains("test result: ok. 1 passed"),
        "CI 未对真实归档步骤把「恰好一个测试通过」的关：零命中不会被发现"
    );
    let win = read("src/platform/windows.rs");
    let at = win
        .find("fn official_artifact_installs_usable_npm")
        .expect("windows.rs 缺 official_artifact_installs_usable_npm（真实归档的唯一实测口）");
    let guard = win[..at]
        .rfind("#[ignore")
        .expect("真实归档测试未标 ignore：它会联网下载归档，不该被普通测试集执行");
    assert!(
        at - guard < 240,
        "真实归档测试的 #[ignore] 离声明过远（可能标在别的测试上）：{} 字节",
        at - guard
    );
}

/// 安装事件唯一发射点的结构性判据（B4），返回违规项（空 = 通过）。
///
/// 四条子判据各自的失效模式（都是**本轮真实修掉**的）：
///   A `src` 里第二处按事件名发射 install_* —— 于是同一件事有两个作者，形态会分叉；
///   B `"kind": "node|npm|kernel|shell"` —— 安装 kind 的字面量只允许来自
///     `InstallKind::as_str` 的四个分支，否则「哪些东西可被安装」在 Rust 与前端各有一份账
///     （前端 `10-ui.js` 的 `INSTALL_TARGET` 有四个，旧枚举只有两个）。
///     判据只认这四个值：`error.rs` / `mirror.rs` 里另有**语义完全不同**的 `kind` 字段，
///     泛判 `"kind": "` 会把它们误伤（第一版就是这样，靠反向样本外的大面积假阳性才发现）。
///   C 「正在下载」/「正在安装内核」在 install.rs 之外被拼装 —— 文案与比值分两处写就会出现
///     自相矛盾（旧桌面壳下载行就是这样：文字说 MB、比值另算一套）。
///     反向保证：`install.rs` 里必须**出现**这两个措辞（见下面的前置断言），
///     否则「不得有第二处」会退化成「一处都没有」的空转门禁。
///   D `progress` 被写成裸数字 —— 旧的阶段分数（0.1 / 0.3 / 0.85）按代码顺序编造，
///     与真实进度无关；无分母必须发 `None`，前端的条据此隐藏（T-7，见 G-3）。
fn g12_violations(files: &[(String, String)]) -> Vec<String> {
    let mut v = Vec::new();
    for (path, text) in files {
        let in_owner = path.ends_with("src/domain/install.rs");
        for line in code_only(text).lines() {
            let t = line.trim();
            if t.is_empty() {
                continue;
            }
            for event in ["\"install_progress\"", "\"install_done\"", "\"install_error\""] {
                if t.contains(event) && !in_owner {
                    v.push(format!("{}: 第二处发射 {}（install_* 只能由 domain/install.rs 发）", path, event));
                }
            }
            for kind in ["node", "npm", "kernel", "shell"] {
                if t.contains(&format!("\"kind\": \"{}\"", kind)) {
                    v.push(format!("{}: 安装 kind 用了裸字符串 {:?}（唯一来源是 InstallKind::as_str）", path, kind));
                }
            }
            for wording in ["正在下载", "正在安装内核"] {
                if t.contains(wording) && !in_owner {
                    v.push(format!("{}: 在 install.rs 之外拼装安装/下载文案：{}", path, t));
                }
            }
            // 快照的写入点同样只允许在 owner 里带上比值；其余位置必须显式 Some(..)/None。
            if !in_owner && t.contains(".progress =") && !t.contains("Some(") && !t.contains("None") {
                v.push(format!("{}: progress 被写成裸数字（无分母必须是 None）：{}", path, t));
            }
        }
    }
    // 类型的形状就是判据本身：`f32` 无法表达「这一步没有可测分母」，那正是编造分数的入口。
    match files.iter().find(|(p, _)| p.ends_with("src/main.rs")) {
        Some((_, text)) if code_only(text).contains("progress: f32") => {
            v.push("RunState.progress 仍是裸 f32（应为 Option<f32>）".into())
        }
        _ => {}
    }
    v
}

/// G-12：安装/下载进度语义只有一个所有者（B4）。
#[test]
fn g12_install_progress_semantics_have_single_owner() {
    let files: Vec<(String, String)> = walk("src")
        .into_iter()
        .map(|(p, text)| (p.display().to_string().replace('\\', "/"), text))
        .collect();
    let hits = g12_violations(&files);
    assert!(hits.is_empty(), "G-12 失败：\n{}", hits.join("\n"));

    // 前置：唯一所有者必须**真的在发射**（否则上面的「不得有第二处」会退化成「一处都没有」）。
    let owner = read("src/domain/install.rs");
    for event in ["install_progress", "install_done", "install_error"] {
        assert!(
            owner.contains(&format!("\"{}\"", event)),
            "G-12 前置失败：install.rs 未发射 {}（SSOT §2.4 三条事件必须齐）",
            event
        );
    }
    assert!(
        code_only(&owner).contains("progress: Option<f32>"),
        "G-12 前置失败：install::push 的比值不是 Option<f32>"
    );
    // C 的反向保证：唯一所有者必须**真的**持有这两句措辞。
    for wording in ["正在下载", "正在安装内核"] {
        assert!(
            owner.contains(wording),
            "G-12 前置失败：install.rs 未持有「{}」措辞（判据 C 将无的放矢）",
            wording
        );
    }

    // 反向：旧形态（main.rs 里 push_status 自行发射 + 命令层裸 kind + 编造分数）必须逐条判红。
    let old = vec![
        (
            "src-tauri/src/main.rs".to_string(),
            "pub(crate) struct RunState { progress: f32 }\n\
             let _ = app.emit(\"install_progress\", json!({ \"kind\": kind.as_str(), \"progress\": p }));\n"
                .to_string(),
        ),
        (
            "src-tauri/src/commands/mod.rs".to_string(),
            "st.progress = 0.0;\n\
             let _ = handle.emit(\"install_progress\", json!({ \"kind\": \"shell\", \"status\": text }));\n\
             let text = format!(\"正在下载桌面版本 {:.1} MB…\", got);\n"
                .to_string(),
        ),
    ];
    let oh = g12_violations(&old);
    for want in [
        "第二处发射",
        "kind 用了裸字符串",
        "之外拼装",
        "裸数字",
        "裸 f32",
    ] {
        assert!(
            oh.iter().any(|s| s.contains(want)),
            "G-12 判据对旧形态的 {} 无反应（空转）：{:?}",
            want,
            oh
        );
    }
}

/// 环境探测记录的所有者判据（B5）。返回违规清单，空 = 结构成立。
///
/// 缺陷形状（同一类问题第三次换皮出现）：探测结论以「散装字段 + 各处手工拼接文案」的形态
///   散在 nodeprobe（只有 node 候选）、命令层（npm 现拼现用）与前端（`env_trace=` 再拼一遍）
///   三处。后果：加一个维度要改三遍文案，漏改不报错 —— 用户看到的即「检测环境时看不见
///   npm 检测」；而真正会打死内核安装的「prefix 不可写」从来没被探测过。
/// 判据：维度表、记录形态与两种渲染（文本 / JSON）都在 `domain/probes.rs`，其余只许消费。
fn g13_violations(files: &[(String, String)]) -> Vec<String> {
    let mut v = Vec::new();
    for (path, text) in files {
        let code = code_only(text);
        let in_owner = path.ends_with("src/domain/probes.rs");
        if path.ends_with(".js") || path.ends_with(".html") {
            // 维度名与顺序由壳回传的记录给出：前端抄一份表，就会和壳侧漂移。
            if text.contains(".probes") && !path.ends_with("js/10-ui.js") {
                v.push(format!("{}: 在 10-ui.js 之外解析探测记录（渲染出口只有一个）", path));
            }
            if text.contains("env_trace=") {
                v.push(format!("{}: 仍手工拼接逐候选追踪（应渲染壳给的记录表）", path));
            }
            continue;
        }
        if code.contains("pub enum Probe") && !in_owner {
            v.push(format!("{}: 第二份探测维度表（唯一来源是 domain/probes.rs 的 Probe）", path));
        }
        if code.contains("struct TraceEntry") {
            v.push(format!("{}: 仍有单维度追踪类型 TraceEntry（应登记为 Probe::Node 记录）", path));
        }
        // 类型的形状就是判据本身：裸 `bool` 无法表达「这一步还没跑完」，
        //   于是未知被显示成失败（旧实现把在飞步骤记成 ok=false）。
        //   只约束**记录形态**（含 `pub ms`）的类型：mirror 的逐源探测结果是另一件事，
        //   它的 ok 没有第三种可能（请求要么发了要么没发）。
        if code.contains("pub ok: bool") && code.contains("pub ms:") {
            v.push(format!("{}: 探测结论是裸 bool（三态必须是 Option<bool>，未知 != 失败）", path));
        }
        // 记录 → JSON 的字段形态只有一处（Record::json）：第二处拼字段就是第二份契约。
        if code.contains("\"probe\":") && code.contains("\"note\":") && !in_owner {
            v.push(format!("{}: 第二处拼装探测记录字段（形态在 Record::json）", path));
        }
        // npm 只能被执行一次（T-1b 的真实执行 + T-10 的同一条 spawn 路径）。
        //   允许出现的地方：定义与内部复用（runtime_contract）、探针所有者（probes）。
        if code.contains("probe_npm_usable(") && !in_owner && !path.ends_with("src/runtime_contract.rs") {
            v.push(format!("{}: 自行调用 npm 可用性探针（探针只有一个所有者）", path));
        }
    }
    v
}

/// G-13：环境探测记录只有一个所有者，且所有者确实持有全部维度与三态形态。
#[test]
fn g13_env_probe_records_have_single_owner() {
    let mut files: Vec<(String, String)> = walk("src")
        .into_iter()
        .map(|(p, text)| (p.display().to_string().replace('\\', "/"), text))
        .collect();
    files.extend(
        walk("bootstrap")
            .into_iter()
            .map(|(p, text)| (p.display().to_string().replace('\\', "/"), text)),
    );
    let hits = g13_violations(&files);
    assert!(hits.is_empty(), "G-13 失败：\n{}", hits.join("\n"));

    // 前置：所有者必须**真的**登记了四个维度并持有两种渲染 —— 否则上面的判据会退化成「一处都没有」。
    let owner = read("src/domain/probes.rs");
    for arm in ["Node", "Npm", "Registry", "Prefix"] {
        assert!(
            owner.contains(&format!("Probe::{}", arm)),
            "G-13 前置失败：probes.rs 未登记维度 {}（判据将无的放矢）",
            arm
        );
    }
    assert!(owner.contains("pub fn render"), "G-13 前置失败：缺文本渲染唯一出口");
    assert!(owner.contains("pub fn json"), "G-13 前置失败：缺 JSON 渲染（面板字段来自它）");
    assert!(
        code_only(&owner).contains("ok: Option<bool>"),
        "G-13 前置失败：记录结论不是三态 Option<bool>"
    );
    assert!(
        owner.contains("DEPENDENT_TTL"),
        "G-13 前置失败：依赖维度无复用窗口（node_status 每 400ms 轮询，每次都 spawn 子进程即新的卡死源）"
    );
    // 前缀探测会 stat/写文件：非本地固定盘必须先被挡掉（网络盘上的 exists() 本身就无界）。
    assert!(
        owner.contains("is_local_fixed_dir"),
        "G-13 前置失败：prefix 探针未做本地固定盘判定（可能把探测拖成网络盘阻塞）"
    );
    // node 侧必须复用同一形态，而不是留一份自己的追踪类型。
    let np = read("src/nodeprobe.rs");
    assert!(np.contains("Probe::Node"), "nodeprobe 的逐候选结论未登记为 Probe::Node 记录");
    assert!(np.contains("Record::pending"), "nodeprobe 的在飞步骤未以「未知」形态入表");

    // 反向：旧形态（单维度 TraceEntry + 裸 bool + 命令层自拼 JSON + 前端再拼一段 + 第二处探针）
    //   必须逐条判红 —— 逐条列出，防止某一条判据空转。
    let old = vec![
        (
            "src-tauri/src/nodeprobe.rs".to_string(),
            "pub struct TraceEntry { pub source: String, pub ms: u128, pub ok: bool }\n".to_string(),
        ),
        (
            "src-tauri/src/commands/mod.rs".to_string(),
            "pub enum Probe { Node, Npm }\n\
             o[\"trace\"] = serde_json::json!({ \"probe\": t.source, \"note\": t.note });\n\
             let u = crate::runtime_contract::probe_npm_usable(p, b);\n"
                .to_string(),
        ),
        (
            "src-tauri/bootstrap/js/20-env.js".to_string(),
            "st.probes.map(function (p) { return p.path; });\n'env_trace=' + x\n".to_string(),
        ),
    ];
    let oh = g13_violations(&old);
    for want in [
        "第二份探测维度表",
        "TraceEntry",
        "裸 bool",
        "第二处拼装探测记录字段",
        "自行调用 npm",
        "10-ui.js 之外",
        "逐候选追踪",
    ] {
        assert!(
            oh.iter().any(|s| s.contains(want)),
            "G-13 判据对旧形态的 {} 无反应（空转）：{:?}",
            want,
            oh
        );
    }
}

// ── G-14：观测报告（P7）投放侧 ──
//
// 缺陷面：内核的环境表单多了一维「桌面壳所见」，读的是 `<状态根>/supervisor/shell-report.json`。
//   这一份由壳投，投错形态（改键名、丢 schema、把三态折成 bool）就等于对内核**整份不存在**，
//   而面板只会说「壳还没报过」——排查方向被指到一台没装壳的机器上。
// 更细的一根线：报告必须**投影**既有探针结论。壳里再跑一次 npm/再起一次子进程，就成了
//   「面板用的探针」与「契约用的探针」之外的第三条路径（T-14/T-10 已经为这条栽过两次）。
//
// 锁定不变量（T-18）：
//   形态：schema=1 握手、文件名、七个契约键、原子写（tmp + rename）
//   纪律：不拼记录字段（形态在 Record::json）、不折叠三态、不自行采集
//   接线：全仓有且只有一个投放点，且它就在 node_status 的同一轮探测出口
// 失效模式：第二处 publish() 让同一轮探测投出两份（时刻与频控双双失控）；自拼字段让内核读成 null。
fn g14_violations(files: &[(String, String)]) -> Vec<String> {
    let mut v = Vec::new();
    let code = |suffix: &str| -> String {
        files
            .iter()
            .find(|(p, _)| p.ends_with(suffix))
            .map(|(_, t)| code_only(t))
            .unwrap_or_default()
    };
    let owner = code("src/shell_report.rs");
    for needle in [
        "pub const SCHEMA: u32 = 1",
        "\"shell-report.json\"",
        "\"writtenBy\"",
        "\"node\"",
        "\"npm\"",
        "\"prefix\"",
        "\"registry\"",
        "\"records\"",
        "\"at\"",
        "with_extension(\"json.tmp\")",
        "std::fs::rename(",
    ] {
        if !owner.contains(needle) {
            v.push(format!("观测报告契约形态缺失 {}（内核按整份读不出处理）", needle));
        }
    }
    if owner.contains("\"note\":") {
        v.push("在 shell_report.rs 里拼探测记录字段（形态的唯一来源是 Record::json，拼一份就是第二份契约）".to_string());
    }
    for collapse in ["unwrap_or(false)", "unwrap_or(true)", "unwrap_or_default()"] {
        if owner.contains(collapse) {
            v.push(format!("三态被折叠成 bool（{}）：壳判不出必须原样投 null", collapse));
        }
    }
    for own in ["std::process::Command", "probe_npm_usable(", "ureq::"] {
        if owner.contains(own) {
            v.push(format!("观测报告自行采集（{}）：采集的唯一所有者是 nodeprobe / domain::probes / mirror", own));
        }
    }
    // 按**调用次数**计数，不按文件计数：同一文件里两处 publish() 同样会把同一轮探测投两份
    //   （时刻与频控双双失控），按文件数判它永远等于 1 —— 那是一条认不出失败形态的判据。
    let mut posts: Vec<String> = Vec::new();
    for (p, text) in files {
        if p.ends_with("src/shell_report.rs") {
            continue;
        }
        for _ in 0..code_only(text).split("shell_report::publish(").count() - 1 {
            posts.push(if p.ends_with("src/commands/mod.rs") { "commands".to_string() } else { p.clone() });
        }
    }
    if posts.len() != 1 {
        v.push(format!("观测报告应有且只有一个投放点（多处投放=同一轮探测投两份，频控与时刻双双失控），实为 {:?}", posts));
    } else if posts[0] != "commands" {
        v.push(format!("投放点必须就在 node_status 的探测出口（commands/mod.rs），实为 {}", posts[0]));
    }
    if !code("src/main.rs").contains("mod shell_report;") {
        v.push("main.rs 未登记 shell_report 模块（文件进不了产线，是死代码）".to_string());
    }
    v
}

/// G-14：观测报告的形态、投影纪律与接线。
#[test]
fn g14_shell_report_probes_nothing_and_posts_once() {
    let files: Vec<(String, String)> = walk("src")
        .into_iter()
        .map(|(p, text)| (p.display().to_string().replace('\\', "/"), text))
        .collect();
    let hits = g14_violations(&files);
    assert!(hits.is_empty(), "G-14 失败：\n{}", hits.join("\n"));

    // 前置：投影必须**真的**接在三个采集所有者上 —— 否则「不得自行采集」会退化成「一处都没有」。
    let owner = files
        .iter()
        .find(|(p, _)| p.ends_with("src/shell_report.rs"))
        .map(|(_, t)| t.clone())
        .expect("G-14 前置失败：src/shell_report.rs 不存在");
    for reuse in [
        "crate::nodeprobe::Outcome",
        "crate::domain::probes",
        "crate::mirror::cached()",
        ".json()",
    ] {
        assert!(
            owner.contains(reuse),
            "G-14 前置失败：shell_report.rs 未复用采集所有者 {}（判据将无的放矢）",
            reuse
        );
    }
}

/// 反向夹具：旧形态必须被同一判据逐条认出，否则 G-14 只是装饰。
#[test]
fn g14_reverse_old_form_is_caught() {
    let old = r#"
fn report(npm_ok: Option<bool>) -> serde_json::Value {
    let ok = npm_ok.unwrap_or(false);
    std::process::Command::new("npm").spawn();
    serde_json::json!({ "probe": "prefix", "note": "x", "value": ok })
}
"#;
    let files = vec![
        ("src/shell_report.rs".to_string(), old.to_string()),
        (
            "src/commands/mod.rs".to_string(),
            "crate::shell_report::publish(&a, &b);\ncrate::shell_report::publish(&c, &d);\n".to_string(),
        ),
        ("src/main.rs".to_string(), "mod node;\n".to_string()),
    ];
    let hits = g14_violations(&files);
    for want in [
        "契约形态缺失",
        "拼探测记录字段",
        "三态被折叠",
        "自行采集",
        "只有一个投放点",
        "未登记 shell_report 模块",
    ] {
        assert!(
            hits.iter().any(|s| s.contains(want)),
            "G-14 判据对旧形态的 {} 无反应（空转）：{:?}",
            want,
            hits
        );
    }

    // 另一失效形态：次数对但位置错 —— 投放点挪到安装命令里（同一轮探测之外，时刻与载荷双双对不上）。
    let misplaced = vec![
        ("src/shell_report.rs".to_string(), String::new()),
        (
            "src/domain/install.rs".to_string(),
            "crate::shell_report::publish(&a, &b);\n".to_string(),
        ),
        ("src/main.rs".to_string(), "mod shell_report;\n".to_string()),
    ];
    let mh = g14_violations(&misplaced);
    assert!(
        mh.iter().any(|s| s.contains("投放点必须就在")),
        "G-14 判据认不出「唯一但位置错」的投放点（空转）：{:?}",
        mh
    );
}
