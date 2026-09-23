//! 引导页静态资源引用门禁。
//!
//! 引导页的 logo 由 CSS `mask: url("/dsh-logo.svg")` 现出来。webview 对取不到的 mask 资源
//! 不报错、不降级，只是让整块元素消失（mask 图为空 = 全透明）—— 页面照常跑完，图标静默不见。
//! `src=` 引到的 js 模块更极端：文件缺失即整段逻辑不加载，引导要到最后一步才失败。
//! 两类失效都不在编译期、也不在任何运行期校验的覆盖范围内，只能由门禁在提交时钉住。
//!
//! 锁定不变量：
//!   A-1 判据是纯函数（引用清单 + 存在性谓词 → 未解析清单），合成样本与实盘走同一路径
//!   A-2 反向：缺资源的合成样本必须被判出，且存在性谓词真的被消费
//!   A-3 实盘：两个引导页的每条 url()/src= 都指向 bootstrap/ 下真实存在的文件
//!   A-4 外链与 data: 不参与存在性判定
//!   A-5 反空转：实盘解析到的引用条数有下限，标记写法一变必须先红在这里

use std::fs;
use std::path::PathBuf;

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// 取 `start` 处（marker 之后）被引用的值：带引号取引号内，不带引号按 CSS 取到 `)`/`;`。
fn extract_quoted(text: &str, start: usize) -> Option<(String, usize)> {
    let b = text.as_bytes();
    let mut i = start;
    while i < b.len() && (b[i] == b' ' || b[i] == b'\t') {
        i += 1;
    }
    if i >= b.len() {
        return None;
    }
    let q = b[i];
    if q != b'"' && q != b'\'' {
        if q == b')' {
            return None;
        }
        let mut k = i;
        while k < b.len() && b[k] != b')' && b[k] != b';' && b[k] != b'"' {
            k += 1;
        }
        return if k == i {
            None
        } else {
            Some((text[i..k].to_string(), k))
        };
    }
    let close = (i + 1..b.len()).find(|&k| b[k] == q)?;
    Some((text[i + 1..close].to_string(), close + 1))
}

/// 扫出 `url(...)` 与 `src=...` 的引用目标原文。
fn refs(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    for marker in ["url(", "src="] {
        let mut from = 0usize;
        while let Some(i) = html[from..].find(marker) {
            let at = from + i + marker.len();
            match extract_quoted(html, at) {
                Some((v, next)) => {
                    out.push(v);
                    from = next;
                }
                None => from = at,
            }
        }
    }
    out
}

/// 引用目标 → 相对 frontendDist 根的本地键；网络/内联/锚点引用不参与判定。
fn local_key(target: &str) -> Option<String> {
    let t = target.trim();
    if t.is_empty()
        || t.starts_with("data:")
        || t.starts_with("http:")
        || t.starts_with("https:")
        || t.starts_with('#')
    {
        return None;
    }
    let t = match t.find(|c| c == '#' || c == '?') {
        Some(i) => &t[..i],
        None => t,
    };
    let t = t.trim_start_matches('/');
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

/// 判据本体：返回解析不到本地文件的引用。
fn unresolved<E: Fn(&str) -> bool>(pages: &[(String, String)], exists: E) -> Vec<String> {
    let mut bad = Vec::new();
    for (name, html) in pages {
        for r in refs(html) {
            if let Some(k) = local_key(&r) {
                if !exists(&k) {
                    bad.push(format!("{}: {} -> {}", name, r, k));
                }
            }
        }
    }
    bad
}

#[test]
fn a2_reverse_missing_refs_are_caught() {
    let pages = vec![(
        "p.html".to_string(),
        r#".x { mask: url("/gone.svg"); } <script src="js/also-gone.js"></script>"#.to_string(),
    )];
    let all_missing = unresolved(&pages, |_| false);
    assert_eq!(
        all_missing.len(),
        2,
        "A-2 FAIL 缺资源的样本必须全部判出，实际 {all_missing:?}"
    );
    let one_present = unresolved(&pages, |k| k == "gone.svg");
    assert_eq!(
        one_present.len(),
        1,
        "A-2 FAIL 存在性谓词未被消费（判据恒真）：{one_present:?}"
    );
    assert!(
        one_present[0].contains("also-gone.js"),
        "A-2 FAIL 判出的不是那条缺的文件：{one_present:?}"
    );
    let ext = vec![(
        "e.html".to_string(),
        r#"<img src="https://x.test/a.png"> .y { background: url(data:image/png;base64,AAA); }"#
            .to_string(),
    )];
    assert!(
        unresolved(&ext, |_| false).is_empty(),
        "A-4 FAIL 外链或 data: 被误判为缺资源"
    );
}

#[test]
fn a3_real_bootstrap_refs_all_resolve() {
    let root = manifest_dir().join("bootstrap");
    let mut pages: Vec<(String, String)> = Vec::new();
    for name in ["bootstrap.html", "shell.html"] {
        let html = fs::read_to_string(root.join(name)).expect("read bootstrap page");
        assert!(
            html.len() > 2000,
            "A-5 FAIL {name} 内容过短，判据将空转"
        );
        pages.push((name.to_string(), html));
    }
    // 9 个 js 模块 + mask 的两条写法（标准与 -webkit-）；解析器失配时先红在这里
    let parsed: usize = pages.iter().map(|(_, h)| refs(h).len()).sum();
    assert!(
        parsed >= 11,
        "A-5 FAIL 实盘只解析到 {parsed} 条引用（下限 11）—— 标记写法或页面结构已变，本门禁正在空转"
    );
    let bad = unresolved(&pages, |k| root.join(k).exists());
    assert!(
        bad.is_empty(),
        "A-3 FAIL 引导页引用了不存在的资源：{}",
        bad.join(" | ")
    );
}
