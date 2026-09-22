//! 镜像源适配（壳自持）：装机时机器上没有内核，壳必须先于内核完成镜像选择，内核消费壳投放的 registry.json。
//! 不变量：并行探测全部候选（串行会被最慢源拖死）；Node 版本取全部可达源中的最高版本（镜像同步滞后，首个成功即采用会装到旧版）；
//! 选最快且确实提供该版本的源下载，结果缓存到 ~/.dsh/shell/mirrors.json 并导出内核。

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// npm registry 预设：全部经真实 tarball 下载验证过 - 仅元数据可读不算可用（部分镜像只代理元数据、不代理 tarball）。
pub const NPM_PRESETS: [&str; 6] = [
    "https://registry.npmmirror.com",
    "https://registry.npmjs.org",
    "https://repo.huaweicloud.com/repository/npm/",
    "https://mirrors.cloud.tencent.com/npm",
    "https://npmreg.proxy.ustclug.org",
    "https://r.cnpmjs.org",
];

/// Node 发行镜像预设：全部经真实下载 + 该源自身 SHASUMS256 校验通过，比「URL 可达」严格得多
/// （能过滤代理不完整、文件损坏、清单与文件不匹配的镜像）。
/// 各源同步进度不同不影响使用：latest_lts() 跨全部可达源取最高版本，再在提供该版本的源中选最快者，滞后源作回退。
pub const NODE_PRESETS: [&str; 10] = [
    "https://nodejs.org/dist",
    "https://npmmirror.com/mirrors/node",
    "https://mirrors.huaweicloud.com/nodejs",
    "https://mirrors.aliyun.com/nodejs-release",
    "https://mirror.sjtu.edu.cn/nodejs-release",
    "https://mirrors.cloud.tencent.com/nodejs-release",
    "https://mirror.nju.edu.cn/nodejs-release",
    "https://mirrors.tuna.tsinghua.edu.cn/nodejs-release",
    "https://mirrors.bfsu.edu.cn/nodejs-release",
    "https://mirrors.pku.edu.cn/nodejs-release",
];

/// 壳自更新清单预设（Tauri updater 的 endpoints，完整清单 URL）：必须是能直链**静态 JSON 文件**的源 -
/// 多数 npm 镜像只提供 registry 元数据 API，npmmirror /files/ 返回 403，npm 官方无静态文件服务。
pub const SHELL_PRESETS: [&str; 2] = [
    "https://unpkg.com/@dsh-sup/shell-release@latest/shell-manifest.json",
    "https://cdn.jsdelivr.net/npm/@dsh-sup/shell-release@latest/shell-manifest.json",
];

/// 安装包（清单里 `platforms.*.url` 指向的东西）的可换主机 npm CDN。
/// 入选判据：实测能把本平台安装包的完整字节取回（HTTP 200 + 全量），只看清单或元数据可达不算；
/// 逐源取样与排除理由见 docs/SHELL-UPDATE-CHANNEL-VERIFICATION.md。
pub const SHELL_ARTIFACT_NPM_CDNS: [&str; 2] = [
    "https://unpkg.com",
    "https://cdn.jsdelivr.net/npm",
];

/// 安装包的另一类源：CI 挂上 GitHub Release 的同名安装程序（`v<ver>/<文件名>`）。
pub const SHELL_ARTIFACT_RELEASE_BASE: &str =
    "https://github.com/lobbowen/dsh-supervisor-launcher/releases/download";

