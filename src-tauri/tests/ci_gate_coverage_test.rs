//! CI 覆盖性门禁（2026-09-13）。
//!
//! ## 缺陷 ①（门禁漏跑）
//!
//! 壳仓 CI 的「门禁测试」步骤原先硬编码枚举 test target：
//!   cargo test --bins --test bootstrap_flow --test update_guard_test --test platform_unsupported_structure_test
//! 于是新增的 tests/*.rs 被静默排除在 CI 之外。实测漏了 4 个：
//!   · platform_shared_items_test —— 正是为「平台文件用了父模块项却没导入
//!     → macOS E0425」建立的那道门禁（建了却不在 CI 跑）；
//!   · mirror_env_wiring_test、round13_p3_batch_test（本轮新增）；
//!   · service_self_heal_test（既有的，一直没在 CI 跑）。
//! 该步骤自己的注释就写着「60+ 个门禁……此前从未在 CI 执行」——同类问题又复发了。
//!
//! ## 缺陷 ②（轻量替代 = 掩盖问题）
//!
//! 中途曾引入一个只做 cargo check 的轻量 job 并给 build 加 tag 守卫，
//! 让日常 push 走轻量路径。这是错的：仅编译校验发现不了打包/签名/产物装配阶段的问题
//! （glibc 基座、bundle 装配、验签清单、UPDATE 资产），而本仓恰在那些阶段踩过坑；
//! 轻量检查给出「绿了」的假象，反而掩盖问题。
//! 现按明确要求修订为：push main 直接跑完整构建矩阵（不设轻量替代、不设 tag 守卫）。
//!
//! ## 缺陷 ③（产线承诺与执行不符）
//!
//! 打包步骤注释写着「未配置 secret 时为空，仅构建不产 .sig，不阻断」，但 secret 未配置时
//! Actions 把它展开成空字符串并继续导出，Tauri v2 CLI 视其为一把非法私钥
//! （incorrect updater private key password）而失败。撤掉空变量后仍会红在下一步：
//! 配置内置了 pubkey，Tauri 见「有公钥无私钥」即报 A public key has been found, but no
//! private key，所以验证构建必须同时关掉 updater 产物。随后的组装步骤又对缺 .sig 无条件
//! exit 1，验签验收步骤也无条件跑。叠加结果是：任何没有签名密钥的构建必然全红，
//! 而该红与本次代码改动无关，等于完整构建矩阵不可用。
//! 修法是把承诺变成执行：撤掉空变量 + 关 updater 产物，安装程序照常产出；缺 .sig 的强校验
//! 只在 tag 构建生效（发布仍必须成对，不可放宽）；验签验收只在 tag 上跑。
//! 注意这不能靠给 build job 加 if: 实现，那会命中 C-c/C-d。
//!
//! ## 锁定不变量
//!   C-a  CI 门禁步骤必须自动枚举 tests/*.rs（不得硬编码 target 列表）
//!   C-b  唯一允许排除的 target 是 updater_artifacts（它需打包产物 SHELL_REHEARSAL_DIR）
//!   C-c  push 时必须跑完整构建矩阵：build job 不得被 if: 守卫限制；
//!        matrix 必须含 macOS 与 Windows；且 build 内要跑 cargo test
//!   C-d  反悔防护：不得存在「只做 cargo check、不做构建」的轻量 job 替代完整构建
//!   C-e  反向：判据能识别旧形态（门禁非空转）
//!   C-f  无密钥构建不得被阻断：打包步骤在 key 缺失分支内撤掉空签名变量并关掉 updater 产物
//!        （override 必须真的传给 build 命令）；组装对 .sig 的强校验按 tag 分流；
//!        验签验收步骤有 tag 级步骤 if
//!   C-g  发布侧严格性不得被放宽：组装器仍对缺 .sig 早失败，且该失败挂在 require-sig 上；
//!        parseArgs 必须能识别布尔开关；备用形态只在主形态零命中时启用
//!   C-h  反向：C-f/C-g 的判据能识别旧形态，且 step_block 定位准确
//!   C-i  构建工具链版本必须钉死：打包步骤引用三段号的 TAURI_CLI_VERSION，
//!        全文件不得再有浮动 major 的 CLI 引用（同一 commit 的三平台产物不得出自不同 CLI 版本）；
//!        产线文档 H4 行必须写同一形态（命令改了文档没跟＝把浮动版本写成事实）。

