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

fn read(rel: &str) -> String {
    fs::read_to_string(root().join(rel)).unwrap_or_else(|e| panic!("读取 {} 失败: {}", rel, e))
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
        .filter_map(|p| fs::read_to_string(&p).ok().map(|s| (p, s)))
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

/// G-1：node_status 必须回传 npm 两字段，且来源是**真实探测**（probe_npm）。
/// 不变量 T-1：npmOk 不得伪造 —— 字段存在还不够，必须能追溯到 probe_npm。
#[test]
fn g1_node_status_exposes_real_npm_probe() {
    let cmd = read("src/commands/mod.rs");
    assert!(cmd.contains("npmOk"), "node_status 未回传 npmOk（npm 与 node 同权，缺失即漏判）");
    assert!(cmd.contains("npmPath"), "node_status 未回传 npmPath（UI/排障无法定位 npm）");
    assert!(cmd.contains("probe_npm"), "node_status 的 npmOk 未追溯 probe_npm（禁止伪造 npm 存在）");
    // 判 false 时必须同时给出**为什么**：面板只有拿到原因才能区分「缺 npm」与「npm 拉不起来」。
    assert!(cmd.contains("npmWhy"), "node_status 未回传 npmWhy（不可用原因不上屏，排障只能靠猜）");
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
#[test]
fn g2_run_install_validates_npm_inside_body() {
    let main = read("src/main.rs");
    let body = code_only(&fn_body(&main, "fn run_install"));
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

/// G-3：递归扫描 bootstrap（含 .html/.js）不得残留进度条实现。
/// 不变量 T-7：进度条与 showProgress/hideProgress 全部移除，安装/下载只留文字。
#[test]
fn g3_bootstrap_has_no_progress_bar_leftovers() {
    let banned = ["showProgress", "hideProgress", "progBar", "id=\"prog\""];
    let mut hits: Vec<String> = Vec::new();
    for (p, text) in walk("bootstrap") {
        for (n, line) in text.lines().enumerate() {
            if banned.iter().any(|b| line.contains(b)) {
                hits.push(format!("{}:{} {}", p.display(), n + 1, line.trim()));
            }
        }
    }
    assert!(
        hits.is_empty(),
        "bootstrap 仍残留进度条实现（SSOT §3.2 不变量 T-7）：\n{}",
        hits.join("\n")
    );
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

/// 抽出每个 `handle.emit(` 调用（到与之配平的右括号为止）。
/// 为什么按调用切块：kind 与 version 必须在**同一次 emit 内**对账。按字符窗口取会串到下一条
///   事件，于是「npm 的事件带 node 版本」这种串位永远查不出来。
fn emit_calls(src: &str) -> Vec<String> {
    let chars: Vec<char> = src.chars().collect();
    let sig: Vec<char> = "handle.emit(".chars().collect();
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
fn g7_violations(main: &str, cmd: &str) -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    let head = main
        .lines()
        .find(|l| l.contains("fn run_install"))
        .unwrap_or("")
        .to_string();
    if !head.contains("NodeRuntime") {
        v.push("run_install 未返回运行期契约：node 与 npm 的事实被拆成字段子集，npm 版本无处可来".into());
    }
    let body = code_only(&fn_body(cmd, "pub fn start_node_install"));
    let done: Vec<String> = emit_calls(&body)
        .into_iter()
        .filter(|c| c.contains("\"install_done\""))
        .collect();
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
    let main = read("src/main.rs");
    let cmd = read("src/commands/mod.rs");
    let hits = g7_violations(&main, &cmd);
    assert!(
        hits.is_empty(),
        "G-7 失败：\n{}\n",
        hits.join("\n")
    );

    // 反向 1：旧形态（返回 (String, String)、只发一条 kind=npm 带 node 版本）必须逐条判红。
    let old_main = "fn run_install(app: &H) -> Result<(String, String), Failure> {\n    Ok((p, v))\n}\n";
    let old_cmd = "pub fn start_node_install() -> R {\n    let _ = handle.emit(\"install_done\", json!({ \"kind\": Npm.as_str(), \"version\": version }));\n}\n";
    let old = g7_violations(old_main, old_cmd);
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
         let _ = handle.emit(\"install_done\", json!({ \"kind\": InstallKind::Node.as_str(), \"version\": rt.version }));\n\
         let _ = handle.emit(\"install_done\", json!({ \"kind\": InstallKind::Npm.as_str(), \"version\": rt.version, \"note\": rt.npm_version }));\n\
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
/// 不变量 T-8：选择 npm 程序时按平台可执行性过滤，且平台层之外不得出现把程序包进 cmd 的调用。
/// 为什么要钉住这一点（Windows 实测根因之一）：`npm.cmd` 是 cmd.exe 的脚本，CreateProcessW 认不了它。
///   旧实现让探针经 `cmd /C` 跑通，于是面板报「npm 可用」，而真正的消费者（`npm install -g`、
///   `npm prefix -g`）用同一个路径直接 spawn 必失败 —— 包装把缺陷藏成了成功。
#[test]
fn g9_npm_probe_shares_the_consumer_spawn_path() {
    let rt = read("src/runtime_contract.rs");
    let at = code_only(&rt)
        .find("pub fn probe_npm(")
        .expect("runtime_contract 缺 pub fn probe_npm(（npm 程序选择的唯一实现口）");
    assert!(
        code_only(&rt)[at..].contains("is_directly_spawnable"),
        "probe_npm 未按平台可执行性筛选 npm 程序：不可执行的垫片会被直接交给消费者"
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
/// 不变量 T-9：解包器可以静默截断深层路径（Expand-Archive 对 >260 字符路径如此且退出码为 0），
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