/// 除清单声明的那一个 URL 外，安装包还该按序尝试哪些源（声明源永远第一）。
/// Tauri 清单每平台只有一个产物 URL，插件下载阶段不会自己换源；Update::download_url 是公开字段，壳可改写它，
/// 验签仍在插件内按清单签名做，任何源都没有让 updater 装上篡改包的能力。
/// 文件名不含架构标识时不挂 Release 候选：macOS 两架构产物同名，换过去取到的是错架构的包（表现为验签失败，更难排障）。
pub fn artifact_candidates(declared: &tauri::Url, ver: &str) -> Vec<tauri::Url> {
    let mut out = vec![declared.clone()];
    let path = declared.path().to_string();
    let file = path.rsplit('/').next().unwrap_or("").to_string();
    let mut add = |raw: String| {
        if let Ok(u) = tauri::Url::parse(&raw) {
            if !out.iter().any(|e| e == &u) {
                out.push(u);
            }
        }
    };
    // 只有 npm 包内路径才能换主机（清单生成器用的就是这个形态）。
    if let Some(tail) = path.strip_prefix("/@dsh-sup/") {
        for base in SHELL_ARTIFACT_NPM_CDNS {
            add(format!("{}/@dsh-sup/{}", base, tail));
        }
    }
    if !ver.is_empty() && ARCH_TOKENS.iter().any(|a| file.contains(a)) {
        add(format!("{}/v{}/{}", SHELL_ARTIFACT_RELEASE_BASE, ver, file));
    }
    out
}

const ARCH_TOKENS: [&str; 5] = ["x64", "amd64", "x86_64", "arm64", "aarch64"];

/// 单次探测的总超时。
/// index.json 单个就有 1.5~2MB（90+ 版本）：过短的超时会让全部镜像在慢网/代理下一起超时，
/// 把「探针过短」误报成「全部 Node 镜像均不可用」。
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(20);

/// 进程级 HTTP agent（统一代理与超时）：壳的全部 HTTP 都必须经本 agent，否则探测与下载会出现两套代理/超时行为。
/// ureq 默认既不读环境变量也不读系统代理，「只有代理、没有直连」的机器上会全源直连必败而无从得知原因；
/// 故此处按 ALL_PROXY / HTTPS_PROXY / HTTP_PROXY（大小写）显式解析，无效地址忽略并记日志。
pub fn agent() -> &'static ureq::Agent {
    static A: std::sync::OnceLock<ureq::Agent> = std::sync::OnceLock::new();
    A.get_or_init(|| {
        let mut b = ureq::AgentBuilder::new().timeout(PROBE_TIMEOUT);
        if let Some(u) = proxy_url() {
            match ureq::Proxy::new(&u) {
                Ok(p) => { b = b.proxy(p); }
                Err(e) => { crate::update::log(&format!("代理地址无效，已忽略（{}）: {}", u, e)); }
            }
        } else {
            b = b.try_proxy_from_env(true);
        }
        b.build()
    })
}

fn proxy_url() -> Option<String> {
    for k in ["ALL_PROXY", "all_proxy", "HTTPS_PROXY", "https_proxy", "HTTP_PROXY", "http_proxy"] {
        if let Ok(v) = std::env::var(k) {
            let v = v.trim();
            if !v.is_empty() { return Some(v.to_string()); }
        }
    }
    // 环境变量没有时，退到**系统代理**（Windows WinINET / macOS SystemConfiguration）。
    crate::platform::system_proxy()
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 壳自持镜像配置（~/.dsh/shell/mirrors.json）。
#[derive(Clone)]
pub struct Mirrors {
    pub node: Vec<String>,
    pub npm: Vec<String>,
    pub shell: Vec<String>,
    pub selected_node: Option<String>,
    pub selected_npm: Option<String>,
    /// 选中 npm 源在**实测时**的延迟（ms）。随 selected_npm 一起落盘 ——
    /// 使「选择」与「延迟」同源，避免导出时被调用方传入别的延迟（如 Node 侧）。
    pub selected_npm_latency_ms: Option<u64>,
    pub checked_at: Option<u64>,
}

impl Default for Mirrors {
    fn default() -> Self {
        Mirrors {
            node: NODE_PRESETS.iter().map(|s| s.to_string()).collect(),
            npm: NPM_PRESETS.iter().map(|s| s.to_string()).collect(),
            shell: SHELL_PRESETS.iter().map(|s| s.to_string()).collect(),
            selected_node: None,
            selected_npm: None,
            selected_npm_latency_ms: None,
            checked_at: None,
        }
    }
}

fn cfg_path() -> PathBuf {
    crate::update::state_dir().join("mirrors.json")
}

/// 读取壳镜像配置；缺失或损坏时返回内置预设（**绝不失败** —— 装机首启必须可用）。
pub fn load() -> Mirrors {
    let mut m = Mirrors::default();
    if let Ok(s) = std::fs::read_to_string(cfg_path()) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&s) {
            let take = |key: &str| -> Option<Vec<String>> {
                v.get(key).and_then(|x| x.as_array()).map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str())
                        .map(|x| x.to_string())
                        .filter(|x| !x.is_empty())
                        .collect::<Vec<_>>()
                })
            };
            if let Some(list) = take("node") {
                if !list.is_empty() {
                    m.node = list;
                }
            }
            if let Some(list) = take("npm") {
                if !list.is_empty() {
                    m.npm = list;
                }
            }
            if let Some(list) = take("shell") {
                if !list.is_empty() {
                    m.shell = list;
                }
            }
            if let Some(s2) = v.get("selectedNode").and_then(|x| x.as_str()) {
                m.selected_node = Some(s2.to_string());
            }
            if let Some(s2) = v.get("selectedNpm").and_then(|x| x.as_str()) {
                m.selected_npm = Some(s2.to_string());
            }
            m.selected_npm_latency_ms = v.get("selectedNpmLatencyMs").and_then(|x| x.as_u64());
            m.checked_at = v.get("checkedAt").and_then(|x| x.as_u64());
        }
    }
    m
}