use std::fs;
use std::path::PathBuf;

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn ci() -> String {
    let p = manifest_dir().join("..").join(".github").join("workflows").join("build.yml");
    fs::read_to_string(&p).unwrap_or_else(|e| panic!("读取 {:?} 失败: {}", p, e))
}

/// 去掉整行 # 注释（YAML 注释）—— 本仓多次被自己的说明文字骗过。
fn strip_yaml_comments(src: &str) -> String {
    src.lines().filter(|l| !l.trim_start().starts_with('#')).collect::<Vec<_>>().join("\n")
}

/// 是否存在「硬编码的 --test 目标名」（除 updater_artifacts 外）。
fn has_hardcoded_test_targets(yaml: &str) -> bool {
    yaml.lines()
        .filter(|l| l.contains("--test "))
        // 自动枚举那一行里也含 "--test "（在 sed 表达式内），但它同时含 "sed" 与 "TARGETS"
        .filter(|l| !(l.contains("sed") && l.contains("TARGETS")))
        .any(|l| !l.contains("updater_artifacts"))
}

/// 抽取某个顶层 job 的文本块（从两空格缩进的 name: 到下一个顶层 job 或文件尾）。
fn job_block(yaml: &str, name: &str) -> String {
    let marker = format!("\n  {}:\n", name);
    let start = match yaml.find(&marker) {
        Some(i) => i + 1,
        None => return String::new(),
    };
    let rest = &yaml[start..];
    let mut end = rest.len();
    for (idx, _line) in rest.match_indices("\n  ") {
        // 仅当该位置确为「一个新的顶层 job」：下一行形如  <name>: 且其后是行尾
        let after = &rest[idx + 1..];
        let first = after.lines().next().unwrap_or("");
        if first.starts_with("  ") && !first.starts_with("   ") && first.trim_end().ends_with(':') {
            end = idx;
            break;
        }
    }
    rest[..end].to_string()
}

#[test]
fn c_a_ci_gate_step_auto_enumerates_test_targets() {
    let y = strip_yaml_comments(&ci());
    assert!(
        y.contains("ls tests/*.rs"),
        "C-a FAIL CI 门禁步骤未自动枚举 tests/*.rs —— 新增门禁文件会被静默排除在 CI 之外"
    );
    assert!(
        y.contains("TARGETS") && y.contains("cargo test --bins"),
        "C-a FAIL 未把枚举结果传给 cargo test"
    );
    assert!(
        !has_hardcoded_test_targets(&y),
        "C-a FAIL 仍存在硬编码 --test 枚举（会漏掉新增门禁）"
    );
}

#[test]
fn c_b_only_updater_artifacts_is_excluded() {
    let y = strip_yaml_comments(&ci());
    assert!(
        y.contains("grep -v '^updater_artifacts$'"),
        "C-b FAIL 未按预期排除 updater_artifacts（或排除写法变了，需同步本门禁）"
    );
    for l in y.lines().filter(|l| l.contains("grep -v")) {
        assert!(
            l.matches("grep -v").count() == 1,
            "C-b FAIL 出现了多个 grep -v 排除（排除列表被扩大）：{}",
            l.trim()
        );
    }
}

