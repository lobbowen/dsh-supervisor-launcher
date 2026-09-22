//! 有界子进程执行（**公共设施**，2026-09-11）。
//!
//! == 为什么需要它（同一根因的第二次出现） ==
//!
//! 「环境探测卡死」的根因是「无界阻塞调用 + 被 await」。审计发现**同一模式散布在多处**：
//!
//!   · 当时的 service.rs 与 main.rs —— systemctl / loginctl / launchctl / schtasks 全用 .output()（无界）；
//!   · 当时的 main.rs —— start_guard_service / stop_guard_service / taskkill 同样无界；
//!   · node.rs    —— pkexec / osascript / powershell 同样无界。
//!
//! 而这些调用**几乎都在引导的关键路径上**（建立服务定义 → 启动守卫 → 进入面板）。
//! systemctl 在 dbus 会话异常、systemd 无响应时会长时间挂起 —— 此时
//! guard_start 永不返回，引导页永久停在「正在启动守卫…」。
//!
//! 故把「有界执行」提取为公共设施，**所有**外部命令一律经它执行，
//! 而不是在每个调用点各写一遍（那正是缺陷能够分散潜伏的原因）。
//!
//! == 实现要点 ==
//!
//! · 输出重定向到**临时文件**而非管道：若用 Stdio::piped() 且不读取，
//!   冗长输出填满 OS 管道缓冲区（约 64KB）后子进程会阻塞，反而制造死锁。
//! · 轮询 try_wait + 超时 kill：std 无跨平台的 wait-with-timeout，
//!   而子进程一旦挂起，同步 wait 就是无界的 —— 必须自己轮询。
//! · Windows 加 CREATE_NO_WINDOW：GUI 进程调控制台程序不弹黑框。

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// 一次外部命令的**完整执行记录**（全仓唯一的子进程结果形态）。
///
/// 原 `struct Output { code: Option<String> }` 有两个问题，都是线上故障的直接成因：
///
/// · **命令原文一律丢失**。`run()` 只回传退出码与输出，于是「schtasks 失败（退出码 1）」
///   就是用户能拿到的全部信息 —— 不知道是哪个任务、什么动作、由哪条命令产出。
///   现在 `program`/`args` 在 `run()` 内部从 `Command` 直接捕获，调用方**无法**漏记。
/// · `code: Option<String>` 把「未取到退出码」这一实现细节当成了文案载体
///   （各调用点分别往里塞 `"killed"` / `"超时被终止"`），于是「超时」与「被信号终止」
///   两种含义不同的事实在类型上塌成同一个字符串。现 `code: Option<i32>` 只装真退出码，
///   「超时」由 `timed_out` 承载，措辞统一由 [`ExecRecord::code_label`] 决定。
pub struct ExecRecord {
    pub program: String,
    pub args: Vec<String>,
    /// 因超时被终止（此时 `code` 为 `None`）。
    pub timed_out: bool,
    /// 本次执行的超时预算（秒），供超时文案如实说出「超过多少秒」。
    pub timeout_secs: u64,
    /// 子进程退出码；`None` = 未取到（被终止或超时）。
    pub code: Option<i32>,
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

impl ExecRecord {
    /// 人可读正文：stderr 优先，为空才回退 stdout（两路都不丢）。
    pub fn detail(&self) -> &str {
        let e = self.stderr.trim();
        if !e.is_empty() {
            return e;
        }
        self.stdout.trim()
    }

    /// 命令行（诊断用；只截断不省略，便于直接贴回终端复现）。
    pub fn command_line(&self) -> String {
        cmd_line_of(&self.program, &self.args)
    }

    /// 退出状态的**统一措辞**（原 4 处各写一份："killed" / "超时被终止" / 裸数字）。
    pub fn code_label(&self) -> String {
        if self.timed_out {
            return format!("超时被终止（>{}s）", self.timeout_secs);
        }
        match self.code {
            Some(c) => format!("退出码 {}", c),
            None => "被终止（无退出码）".to_string(),
        }
    }

    /// **唯一**的失败文案渲染点：`<什么事> 失败（<退出状态> · <命令>）：<正文>`。
    ///
    /// 收敛理由：`core.rs`（npm）、`runtime_contract.rs`（版本探针）、`platform/windows.rs`
    /// （tar / Expand-Archive）此前各自 `format!` 一遍，四处措辞不一致且都不带命令原文 ——
    /// 报错越不一致，跨阶段对比现场时越难判断是同一条路还是两条路。
    pub fn failure(&self, what: &str) -> String {
        let detail = self.detail();
        format!(
            "{} 失败（{} · {}）：{}",
            what,
            self.code_label(),
            self.command_line(),
            if detail.is_empty() { "子进程无任何输出".to_string() } else { tail(detail, 700) }
        )
    }

