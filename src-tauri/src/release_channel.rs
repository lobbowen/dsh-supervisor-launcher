//! 发布通道选版 —— **契约 §3 的冻结算法**在桌面壳里的唯一实现。
//!
//! 事实源：仓库根目录 `RELEASE-CHANNEL-CONTRACT.md`
//!   · §2 通道定义（canary / beta / rc / latest / rollback）
//!   · §3 选版算法（五步，**冻结**，两仓必须一致）
//!   · §5 灰度名单（本机配置 + installId 主依据 / 主机名兜底 + 可选包）
//!
//! 本模块**不触网**：registry 元数据由调用方（`core.rs`）探测后传入 ——
//! 选版规则因此可以被完整地单元测试（契约 §6 的 RC-G1/RC-G2 门禁正是断言「决策在这里」）。
//!
//! ## 为什么**不**取 `dist-tags` + `versions` 的全量最高（RC-1）
//!
//! 全量最高会**绕过通道控制**：BETA 的数字可能压过 RC，于是「正式版用户」被装上测试版。
//! 通道语义由 npm 的 dist-tag 承载，选版必须**优先信 tag**：
//! `rollback` > （灰度机）`canary` > `latest` > `versions` 兜底。
//!
//! ## 为什么回退用**独立 tag**，而**不**用「latest 低于全量最高即回退」（契约 §2）
//!
//! 后者经验证不可行，有三条硬缺陷：
//!   1. **语义歧义**：`latest < max` 既可能「latest 陈旧未更新」，也可能「人工回退」，
//!      二者**机器不可分**。实证：npm 上曾出现 `latest=0.1.1-BETA.1`、`max=0.1.5-BETA.7`，
//!      真相是陈旧（SEA 时代遗留）；按该规则会把全体用户「回退」到废弃架构。
//!   2. **发布竞态**：`npm publish` 写 `versions` 与设 tag **非原子**，中间必有一拍
//!      `latest=旧, max=新` —— 每次正常发布都会**误触发全量回退**。
//!   3. **不可观测**：`latest` 永远存在，无法回答「当前是否有回退在生效」。
//!
//! 结论：回退必须是**显式信号**（独立 `rollback` tag）；解除回退 = `npm dist-tag rm <pkg> rollback`，
//!   **不依赖任何版本比较**（RC-3）；`rollback` tag 存在 = 回退进行中，一眼可见（§4）。

use serde_json::Value;

// ── 通道字面量：tag 名与「选择依据」共用一套（避免散落字符串）──
/// 紧急回退通道（**最高优先级**，RC-2）。
pub const CH_ROLLBACK: &str = "rollback";
/// 灰度通道（**仅名单内机器**，RC-4）。
pub const CH_CANARY: &str = "canary";
/// 正式通道（RC-1：唯一正式真源）。
pub const CH_LATEST: &str = "latest";
/// 第 ④ 步的兼容兜底来源 —— 不是 tag，是「从 versions 表里取最高合法版本」。
pub const CH_VERSIONS: &str = "versions";

/// 灰度名单的**包内来源**（契约 §5 第 2 条；可选包，不存在不影响）。
pub const CANARY_ALLOWLIST_PKG: &str = "@dsh-sup/canary-allowlist";

/// 本机配置里声明灰度的键（契约 §5 第 1 条：「本机配置 canary: true」）。
const CFG_CANARY: &str = "canary";
/// 本机配置里声明「愿意让**包内名单**决定灰度」的键（§5 第 2 条的入口，见 `canary_machine_with`）。
const CFG_ALLOWLIST: &str = "canaryAllowlist";
/// 等价于配置的环境变量（测试机/CI 不必改 config.json）。
const ENV_CANARY: &str = "DSH_CANARY";
/// 等价于 `canaryAllowlist` 配置的环境变量。
const ENV_ALLOWLIST: &str = "DSH_CANARY_ALLOWLIST";
/// installId 的**环境变量覆盖**（与内核 `install-id.js` 同口径）：
/// 测试机/外部灰度用户显式声明本机 installId，免去读文件，也便于测试。
const ENV_INSTALL_ID: &str = "DSH_CANARY_ID";

/// 选版结果：目标版本 + **选择依据**。
///
/// `via` 只作可观测性（日志 / `--core-plan` 自检）：它回答「这一版是从哪个通道选出来的」，
///   与版本号自身的命名（`channel_of`：beta/rc/…）是两件事 ——
///   例如回退目标本身可能是一个 `-BETA.` 版本，此时 `via=rollback` 而版本名是 beta。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selected {
    pub version: String,
    pub via: &'static str,
}

