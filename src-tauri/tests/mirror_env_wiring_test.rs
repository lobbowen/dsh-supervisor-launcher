//! 壳仓「证据写不出去 / 闸门用错尺」回归门禁。
//!
//! ## 缺陷 ①（跨仓契约）：壳算出的测速结论到不了内核
//!
//! \`mirror.rs::warmup_async\` 曾把 npm 最快源与延迟只放进内存 \`ProbeSnapshot\`，
//! 全仓没有任何地方写 \`m.selected_npm\`（只在 load 时读、在 mirror_set 时置 None）→
//! 契约里的 \`selected\` **永远是 null** → 内核「优先采用壳投放的选择」分支永不执行。
//!
//! schema3 之后这条链改成：壳把**逐源实测**落 mirrors.json 并投进契约 \`measurements\`；
//! 选择（mode / manualOrigin / 候选列表）搬到内核自持的 registry-choice.json，壳从不写那一份。
//! 「用户固定过源」因此不再让镜像目录停止更新 —— 旧实现读到 mode=manual 就整份不重写，
//! 固定一次 = 目录永久停更（新镜像上线、目录里某个源死掉，内核都再也拿不到）。
//!
//! 附带第二处缺陷（潜伏）：唯一带延迟的调用点 \`node.rs\` 传的是 **Node 源**延迟，
//! 却与 npm 语义的 \`selected\` 配对 —— 只因 origin 恒为空才未显形。
//!
//! ## 缺陷 ②（失效模式 e：闸门恒不可达）：Node 安装全程零进度/零状态反馈
//!
//! 前端 \`env_progress\` 监听器原为 \`if (p.busy) { ... }\`，而该条件**永远为假**：
//! 带 busy 的唯一 emit 是 \`commands/mod.rs\` 的 \`crate::log(&s)\`，
//! 而它在**前一行刚把 busy 置回 false**；安装过程的进度事件来自 \`main.rs::push_status\`，
//! 其 payload **根本不含 busy**。
//! （上述两处**都已不存在**：\`push_status\` 于 B4 随安装语义整体迁入
//!   \`domain/install.rs\`，本文件因此只看 payload 形态，不看 main.rs。）
//! 后果：装 Node（30~90MB、慢网数分钟）期间引导页停在静态文案，
//! Rust 侧 0.1/0.2/0.3/0.8 进度与状态**全被丢弃**。
//!
//! ## 锁定不变量
//!   M-a  每轮 npm 逐源探测都落盘并重投契约（预热与面板重测两条路径共用同一个出口）
//!   M-b  契约只交证据（catalog/probe/measurements）：任何**选择**字段出现即红；
//!        probe.timeoutMs 由 PROBE_TIMEOUT 派生，schema 为 3
//!   M-b2 导出不含「读到 mode/manual 就跳过」的冻结支，且原子写
//!   M-c  node.rs 不再把 Node 侧延迟传给 npm 语义的导出，也不碰 npm 实测
//!   M-d  内核的手动选择从内核自持的选择文档读；壳自己投的契约不参与
//!   M-e  地址形态只有一把尺（registry_base）；产物地址另过主机闸（asset_url），
//!        生产代码里的 \`starts_with("http://")\` 弱判据必须绝迹
//!   E-a  install_progress 监听器不再以 busy 为闸，且经统一入口展示**文字**（SSOT 2.4/3.3）
//!   E-b  监听器不自行画进度条（SSOT 3.2：条只准由真分母驱动，唯一渲染点在 10-ui.js）
//!   E-c  安装语义的唯一所有者 \`domain/install.rs\` 发统一 install_progress {kind,status,progress}
//!        （不再带 busy 补丁；发射点不在 main.rs，见 B4）
//!   E-d  反向：判据能识别「以 busy 为闸」的旧形态（门禁非空转）

use std::fs;
use std::path::PathBuf;

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// 读取待检查文件，并**归一化换行为 LF**。
///
/// 2026-09-13：这些门禁大量做**多行源码片段**的文本断言，而 Windows 检出
///   可能是 CRLF（core.autocrlf + 本仓原先无 .gitattributes）→ 内嵌换行的针脚永不匹配：
///     · 正向 find → 退化为 usize::MAX（失败）；
///     · 反向 !contains → 退化为恒真（假绿、门禁空转，比失败更糟）。
///   实测：把工作树整体转成 CRLF 后跑全套，update_guard_test 的 G6-g 立刻失败 ——
///   这正是 Windows leg 在 CI 上红掉的原因（macOS/Linux 是 LF 故全绿）。
///   修法：**在读取处归一化**，使断言在任何平台检查的是同一件事。
fn read(rel: &str) -> String {
    let s = fs::read_to_string(manifest_dir().join(rel))
        .unwrap_or_else(|e| panic!("读取 {} 失败: {}", rel, e));
    if s.contains('\r') { s.replace("\r\n", "\n") } else { s }
}