#[test]
fn c_c_full_build_runs_on_push_for_all_platforms() {
    let y = strip_yaml_comments(&ci());
    assert!(
        y.contains("branches: [main]" ) || y.contains("branches: ['main']"),
        "C-c FAIL push main 未触发 CI"
    );

    let build = job_block(&y, "build");
    assert!(!build.is_empty(), "C-c FAIL 未找到 build job");

    // ① 不得有 **job 级** if: 守卫（否则日常 push 会跳过完整构建）。
    //    只认**恰好 4 空格缩进**的 if: —— job 级键缩进 4，步骤级在 steps 下缩进 8。
    //      第一版用 trim_start() 判会命中步骤级 if（如「tag 才上传 Release 资产」），假红。
    let job_level_if: Vec<&str> = build
        .lines()
        .filter(|l| l.starts_with("    if:") && !l.starts_with("     "))
        .collect();
    assert!(
        job_level_if.is_empty(),
        "C-c FAIL build job 存在 job 级 if: 守卫（日常 push 会跳过完整构建，不得用轻量检查替代）：{:?}",
        job_level_if
    );

    // ② 完整矩阵必须含 macOS 与 Windows（否则 cfg 屏蔽的平台文件永不被编译）
    assert!(
        build.contains("macos-latest") && build.contains("windows-latest"),
        "C-c FAIL build 矩阵未覆盖 macOS/Windows —— 平台文件不会被编译，问题无法暴露"
    );

    // ③ 门禁测试必须随完整构建执行
    assert!(
        build.contains("cargo test"),
        "C-c FAIL build job 内未执行 cargo test —— 门禁不随构建跑"
    );
}

#[test]
fn c_d_no_lightweight_substitute_job() {
    let y = strip_yaml_comments(&ci());
    // 反悔防护：不得存在只做 cargo check、不做 build 的独立 job 来替代完整构建。
    for name in ["build", "platform-check", "check", "compile-check", "fast-check"] {
        let b = job_block(&y, name);
        if b.is_empty() {
            continue;
        }
        let has_check = b.contains("cargo check");
        let has_full = b.contains("cargo build") || b.contains("tauri build") || b.contains("tauri-action");
        assert!(
            !(has_check && !has_full),
            "C-d FAIL job {} 只做 cargo check 而不做完整构建 —— 这是轻量替代，会掩盖问题",
            name
        );
    }
    assert!(
        job_block(&y, "platform-check").is_empty(),
        "C-d FAIL 轻量 platform-check job 又回来了 —— 应以完整构建替代"
    );
}

/// 抽取某个步骤（`      - name: X`）的文本块，到下一个步骤名或文件尾结束。
fn step_block(yaml: &str, name: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    let mut hit = false;
    for l in yaml.lines() {
        let t = l.trim_start();
        if let Some(rest) = t.strip_prefix("- name:") {
            if hit {
                break;
            }
            if rest.trim() == name {
                hit = true;
            }
        }
        if hit {
            out.push(l);
        }
    }
    out.join("\n")
}

/// 「未配置私钥就不阻断」是否真被执行：只在 key 缺失分支里把空变量从环境撤掉才算。
/// 判据只看该分支体内的代码行，注释与 echo 里的承诺不算。
fn clears_empty_signing_env(step: &str) -> bool {
    let i = match step.find("-z \"${TAURI_SIGNING_PRIVATE_KEY}\"") {
        Some(i) => i,
        None => return false,
    };
    let tail = &step[i..];
    let branch = match tail.find("\n          fi") {
        Some(e) => &tail[..e],
        None => tail,
    };
    branch.contains("unset TAURI_SIGNING_PRIVATE_KEY")
}

/// 缺 .sig 的判罚是否随 ref 变化：必须同时出现开关与 tag 判据，写死一边都不算。
fn sig_strictness_is_tag_gated(step: &str) -> bool {
    step.contains("--require-sig") && step.contains("refs/tags/v")
}

/// 组装器里「缺 .sig 就退出」那段是否受开关控制（无条件 exit 1 = 无密钥构建永红）。
fn assembler_failure_is_conditional(src: &str) -> bool {
    let i = match src.find("const missing = entries.filter") {
        Some(i) => i,
        None => return false,
    };
    let block = match src[i..].find("\n}") {
        Some(e) => &src[i..i + e],
        None => &src[i..],
    };
    block.contains("require-sig") && block.contains("process.exit(1)")
}