/// 取一个 **dist-tag**，且**只认合法版本字面量**。
///
/// 非法 tag（空串 / 占位符 / 拼错）必须当作「不存在」继续走下一步 ——
///   否则一个手滑的 `npm dist-tag add` 会让选版卡死或装上一个不存在的版本。
fn tag(meta: &Value, name: &str) -> Option<String> {
    let v = meta.get("dist-tags")?.get(name)?.as_str()?;
    if crate::core::is_valid_version(v) {
        Some(v.to_string())
    } else {
        None
    }
}

/// `versions` 表里的**最高合法版本**（第 ④ 步的兼容兜底）。
fn highest_version(meta: &Value) -> Option<String> {
    let obj = meta.get("versions")?.as_object()?;
    let mut best: Option<String> = None;
    for k in obj.keys() {
        if !crate::core::is_valid_version(k) {
            continue;
        }
        let better = best
            .as_deref()
            .map(|b| crate::core::semver_cmp(k, b) > 0)
            .unwrap_or(true);
        if better {
            best = Some(k.clone());
        }
    }
    best
}

/// 第 ⑤ 步的失败信息：**必须带现场**（出现过哪些 tag、versions 有多少条），
///   否则「什么都没有」这种结论在用户现场无从排查（RC-5：绝不静默，也绝不空口）。
fn empty_error(meta: &Value) -> String {
    let tags: Vec<&str> = meta
        .get("dist-tags")
        .and_then(|x| x.as_object())
        .map(|o| o.keys().map(|s| s.as_str()).collect())
        .unwrap_or_default();
    let n = meta
        .get("versions")
        .and_then(|x| x.as_object())
        .map(|o| o.len())
        .unwrap_or(0);
    format!(
        "registry 元数据中没有任何可用版本（dist-tags=[{}]；versions 条目 {} 个且无一合法）",
        if tags.is_empty() { "无".to_string() } else { tags.join(",") },
        n
    )
}

/// 契约 §3 的**冻结算法**：输入 registry 元数据与本机灰度判定，输出目标版本。
///
/// 五步顺序**不可调换**（契约 §3 逐条对应）：
///   ① `dist-tags.rollback` 合法 → 取它（RC-2：优先级高于一切，**含灰度**）
///   ② 本机在灰度名单 且 `dist-tags.canary` 合法 → 取它（RC-4：定向）
///   ③ `dist-tags.latest` 合法 → 取它（RC-1：**优先信 latest**）
///   ④ 否则取 `versions` 中最高合法版本（兼容兜底）
///   ⑤ 皆无 → **明确 Err**（RC-5：绝不猜、绝不静默降级为「已是最新」）
///
/// `canary_machine` 由调用方用 [`canary_machine_with`] 判定后传入 —— 本函数保持**纯函数**，
///   使 §3 的每个分支都能被直接构造用例覆盖（契约 §6 门禁要求）。
pub fn select(meta: &Value, canary_machine: bool) -> Result<Selected, String> {
    // ① 回退：最高优先级。注意它**先于**灰度 —— 紧急回退必须对灰度机同样立即生效（RC-2）。
    if let Some(v) = tag(meta, CH_ROLLBACK) {
        return Ok(Selected { version: v, via: CH_ROLLBACK });
    }
    // ② 灰度：只有名单内机器才看 canary；名单外机器即使 tag 存在也不受影响（RC-4）。
    if canary_machine {
        if let Some(v) = tag(meta, CH_CANARY) {
            return Ok(Selected { version: v, via: CH_CANARY });
        }
    }
    // ③ 正式：跟随我们的发布（RC-1）。**不得**因为 versions 里有更大的数字就改选它。
    if let Some(v) = tag(meta, CH_LATEST) {
        return Ok(Selected { version: v, via: CH_LATEST });
    }
    // ④ 兼容兜底：latest 缺失/非法（例如误删 tag）时不至于让全体用户失效。
    if let Some(v) = highest_version(meta) {
        return Ok(Selected { version: v, via: CH_VERSIONS });
    }
    // ⑤ 明确失败。
    Err(empty_error(meta))
}

// ── §5 灰度名单 ──────────────────────────────────────────────────────────

/// 本机配置是否声明灰度（`config.json` 的 `canary: true`）。
pub fn canary_in_config() -> bool {
    crate::env::config_flag(CFG_CANARY)
}

/// 环境变量是否声明灰度（`DSH_CANARY=1`）。
pub fn canary_in_env() -> bool {
    std::env::var(ENV_CANARY).map(|v| v.trim() == "1").unwrap_or(false)
}

