//! 安装/通道冒烟门禁。
//!
//! ## 缺陷
//!
//! `docs/RELEASE-STANDARD.md` §5 一直写着「安装冒烟 | 各平台安装包 | 能装、能起、能更新」，
//! 但产线里**没有任何一步安装过包**：H3 的全部判据跑的都是 `./target/debug/` 下的构建产物。
//! 于是「装机形态」这条路径（deb/dmg/NSIS 落盘 → 装进系统的那份字节起得来 → 覆盖升级换掉文件）
//! 从来没被执行过，而用户报障恰恰落在这条路径上。
//!
//! 发布通道同理：清单发出去以后，没有任何自动化从**用户真正读到的端点**把它取回来验签，
//! 全靠人工查询，所以 1.2.3 之后的发布对通道只有一句「已查」。
//!
//! ## 锁定不变量
//!   I-a  install-smoke job 存在、四平台矩阵、needs build
//!   I-b  每条腿真的执行平台安装动词（dpkg -i / hdiutil attach / NSIS /S）
//!   I-c  该 job 结构上拿不到构建产物（不得出现 target/debug 或 release 二进制路径）
//!   I-d  publish 必须 needs install-smoke
//!   I-e  published-channel-smoke 存在、只在 tag 或手工触发跑、且真做验签
//!   I-f  判据形态：装完必须读**结论**（自报版本 + 包管理器版本 + 落盘 exe + 覆盖后字节变化）
//!   I-g  启动链夹具单源（不得再在 workflow 里内联第二份伪内核）
//!   I-h  反向：以上判据能识别旧形态（门禁非空转）
//!   I-i  取 A 的基线按候选指向的 commit 筛（与本次 HEAD 同一份内容的不配当「升级前那一份」）

use std::fs;
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().expect("src-tauri 的上级").to_path_buf()
}

fn read(rel: &str) -> String {
    let p = repo_root().join(rel);
    let s = fs::read_to_string(&p).unwrap_or_else(|e| panic!("读 {} 失败: {}", p.display(), e));
    s.replace("\r\n", "\n")
}

fn workflow() -> String {
    read(".github/workflows/build.yml")
}

/// 去掉整行 # 注释：YAML 里的承诺不算执行证据（本仓多次被自己的说明文字骗过）。
fn strip_comments(src: &str) -> String {
    src.lines().filter(|l| !l.trim_start().starts_with('#')).collect::<Vec<_>>().join("\n")
}

/// 取某个顶层 job 的代码块（含其全部步骤），用于把判据限定在单个 job 内。
fn job_block(yaml: &str, name: &str) -> String {
    let marker = format!("\n  {}:\n", name);
    let start = match yaml.find(&marker) {
        Some(i) => i + 1,
        None => return String::new(),
    };
    let rest = &yaml[start..];
    let mut end = rest.len();
    for (idx, _) in rest.match_indices("\n  ") {
        let line = rest[idx + 1..].lines().next().unwrap_or("");
        if line.starts_with("  ") && !line.starts_with("   ") && line.trim_end().ends_with(':') {
            end = idx;
            break;
        }
    }
    rest[..end].to_string()
}

/// H11 通道校验脚本该判什么 —— 抽成函数，好让 I-h 用同一把尺子量反向夹具
/// （判据写在断言里就没法证伪，本仓被这种「只对自己为真的门禁」骗过多次）。
///
/// 分工：签名**有效性**由 `src-tauri/tests/updater_artifacts.rs` 的 V2/V3/V4 判 —— 它用的是
/// tauri-plugin-updater 内部同一个 minisign-verify crate，与用户端逐字同口径。
/// node 里重实现只有两种结局：口径不对年年假红（实测 minisign 的 "ED" 是 prehash 变体，
/// Node stdlib 的纯 Ed25519 验签对不上），口径错了还判绿。所以这里判的是「钥匙对不对、
/// 字节是不是本次构建的那份」。
fn missing_channel_checks(js: &str) -> Vec<&'static str> {
    let checks: Vec<(&'static str, bool)> = vec![
        ("读客户端端点", js.contains("plugins.updater.endpoints")),
        ("剥 minisign 文本壳", js.contains("unwrapSig") && js.contains("untrusted comment")),
        ("签名块结构 74 字节", js.contains("blob.length !== 74")),
        ("比对 key id", js.contains("pub.keyId")),
        ("比对产物字节摘要", js.contains("sha256")),
        ("验签交给 updater_artifacts", js.contains("updater_artifacts")),
        ("不在 node 里重实现验签", !js.contains("crypto.verify")),
    ];
    checks.into_iter().filter(|(_, ok)| !*ok).map(|(name, _)| name).collect()
}