/// 布尔开关解析：flag 后面跟另一个 flag 或到末尾时不得被当成值吞掉。
fn parse_args_handles_bool_flags(src: &str) -> bool {
    let i = match src.find("function parseArgs") {
        Some(i) => i,
        None => return false,
    };
    let body = match src[i..].find("\n}") {
        Some(e) => &src[i..i + e],
        None => &src[i..],
    };
    body.contains("startsWith('--')") && body.contains("= true")
}

/// 打包步骤是否用**钉死的** Tauri CLI 版本：env 里的值必须是三段纯数字，
/// 且命令必须引用该变量而不是写个浮动 major（`@2` = 每次跑各取当天最新，三平台可能不同版本）。
fn cli_version_pinned(step: &str) -> bool {
    let at = match step.find("TAURI_CLI_VERSION:") {
        Some(i) => i,
        None => return false,
    };
    let line = step[at..].lines().next().unwrap_or("");
    let val = match line.split_once(':') {
        Some((_, v)) => v.trim().trim_matches('"').trim(),
        None => return false,
    };
    let parts: Vec<&str> = val.split('.').collect();
    let exact = parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()));
    exact && step.contains("\"@tauri-apps/cli@${TAURI_CLI_VERSION}\"")
}

/// 无私钥时是否真的关掉 updater 产物：配置内置了 pubkey，Tauri 见「有公钥无私钥」即失败
/// （A public key has been found, but no private key），只 unset 变量并不足以让构建通过。
fn disables_updater_artifacts_without_key(step: &str) -> bool {
    let i = match step.find("-z \"${TAURI_SIGNING_PRIVATE_KEY}\"") {
        Some(i) => i,
        None => return false,
    };
    let tail = &step[i..];
    let branch = match tail.find("\n          fi") {
        Some(e) => &tail[..e],
        None => tail,
    };
    branch.contains("\"createUpdaterArtifacts\":false")
        && branch.contains("--config")
        && step.contains("${UPDATER_OFF}")
}

/// 备用形态只能在主形态零命中时启用（否则 tag 发布会把 .dmg 当更新产物打进店内）。
fn assembler_fallback_is_last_resort(src: &str) -> bool {
    let primary = src.find("ARTIFACT_PATTERNS[installer]");
    let guard = src.find("if (!hits.length && FALLBACK_PATTERNS[installer])");
    match (primary, guard) {
        (Some(p), Some(g)) => p < g && src.contains("FALLBACK_PATTERNS"),
        _ => false,
    }
}

#[test]
fn c_f_unsigned_build_does_not_break_the_pipeline() {
    let y = strip_yaml_comments(&ci());
    let bundle = step_block(&y, "Build + bundle (Tauri)");
    assert!(
        !bundle.is_empty(),
        "C-f FAIL 未找到打包步骤 —— 门禁空转"
    );
    assert!(
        clears_empty_signing_env(&bundle),
        "C-f FAIL 未配置私钥时没有撤掉空的签名环境变量 —— Tauri CLI 会把空串当非法私钥而失败，\
         注释里承诺的「非 tag 构建不阻断」并不成立"
    );
    assert!(
        disables_updater_artifacts_without_key(&bundle),
        "C-f FAIL 无私钥时没有关掉 updater 产物 —— 配置内置了 pubkey，Tauri 会以\
         「有公钥无私钥」直接失败，构建矩阵仍不可用"
    );

    let assemble = step_block(&y, "组装 npm 包（更新产物 + 签名）");
    assert!(
        sig_strictness_is_tag_gated(&assemble),
        "C-f FAIL 组装步骤对 .sig 的强校验没有按 tag 分流 —— 无密钥的验证构建会必然变红"
    );

    let accept = step_block(&y, "产物验收（Tauri 同源验签 + 清单契约）");
    assert!(
        !accept.is_empty(),
        "C-f FAIL 未找到产物验收步骤 —— 门禁空转"
    );
    let has_tag_if = accept
        .lines()
        .any(|l| l.trim_start().starts_with("if:") && l.contains("refs/tags/v"));
    assert!(
        has_tag_if,
        "C-f FAIL 验签验收步骤无条件执行 —— 没有 .sig 时它必然红，发布专用校验应在 tag 上才跑"
    );

    // 反向：C-c 的「build job 不得有 job 级 if」与上面的步骤级 if 不冲突。
    let build = job_block(&y, "build");
    assert!(
        build.contains("产物验收"),
        "C-f FAIL 验收步骤已不在 build job 内 —— 位置变了需同步本门禁"
    );
    assert!(
        !build
            .lines()
            .any(|l| l.starts_with("    if:") && !l.starts_with("     ")),
        "C-f FAIL 为了跳过验签给 build job 加了 job 级 if —— 那会让日常 push 跳过完整构建"
    );
}