/// 写壳镜像配置（原子写）。
pub fn save(m: &Mirrors) -> Result<(), String> {
    let path = cfg_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("创建壳状态目录失败: {}", e))?;
    }
    let v = serde_json::json!({
        "node": m.node,
        "npm": m.npm,
        "shell": m.shell,
        "selectedNode": m.selected_node,
        "selectedNpm": m.selected_npm,
        "selectedNpmLatencyMs": m.selected_npm_latency_ms,
        "checkedAt": m.checked_at,
    });
    let body = serde_json::to_string_pretty(&v).unwrap_or_default();
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, body + "\n").map_err(|e| format!("写入镜像配置失败: {}", e))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("提交镜像配置失败: {}", e))?;
    Ok(())
}

/// 契约 schema 版本（内核据此判断格式是否兼容）：v2 在 mode/origins/manualOrigin 之外
/// 增加 catalog（全集）/ selected（选择结果）/ probe（探测规格）- 内核照 probe 规格执行即可与壳得到同一答案。
pub const CONTRACT_SCHEMA: u64 = 2;

/// 导出镜像契约给内核（`<产品状态根>/supervisor/registry.json`）。
/// 所有权在壳：装壳那一刻机器上没有内核，壳必须先于内核完成镜像选择，内核只消费产物。
/// 写全集 catalog + selected + probe（而非仅 origins）；由 main.rs setup 在启动时无条件导出；内核已写 manual 时不覆盖。
pub fn export_to_kernel(m: &Mirrors) -> Result<(), String> {
    export_to_kernel_with(m, None)
}

/// 同 [`export_to_kernel`]，但可携带选中源的实测延迟（`selected.latencyMs`）。
pub fn export_to_kernel_with(m: &Mirrors, latency_ms: Option<u128>) -> Result<(), String> {
    if m.npm.is_empty() {
        return Ok(());
    }
    let path = crate::env::supervisor_dir().join("registry.json");
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("创建内核状态目录失败: {}", e))?;
    }
    // 用户在内核面板手动固定过源，不覆盖（尊重显式意图）。
    if let Ok(s) = std::fs::read_to_string(&path) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&s) {
            if v.get("mode").and_then(|x| x.as_str()) == Some("manual") {
                return Ok(());
            }
        }
    }
    // 不变量：selected 的延迟必须与**所选 npm 源同源**。
    // 优先用随 selected_npm 一起落盘的实测延迟；调用方传入仅作兜底。
    let eff_latency_ms: Option<u128> = m
        .selected_npm_latency_ms
        .map(|v| v as u128)
        .or(latency_ms);
    let selected = m.selected_npm.as_ref().map(|origin| {
        serde_json::json!({
            "origin": origin,
            "latencyMs": eff_latency_ms,
            "checkedAt": m.checked_at,
        })
    });
    let v = serde_json::json!({
        "schema": CONTRACT_SCHEMA,
        "writtenBy": format!("shell@{}", env!("CARGO_PKG_VERSION")),
        "writtenAt": now_secs(),
        "mode": "auto",
        // 既有字段：保持向后兼容（旧内核只读这三项也能工作）
        "origins": m.npm,
        "manualOrigin": m.selected_npm.clone().unwrap_or_else(|| m.npm.first().cloned().unwrap_or_default()),
        // v2 字段：全集 + 选择结果 + 探测规格
        "catalog": m.npm,
        "selected": selected,
        "probe": {
            "kind": "package-metadata",
            "pathTemplate": npm_probe_path(),
            // 必须由 PROBE_TIMEOUT 派生（单一事实源）：两侧探测超时不一致时，
            //   介于两者之间的源会一侧判可达、另一侧判不可达，选源再次分叉。
            "timeoutMs": PROBE_TIMEOUT.as_millis() as u64,
        },
    });
    let body = serde_json::to_string_pretty(&v).unwrap_or_default();
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, body + "\n").map_err(|e| format!("写入内核 registry.json 失败: {}", e))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("提交内核 registry.json 失败: {}", e))?;
    Ok(())
}