#[test]
fn i_a_install_smoke_job_exists_with_four_platforms() {
    let y = strip_comments(&workflow());
    let job = job_block(&y, "install-smoke");
    assert!(!job.is_empty(), "I-a FAIL 产线里没有 install-smoke job —— 安装冒烟又是只在文档里存在");
    for art in ["linux-x64", "win-x64", "darwin-arm64", "darwin-x64"] {
        assert!(job.contains(&format!("artifact: {}", art)), "I-a FAIL 安装冒烟缺平台 {}", art);
    }
    let legs = job.matches("- os:").count();
    assert_eq!(legs, 4, "I-a FAIL 安装冒烟矩阵腿数 = {}（应为 4）", legs);
    assert!(
        job.contains("needs: [version, build]"),
        "I-a FAIL install-smoke 未依赖 build（没有本次产物就无法覆盖升级）"
    );
    assert!(job.contains("download-artifact"), "I-a FAIL 没有下载本次构建产物的步骤");
    assert!(job.contains("gh release download"), "I-a FAIL 没有取上一已发布版本安装包（A）的步骤");
    eprintln!("I-a PASS install-smoke job 四平台齐备");
}

#[test]
fn i_b_each_platform_actually_installs() {
    let sh = read("ci/install-smoke.sh");
    let ps = read("ci/install-smoke-win.ps1");
    let y = strip_comments(&workflow());
    let job = job_block(&y, "install-smoke");
    assert!(job.contains("ci/install-smoke.sh"), "I-b FAIL job 未引用 Linux/macOS 安装脚本");
    assert!(job.contains("ci/install-smoke-win.ps1"), "I-b FAIL job 未引用 Windows 安装脚本");
    // 真正的安装动词必须在脚本里：只做 dpkg-deb -f / hdiutil 挂载 / 复制 exe 都不算「装上了」。
    assert!(sh.contains("sudo dpkg -i"), "I-b FAIL Linux 没有 dpkg 真安装动作");
    assert!(sh.contains("hdiutil attach"), "I-b FAIL macOS 没有挂载 dmg");
    assert!(sh.contains("cp -R"), "I-b FAIL macOS 没有把 .app 复制到安装位置");
    assert!(ps.contains("-ArgumentList '/S'"), "I-b FAIL Windows 没有静默安装 NSIS 包");
    // 装完必须**从系统里**找到那份二进制，而不是拿包里的文件自证。
    assert!(sh.contains("dpkg -L"), "I-b FAIL Linux 没有从包管理器读实际落盘路径");
    assert!(ps.contains("Uninstall"), "I-b FAIL Windows 没有读卸载注册表项定位安装目录");
    eprintln!("I-b PASS 四平台都真的执行安装动作");
}

#[test]
fn i_c_smoke_runs_installed_bytes_not_build_tree() {
    let y = strip_comments(&workflow());
    let job = job_block(&y, "install-smoke");
    assert!(!job.is_empty(), "I-c FAIL 没有 install-smoke job，判据无从谈起");
    for banned in ["target/debug", "target/release/dsh-supervisor-gui", "cargo build"] {
        assert!(
            !job.contains(banned),
            "I-c FAIL 安装冒烟里出现 {} —— 一旦能拿到构建树，「装上的那份字节」就不是被判的对象",
            banned
        );
    }
    eprintln!("I-c PASS 安装冒烟结构上只能跑装机产物");
}

#[test]
fn i_d_publish_requires_install_smoke() {
    let y = strip_comments(&workflow());
    let job = job_block(&y, "publish");
    assert!(job.contains("needs: [version, build, install-smoke]"),
        "I-d FAIL publish 未 needs install-smoke —— 装不上的安装包仍会被发给用户");
    eprintln!("I-d PASS 发布被安装冒烟门禁挡住");
}