    /// 成功时取 stdout（已 trim）；失败时给出含命令原文的结构化失败文案。
    pub fn ok_or_stderr(self, what: &str) -> Result<String, String> {
        if self.success {
            Ok(self.stdout.trim().to_string())
        } else {
            Err(self.failure(what))
        }
    }
}

/// 命令行的唯一拼装点：`ExecRecord::command_line` 与「命令没能跑起来」的 `Err` 共用，
/// 避免同一条命令在两种出口下长得不一样（那会让跨阶段比对现场失效）。
fn cmd_line_of(program: &str, args: &[String]) -> String {
    if args.is_empty() {
        return program.to_string();
    }
    format!("{} {}", program, args.join(" "))
}

/// 给外部命令加上「不弹控制台窗口」标志（POSIX 平台无需处理）。
pub fn prepare(cmd: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    #[cfg(not(windows))]
    {
        let _ = cmd;
    }
}

/// 子进程**运行期间**的现场（只含测得到的量）。
///
/// 为什么需要：`npm install` 不吐百分比，输出又被重定向到临时文件（不经管道，见模块头），
///   所以运行期唯一能如实说出的是「已经等多久 + 它自己写了多少行 + 最后一行写了什么」。
///   此前这些信息一个字都没传给 UI，于是「正在安装内核…」可以静默 15 分钟。
pub struct Live {
    pub elapsed: Duration,
    /// 已产出的**非空**行数（stdout + stderr 合计）。
    pub lines: usize,
    /// 最后一行非空输出（已 trim）；无输出时为空串。
    pub last_line: String,
}

/// 有界执行子进程 + **运行期心跳**：每 `heartbeat` 把 [`Live`] 交给 `on_live`。
///
/// 与 [`run`] 的唯一区别就是这条心跳；超时/终止/临时文件清理语义完全相同
///   （二者共用 [`run_inner`]，因此不存在「两条路行为分叉」的可能）。
pub fn run_watch(
    cmd: &mut Command,
    timeout: Duration,
    heartbeat: Duration,
    on_live: &dyn Fn(&Live),
) -> Result<ExecRecord, String> {
    run_inner(cmd, timeout, Some((heartbeat, on_live)))
}

/// 有界执行子进程：超出 timeout 即 kill（**绝不无限阻塞调用方**）。
///
/// 返回值语义（2026-09-21 收口）：只有「**没能跑起来**」才是 `Err`（临时文件建不了、
/// spawn 失败、等待子进程出错）；「跑完了但没成功」**含超时被杀**一律是 `Ok(ExecRecord)`。
/// 原实现在超时这条路上 `return Err(format!(...))`，把已经拿到的 stdout/stderr 与退出状态
/// 一起降格成一条字符串 —— 调用方无法区分「命令不存在」与「命令挂了」，
/// 而这两件事对用户的可操作结论完全不同。
pub fn run(cmd: &mut Command, timeout: Duration) -> Result<ExecRecord, String> {
    run_inner(cmd, timeout, None)
}

/// `run` / `run_watch` 的**唯一**实现体（见 [`run_watch`] 关于行为分叉的说明）。
fn run_inner(
    cmd: &mut Command,
    timeout: Duration,
    watch: Option<(Duration, &dyn Fn(&Live))>,
) -> Result<ExecRecord, String> {
    // 命令原文在此捕获（而不是让每个调用方自己记得带上）：这是「诊断必含命令」的唯一保证。
    let program = cmd.get_program().to_string_lossy().into_owned();
    let args: Vec<String> = cmd.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
    let timeout_secs = timeout.as_secs();
    let cmd_line = cmd_line_of(&program, &args);
    let dir = std::env::temp_dir();
    let stamp = format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );
    let out_path = dir.join(format!("dsh-cmd-out-{}.log", stamp));
    let err_path = dir.join(format!("dsh-cmd-err-{}.log", stamp));