/// 契约 §5 **来源①**：本机配置或环境变量命中。
pub fn local_canary_hit() -> bool {
    canary_in_config() || canary_in_env()
}

/// 本机是否显式声明「愿意让**包内名单**决定灰度」（§5 来源②的入口）。
pub fn allowlist_opt_in() -> bool {
    crate::env::config_flag(CFG_ALLOWLIST)
        || std::env::var(ENV_ALLOWLIST).map(|v| v.trim() == "1").unwrap_or(false)
}

/// 内核 installId 的落盘文件（相对内核状态目录）—— 契约 §5.2：
/// 内核 `src/platform/install-id.js` **首次读取时生成一次**，此后只读不改。
const INSTALL_ID_FILE: &str = "install-id";

/// 名单文档的**唯一格式版本**（契约 §5.3）：`schema` 不为 1 → 整份名单作废。
const ALLOWLIST_SCHEMA: u64 = 1;

/// 命中依据：本机配置/环境变量直接声明灰度（契约 §5.5 ①，短路，不查包）。
pub const MATCH_LOCAL: &str = "local";
/// 命中依据：`entries[].installId` 命中（契约 §5.3 **主依据**）。
pub const MATCH_INSTALL_ID: &str = "installId";
/// 命中依据：`hostnames[]` 命中（契约 §5.3 **兜底**，仅 CI/容器）。
pub const MATCH_HOSTNAME: &str = "hostname";

/// 名单命中结果：`via` 是命中依据（可观测性），`note` 是名单里那句「谁/为何」的备注。
///
/// `note` **只用于日志** —— 排障时能直接看出"为什么这台机器进了灰度"；
///   它**绝不参与匹配**（契约 §5.3 明列），否则一句备注就能意外放行灰度。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AllowlistHit {
    pub via: &'static str,
    pub note: Option<String>,
}

/// 读取本机 installId（契约 §5.2：由**内核**生成并持久化，壳只读不生成）。
///
/// ## 为什么壳**绝不**自己生成一份
///
/// 灰度名单里登记的是用户从面板复制的那个 UUID —— 即**内核写下的那个**。
/// 若壳再生成一份，两仓 UUID 不一致 → 名单**永远匹配不上**，且现象是
///   「灰度静默失效」，在用户现场几乎无法排查。故本函数只有两个来源，
///   **没有任何生成分支**：
///   · `DSH_CANARY_ID` 环境变量**优先**（测试机/外部灰度用户显式声明，也便于测试）；
///   · 否则读 `<supervisorDir>/install-id`（纯文本一行）。
///
/// 返回 `None` = **内核尚未启动过**（文件不存在/为空/不可读）——
///   调用方按**非灰度**处理并留痕（见 `canary_machine`），**不得**在这里补生成。
pub fn install_id() -> Option<String> {
    if let Ok(s) = std::env::var(ENV_INSTALL_ID) {
        let t = s.trim();
        if !t.is_empty() {
            return Some(t.to_string());
        }
    }
    read_install_id_file(&crate::env::supervisor_dir().join(INSTALL_ID_FILE))
}

/// 读 installId 文件（**纯函数，不碰进程环境**，故可用临时文件直接测）。
///
/// 只取**第一行**并 trim：契约 §5.2 规定"纯文本一行"，但容忍末尾换行/空白
///   —— 若把整份内容（含换行）拿去比较，就永远不可能等于名单里的 UUID。
fn read_install_id_file(path: &std::path::Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let line = text.lines().next().unwrap_or("").trim();
    if line.is_empty() {
        None
    } else {
        Some(line.to_string())
    }
}

/// 兜底标识：主机名（Unix/macOS 的 `HOSTNAME`、Windows 的 `COMPUTERNAME`）。
///
/// ## 为什么它只是**兜底**（契约 §5.1）
///
/// 主机名可改、容器里随机、还会重名 —— 拿它当主依据会让灰度"命中不该命中的机器"。
///   仅用于 **CI/容器**（无常驻 UUID）这类特例，且名单侧新增条目须在 PR 注明理由。
/// std 没有 `gethostname`，环境变量是零新增依赖的等价来源。
pub fn hostnames() -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    for k in ["HOSTNAME", "COMPUTERNAME"] {
        if let Ok(s) = std::env::var(k) {
            let t = s.trim();
            if !t.is_empty() && !v.iter().any(|x| x.eq_ignore_ascii_case(t)) {
                v.push(t.to_string());
            }
        }
    }
    v
}