#[test]
fn c_g_assembler_keeps_release_strictness() {
    let p = manifest_dir()
        .join("..")
        .join("shell-release")
        .join("assemble-shell-pkg.js");
    let src = fs::read_to_string(&p).unwrap_or_else(|e| panic!("读取 {:?} 失败: {}", p, e));
    assert!(
        assembler_failure_is_conditional(&src),
        "C-g FAIL 缺 .sig 的退出没有挂在 require-sig 上 —— 要么无密钥构建全红，要么发布不再强校验"
    );
    assert!(
        src.contains("process.exit(1)"),
        "C-g FAIL 组装器不再对缺签名早失败 —— tag 发布必须保留强校验"
    );
    assert!(
        parse_args_handles_bool_flags(&src),
        "C-g FAIL parseArgs 不识别布尔开关 —— 末尾的 --require-sig 会取到 undefined"
    );
    assert!(
        assembler_fallback_is_last_resort(&src),
        "C-g FAIL 备用形态没有「主形态零命中」的前提 —— tag 发布可能把 .dmg 当成更新产物打进店内"
    );
}

#[test]
fn c_h_new_judges_are_not_vacuous() {
    // 旧形态：只 echo 警告，没有撤掉空变量。
    let old_bundle = "        run: |\n          if [ -z \"${TAURI_SIGNING_PRIVATE_KEY}\" ]; then\n            echo \"::warning::未配置 —— 不阻断\"\n          fi\n          npx --yes @tauri-apps/cli@2 build\n";
    assert!(
        !clears_empty_signing_env(old_bundle),
        "C-h FAIL 判据无法识别「只承诺不撤变量」的旧形态 —— C-f 空转"
    );
    let new_bundle = "        run: |\n          if [ -z \"${TAURI_SIGNING_PRIVATE_KEY}\" ]; then\n            echo \"::warning::未配置 —— 不阻断\"\n            unset TAURI_SIGNING_PRIVATE_KEY TAURI_SIGNING_PRIVATE_KEY_PASSWORD\n          fi\n          npx --yes @tauri-apps/cli@2 build\n";
    assert!(
        clears_empty_signing_env(new_bundle),
        "C-h FAIL 修复形态被误判 —— 判据过严会挡住正确的改动"
    );
    // 把 unset 写在 fi 之外（key 存在分支）不算修好。
    let misplaced = "        run: |\n          if [ -z \"${TAURI_SIGNING_PRIVATE_KEY}\" ]; then\n            echo \"::warning::未配置\"\n          fi\n          unset TAURI_SIGNING_PRIVATE_KEY\n";
    assert!(
        !clears_empty_signing_env(misplaced),
        "C-h FAIL unset 在 key 缺失分支之外也被判为通过 —— 会把有密钥的 tag 构建判错"
    );

    assert!(
        !sig_strictness_is_tag_gated("node assemble.js --platform p --require-sig"),
        "C-h FAIL 无条件 --require-sig 被误判为已按 tag 分流"
    );
    // 只 unset 不关 updater 产物（实测会报 A public key has been found, but no private key）
    let unset_only = "        run: |\n          UPDATER_OFF=\"\"\n          if [ -z \"${TAURI_SIGNING_PRIVATE_KEY}\" ]; then\n            unset TAURI_SIGNING_PRIVATE_KEY\n          fi\n          npx --yes @tauri-apps/cli@2 build ${UPDATER_OFF}\n";
    assert!(
        !disables_updater_artifacts_without_key(unset_only),
        "C-h FAIL 判据无法识别「撤了变量但没关 updater 产物」的半修形态"
    );
    let off_written_but_unused = "        run: |\n          if [ -z \"${TAURI_SIGNING_PRIVATE_KEY}\" ]; then\n            printf '%s' '{\"bundle\":{\"createUpdaterArtifacts\":false}}' > t.json\n            UPDATER_OFF=\"--config t.json\"\n          fi\n          npx --yes @tauri-apps/cli@2 build\n";
    assert!(
        !disables_updater_artifacts_without_key(off_written_but_unused),
        "C-h FAIL 写好了 override 却没传给 build 命令也被判为通过"
    );
    let off_wired = "        run: |\n          UPDATER_OFF=\"\"\n          if [ -z \"${TAURI_SIGNING_PRIVATE_KEY}\" ]; then\n            printf '%s' '{\"bundle\":{\"createUpdaterArtifacts\":false}}' > t.json\n            UPDATER_OFF=\"--config t.json\"\n          fi\n          npx --yes @tauri-apps/cli@2 build ${UPDATER_OFF}\n";
    assert!(
        disables_updater_artifacts_without_key(off_wired),
        "C-h FAIL 正确形态被误判 —— 判据过严会挡住正确的改动"
    );
    // 备用形态若无前提即启用，tag 发布会拿 .dmg 当更新产物
    assert!(
        !assembler_fallback_is_last_resort("  let hits = by(FALLBACK_PATTERNS[installer]);\n"),
        "C-h FAIL 判据无法识别无条件使用备用形态的旧写法"
    );
    assert!(
        !assembler_fallback_is_last_resort("  const hits = by(ARTIFACT_PATTERNS[installer]);\n"),
        "C-h FAIL 没有备用形态时判据被误判为通过"
    );
    assert!(
        !sig_strictness_is_tag_gated("node assemble.js --platform p"),
        "C-h FAIL 完全没有签名开关时被误判为通过"
    );
    assert!(
        sig_strictness_is_tag_gated("REQ=\"--require-sig\"; [[ \"${GITHUB_REF}\" == refs/tags/v* ]] || REQ=\"\"\nnode assemble.js ${REQ}"),
        "C-h FAIL 正确的 tag 分流形态被误判"
    );

    let old_asm = "  const missing = entries.filter((e) => !e.sig);\n  if (missing.length) {\n    console.error('缺 .sig');\n    process.exit(1);\n  }\n}\n";
    assert!(
        !assembler_failure_is_conditional(old_asm),
        "C-h FAIL 判据无法识别无条件早失败的旧形态"
    );
    let old_parse = "function parseArgs(argv) {\n  const o = {};\n  for (let i = 2; i < argv.length; i += 1) {\n    const a = argv[i];\n    if (a.startsWith('--')) { o[a.slice(2)] = argv[i + 1]; i += 1; }\n  }\n  return o;\n}\n";
    assert!(
        !parse_args_handles_bool_flags(old_parse),
        "C-h FAIL 判据无法识别吞掉布尔开关的旧 parseArgs"
    );

    // step_block 定位能力（否则 C-f 空转）
    let sample = "steps:\n      - name: A\n        run: one\n      - name: B\n        run: two\n      - name: A again\n";
    let b = step_block(sample, "B");
    assert!(
        b.contains("run: two") && !b.contains("run: one") && !b.contains("run: A again"),
        "C-h FAIL step_block 定位错误，C-f 会空转：{:?}",
        b
    );
}