#[test]
fn i_e_channel_smoke_verifies_published_endpoints() {
    let y = strip_comments(&workflow());
    let job = job_block(&y, "published-channel-smoke");
    assert!(!job.is_empty(), "I-e FAIL 没有 published-channel-smoke job");
    assert!(job.contains("workflow_dispatch"), "I-e FAIL 通道冒烟没有手工复核历史版本的入口");
    assert!(job.contains("refs/tags/v"), "I-e FAIL 通道冒烟未限定在 tag 发布之后");
    assert!(job.contains("needs.publish.result == 'success'"),
        "I-e FAIL 发布没成仍会跑通道冒烟（判据挂在不存在的东西上）");
    assert!(job.contains("shell-release/verify-channel.js"), "I-e FAIL 未引用通道校验脚本");

    let js = read("shell-release/verify-channel.js");
    let missing = missing_channel_checks(&js);
    assert!(missing.is_empty(), "I-e FAIL 通道校验缺判据: {}", missing.join(" / "));
    assert!(!js.contains("attestation"),
        "I-e FAIL 壳的发布链没有 npm attestation，判据里出现它只会指向空对象");
    eprintln!("I-e PASS 通道冒烟按客户端端点读签名壳与字节，验签归 updater_artifacts");
}

#[test]
fn i_f_installed_probe_reads_conclusions_not_survival() {
    let sh = read("ci/install-smoke.sh");
    let ps = read("ci/install-smoke-win.ps1");
    // 每次安装后都要读到「装进去的那份自报了版本」，且与包管理器记录一致。
    assert!(sh.contains("shell_version="), "I-f FAIL Linux/macOS 没判装后自报版本");
    assert!(ps.contains("shell_version="), "I-f FAIL Windows 没判装后自报版本");
    assert!(sh.contains("dpkg-query"), "I-f FAIL Linux 没比对包管理器记录的版本");
    assert!(sh.contains("CFBundleShortVersionString"), "I-f FAIL macOS 没读 Info.plist 版本");
    // 落盘链路（identity.json 的 exe + shell.log）是壳自己的事实源，装机形态必须写对。
    assert!(sh.contains("identity.json"), "I-f FAIL 没判 identity.json 落盘");
    assert!(ps.contains("identity.json"), "I-f FAIL Windows 没判 identity.json 落盘");
    // 壳在 Windows 上是 GUI 子系统进程：调用运算符不等待、也不接它的 stdout，
    // 于是探针读到空输出 + 空退出码，「装上了但测不到」与「压根没装上」在日志里长得一样。
    assert!(ps.contains("RedirectStandardOutput") && ps.contains("WaitForExit"),
        "I-f FAIL Windows 探针未接管 GUI 子系统进程的输出与等待");
    assert!(sh.contains("shell.log"), "I-f FAIL 没判 shell.log 落盘");
    // 覆盖安装必须真的换掉字节，否则「升级」只是把旧文件又装了一遍。
    assert!(sh.contains("HASH_A"), "I-f FAIL Linux/macOS 没有比对覆盖前后的字节摘要");
    assert!(ps.contains("hashA"), "I-f FAIL Windows 没有比对覆盖前后的字节摘要");
    // Linux 一条链要把装好的壳拉起到就绪：这是「能起」的全部含义。
    assert!(sh.contains("--watchdog"), "I-f FAIL 没让装好的壳走就绪判据链");
    assert!(sh.contains("ports.json"), "I-f FAIL 没核对进程真绑定了端口");
    // 版本未提升时产物可逐字节相同，那条判据此时不成立 —— 必须按版本分流。
    assert!(sh.contains("\"$AVER\" != \"$BVER\""), "I-f FAIL 字节变化判据未按版本分流");
    assert!(ps.contains("-ne $VerB") || ps.contains("$VerA -ne $VerB"),
        "I-f FAIL Windows 字节变化判据未按版本分流");
    eprintln!("I-f PASS 装机判据读的是结论而不是进程没崩");
}

#[test]
fn i_g_chain_fixture_has_single_source() {
    let y = strip_comments(&workflow());
    assert!(
        !y.contains("<<'JS'"),
        "I-g FAIL workflow 里仍内联伪内核夹具 —— 两份夹具会各自漂移，H3 绿不代表 H10 绿"
    );
    assert!(y.contains("ci/fake-core.js"), "I-g FAIL 启动链冒烟未引用随仓夹具");
    assert!(read("ci/install-smoke.sh").contains("fake-core.js"),
        "I-g FAIL 安装冒烟未共用同一夹具（它自己另造一份就会与 H3 漂移）");
    assert!(
        !job_block(&y, "install-smoke").contains("cat > "),
        "I-g FAIL 安装冒烟又现造了一份伪内核"
    );
    eprintln!("I-g PASS 伪内核夹具单源（H3 与 H10 共用）");
}