/// 从 JSON 数组里取**字符串字段**（`key` 为空 = 数组元素本身就是字符串）。
///
/// 只取字符串、逐个 trim、丢掉空白：名单是**人工维护**的 JSON，
///   多一个空格或写成数字都不该被"猜"成有效标识（宁可漏，不可错）。
fn strings_in(list: Option<&Vec<Value>>, key: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let Some(list) = list else { return out };
    for it in list {
        let s = if key.is_empty() {
            it.as_str()
        } else {
            it.get(key).and_then(|x| x.as_str())
        };
        if let Some(s) = s {
            let t = s.trim();
            if !t.is_empty() {
                out.push(t.to_string());
            }
        }
    }
    out
}

/// 名单是否命中本机（契约 §5.3 格式 + §5.5 ③ 判定）。
///
/// ## 为什么删除"兼容 7 种 JSON 形状"（契约 §5.3 收敛要求）
///
/// 旧实现靠 `ID_KEYS` 猜 `id/machine/machineId/...` 六七种键。猜测的代价不是"漏命中"，
///   而是**误命中**：一份改了结构的新名单可能被旧键意外命中，把**不该进灰度的机器**放进去。
///   契约已把格式收敛为唯一形状，故这里**只认** `schema:1` + `entries[].installId` + `hostnames[]`，
///   **不认识的形状一律不匹配**（不猜）。
///
/// 匹配顺序**先主后辅**：installId（§5.1 铁律：唯一主依据）命中即返回；
///   主机名仅在前者未命中时兜底（CI/容器无常驻 UUID）。返回的 `via` 如实记录命中依据，
///   `note` 是名单里"谁/为何"的备注，**只供日志**。
pub fn allowlist_match(
    doc: &Value,
    install_id: Option<&str>,
    hostnames: &[String],
) -> Option<AllowlistHit> {
    // schema 不为 1（含缺失/类型不对）→ **忽略整份名单**（契约 §5.3：不猜）。
    //   注意这是"整份作废"而非"按旧格式尽力匹配"：未来格式变化时，
    //   宁可让灰度暂时不命中（可见、可修），也绝不用旧规则去解释新文档（可能误命中且不可见）。
    if doc.get("schema").and_then(|x| x.as_u64()) != Some(ALLOWLIST_SCHEMA) {
        return None;
    }

    let entries = doc.get("entries").and_then(|x| x.as_array());

    // 主依据：installId（UUID，精确匹配、大小写不敏感）。
    if let Some(id) = install_id.map(str::trim).filter(|s| !s.is_empty()) {
        let ids = strings_in(entries, "installId");
        if let Some(i) = ids.iter().position(|e| e.eq_ignore_ascii_case(id)) {
            // note 与 entries 同下标取值 —— 只作日志，绝不参与匹配。
            let note = entries
                .and_then(|a| a.get(i))
                .and_then(|x| x.get("note"))
                .and_then(|x| x.as_str())
                .map(|s| s.to_string());
            return Some(AllowlistHit {
                via: MATCH_INSTALL_ID,
                note,
            });
        }
    }

    // 兜底：主机名（仅 CI/容器；契约 §5.1 表格已注明它不稳定、可伪造）。
    //   先取一次名单侧的 hostnames，避免对本机每个候选主机名重复解析整份数组。
    let listed = strings_in(doc.get("hostnames").and_then(|x| x.as_array()), "");
    if hostnames.iter().any(|h| {
        let h = h.trim();
        !h.is_empty() && listed.iter().any(|e| e.eq_ignore_ascii_case(h))
    }) {
        return Some(AllowlistHit {
            via: MATCH_HOSTNAME,
            note: None,
        });
    }

    None
}