/// 去掉整行注释后的代码（本仓多次被自己的说明文字骗过）。
fn strip_comments(src: &str) -> String {
    src.lines()
        .filter(|l| {
            let t = l.trim_start();
            !t.starts_with("//")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// 只取 `mod tests` 之前的生产代码。
///
/// 形态尺的门禁判的是「不许再用 starts_with 弱判据」，而 mirror.rs 自己的单测里合法地
/// 用 `starts_with("https://cdn…")` 断言候选表 —— 不排除测试区就会假红，反过来为了变绿
/// 放宽判据又会漏掉生产代码里的真弱判据。
fn prod_code(rel: &str) -> String {
    let code = strip_comments(&read(rel));
    match code.find("#[cfg(test)]") {
        Some(i) => code[..i].to_string(),
        None => code,
    }
}

/// 取某个函数签名的**完整函数体**（大括号配平，含首尾花括号）。
///
/// 旧写法是 `&code[i..i + 4000]` 定长切片，那会把相邻函数的语句卷进来：
/// 「契约里没有 selected」这类反向判据会因为邻居的赋值而假红，正向判据会因为邻居而假绿。
/// 判据要成立，搜索范围必须恰好是被测函数自己。
fn body_from(code: &str, sig: &str) -> String {
    let i = code.find(sig).unwrap_or_else(|| panic!("未找到签名 {}", sig));
    let tail = &code[i..];
    let open = tail.find('{').unwrap_or_else(|| panic!("{} 没有函数体", sig));
    let chars: Vec<char> = tail[open..].chars().collect();
    let mut depth = 0usize;
    for (n, c) in chars.iter().enumerate() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return chars[..=n].iter().collect();
                }
            }
            _ => {}
        }
    }
    panic!("{} 的函数体大括号不配平", sig)
}

/// 抽出 JSON 字面量的键名（`"name":` 形态，键只认 ASCII 字母与下划线）。
/// 中文文案里的引号不会带 ASCII 字母紧跟冒号，因此不会被误判为键。
fn json_keys(body: &str) -> Vec<String> {
    let b = body.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'"' {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        while j < b.len() && b[j] != b'"' {
            j += 1;
        }
        if j >= b.len() {
            break;
        }
        let key = &b[i + 1..j];
        let mut k = j + 1;
        while k < b.len() && matches!(b[k], b' ' | b'\t' | b'\n') {
            k += 1;
        }
        if !key.is_empty()
            && key.iter().all(|c| c.is_ascii_alphabetic() || *c == b'_')
            && k < b.len()
            && b[k] == b':'
        {
            out.push(String::from_utf8_lossy(key).to_string());
        }
        i = j + 1;
    }
    out
}

/// **选择**类键：所有权在内核自持的 registry-choice.json，出现在壳投的契约里即为回归。
const CHOICE_KEYS: [&str; 4] = ["mode", "manualOrigin", "selected", "origins"];

/// 契约文档里泄漏了哪几个选择键（返回命中的键名，便于失败时回显证据）。
fn choice_leaks(doc: &str) -> Vec<String> {
    json_keys(doc)
        .into_iter()
        .filter(|k| CHOICE_KEYS.contains(&k.as_str()))
        .collect()
}

/// 判据本体：是否把壳自己投出的契约当成用户意图来源。
/// 针脚带引号，所以 `registry-choice.json` 不会被误判 —— 内核自持的选择文档正是该读的那一份。
fn reads_shell_contract(body: &str) -> bool {
    body.contains("\"registry.json\"")
}

/// 判据本体：一轮探测是否真的走到了「落盘 + 重投」。
/// 反向样本必须调用**同一份**判定代码，否则反向断言只是把常量比常量（恒真、门禁空转）。
fn persists_and_reexports(body: &str) -> bool {
    body.contains("record_npm_measurements(")
        || (body.contains("save(&m)") && body.contains("export_to_kernel(&m)"))
}