    let out_file = std::fs::File::create(&out_path)
        .map_err(|e| format!("{} 无法执行（创建临时输出文件失败）: {}", cmd_line, e))?;
    // 不变量：任一临时文件创建失败时必须清掉已建的文件（不留残渣）。
    //   空的 dsh-cmd-out-*.log 在 temp 目录（本仓另有清理脚本会竞争，见 AUDIT-HANDOFF 9.4）。
    //   同理，spawn 失败时两个文件都已建好，也必须一并清理。
    let err_file = match std::fs::File::create(&err_path) {
        Ok(f) => f,
        Err(e) => {
            cleanup(&out_path, &err_path); // 清掉已建的 out_path（err_path 可能未建，cleanup 容忍）
            return Err(format!("{} 无法执行（创建临时错误文件失败）: {}", cmd_line, e));
        }
    };
    cmd.stdout(Stdio::from(out_file));
    cmd.stderr(Stdio::from(err_file));
    cmd.stdin(Stdio::null());
    prepare(cmd);

    // spawn 失败（命令不存在/不可执行）此前直接返回 Err，两个临时文件永久残留。
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            cleanup(&out_path, &err_path);
            return Err(format!("{} 无法启动: {}", cmd_line, e));
        }
    };

    let start = Instant::now();
    let mut next_beat = start;
    let status = loop {
        match child.try_wait() {
            Ok(Some(st)) => break st,
            Ok(None) => {
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    let err = read_log(&err_path);
                    let out = read_log(&out_path);
                    cleanup(&out_path, &err_path);
                    // 超时不再降格成一条 Err 字符串：已拿到的输出与「超时」这一事实
                    //   一起留在记录里，调用方才能说出「挂了」而不是「失败了」。
                    return Ok(ExecRecord {
                        program,
                        args,
                        timed_out: true,
                        timeout_secs,
                        code: None,
                        success: false,
                        stdout: out,
                        stderr: err,
                    });
                }
                if let Some((heartbeat, on_live)) = watch {
                    // 只在心跳节拍上读一次文件：每 25ms 轮询临时文件是纯粹的浪费。
                    let now = Instant::now();
                    if now >= next_beat {
                        next_beat = now + heartbeat;
                        on_live(&live_of(&out_path, &err_path, now - start));
                    }
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                cleanup(&out_path, &err_path);
                return Err(format!("{} 等待子进程失败: {}", cmd_line, e));
            }
        }
    };

    let stdout = read_log(&out_path);
    let stderr = read_log(&err_path);
    cleanup(&out_path, &err_path);
    Ok(ExecRecord {
        program,
        args,
        timed_out: false,
        timeout_secs,
        code: status.code(),
        success: status.success(),
        stdout,
        stderr,
    })
}

/// 从两份临时输出里取出运行期现场（**不等子进程结束**，所以允许读到半行）。
fn live_of(out_path: &std::path::Path, err_path: &std::path::Path, elapsed: Duration) -> Live {
    let out = read_log(out_path);
    let err = read_log(err_path);
    let mut lines = 0usize;
    let mut last_line = String::new();
    for l in out.lines().chain(err.lines()) {
        let t = l.trim();
        if t.is_empty() {
            continue;
        }
        lines += 1;
        last_line = t.to_string();
    }
    Live { elapsed, lines, last_line }
}

/// 执行并返回 stdout，失败即 Err（供「只关心成败」的调用点使用）。
pub fn run_checked(cmd: &mut Command, timeout: Duration, what: &str) -> Result<String, String> {
    run(cmd, timeout)?.ok_or_stderr(what)
}

/// 执行但**忽略结果**（用于尽力而为的动作，如 daemon-reload）。仍然有界。
pub fn run_lossy(cmd: &mut Command, timeout: Duration) {
    let _ = run(cmd, timeout);
}

/// 读取子进程输出（**按平台码页解码**，见 [`decode_console`]）。
fn read_log(p: &std::path::Path) -> String {
    match std::fs::read(p) {
        Ok(bytes) => decode_console(&bytes),
        Err(_) => String::new(),
    }
}