/// 壳启动时无条件导出契约：探测失败时也必须写，否则内核完全拿不到源。
/// 失败只记日志，绝不阻断引导 - 契约是增强，不是壳启动的前提。
pub fn export_on_boot() {
    let m = load();
    if let Err(e) = export_to_kernel(&m) {
        crate::update::log(&format!("启动导出镜像契约失败（不影响引导）: {}", e));
    }
}

/// 一次并行探测的结果。
pub struct Probe {
    pub source: String,
    pub ok: bool,
    pub latency_ms: u128,
    pub body: Option<String>,
    /// 失败原因（HTTP / DNS / TLS / 代理 / 读体）。必须保留：
    ///   排障时要能区分是断网、证书、代理还是镜像 404。
    pub error: Option<String>,
}

/// npm registry 的探测探针包名（必须是一个真实存在的包）：多数 registry 根路径返回 404，
/// 用根路径会把健康源判为不可达、无谓地少一个可用镜像。用我们自己的平台包：真实存在，且与最终用途一致。
fn npm_probe_path() -> String {
    // 探测用包：优先内核平台包（真实存在）；失败时退回一个必然存在的小包。
    crate::core::package_name().unwrap_or_else(|_| "@dsh-sup/dsh-core-linux-x64".to_string())
}

/// **并行**探测全部候选：对每个源请求 path，记录延迟与响应体。
/// 用 std::thread::scope 实现并发（std 自带，无需新依赖）；单源超时 PROBE_TIMEOUT，整体耗时约为最慢者而非累加。
/// `path` 为空时视为 npm registry 探测：自动使用真实包名而非根路径。
pub fn probe_all(sources: &[String], path: &str) -> Vec<Probe> {
    // 空 path -> npm 探测：用真实包名（根路径会 404，导致健康源被误判不可达）。
    let owned;
    let path = if path.is_empty() {
        owned = npm_probe_path();
        owned.as_str()
    } else {
        path
    };
    let out: Mutex<Vec<Probe>> = Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for src in sources {
            let out = &out;
            scope.spawn(move || {
                let url = format!(
                    "{}/{}",
                    src.trim_end_matches("/"),
                    path.trim_start_matches("/")
                );
                let started = Instant::now();
                let mut probe = Probe {
                    source: src.clone(),
                    ok: false,
                    latency_ms: 0,
                    body: None,
                    error: None,
                };
                match agent().get(&url).timeout(PROBE_TIMEOUT).call() {
                    Ok(resp) => {
                        let mut buf = Vec::new();
                        use std::io::Read;
                        match resp.into_reader().read_to_end(&mut buf) {
                            Ok(_) => {
                                probe.latency_ms = started.elapsed().as_millis();
                                probe.ok = true;
                                probe.body = Some(String::from_utf8_lossy(&buf).into_owned());
                            }
                            Err(e) => {
                                probe.latency_ms = started.elapsed().as_millis();
                                probe.error = Some(format!("读取响应失败: {}", e));
                            }
                        }
                    }
                    Err(e) => {
                        probe.latency_ms = started.elapsed().as_millis();
                        probe.error = Some(format!("{}", e));
                    }
                }
                out.lock().unwrap().push(probe);
            });
        }
    });
    let mut v = out.into_inner().unwrap_or_default();
    v.sort_by(|a, b| match (a.ok, b.ok) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.latency_ms.cmp(&b.latency_ms),
    });
    v
}