/// 判据本体：导出路径里是否存在「读到 mode/manual 就不写」的冻结支。
fn has_manual_freeze(body: &str) -> bool {
    body.contains("Some(\"manual\")") || body.contains(".get(\"mode\")")
}

/// 判据本体：是否用「前缀字符串」代替真正的 URL 校验。
fn has_weak_scheme_check(body: &str) -> bool {
    body.contains("starts_with(\"http://\")") || body.contains("starts_with(\"https://\")")
}

#[test]
fn m_a_every_probe_round_persists_and_reexports() {
    let code = prod_code("src/mirror.rs");
    // 「只在一处」的落盘出口本体：先确认它自己真的落盘 + 重投。
    let rec = body_from(&code, "pub fn record_npm_measurements");
    assert!(
        rec.contains("m.npm_measurements = "),
        "M-a FAIL record_npm_measurements 未把逐源实测写进 Mirrors，实际体: {:?}",
        rec
    );
    assert!(
        rec.contains("save(&m)"),
        "M-a FAIL record_npm_measurements 落盘后未 save（重启即丢证据）"
    );
    assert!(
        rec.contains("export_to_kernel(&m)"),
        "M-a FAIL record_npm_measurements 未重投契约（内核读到的还是上一轮结论）"
    );
    // 两条探测入口都必须走这个出口，而不是各自手写一份落盘。
    let warm = body_from(&code, "pub fn warmup_async");
    assert!(
        persists_and_reexports(&warm),
        "M-a FAIL 预热未走统一落盘出口 —— 内核拿不到本轮实测"
    );
    let cmd = prod_code("src/commands/mod.rs");
    assert!(
        cmd.contains("record_npm_measurements("),
        "M-a FAIL 面板重测未走统一落盘出口 —— 结果只留在内存，内核仍按旧证据选源"
    );
    // 反向：「只放内存快照」的旧形态必须被同一判据判为不合格。
    let old_form = "let snapshot = ProbeSnapshot { best: npm_best, latency: lat };";
    assert!(
        !persists_and_reexports(old_form),
        "M-a FAIL 判据无法识别只写内存的旧形态 —— 门禁空转"
    );
}

#[test]
fn m_b_contract_carries_evidence_only() {
    let code = prod_code("src/mirror.rs");
    let doc = body_from(&code, "fn contract_doc");
    let leaked = choice_leaks(&doc);
    assert!(
        leaked.is_empty(),
        "M-b FAIL 契约写了选择字段 {:?} —— 那份所有权在内核的 registry-choice.json，壳一写就等于覆盖用户固定",
        leaked
    );
    assert!(
        doc.contains("\"catalog\": m.npm") && doc.contains("\"measurements\": "),
        "M-b FAIL 契约未同时交目录与逐源实测，实际键集合: {:?}",
        json_keys(&doc)
    );
    // 探测超时必须由 PROBE_TIMEOUT 派生：两侧超时不同 → 介于其间的源一侧可达一侧不可达，选源再分叉。
    assert!(
        doc.contains("PROBE_TIMEOUT.as_millis() as u64"),
        "M-b FAIL probe.timeoutMs 未由 PROBE_TIMEOUT 派生（写死字面量即两侧口径漂移）"
    );
    assert!(
        code.contains("pub const CONTRACT_SCHEMA: u64 = 3;"),
        "M-b FAIL schema 不为 3：内核按旧 schema 会把 measurements 当缺失"
    );
    // 反向：v2 旧契约（含 selected/mode/manualOrigin/origins）必须被同一判据全部揪出。
    let v2 = serde_json::json!({
        "schema": 2, "writtenBy": "shell@1.2.8", "mode": "auto",
        "manualOrigin": null, "selected": { "origin": "https://x/" }, "origins": ["https://y/"],
    });
    let want: Vec<String> = CHOICE_KEYS.iter().map(|k| k.to_string()).collect();
    assert_eq!(
        choice_leaks(&v2.to_string()),
        want,
        "M-b FAIL 泄漏判据认不出 v2 旧形状 —— 门禁空转"
    );
}