#[test]
fn c_e_offender_detector_is_not_vacuous() {
    let old = "cargo test --bins --test bootstrap_flow --test update_guard_test --test platform_unsupported_structure_test 2>&1 | tail -80";
    assert!(
        has_hardcoded_test_targets(old),
        "C-e FAIL 判据无法识别硬编码 --test 列表 —— 门禁空转"
    );
    let new = "TARGETS=$(ls tests/*.rs | sed 's#tests/##' | grep -v '^updater_artifacts$' | sed 's/^/--test /')";
    assert!(
        !has_hardcoded_test_targets(new),
        "C-e FAIL 修复后的自动枚举行被误报为硬编码"
    );
    assert!(
        !has_hardcoded_test_targets("cargo test --test updater_artifacts -- --nocapture"),
        "C-e FAIL updater_artifacts 的单独步骤被误报"
    );
    // 反向：job_block 能正确定位（否则 C-c/C-d 空转）
    let sample = "jobs:\n  build:\n    runs-on: x\n    steps:\n      - run: cargo build\n\n  publish:\n    runs-on: y\n";
    let b = job_block(sample, "build");
    assert!(
        b.contains("cargo build") && !b.contains("runs-on: y"),
        "C-e FAIL job_block 定位错误，C-c/C-d 会空转：{:?}",
        b
    );
}