// 镜像是壳全部网络动作的基础设施，不是「下载 Node 的辅助」：引导开始即后台预热（不阻塞任何步骤），
// 结果任何时刻经 cached() 可读（无网络 I/O）并写透诊断串。旧引导只在缺 Node/版本过低才调 probeMirrorThen，
// Node 达标的主力用户永远看不到镜像结果，现与「是否需要下载 Node」解耦。

/// 预热结果缓存：`None` 表示尚未测速完成。
static WARM: std::sync::OnceLock<Mutex<Option<ProbeSnapshot>>> = std::sync::OnceLock::new();

fn warm() -> &'static Mutex<Option<ProbeSnapshot>> {
    WARM.get_or_init(|| Mutex::new(None))
}

/// 一次预热的结果快照（含逐源延迟，供诊断展示）。
#[derive(Clone)]
pub struct ProbeSnapshot {
    /// 选中的 Node 源（最快且提供目标版本者；预热阶段仅取最快可达）。
    pub node_best: Option<String>,
    pub node_latency_ms: Option<u128>,
    pub npm_best: Option<String>,
    pub npm_latency_ms: Option<u128>,
    pub npm_probes: Vec<(String, bool, u128)>,
    pub at: u64,
}

/// 读预热缓存（**无 I/O**，任何时刻可安全调用）。
pub fn cached() -> Option<ProbeSnapshot> {
    match warm().lock() {
        Ok(g) => g.clone(),
        Err(e) => e.into_inner().clone(),
    }
}

/// 是否已在飞（避免重复预热）。
static WARMING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// 后台预热镜像测速：立即返回，结果稍后经 `cached()` 读取。
/// 全部网络 I/O 在独立线程完成，调用方（引导页）绝不阻塞。
pub fn warmup_async() {
    if WARMING.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return; // 已有在飞预热
    }
    let _ = std::thread::Builder::new()
        .name("mirror-warmup".to_string())
        .spawn(|| {
            let m = load();
            // 两组并行探测（各自内部已并行）
            let node_p = probe_all(&m.node, "index.json");
            let npm_p = probe_all(&m.npm, "");
            // 选中的源 + 其延迟。
            let pick = |v: &[Probe]| -> (Option<String>, Option<u128>) {
                let best = v.iter().find(|p| p.ok);
                (best.map(|p| p.source.clone()), best.map(|p| p.latency_ms))
            };
            // 逐源明细只给 npm（`ProbeSnapshot::npm_probes` 消费）；node 侧明细无消费方，不返回。
            let pick_all = |v: &[Probe]| -> Vec<(String, bool, u128)> {
                v.iter().map(|p| (p.source.clone(), p.ok, p.latency_ms)).collect()
            };
            let (nb, nl) = pick(&node_p);
            let (mb, ml) = pick(&npm_p);
            let mp = pick_all(&npm_p);
            let snap = ProbeSnapshot {
                node_best: nb,
                node_latency_ms: nl,
                npm_best: mb,
                npm_latency_ms: ml,
                npm_probes: mp,
                at: now_secs(),
            };
            // 必须把选中的 npm 源与延迟落盘（并刷新 checked_at）：只放内存快照的话，
            //   registry.json 的 selected 永远是 null，内核「优先采用壳投放的 selected」分支
            //   永不执行，跨仓「同源」承诺失效。只在探测确实得到结果时写，
            //   避免把「全不可达」写成一次有效选择。
            let mut m = m;
            if let (Some(best), Some(ms)) = (snap.npm_best.clone(), snap.npm_latency_ms) {
                m.selected_npm = Some(best);
                m.selected_npm_latency_ms = Some(ms as u64);
                m.checked_at = Some(now_secs());
                if let Err(e) = save(&m) {
                    crate::update::log(&format!("预热结果落盘失败（不影响本次引导）: {}", e));
                }
            }
            if let Ok(mut g) = warm().lock() {
                *g = Some(snap);
            }
            WARMING.store(false, std::sync::atomic::Ordering::SeqCst);
        });
}