#[test]
fn i_h_reverse_judgements_are_not_vacuous() {
    // 旧形态：没有安装 job，夹具内联在 workflow 里，publish 不依赖安装冒烟。
    let old = "jobs:\n  version:\n    runs-on: x\n  build:\n    runs-on: y\n    steps:\n      - run: cat > f <<'JS'\n      - run: cargo test\n  publish:\n    needs: [version, build]\n";
    assert!(job_block(old, "install-smoke").is_empty(), "I-h FAIL job_block 在无该 job 时返回了内容");
    assert!(old.contains("<<'JS'"), "I-h FAIL 夹具内联的旧形态自检失败");
    assert!(!job_block(old, "publish").contains("needs: [version, build, install-smoke]"),
        "I-h FAIL 旧 publish 依赖被误判为已含安装冒烟");
    // 只查清单能不能下载的假通道冒烟
    let fake = "  const m = await (await fetch(u)).json();\n  console.log(m.version);\n";
    // 调用运算符跑 GUI 子系统进程：不等待、读不到 stdout，形似有判据实则空转。
    let lazy = "function RunExe($exe, $argv) { (@(& $exe @argv 2>&1) -join \"`n\") + \"exit=$LASTEXITCODE\" }";
    assert!(!lazy.contains("RedirectStandardOutput") && !lazy.contains("WaitForExit"),
        "I-h FAIL 调用运算符形态被判为已接管输出与等待");
    assert!(missing_channel_checks(fake).contains(&"剥 minisign 文本壳"),
        "I-h FAIL 只下载清单的假通道冒烟被误判为通过 I-e");
    // 旧形态：把清单里的 signature 当单层 base64，直接在 node 里 ed25519 验签。
    // 看着比新脚本更「像在验签」，但长度与 keyId 偏移全错，且口径与用户端不同 —— 必须判否。
    let naive = "const sig = Buffer.from(p.signature, 'base64');\n\
                 if (sig.length !== 72) fail(key + ' 签名长度不对');\n\
                 const spki = Buffer.concat([Buffer.from('302a300506032b6570032100', 'hex'), pub.key]);\n\
                 ok = crypto.verify(null, bytes, { key: spki, format: 'der', type: 'spki' }, sig.subarray(8));\n";
    let naive_missing = missing_channel_checks(naive);
    assert!(naive_missing.contains(&"剥 minisign 文本壳") && naive_missing.contains(&"签名块结构 74 字节")
        && naive_missing.contains(&"不在 node 里重实现验签"),
        "I-h FAIL 单层 base64 + crypto.verify 的旧形态被判为通过 I-e: {:?}", naive_missing);
    // 把安装换成「解包看看」的假安装冒烟
    let fake_install = "dpkg-deb -f pkg.deb Version\ntar xf pkg.deb\n";
    assert!(!fake_install.contains("sudo dpkg -i"), "I-h FAIL 未安装却自称安装");
    // job_block 定位能力（否则 I-a/I-c/I-d/I-e 全空转）
    let sample = "jobs:\n  a:\n    runs-on: x\n    steps:\n      - run: one\n  b:\n    runs-on: y\n";
    let ab = job_block(sample, "a");
    assert!(ab.contains("run: one") && !ab.contains("runs-on: y"),
        "I-h FAIL job_block 边界错：{:?}", ab);
    // 注释里的承诺不算证据
    let commented = "  # 会执行 sudo dpkg -i 安装\n";
    assert!(!strip_comments(commented).contains("sudo dpkg -i"), "I-h FAIL 注释未被剥掉");
    eprintln!("I-h PASS 反向判据有效");
}

/// 取 A 那一步的形态检查，返回违规清单（空 = 合规）。纯函数：反向夹具要靠它证明判据认得旧形态。
/// 输入必须是**剥掉注释**的 job 块 —— 写在 YAML 注释里的承诺不算执行证据（见 strip_comments）。
fn a_baseline_problems(job: &str) -> Vec<String> {
    let mut v = Vec::new();
    if job.is_empty() {
        v.push("没有 install-smoke job，A 基线无从谈起".to_string());
        return v;
    }
    // 计数单位 = 「自己推导一遍 A 候选」的处数：第二处一旦存在，两处会在不同 run 里选出不同的 A，
    // 而每条判据只看得见自己那份（同一文件里两处同样各算一次，不按文件计）。
    let picks = job.matches("gh release list").count();
    if picks != 1 {
        v.push(format!("A 的候选必须只在一处推导（gh release list 实为 {} 处）", picks));
    }
    if !job.contains("gh release download") {
        v.push("没有从 Release 下载 A 的动作，候选筛得再对也拿不到安装包".to_string());
    }
    match job.find("GITHUB_SHA") {
        None => v.push(
            "没把候选与本次 HEAD 比对：同一 commit 的 Release 会被当成「升级前那一份」，\
             而它刚建出时资产常还没传完"
                .to_string(),
        ),
        Some(cmp) => {
            if !job.contains("commits/") {
                v.push("没解析候选 Release 指向的 commit，比对只能停在 ref 名上".to_string());
            }
            match job.find("A_TAG=\"$t\"") {
                None => v.push("没有给 A 赋值的语句，判据无的放矢".to_string()),
                Some(pick) if pick < cmp => {
                    v.push("首条候选未经比对就被选为 A（比对写在赋值之后 = 空转）".to_string())
                }
                Some(_) => {}
            }
        }
    }
    v
}