/// 导出路径不得因「内核固定过源」而不写目录 —— 这正是镜像目录永久停更的根因。
#[test]
fn m_b2_export_has_no_manual_freeze_gate() {
    let code = prod_code("src/mirror.rs");
    let export = body_from(&code, "pub fn export_to_kernel");
    assert!(
        !has_manual_freeze(&export),
        "M-b2 FAIL 导出仍读 mode/manual 决定是否跳过（固定一次源 = 目录永久停更），实际体: {:?}",
        export
    );
    assert!(
        export.contains("contract_doc(m)"),
        "M-b2 FAIL 导出未走 contract_doc（绕过纯函数就无法被 m_b 钉住形状）"
    );
    assert!(
        export.contains("rename(&tmp, &path)"),
        "M-b2 FAIL 导出非原子写（内核可能读到半份 JSON）"
    );
    let old_freeze = "if v.get(\"mode\").and_then(|x| x.as_str()) == Some(\"manual\") { return Ok(()); }";
    assert!(
        has_manual_freeze(old_freeze),
        "M-b2 FAIL 判据认不出旧的 manual 冻结支 —— 门禁空转"
    );
}

#[test]
fn m_c_node_export_does_not_touch_npm_evidence() {
    let code = prod_code("src/node.rs");
    assert!(
        !code.contains("export_to_kernel_with"),
        "M-c FAIL node.rs 仍调用带延迟参数的旧导出（Node 源延迟会被当成 npm 的选择）"
    );
    assert!(
        !code.contains("npm_measurements"),
        "M-c FAIL node.rs 写了 npm 逐源实测 —— Node 发行源的探测与 npm 契约证据无关"
    );
    assert!(
        code.contains("crate::mirror::export_to_kernel(&m)"),
        "M-c FAIL node.rs 改目录后未重投契约"
    );
    let old_call = "crate::mirror::export_to_kernel_with(&m, latency_ms)?";
    assert!(
        old_call.contains("export_to_kernel_with"),
        "M-c FAIL 判据认不出旧的带延迟导出调用 —— 门禁空转"
    );
}

/// 内核的手动选择只从内核自持的选择文档读；壳自己投的契约不参与。
#[test]
fn m_d_manual_choice_comes_from_kernel_choice_doc() {
    let code = prod_code("src/core.rs");
    let kc = body_from(&code, "fn kernel_choice()");
    assert!(
        kc.contains("registry-choice.json"),
        "M-d FAIL 未读内核自持的选择文档，实际函数体: {:?}",
        kc
    );
    assert!(
        kc.contains("Some(\"manual\")"),
        "M-d FAIL 未只在 mode=manual 时采纳 manualOrigin（auto 下读到的候选不得当成固定源）"
    );
    let origins = body_from(&code, "pub fn registry_origins()");
    assert!(
        origins.contains("kernel_choice()"),
        "M-d FAIL 候选集合未取内核选择（壳的安装会与内核面板固定的源分叉）"
    );
    assert!(
        origins.contains("crate::mirror::load().npm"),
        "M-d FAIL 内核未维护候选时未回退到壳自持目录"
    );
    // 反向：契约（壳自己写的 registry.json）不得绕一圈被当成用户意图。
    assert!(
        !reads_shell_contract(&origins),
        "M-d FAIL registry_origins 读了壳投的契约 —— 把自己的目录当成用户意图: {:?}",
        origins
    );
    let old_shape = "let list = read_json(supervisor_dir().join(\"registry.json\")).get(\"origins\")";
    assert!(
        reads_shell_contract(old_shape),
        "M-d FAIL 判据认不出「把契约候选当用户意图」的旧形态 —— 门禁空转"
    );
}