#[cfg(test)]
mod tests {
    use super::artifact_candidates;

    fn url(s: &str) -> tauri::Url {
        tauri::Url::parse(s).expect("测试内的 URL 必须是合法的")
    }
    fn strs(v: &[tauri::Url]) -> Vec<String> {
        v.iter().map(|u| u.to_string()).collect()
    }

    /// 声明源永远第一，且不重复（换源是把「这一台没给到字节」接下去，不是替换主源）。
    #[test]
    fn declared_source_stays_first_and_deduped() {
        let d = url("https://unpkg.com/@dsh-sup/shell-win-x64@1.2.0/artifact/dsh-supervisor_1.2.0_x64-setup.exe");
        let got = artifact_candidates(&d, "1.2.0");
        assert_eq!(got[0], d, "第一个候选必须是清单声明的那个 URL");
        let all = strs(&got);
        assert_eq!(all.len(), all.iter().collect::<std::collections::HashSet<_>>().len(),
            "候选不能重复: {:?}", all);
    }

    /// Windows：npm 换主机 + 同名 Release 资产；且**绝不**出现实测不成立的源。
    #[test]
    fn windows_candidates_cover_measured_sources_only() {
        let d = url("https://unpkg.com/@dsh-sup/shell-win-x64@1.2.0/artifact/dsh-supervisor_1.2.0_x64-setup.exe");
        let all = strs(&artifact_candidates(&d, "1.2.0"));
        assert!(all.iter().any(|u| u.starts_with("https://cdn.jsdelivr.net/npm/@dsh-sup/shell-win-x64@1.2.0/")),
            "jsdelivr 的 npm 路径要在（它对 .exe 会给 403，换下一个源是预期）: {:?}", all);
        assert!(all.contains(&"https://github.com/lobbowen/dsh-supervisor-launcher/releases/download/v1.2.0/dsh-supervisor_1.2.0_x64-setup.exe".to_string()),
            "文件名带架构 → 同名 Release 资产要在: {:?}", all);
        // 实测取不到安装包字节的源，一律不许回到表里（判据与理由见 SHELL-UPDATE-CHANNEL-VERIFICATION.md）。
        for banned in [
            "npmmirror", "jsdmirror", "fastly", "gcore", "testingcf",
            "tencent", "aliyun", "huaweicloud", "unpkg.net", "gh-proxy", "ghfast", "gitmirror",
        ] {
            assert!(all.iter().all(|u| !u.contains(banned)),
                "假镜像回流: {} 出现在 {:?}", banned, all);
        }
    }

    /// macOS 两架构产物同名，不给 Release 候选（拿到的会是错架构的包）。
    #[test]
    fn ambiguous_asset_name_loses_the_release_candidate() {
        let d = url("https://unpkg.com/@dsh-sup/shell-darwin-arm64@1.2.0/artifact/dsh-supervisor.app.tar.gz");
        let all = strs(&artifact_candidates(&d, "1.2.0"));
        assert!(all.iter().all(|u| !u.contains("releases/download")),
            "文件名不含架构时不该挂 Release 候选: {:?}", all);
    }

    /// 非 npm 包路径（清单直接指向别处）时不得拼出垃圾候选；版本空则不挂 Release。
    #[test]
    fn foreign_or_missing_version_yields_only_the_declared_url() {
        let d = url("https://example.com/files/shell-setup.exe");
        assert_eq!(strs(&artifact_candidates(&d, "1.2.0")), vec![d.to_string()]);
        let npm = url("https://unpkg.com/@dsh-sup/shell-linux-x64@1.2.0/artifact/dsh-supervisor_1.2.0_amd64.deb");
        let nover = strs(&artifact_candidates(&npm, ""));
        assert_eq!(nover.len(), 2, "无版本号时只保留 npm 同路径候选: {:?}", nover);
    }
}