/// 灰度判定的**纯逻辑核**（本机标识与网络都由调用方注入，便于测试）。
///
/// ## 短路顺序 = 性能约束（契约 §5.5）
///
/// 「包内名单」是一次**额外的网络请求**。若无条件查它，每个普通用户在每次引导都要多付一次；
/// 而契约 §5.5 明确「名单**否定优先**，未命中即非灰度」。故：
///   ① `local_hit`（`canary: true` / `DSH_CANARY=1`）→ **本地已命中，直接判灰度**，
///      **不再查包**（结论已定，任何额外请求都不可能改变它）；
///   ② 未命中且 **未** `opt_in`（`canaryAllowlist: true` / `DSH_CANARY_ALLOWLIST=1`）
///      → `None`，**零额外请求**（这是绝大多数机器的路径）；
///   ③ 只有**显式进入候选**的机器才读包（供外部灰度用户 ——
///      他们本机没有 `canary:true`，靠我们远程维护的包内名单定向；
///      我们控制名单，客户只负责 opt-in）。
///
/// 返回 `Ok(None)` = 未命中；`Ok(Some(hit))` = 命中（`hit.via` 说明依据，供日志）。
/// 返回 `Err` 表示名单包**读取失败**（网络/解析）——由调用方决定是否记日志；
///   语义上它等价于「未命中」（可选包不存在**不影响**选版）。
pub fn canary_machine_with<F>(
    local_hit: bool,
    opt_in: bool,
    install_id: Option<&str>,
    hostnames: &[String],
    fetch: F,
) -> Result<Option<AllowlistHit>, String>
where
    F: FnOnce(&str) -> Result<Value, String>,
{
    if local_hit {
        return Ok(Some(AllowlistHit {
            via: MATCH_LOCAL,
            note: None,
        }));
    }
    if !opt_in {
        return Ok(None);
    }
    let doc = fetch(CANARY_ALLOWLIST_PKG)?;
    Ok(allowlist_match(&doc, install_id, hostnames))
}