/// 子进程原始字节 → 字符串，**全仓唯一解码点**。
///
/// ## 为什么不是 `String::from_utf8_lossy`（原实现，2026-09-12）
///
/// 上一次「GBK 修复」只做到**详情不丢**，代价是**详情不可读**：中文 Windows 的控制台程序
/// （`schtasks` / `taskkill` / `tar.exe`）按 **OEM 码页**（zh-CN 为 936/GBK）写 stderr，
/// 按 UTF-8 做 lossy 解码会把每个双字节换成 U+FFFD。线上表现就是用户报来的那段乱码：
///
/// ```text
/// schtasks /Run 失败（退出码 1）：<乱码>
/// ```
///
/// `锟斤拷` 连排是「GBK 双字节被按 UTF-8 lossy」的指纹，所以能确定原文是一条**中文控制台
/// 消息**（具体字串已被 lossy 抹掉、不可恢复；`schtasks /Run` 退 1 在中文 Windows 上最常见
/// 的就是「系统找不到指定的文件。」）。无论具体是哪句，它都是整条链路上唯一的真话 ——
/// 也就是说：缺陷不是「详情丢了」，是「真话被毁容」，
/// 于是每一次 Windows 现场排障都只能靠猜。丢字节与丢可读性是同一个缺陷的两半。
///
/// ## 现在的做法
///
/// 1. 先按 UTF-8 试：`node` / `npm` / 新式 `powershell` 输出本就是 UTF-8；GBK 的 ASCII 半区
///    与 UTF-8 同形，纯英文消息不会因这一步被跳过（这一步保证不回归现有正确输出）。
/// 2. 不是 UTF-8 才交给**操作系统**按当前控制台输出码页做 MBCS→UTF-16
///    （`MultiByteToWideChar`）。因此对 936/950/437/1251 等任意本地码页都成立 ——
///    硬编码「中文 = GBK」只会把繁体与俄语用户留在乱码里。
/// 3. 转换仍失败才 lossy，保底不丢字节。
///
/// ## 为什么落在 infra 而不是 `platform/`
///
/// 这是「**进程创建与交互**」这一原语的平台差异，与同文件的 [`prepare`]（`CREATE_NO_WINDOW`）
/// 完全同类，G1 白名单登记的就是这一条理由；若下沉到 `platform/`，`platform` 已经依赖
/// `bounded`（见 `platform/mod.rs` 分层说明），会形成反向依赖。
#[cfg(windows)]
fn decode_console(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::new();
    }
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.to_string();
    }
    match decode_codepage(bytes, console_code_page()) {
        Some(s) => s,
        None => String::from_utf8_lossy(bytes).into_owned(),
    }
}

/// 当前控制台码页：**问操作系统**，不猜语言。
///
/// GUI 子系统没有控制台时 `GetConsoleOutputCP` 返回 0，回退系统 OEM 码页。
#[cfg(windows)]
fn console_code_page() -> u32 {
    extern "system" {
        fn GetConsoleOutputCP() -> u32;
        fn GetOEMCP() -> u32;
    }
    let c = unsafe { GetConsoleOutputCP() };
    if c == 0 {
        unsafe { GetOEMCP() }
    } else {
        c
    }
}

/// 按**指定码页**做 MBCS→UTF-16；转不动返回 `None`，由调用方 lossy 保底（不丢字节）。
///
/// 为什么允许显式传码页：生产路径传的是 `console_code_page()`，而「GBK 现场」的回归用例必须在
///   **任意语言的 runner** 上确定性复现 —— 绑在 runner 的码页上，门禁就跟着机器抖（英文 runner
///   的 OEM 码页是 437，同一批字节解出来是另一副样子）。
#[cfg(windows)]
fn decode_codepage(bytes: &[u8], cp: u32) -> Option<String> {
    extern "system" {
        fn MultiByteToWideChar(
            code_page: u32,
            flags: u32,
            multi_byte_str: *const i8,
            multi_byte_len: i32,
            wide_char_str: *mut u16,
            wide_char_len: i32,
        ) -> i32;
    }
    let len = bytes.len().min(i32::MAX as usize) as i32;
    let src = bytes.as_ptr() as *const i8;
    // 第一次调用传空目的缓冲，取所需 UTF-16 字数。
    let need = unsafe { MultiByteToWideChar(cp, 0, src, len, std::ptr::null_mut(), 0) };
    if need <= 0 {
        return None;
    }
    let mut wide = vec![0u16; need as usize];
    let got = unsafe { MultiByteToWideChar(cp, 0, src, len, wide.as_mut_ptr(), need) };
    if got <= 0 {
        return None;
    }
    Some(String::from_utf16_lossy(&wide[..got as usize]))
}