#[test]
fn c_i_tauri_cli_version_is_pinned() {
    let y = strip_yaml_comments(&ci());
    let bundle = step_block(&y, "Build + bundle (Tauri)");
    assert!(!bundle.is_empty(), "C-i FAIL 未找到打包步骤 —— 门禁空转");
    assert!(
        cli_version_pinned(&bundle),
        "C-i FAIL 打包用的 Tauri CLI 版本没有钉死（浮动 major 会让三平台产物出自不同版本 CLI）"
    );
    // 全文件都不许再出现浮动形态（注释里的历史说明已被剥掉，不算命中）。
    let floating: Vec<&str> = y
        .lines()
        .filter(|l| l.contains("@tauri-apps/cli@") && !l.contains("@tauri-apps/cli@${"))
        .map(|l| l.trim())
        .collect();
    assert!(
        floating.is_empty(),
        "C-i FAIL 仍有直接引用浮动 CLI 的行：{:?}",
        floating
    );

    // 反向：判据必须认得「写死但仍是浮动 major」与「有 env 却没被引用」两种劣化形态。
    let floating_major = "        env:\n          TAURI_CLI_VERSION: 2\n        run: |\n          npx --yes @tauri-apps/cli@2 build\n";
    assert!(
        !cli_version_pinned(floating_major),
        "C-i 反向失败：浮动 major 形态被判为已钉版"
    );
    let env_not_used = "        env:\n          TAURI_CLI_VERSION: 2.11.5\n        run: |\n          npx --yes @tauri-apps/cli@2 build\n";
    assert!(
        !cli_version_pinned(env_not_used),
        "C-i 反向失败：只声明 env 而命令未引用，也被判为已钉版"
    );
    let good = "        env:\n          TAURI_CLI_VERSION: 2.11.5\n        run: |\n          npx --yes \"@tauri-apps/cli@${TAURI_CLI_VERSION}\" build\n";
    assert!(
        cli_version_pinned(good),
        "C-i 反向失败：正确形态被判红（判据不可用）"
    );

    // 产线文档的 H4 行必须与 build.yml 同形态：命令换了而文档留着浮动写法，等于把旧事实继续传播。
    let dp = manifest_dir()
        .join("..")
        .join("docs")
        .join("RELEASE-STANDARD.md");
    let doc = fs::read_to_string(&dp).unwrap_or_else(|e| panic!("C-i FAIL 读取 {:?} 失败: {}", dp, e));
    let row_pinned = |row: &str| row.contains("@tauri-apps/cli@${TAURI_CLI_VERSION}");
    let h4 = doc
        .lines()
        .find(|l| l.contains("| H4 |"))
        .unwrap_or("（H4 行不存在）");
    assert!(
        row_pinned(h4),
        "C-i FAIL 产线文档 H4 行的 CLI 写法未与 build.yml 同步：{:?}",
        h4.trim()
    );
    assert!(
        !row_pinned("| H4 | 构建 + 打包 | `npx --yes @tauri-apps/cli@2 build` | 是 |"),
        "C-i 反向失败：浮动形态的文档行被判为已同步"
    );
}