/// 生产入口：读取本机 installId/主机名，命中与失败都**留痕**，失败按非灰度处理。
pub fn canary_machine<F>(local_hit: bool, opt_in: bool, fetch: F) -> bool
where
    F: FnOnce(&str) -> Result<Value, String>,
{
    let id = install_id();
    // 契约 §5.2 + §5.5：**内核尚未启动过**（install-id 文件不存在）时按**非灰度**处理并留痕。
    //   这里**绝不**补生成 UUID —— 生成是内核 `install-id.js` 的职责；
    //   壳若自行生成，面板上显示的会是另一个 UUID，灰度名单永远匹配不上（静默失效）。
    //   仍照常调用匹配（主机名兜底对 CI/容器有效），只是主依据缺席。
    if id.is_none() && opt_in && !local_hit {
        crate::update::log(
            "灰度名单：本机读不到 installId（内核尚未启动过？也未设 DSH_CANARY_ID），\
             本轮只能按主机名兜底匹配；壳不会自行生成 installId（那是内核 install-id.js 的职责）",
        );
    }
    match canary_machine_with(local_hit, opt_in, id.as_deref(), &hostnames(), fetch) {
        Ok(Some(hit)) => {
            crate::update::log(&format!(
                "灰度名单命中（依据={}{}）",
                hit.via,
                hit.note
                    .as_deref()
                    .map(|n| format!("；名单备注：{}", n))
                    .unwrap_or_default()
            ));
            true
        }
        Ok(None) => false,
        Err(e) => {
            // 留痕但**不致命**：可选包缺失/不可达 ≠ 选版失败。
            crate::update::log(&format!("灰度名单包不可用（按非灰度处理）：{}", e));
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Map};

    /// 造 registry 元数据：`dist-tags` + `versions`。
    fn meta(tags: &[(&str, &str)], versions: &[&str]) -> Value {
        let mut t = Map::new();
        for (k, v) in tags {
            t.insert((*k).to_string(), json!(v));
        }
        let mut vs = Map::new();
        for v in versions {
            vs.insert((*v).to_string(), json!({}));
        }
        json!({ "dist-tags": Value::Object(t), "versions": Value::Object(vs) })
    }

    /// 断言 §3 的某一步：返回选中版本与选择依据。
    fn sel(m: &Value, canary: bool) -> (String, &'static str) {
        let s = select(m, canary).expect("应当选出目标版本");
        (s.version, s.via)
    }

    // ── ① rollback：最高优先级（RC-2）────────────────────────────────────

    #[test]
    fn step1_rollback_wins_over_everything() {
        let m = meta(
            &[
                ("rollback", "0.1.5-BETA.6"),
                ("canary", "0.9.9-BETA.1"),
                ("latest", "0.2.0"),
            ],
            &["0.1.5-BETA.6", "0.9.9-BETA.1", "0.2.0"],
        );
        // 对**灰度机**也必须是 rollback（RC-2：高于一切，含灰度）
        assert_eq!(sel(&m, true), ("0.1.5-BETA.6".into(), CH_ROLLBACK));
        assert_eq!(sel(&m, false), ("0.1.5-BETA.6".into(), CH_ROLLBACK));
    }

    #[test]
    fn step1_rollback_wins_even_when_lower_than_latest() {
        // 回退目标通常**低于** latest —— 这正是「绝不能按版本高低重排优先级」的理由。
        let m = meta(
            &[("rollback", "0.1.4"), ("latest", "0.1.9")],
            &["0.1.4", "0.1.9"],
        );
        assert_eq!(sel(&m, false), ("0.1.4".into(), CH_ROLLBACK));
    }

    #[test]
    fn step1_invalid_rollback_falls_through() {
        // 非法 tag = 不存在；不得因此卡死，必须继续走下一步。
        let m = meta(
            &[("rollback", "not-a-version"), ("latest", "0.1.9")],
            &["0.1.9"],
        );
        assert_eq!(sel(&m, false), ("0.1.9".into(), CH_LATEST));
    }

    // ── ② canary：定向灰度（RC-4）───────────────────────────────────────

    #[test]
    fn step2_canary_only_for_listed_machine() {
        let m = meta(
            &[("canary", "0.1.6-BETA.1"), ("latest", "0.1.5-BETA.7")],
            &["0.1.5-BETA.7", "0.1.6-BETA.1"],
        );
        assert_eq!(sel(&m, true), ("0.1.6-BETA.1".into(), CH_CANARY));
        // RC-4：名单外机器**不受 canary tag 影响**
        assert_eq!(sel(&m, false), ("0.1.5-BETA.7".into(), CH_LATEST));
    }

    #[test]
    fn step2_canary_invalid_or_absent_falls_to_latest() {
        let bad = meta(
            &[("canary", "v0.1.6"), ("latest", "0.1.5")],
            &["0.1.5"],
        );
        assert_eq!(sel(&bad, true), ("0.1.5".into(), CH_LATEST));
        let absent = meta(&[("latest", "0.1.5")], &["0.1.5"]);
        assert_eq!(sel(&absent, true), ("0.1.5".into(), CH_LATEST));
    }

    // ── ③ latest：正式真源（RC-1）───────────────────────────────────────

    #[test]
    fn step3_latest_is_trusted_even_when_versions_is_higher() {
        // **RC-1 回归**：旧实现取「全量最高」→ 会选到 1.0.0-BETA.1（BETA 压过正式）。
        let m = meta(&[("latest", "0.1.5-BETA.3")], &["0.1.5-BETA.3", "1.0.0-BETA.1"]);
        assert_eq!(sel(&m, false), ("0.1.5-BETA.3".into(), CH_LATEST));
        // 反向自检：旧的「全量最高」形态确实会给出**不同**答案（门禁非空转，RC-G5）
        let old_form = highest_version(&m).unwrap();
        assert_eq!(old_form, "1.0.0-BETA.1");
        assert_ne!(old_form, "0.1.5-BETA.3");
    }

    #[test]
    fn step3_latest_wins_over_versions_even_when_lower() {
        let m = meta(&[("latest", "0.1.1-BETA.1")], &["0.1.1-BETA.1", "0.1.5-BETA.7"]);
        assert_eq!(sel(&m, false), ("0.1.1-BETA.1".into(), CH_LATEST));
    }

    // ── ④ versions 兜底（latest 缺失/非法）──────────────────────────────

    #[test]
    fn step4_falls_back_to_highest_version_when_latest_missing() {
        let m = meta(&[], &["0.1.5-BETA.3", "0.1.5-BETA.7", "0.1.4"]);
        assert_eq!(sel(&m, false), ("0.1.5-BETA.7".into(), CH_VERSIONS));
    }

    #[test]
    fn step4_fallback_ignores_invalid_version_keys() {
        let m = meta(&[("latest", "")], &["bad", "0.1.4", "0.1.5-BETA.1"]);
        assert_eq!(sel(&m, false), ("0.1.5-BETA.1".into(), CH_VERSIONS));
    }

    #[test]
    fn step4_no_dist_tags_at_all() {
        let m = json!({ "versions": { "0.2.0": {}, "0.1.0": {} } });
        assert_eq!(sel(&m, false), ("0.2.0".into(), CH_VERSIONS));
    }

    // ── ⑤ 明确失败（RC-5）─────────────────────────────────────────────

    #[test]
    fn step5_errors_instead_of_guessing() {
        // 全非法 → Err（绝不静默降级为「已是最新」）
        let m = meta(&[("latest", "nope"), ("canary", "")], &["x", "y"]);
        let e = select(&m, true).expect_err("应当明确报错");
        assert!(e.contains("没有任何可用版本"), "错误信息应说明原因：{}", e);
    }

    #[test]
    fn step5_empty_metadata_errors() {
        let e = select(&json!({}), false).expect_err("空元数据必须报错");
        assert!(e.contains("dist-tags=[无]"), "错误信息应带现场：{}", e);
    }

    #[test]
    fn step5_error_mentions_present_tags_for_diagnosis() {
        let m = meta(&[("next", "0.3.0")], &[]);
        let e = select(&m, false).expect_err("无可用通道必须报错");
        assert!(e.contains("next"), "错误信息应列出实际存在的 tag：{}", e);
    }

    // ── §5 灰度名单（契约 §5.3 唯一格式）────────────────────────────────

    /// 进程环境是**全局共享**的：凡改动 `DSH_CANARY_ID` 的用例必须互斥串行，
    ///   否则并行执行时一个用例会读到另一个用例刚设的值 —— 表现为随机假失败。
    /// 用 `into_inner()` 容忍中毒：某个用例 panic 后其余用例仍能继续跑出真实结论。
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// 本机 installId 常量（契约 §5.2：UUID v4，小写）。
    const ID: &str = "550e8400-e29b-41d4-a716-446655440000";
    /// 另一台机器的 installId —— 用于构造"不在名单内"。
    const OTHER_ID: &str = "11111111-2222-4333-8444-555555555555";

    /// 造一份**契约 §5.3 规范格式**的名单。
    fn allowlist(entries: Value, hostnames: Value) -> Value {
        json!({
            "schema": 1,
            "updatedAt": "2026-09-16T00:00:00Z",
            "entries": entries,
            "hostnames": hostnames,
        })
    }

    #[test]
    fn allowlist_local_hit_short_circuits_without_query() {
        // §5.5 ① 命中 → 判灰度，且**绝不**发那次额外请求。
        let hit = canary_machine_with(true, false, None, &[], |_| {
            panic!("本机配置已命中，不得再查名单包（会多一次网络请求）")
        })
        .unwrap();
        assert_eq!(hit.map(|h| h.via), Some(MATCH_LOCAL));
    }

    #[test]
    fn allowlist_normal_machine_pays_no_extra_request() {
        // §5.5 ② 普通机器（未标记、未 opt-in）→ 未命中，且**绝不**发请求。
        let hit = canary_machine_with(false, false, Some(ID), &["my-box".into()], |_| {
            panic!("未 opt-in 的机器不得查名单包（正常用户零额外请求）")
        })
        .unwrap();
        assert!(hit.is_none());
    }

    #[test]
    fn allowlist_matches_install_id() {
        // §5.3 **主依据**：entries[].installId 命中；大小写不敏感。
        let doc = allowlist(
            json!([{ "installId": ID.to_uppercase(), "note": "张工内测机" }]),
            json!([]),
        );
        assert_eq!(
            allowlist_match(&doc, Some(ID), &[]),
            Some(AllowlistHit { via: MATCH_INSTALL_ID, note: Some("张工内测机".into()) }),
        );
        // 不命中 → None
        assert_eq!(allowlist_match(&doc, Some(OTHER_ID), &[]), None);
    }

    #[test]
    fn allowlist_hostname_is_fallback_hit() {
        // §5.3 **兜底**：hostnames[] 命中（CI/容器无常驻 UUID）。
        let doc = allowlist(json!([]), json!(["build-bot-01"]));
        assert_eq!(
            allowlist_match(&doc, Some(OTHER_ID), &["Build-Bot-01".into()]),
            Some(AllowlistHit { via: MATCH_HOSTNAME, note: None }),
        );
        // installId 未命中且主机名也未命中 → None
        assert_eq!(allowlist_match(&doc, Some(OTHER_ID), &["my-box".into()]), None);
        // 本机没有主机名（也不可能命中）→ None
        assert_eq!(allowlist_match(&doc, None, &[]), None);
    }

    #[test]
    fn allowlist_requires_schema_1() {
        // 契约 §5.3：schema 不为 1 → **忽略整份名单**（不猜）。
        for bad in [json!(2), json!("1"), json!(null), json!(1.5)] {
            let doc = json!({
                "schema": bad,
                "entries": [{ "installId": ID }],
                "hostnames": ["build-bot-01"],
            });
            assert_eq!(
                allowlist_match(&doc, Some(ID), &["build-bot-01".into()]),
                None,
                "schema 非 1 时整份名单必须作废：{}",
                doc
            );
        }
        // schema 缺失同样作废（旧格式没有 schema 字段）。
        let no_schema = json!({ "entries": [{ "installId": ID }] });
        assert_eq!(allowlist_match(&no_schema, Some(ID), &[]), None);
    }

    #[test]
    fn allowlist_note_never_participates_in_matching() {
        // §5.3：note 只供日志。它出现在 entries 里、与 installId 同层，也**绝不能**被当标识匹配。
        let doc = allowlist(json!([{ "installId": OTHER_ID, "note": ID }]), json!([ID]));
        // 本机 installId 恰好等于别人的 note → 不得命中 installId 分支；
        // 主机名传空，故整体未命中。
        assert_eq!(allowlist_match(&doc, Some(ID), &[]), None);
        // 但主机名兜底仍照常工作（note 里的值不进入 hostnames 判断）。
        assert_eq!(
            allowlist_match(&doc, None, &[ID.to_string()]),
            Some(AllowlistHit { via: MATCH_HOSTNAME, note: None }),
        );
    }

    #[test]
    fn allowlist_legacy_shapes_are_rejected() {
        // 回归：旧实现"猜 7 种形状"必须已删除 —— 下列旧形状即使包含本机标识也**不得**命中。
        for legacy in [
            json!([ID, OTHER_ID]),
            json!([{ "id": ID }, { "machineId": ID }]),
            json!({ "machines": [ID] }),
            json!({ "allow": [{ "hostname": ID }] }),
            json!({ "allowlist": [ID] }),
            json!({ "ids": [ID] }),
            json!({ "id": ID }),
        ] {
            assert_eq!(
                allowlist_match(&legacy, Some(ID), &[ID.to_string()]),
                None,
                "旧形状必须被拒绝（不再猜测）：{}",
                legacy
            );
        }
    }

    #[test]
    fn allowlist_candidate_reads_package_and_matches() {
        // §5.5 ③：opt-in 的机器才读包，且按 installId 命中。
        let doc = allowlist(json!([{ "installId": ID }]), json!([]));
        assert_eq!(
            canary_machine_with(false, true, Some(ID), &[], |_| Ok(doc.clone()))
                .unwrap()
                .map(|h| h.via),
            Some(MATCH_INSTALL_ID),
        );
        let miss = allowlist(json!([{ "installId": OTHER_ID }]), json!([]));
        assert!(canary_machine_with(false, true, Some(ID), &[], |_| Ok(miss))
            .unwrap()
            .is_none());
    }

    #[test]
    fn allowlist_failure_is_reported_not_fatal() {
        let r = canary_machine_with(false, true, Some(ID), &[], |_| {
            Err("404 Not Found".to_string())
        });
        assert!(r.is_err(), "读取失败必须如实回传（由调用方按非灰度处理并留痕）");
    }

    // ── installId 的读取（契约 §5.2：壳只读内核写的那个文件）────────────

    #[test]
    fn read_install_id_file_takes_first_line_and_tolerates_whitespace() {
        let dir = std::env::temp_dir().join(format!("dsh-install-id-ut-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("install-id");

        // 正常：纯文本一行（内核原子写 + 0600 的产物）。
        std::fs::write(&p, format!("{}\n", ID)).unwrap();
        assert_eq!(read_install_id_file(&p).as_deref(), Some(ID));

        // 无末尾换行（某些平台/编辑器）也必须能读出同一个 UUID。
        std::fs::write(&p, ID).unwrap();
        assert_eq!(read_install_id_file(&p).as_deref(), Some(ID));

        // 空文件 / 只有空白 = **内核尚未写入** → None（调用方按非灰度处理并留痕）。
        std::fs::write(&p, "  \n").unwrap();
        assert_eq!(read_install_id_file(&p), None);

        // 文件不存在 = **内核尚未启动过** → None，**绝不**在这里生成。
        assert_eq!(read_install_id_file(&dir.join("nope")), None);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn install_id_env_override_wins_and_is_trimmed() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // 与内核同口径：DSH_CANARY_ID 覆盖文件来源（也便于测试/外部灰度用户显式声明）。
        std::env::set_var(ENV_INSTALL_ID, format!("  {}  ", ID));
        assert_eq!(install_id().as_deref(), Some(ID));
        std::env::remove_var(ENV_INSTALL_ID);
    }

    #[test]
    fn canary_machine_production_entry_swallows_package_errors() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // 生产入口：包读不到 → 非灰度（不 panic、不阻断选版）
        std::env::set_var(ENV_ALLOWLIST, "1");
        std::env::set_var(ENV_INSTALL_ID, "unit-box");
        let hit = canary_machine(false, true, |_| Err("404".to_string()));
        std::env::remove_var(ENV_ALLOWLIST);
        std::env::remove_var(ENV_INSTALL_ID);
        assert!(!hit);
    }
}