#[test]
fn i_i_a_baseline_must_differ_from_this_head() {
    let y = strip_comments(&workflow());
    let job = job_block(&y, "install-smoke");
    assert!(
        a_baseline_problems(&job).is_empty(),
        "I-i FAIL 取 A 的判据形态不合格: {:?}",
        a_baseline_problems(&job)
    );
    // 规范必须与产线同口径：只改产线不改规范，下次有人照规范重写就把修复抹掉。
    assert!(
        read("docs/RELEASE-STANDARD.md").contains("A 与本次 HEAD 是同一 commit"),
        "I-i FAIL 规范里没写「A 与本次 HEAD 是同一 commit 要跳过」这条口径"
    );
    eprintln!("I-i PASS A 基线按候选指向的 commit 筛");
}

#[test]
fn i_reverse_catches_old_a_baseline_forms() {
    // 旧形态（2026-09-26 判红的那版）：只排除与 GITHUB_REF_NAME 同名的 tag，首条候选即选为 A。
    // 分支 run 上 SELF 恒为空，于是与本次同一 commit 的新 Release 被当成 A。
    let old = "  install-smoke:\n    steps:\n      - run: |\n          SELF=\"\"\n          [[ \"${GITHUB_REF}\" == refs/tags/v* ]] && SELF=\"${GITHUB_REF_NAME}\"\n          for t in $(gh release list --repo r --json tagName); do\n            [ \"$t\" = \"$SELF\" ] && continue\n            A_TAG=\"$t\"\n            break\n          done\n          gh release download \"$A_TAG\" --pattern '*.deb'\n";
    let hits = a_baseline_problems(old);
    assert!(
        hits.iter().any(|s| s.contains("HEAD")),
        "I-i 判据对「不比对 HEAD」的旧形态无反应（空转）: {:?}",
        hits
    );
    // 比对写了，但写在赋值之后：首条已被选走，判据永远不会改变结果。
    let too_late = "      - run: |\n          for t in $(gh release list --repo r); do A_TAG=\"$t\"; break; done\n          [ \"$(gh api repos/r/commits/$A_TAG --jq .sha)\" = \"$GITHUB_SHA\" ] && exit 1\n          gh release download \"$A_TAG\"\n";
    assert!(
        a_baseline_problems(too_late)
            .iter()
            .any(|s| s.contains("未经比对就被选为 A")),
        "I-i 判据认不出「比对在赋值之后」的空转形态: {:?}",
        a_baseline_problems(too_late)
    );
    // 第二处推导：两条腿各选各的 A，任何一条判据都只看得见自己那份。
    let twice = "      - run: |\n          for t in $(gh release list --repo r); do [ \"$(gh api repos/r/commits/$t --jq .sha)\" = \"$GITHUB_SHA\" ] || { A_TAG=\"$t\"; break; }; done\n          gh release download \"$A_TAG\"\n      - run: gh release list --repo r\n";
    assert!(
        a_baseline_problems(twice)
            .iter()
            .any(|s| s.contains("实为 2 处")),
        "I-i 计数判据认不出重复推导: {:?}",
        a_baseline_problems(twice)
    );
    // 正向对照：新形态必须零违规（否则上面三条反向只是恰好都撞在同一条上）。
    let good = "      - run: |\n          for t in $(gh release list --repo r); do\n            sha=$(gh api \"repos/r/commits/$t\" --jq .sha)\n            [ \"$sha\" = \"$GITHUB_SHA\" ] && continue\n            A_TAG=\"$t\"; break\n          done\n          gh release download \"$A_TAG\"\n";
    assert!(a_baseline_problems(good).is_empty(), "I-i 正向夹具被判红: {:?}", a_baseline_problems(good));
}