/// 地址形态只有一把尺；产物地址另过主机闸。
#[test]
fn m_e_addresses_validated_by_one_ruler() {
    for rel in ["src/mirror.rs", "src/core.rs", "src/commands/mod.rs", "src/node.rs"] {
        let code = prod_code(rel);
        assert!(
            !has_weak_scheme_check(&code),
            "M-e FAIL {} 的生产代码仍在用前缀判协议（弱判据放过 ftp://、user:pass@、内嵌空格）",
            rel
        );
    }
    let mirror = prod_code("src/mirror.rs");
    assert!(
        mirror.contains("fn checked_http_url("),
        "M-e FAIL 没有共同形态函数 —— 两把尺各自演化，同一地址两侧答案会不同"
    );
    let base = body_from(&mirror, "pub fn registry_base");
    let asset = body_from(&mirror, "pub fn asset_url");
    assert!(
        base.contains("checked_http_url(") && asset.contains("checked_http_url("),
        "M-e FAIL 两把尺未共用同一形态判定，registry_base/asset 命中: {:?}/{:?}",
        base.contains("checked_http_url("),
        asset.contains("checked_http_url(")
    );
    // 产物地址由 registry 给出（等价一次跨主机跳转），必须过私网主机闸；配置基址由操作者本人选，不过。
    assert!(
        asset.contains("private_host_literal("),
        "M-e FAIL 产物地址未过主机闸（registry 指向 169.254/127.0.0.1 即可打通内网）"
    );
    assert!(
        base.contains("u.query().is_some()"),
        "M-e FAIL 配置基址未拒查询串（同一源会因 token 参数变成两个身份不同的地址）"
    );
    assert!(
        !asset.contains("query()"),
        "M-e FAIL 产物地址拒了查询串 —— 签名 CDN 的凭据就在 query 里，砍掉等于装不上这类镜像: {:?}",
        asset
    );
    assert!(
        prod_code("src/core.rs").contains("crate::mirror::asset_url("),
        "M-e FAIL dist.tarball 未走产物尺"
    );
    assert!(
        prod_code("src/commands/mod.rs").contains("crate::mirror::registry_base("),
        "M-e FAIL 面板写入的镜像源未走配置尺"
    );
    let old_weak = "if !url.starts_with(\"http://\") { return Err(String::from(\"bad\")) }";
    assert!(
        has_weak_scheme_check(old_weak),
        "M-e FAIL 判据认不出弱判据旧形态 —— 门禁空转"
    );
}

#[test]
fn e_a_install_listener_reports_status_text() {
    // 2026-09-16（SSOT §2.4/§3.3）：事件名统一为 install_progress，且前端只消费 **status 文字**。
    //   原断言锁定的是旧形态（env_progress + busy 闸 + 进度条），已被规范废除 ——
    //   若继续断言旧形态，门禁会把「正确的重构」判为失败（正是本次改造中的情形）。
    let code = strip_comments(&read("bootstrap/js/80-init.js"));
    let i = code.find("listen('install_progress'").expect("E-a FAIL 未找到 install_progress 监听");
    let block = &code[i..(i + 600).min(code.len())];
    assert!(
        !block.contains("if (p.busy)"),
        "E-a FAIL 监听器仍以 busy 为闸（该条件恒为假 → 安装全程零反馈）"
    );
    // 文字必达：只要事件到达就更新文案（经统一入口，不再自行拼装）
    assert!(
        block.contains("NS.install.text"),
        "E-a FAIL 未经统一入口展示安装文字（SSOT T-6）"
    );
    assert!(
        !code.contains("showProgress") && !code.contains("progBar"),
        "E-b FAIL 监听器自己画假进度条（条只准由真分母驱动，唯一渲染点在 10-ui.js，见 G-3）"
    );
}

#[test]
fn e_c_push_status_emits_unified_install_progress() {
    // 2026-09-16（SSOT §2.4）：统一 install_progress {kind,status,progress}；
    //   旧 payload 里的 busy 补丁随契约冻结一并移除（前端已不依赖它）。
    // 2026-09-21（B4）：发射点从 main.rs 迁入 domain/install.rs —— 现在它是**全仓唯一**的
    //   install_* 发射点（单点性由 G-12 钉死），本判据只管 payload 形态仍带 kind/status。
    let code = strip_comments(&read("src/domain/install.rs"));
    let i = code.find("install_progress").expect("E-c FAIL 未找到 install_progress 发射点");
    let block = &code[i..(i + 400).min(code.len())];
    assert!(
        block.contains("kind") && block.contains("status"),
        "E-c FAIL install_progress 未带 kind/status（前端无法区分 node 与 npm 步骤）"
    );
}

#[test]
fn e_d_offender_detector_is_not_vacuous() {
    // 反向：判据必须能识别「以 busy 为闸」的旧形态
    let old_form = "NS.evt.listen('env_progress', function (e) {\n\
         var p = e.payload || {};\n\
         if (p.busy) { NS.setStep(1); NS.status(p.status || 'x'); }\n\
       });";
    let i = old_form.find("listen('env_progress'").unwrap();
    let block = &old_form[i..];
    assert!(
        block.contains("if (p.busy)"),
        "E-d FAIL 判据无法识别旧形态 —— 门禁空转"
    );
}