/// POSIX：控制台输出即 UTF-8，lossy 保底（与历史行为一致，不引入新失败模式）。
#[cfg(not(windows))]
fn decode_console(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn cleanup(a: &std::path::Path, b: &std::path::Path) {
    let _ = std::fs::remove_file(a);
    let _ = std::fs::remove_file(b);
}

fn tail(s: &str, n: usize) -> String {
    let t = s.trim();
    if t.chars().count() <= n {
        return t.to_string();
    }
    let skip = t.chars().count() - n;
    t.chars().skip(skip).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 本模块的测试**串行执行**（2026-09-13）。
    /// 默认线程池会并发跑它们，而 A-4 需要数 temp 目录里的 dsh-cmd-*.log：
    /// 并发用例的创建/清理会让计数抖动 → 断言假红。锁把本模块串起来即可消除。
    static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn bounded_run_success() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let mut c = Command::new(if cfg!(windows) { "cmd" } else { "echo" });
        if cfg!(windows) {
            c.args(["/C", "echo hello"]);
        } else {
            c.arg("hello");
        }
        let out = run(&mut c, Duration::from_secs(5)).expect("run");
        assert!(out.success, "stderr={}", out.stderr);
        assert!(out.stdout.contains("hello"), "stdout={}", out.stdout);
    }

    #[test]
    fn bounded_run_reports_timeout_as_evidence_not_error() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        // 睡眠远超上限：必须被 kill，并且以**执行记录**的形式说明「超时」这件事
        //   （原实现把超时压成 Err 字符串，输出与退出状态一并丢失 —— 见 run() 的说明）。
        let mut c = Command::new(if cfg!(windows) { "cmd" } else { "sleep" });
        if cfg!(windows) {
            c.args(["/C", "ping -n 30 127.0.0.1 >NUL"]);
        } else {
            c.arg("30");
        }
        let started = Instant::now();
        let r = run(&mut c, Duration::from_millis(600)).expect("超时应返回记录，而非 Err");
        let el = started.elapsed();
        assert!(r.timed_out, "timed_out 未置位: {:?}", r.code);
        assert!(!r.success, "超时不得被判为成功");
        assert_eq!(r.code, None, "被终止不应有退出码");
        assert!(r.code_label().starts_with("超时被终止"), "措辞: {}", r.code_label());
        assert!(!r.command_line().is_empty(), "记录必须含命令原文");
        assert!(el < Duration::from_secs(10), "耗时过长: {:?}", el);
    }

    #[test]
    fn exec_record_failure_renders_command_and_code_once() {
        // 这是「四处各拼一遍退出码文案」的收口断言：形状必须在**这一处**成立。
        let r = ExecRecord {
            program: "schtasks".into(),
            args: vec!["/Run".into(), "/TN".into(), "DSH-Supervisor".into()],
            timed_out: false,
            timeout_secs: 10,
            code: Some(1),
            success: false,
            stdout: String::new(),
            stderr: "  系统找不到指定的文件。  ".into(),
        };
        let msg = r.failure("服务管理器启动");
        assert!(msg.contains("服务管理器启动 失败"), "{}", msg);
        assert!(msg.contains("退出码 1"), "{}", msg);
        assert!(msg.contains("schtasks /Run /TN DSH-Supervisor"), "缺命令原文: {}", msg);
        // 正文两侧空白必须归一（否则诊断串里出现双空格/换行尾巴）。
        assert!(msg.ends_with("系统找不到指定的文件。"), "{}", msg);
        // stderr 为空才回退 stdout —— 两路都不丢。
        let fallback = ExecRecord { stdout: "only-stdout".into(), stderr: String::new(), ..r };
        assert_eq!(fallback.detail(), "only-stdout");
    }

    #[test]
    fn decode_console_preserves_utf8_and_empty() {
        // 三平台共同契约：UTF-8（node/npm 的输出）必须逐字节等价，不得被二次转换弄脏。
        assert_eq!(decode_console(b""), "");
        assert_eq!(decode_console("内核已对齐 v0.1.5".as_bytes()), "内核已对齐 v0.1.5");
        assert_eq!(decode_console(b"plain ascii"), "plain ascii");
    }

    /// 中文 Windows 现场回归：GBK 字节必须解出**可读的中文**。
    ///
    /// 这段字节就是 `schtasks /Run` 对不存在的任务所写的原文（cp936）。
    /// 旧的 `from_utf8_lossy` 把它变成一串 U+FFFD，正是用户报来的乱码。
    /// 显式传 936 而不是走 `decode_console`：runner 的控制台码页由机器决定（英文 runner 是 437，
    ///   2026-09-22 CI 轮2 实测同一批字节被解成 mojibake），把判据绑在机器码页上等于跟着机器抖。
    ///   「生产路径会问操作系统要码页」由 B62 的形态门禁锁住。
    #[cfg(windows)]
    #[test]
    fn decode_console_reads_gbk_console_output() {
        const GBK: &[u8] = &[
            0xCF, 0xB5, 0xCD, 0xB3, 0xD5, 0xD2, 0xB2, 0xBB, 0xB5, 0xBD, 0xD6, 0xB8, 0xB6, 0xA8,
            0xB5, 0xC4, 0xCE, 0xC4, 0xBC, 0xFE, 0xA1, 0xA3,
        ];
        assert_eq!(decode_codepage(GBK, 936).as_deref(), Some("系统找不到指定的文件。"));
        // 反向钉住旧缺陷：lossy 确实会毁掉这句话（说明本测试不是空转）。
        assert!(!String::from_utf8_lossy(GBK).contains("系统"));
    }

    /// 运行期心跳：子进程还在跑时必须能周期性拿到现场（时长 + 已产出行数）。
    ///
    /// 这是「装内核静默 15 分钟」的机制层回归钉：`run`（无心跳）保持不变，
    ///   心跳只由 `run_watch` 提供，且它读的就是 `run` 自己写的那两份临时输出。
    #[test]
    fn run_watch_emits_heartbeats_while_the_child_runs() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let mut c = Command::new(if cfg!(windows) { "cmd" } else { "sh" });
        if cfg!(windows) {
            c.args(["/C", "echo started & ping -n 6 127.0.0.1 >NUL"]);
        } else {
            c.args(["-c", "echo started; sleep 3"]);
        }
        let beats: std::sync::Mutex<Vec<(u64, usize, String)>> = std::sync::Mutex::new(vec![]);
        let r = run_watch(
            &mut c,
            Duration::from_secs(30),
            Duration::from_millis(150),
            &|l: &Live| {
                beats.lock().unwrap_or_else(|e| e.into_inner())
                    .push((l.elapsed.as_millis() as u64, l.lines, l.last_line.clone()));
            },
        )
        .expect("run_watch");
        assert!(r.success, "stderr={}", r.stderr);
        let b = beats.lock().unwrap_or_else(|e| e.into_inner()).clone();
        assert!(b.len() >= 2, "子进程运行期间没有持续心跳：{:?}", b);
        // 心跳必须**前进**（否则「已用 Ns」会一直卡在同一个数，等于没有进度）。
        assert!(b.last().unwrap().0 > b[0].0, "心跳时长未前进：{:?}", b);
        // 行数与末行来自子进程真实输出：npm 的进度正是这样落在 stderr/stdout 里的。
        assert!(b.last().unwrap().1 >= 1, "未统计到子进程输出行：{:?}", b);
        assert!(b.last().unwrap().2.contains("started"), "末行不是真实输出：{:?}", b);
    }

    #[test]
    fn missing_binary_is_error_not_panic() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let mut c = Command::new("dsh-no-such-binary-xyz");
        assert!(run(&mut c, Duration::from_secs(2)).is_err());
    }

    /// A-4 门禁：失败路径不得在 temp 目录留下 dsh-cmd-*.log 残渣（2026-09-13）。
    ///
