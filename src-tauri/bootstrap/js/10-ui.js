// 10-ui —— 13 个函数（拆分自 bootstrap.html，2026-09-11）。
// 共享状态与跨模块调用经 NS（window.__BOOT_NS）。
(function (NS) {
  function setStep(i, detail) {
    if (i > NS.cur) NS.cur = i;
    NS.stepNames.forEach(function (id, idx) {
      var el = document.getElementById(id);
      if (!el) return;
      var cls = 'step';
      if (idx < NS.cur) cls += ' done';
      else if (idx === NS.cur) cls += ' active';
      el.className = cls;
      if (detail && idx === NS.cur) {
        var d = el.querySelector('.step-detail');
        if (d) d.textContent = detail;
      }
    });
    NS.$('steps').style.display = '';
  }

  function status(t) { NS.$('status').textContent = t; }
  function wait(ms) { return new Promise(function (r) { setTimeout(r, ms); }); }
  function hideFail() { NS.$('fail').style.display = 'none'; }

  // ── 安装/下载的统一文字出口（SSOT §3.2，唯一实现）──
  // 为什么集中在这里：历史上 node / npm / 内核 / 桌面壳各画各的（进度条与纯文字并存），
  //   同一件事在不同阶段长得不一样；而且进度条常与真实进度脱节，反而让人误判「卡死」。
  //   现在一律只改 status 单行文字与步骤条高亮，**不触碰任何进度条元素**（门禁 G-3/T-7）。
  var INSTALL_TARGET = { node: 'Node.js', npm: 'npm', kernel: '内核', shell: '桌面版本' };
  // kind 是枚举（SSOT §2.4）：它只决定文案目标与步骤条落点，**不改变任何样式**。
  var INSTALL_STEP = { node: 0, npm: 0, shell: 1, kernel: 2 };
  var INSTALL_FAIL_PREFIX = {
    node: '运行环境安装失败：', npm: '运行环境安装失败：',
    kernel: '内核安装失败：', shell: '桌面版本更新失败：'
  };

  function installTarget(kind) { return INSTALL_TARGET[kind] || '组件'; }

  // 版本号形态归一（唯一实现）：Node 契约自带 v，npm 与内核/桌面壳的版本号都不带。
  //   形态规则一旦散落到各播报点，同一行里就会出现两种写法，且每处都可能写错。
  //   未探测到版本时返回 ''，由调用方如实说明（绝不拿别的组件的版本顶替）。
  function versionLabel(v) {
    var s = (v == null ? '' : String(v)).trim();
    if (!s) return '';
    return /^\d/.test(s) ? 'v' + s : s;
  }

  function installBegin(kind, text) {
    var at = INSTALL_STEP[kind];
    // 只前进、不回退：守卫对齐等场景会在更靠后的阶段调用，回退步骤条会误导进度。
    if (at != null && at > NS.cur) setStep(at);
    status(text || ('正在下载 ' + installTarget(kind) + ' …'));
  }

  function installText(kind, text) { if (text) status(text); }

  function installDone(kind, text) {
    // 完成文案完全同形：<目标> <版本> 已就绪（SSOT §3.2）；形态归一只经 versionLabel 这一处。
    var v = versionLabel(text);
    status(installTarget(kind) + (v ? ' ' + v : '') + ' 已就绪');
  }

  function installFail(kind, text) {
    // 失败必须走既有 fail 面板（含镜像自助出口的自动展开），安装层不另开样式、不吞错。
    fail((INSTALL_FAIL_PREFIX[kind] || '安装失败：') + (text || '未知'));
  }


  function withTimeout(promise, ms, onTimeoutMsg) {
    return new Promise(function (resolve) {
      var done = false;
      var t = setTimeout(function () {
        if (done) return; done = true;
        resolve({ __timeout: true, error: onTimeoutMsg || '操作超时（无响应）' });
      }, ms);
      promise.then(function (v) {
        if (done) return; done = true; clearTimeout(t); resolve(v);
      }, function (e) {
        if (done) return; done = true; clearTimeout(t); resolve({ __error: String(e) });
      });
    });
  }


  // 阶段上报：写入 ~/.dsh/shell/identity.json + shell.log，供内核观察与问题定位。

  function phase(p) {
    try { if (NS.core) NS.core.invoke('shell_set_phase', { phase: p }).catch(function () {}); } catch (e) {}
  }

  function errText(e) {
    if (e === null || e === undefined) return '未知错误';
    if (typeof e === 'string') return e;
    if (e.kind) {
      var detail = e.cause || e.stage || e.kind;
      return e.hint ? (detail + '；' + e.hint) : detail;
    }
    return String(e.message || e);
  }

  function fail(msg) {
    NS.lastError = msg;
    NS.status('启动未完成');
    NS.$('failMsg').textContent = msg;
    NS.$('fail').style.display = '';
    // 网络/镜像类失败 → **自动展开**镜像设置，让用户一眼看到自助出口；
    // 其余失败（如内核安装报错）不展开，避免噪声。
    // 注意：**不含「超时」** —— 环境检测超时并非网络问题，展开镜像设置会误导用户。
    if (/网络|镜像|下载|不可达|network|mirror/i.test(String(msg))) {
      NS.showMirror();
    }
  }

  function diagText() {
    return [
      'node=' + (NS.toolchain && NS.toolchain.node ? versionLabel(NS.toolchain.node) : 'unknown'),
      // npm 与 node 并列：诊断串只报 node，「npm 探到了没有」就又只能靠读代码猜。
      'npm=' + (NS.toolchain && NS.toolchain.npm ? versionLabel(NS.toolchain.npm) : 'unknown'),
      'core=' + (NS.coreVersion || 'none'),
      'plan=' + (NS.lastPlan ? JSON.stringify(NS.lastPlan) : 'none'),
      'shell=' + (NS.shellId ? (NS.shellId.version + '/' + NS.shellId.installKind + '/capable=' + NS.shellId.selfUpdateCapable) : 'unknown'),
      'shell_update=' + (NS.updPlan ? JSON.stringify({ available: NS.updPlan.available, latest: NS.updPlan.latest, skipped: NS.updPlan.skipped, error: NS.updPlan.error }) : 'none'),
      // ── 环境探测追踪（架构修复 2026-09-11）：探测根因是**环境特有**的，
      //    靠读代码无法确定；这份追踪是定位该类问题唯一可靠的手段。 ──
      'env_candidates=' + ((NS.lastEnv && NS.lastEnv.candidates) || 'none'),
      'env_probe_error=' + ((NS.lastEnv && NS.lastEnv.probeError) || 'none'),
      'env_stuck=' + (NS.envStuck ? (NS.envStuck.on + '/' + NS.envStuck.ms + 'ms') : 'none'),
      'env_trace=' + ((NS.lastEnv && NS.lastEnv.trace && NS.lastEnv.trace.length) ? NS.lastEnv.trace.map(function (t) { return t.path + '(' + t.ms + 'ms,' + (t.ok ? 'ok' : 'no') + ',' + (t.note || '') + ')'; }).join(' > ') : 'none'),
      // 镜像信息**必须始终有值**（2026-09-11 修复）：此前只在「需要下载 Node」时才有，
      // 于是 Node 达标的用户诊断串永远是 mirror=none —— 让人合理地怀疑镜像能力不存在。
      // 现从预热缓存读（与是否需要下载解耦），并在尚未就绪时明确说明「预热中」。
      'mirror=' + (
        (NS.warmMirror && NS.warmMirror.npmBest)
          ? (String(NS.warmMirror.npmBest).replace(/\/+$/, '') + '/' + NS.warmMirror.npmLatencyMs + 'ms')
          : (NS.lastMirror && NS.lastMirror.mirror
              ? (NS.lastMirror.mirror + '/' + NS.lastMirror.latencyMs + 'ms')
              : (NS.warmTimer ? 'warming（预热中）' : 'none（预热未启动或全部不可达）'))),
      'mirror_node_best=' + ((NS.warmMirror && NS.warmMirror.nodeBest) || 'none'),
      'mirror_npm_best=' + ((NS.warmMirror && NS.warmMirror.npmBest) || 'none'),
      'mirror_probes=' + (
        (NS.warmMirror && NS.warmMirror.npmProbes && NS.warmMirror.npmProbes.length)
          ? NS.warmMirror.npmProbes.map(function (p) { return p.source.replace(/^https?:\/\//, '') + (p.ok ? '(' + p.latencyMs + 'ms)' : '(x)'); }).join(' > ')
          : ((NS.lastMirror && NS.lastMirror.probes && NS.lastMirror.probes.length)
              ? NS.lastMirror.probes.map(function (p) { return p.source + (p.ok ? '(' + p.latencyMs + 'ms)' : '(x)'); }).join(' > ')
              : 'none')),
      'error=' + (NS.lastError || 'none'),
    ].join(' | ');
  }

  // ── 导出到 NS（跨模块可调用）──
  NS.setStep = setStep;
  NS.status = status;
  NS.versionLabel = versionLabel;
  NS.wait = wait;
  NS.hideFail = hideFail;
  NS.install = { begin: installBegin, text: installText, done: installDone, fail: installFail };
  NS.withTimeout = withTimeout;
  NS.phase = phase;
  NS.errText = errText;
  NS.fail = fail;
  NS.diagText = diagText;
})(window.__BOOT_NS);