/// 用 spawn 失败这一可复现路径验证「失败不留临时日志」。
    /// 注入：把 spawn 的 match 改回 `cmd.spawn().map_err(...)?` → 本测试 FAIL。
    #[test]
    fn a4_spawn_failure_leaves_no_temp_logs() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir();
        let before = count_dsh_cmd_logs(&dir);
        let mut c = Command::new("dsh-no-such-binary-xyz");
        let r = run(&mut c, Duration::from_secs(2));
        assert!(r.is_err(), "前置：不存在的命令应失败");
        let after = count_dsh_cmd_logs(&dir);

        // 并发测试可能同时创建/清理，故断言「不增长」而非「精确相等」。
        //   本测试的 pid 唯一，自己的那对必然被清掉，不会制造 +N。
        assert!(
            after <= before,
            "A-4 FAIL spawn 失败后 temp 日志未清理（之前 {} 个，之后 {} 个）",
            before,
            after
        );
    }

    fn count_dsh_cmd_logs(dir: &std::path::Path) -> usize {
        std::fs::read_dir(dir)
            .map(|rd| {
                rd.flatten()
                    .filter(|e| {
                        let n = e.file_name().to_string_lossy().to_string();
                        n.starts_with("dsh-cmd-out-") || n.starts_with("dsh-cmd-err-")
                    })
                    .count()
            })
            .unwrap_or(0)
    }
}
