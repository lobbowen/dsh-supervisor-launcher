// 20-env —— 9 个函数（拆分自 bootstrap.html，2026-09-11）。
// 共享状态与跨模块调用经 NS（window.__BOOT_NS）。
(function (NS) {
  // 工具链快照的**唯一写入点**，且只由 readEnv 调用：快照描述的是「最后一次真实探测读到了什么」，
  //   而不是「某个分支决定放行」—— 散装两字段时代每个分支各写一遍，漏写一处不报错，
  //   只让就绪文案少一半，并在重试时残留上一轮的值。
  function applyToolchain(st) {
    // 未完成的读取不覆盖已知事实：超时项没有事实，probing/busy 是中间态（T-2）。
    if (!st || st.__timeout || st.probing || st.busy) return;
    var t = NS.emptyToolchain();
    t.node = st.installed || null;
    t.npm = st.npmVersion || null;
    NS.toolchain = t;
  }

  // node_status 的**唯一读取口**：查询超时预算与快照写入都只在这里发生一次。
  //   三个轮询点各写一遍 15000/超时文案，改一处就会漏两处（快照与探测脱节即由此而来）。
  function readEnv() {
    return NS.withTimeout(NS.core.invoke('node_status'), 15000, '环境查询无响应').then(function (st) {
      var o = st || {};
      applyToolchain(o);
      return o;
    });
  }

  // 就绪行：Node 与 npm **同形并列** —— 工具链两半都是必需项，只报一半就是「npm 隐身」。
  //   npm 版本号缺失时如实说「未回读」，绝不拿 Node 的版本号顶替。
  function toolchainLine() {
    var t = NS.toolchain || NS.emptyToolchain();
    return '环境就绪 · Node ' + (NS.versionLabel(t.node) || '版本未回读') +
      ' · npm ' + (NS.versionLabel(t.npm) || '版本未回读');
  }

  function stepEnv() {
    NS.setStep(0);
    NS.phase('env');
    NS.status('正在检测系统环境…');
    var deadline = Date.now() + NS.ENV_PROBE_BUDGET_MS;
    return new Promise(function (resolve) {
      var settled = false;
      function poll() {
        if (settled) return;
        // 必须给**每次**查询包一层超时（2026-09-11 二次修复）。
        //   原实现是裸 `core.invoke(...).then(...)`：一旦该 invoke 永不 settle，
        //   `poll()` 就**再也不会被调度** —— 既不报错、也不推进，
        //   界面永久停在「正在检测系统环境…」。
        //   这正是用户实测到的「卡住且不报错」：不是探测慢，而是轮询自己停摆了。
        //   （轮询循环与单次调用不同：它**必须**有独立于被调方的心跳。）
        NS.readEnv().then(function (st) {
          if (settled) return;
          if (st.__timeout) {
            // 单次查询无响应：不就此放弃，继续轮询到总预算耗尽再给出口。
            if (Date.now() >= deadline) { settled = true; NS.failEnvTimeout(NS.lastEnv || {}); resolve(null); return; }
            NS.status('正在检测系统环境…（查询无响应，重试中）');
            setTimeout(poll, 400);
            return;
          }
          NS.lastEnv = st;
          // Rust 侧硬上限触发的**明确失败**：立即给出可操作结论，
          // 不等前端预算耗尽（那只会得到一句没有信息量的「超时」）。
          if (st.probeError) {
            settled = true;
            NS.envStuck = st.stuck || null;
            NS.fail('环境探测失败：' + st.probeError);
            NS.$('btnForceNode').style.display = '';
            resolve(null);
            return;
          }
          if (st.probing) {
            var s = st.stuck;
            if (s && s.on) {
              NS.envStuck = s;
              NS.status('正在检测系统环境…（' + s.on + ' · 已 ' + Math.round((s.ms || 0) / 1000) + 's 无响应）');
            }
            if (Date.now() >= deadline) {
              settled = true;
              NS.failEnvTimeout(st);
              resolve(null);
              return;
            }
            setTimeout(poll, 400);
            return;
          }
          settled = true;
          var p = NS.afterEnv(st);
          if (p && p.then) { p.then(function () { resolve(null); }, function () { resolve(null); }); } else { resolve(null); }
        }).catch(function (e) {
          if (settled) return;
          settled = true;
          NS.fail('环境检测异常：' + NS.errText(e));
          resolve(null);
        });
      }
      poll();
    });
  }

  function failEnvTimeout(st) {
    NS.envStuck = (st && st.stuck) || null;
    var extra = (NS.envStuck && NS.envStuck.on)
      ? ' · 卡在：' + NS.envStuck.on + '（已 ' + Math.round((NS.envStuck.ms || 0) / 1000) + 's 无响应）'
      : '';
    NS.fail('环境检测超时（探针无响应，可能有异常的可执行文件占位）' + extra);
    // 给出「跳过检测直接安装」出口：这是**唯一**能让用户自救的路径
    // （下载 Node 不需要本机已有 Node）。
    NS.$('btnForceNode').style.display = '';
  }

  function afterEnv(st) {
    if (st.busy) { NS.setStep(0); NS.status(st.status || '正在准备 Node.js 运行环境…'); return NS.stepNodeWait(); }
    if (!st.installed) {
      NS.setStep(0);
      return NS.probeMirrorThen(function () {
        // 安装文案统一走 NS.install（SSOT §3.2 / 不变量 T-6）：本模块只决定「装什么、说什么」，
        //   样式与进度形态由 10-ui.js 的唯一实现负责，避免再次分裂成各阶段各画各的。
        NS.install.begin('node', '未检测到 Node.js · 正在补全运行环境…');
        return NS.core.invoke('start_node_install').then(function () { return NS.stepNodeWait('node'); });
      });
    }
    // 必须校验**最低门槛**：后端一直回传 minOk（DSH 要求 Node >= v22.12），
    //   而前端曾长期忽略它 —— 装了旧版 Node 也照常放行，直到内核启动才失败。
    if (st.minOk === false) {
      NS.setStep(0);
      return NS.probeMirrorThen(function () {
        NS.install.begin('node', 'Node.js ' + st.installed + ' 低于最低要求（' + (st.minRequired || 'v22.12') + '）· 正在升级…');
        return NS.core.invoke('start_node_install').then(function () { return NS.stepNodeWait('node'); });
      });
    }
    // npm 分支（SSOT §3.1）：npm 与 node **并行同权**，且**独立于**上面的 node 分支 ——
    //   不得与 !installed / minOk === false 共用文案或注释。为什么要分开：两者是不同缺失项，
    //   用户必须能一眼分辨是「没有 Node」还是「有 Node 但缺 npm」；共用文案会把两个根因
    //   混成一句无从下手的话。后端 run_install 已在同一条管线里装 node 并修复 npm（SSOT §2.2），
    //   故这里触发同一次安装调用；npmOk 由 node_status 真实探测回传（不变量 T-1）。
    // 三态：null=未知（探测没取到 node 路径），此时同样**不得放行** ——
    //   只有 npmOk === true（npm 真实执行通过）才算环境就绪（不变量 T-1b）。
    if (st.npmOk !== true) {
      NS.setStep(0);
      return NS.probeMirrorThen(function () {
        // 文案必须自带 npm 字样（SSOT 门禁 G-5）：只说「补全环境」会让 npm 缺失再次被掩盖。
        NS.install.begin('npm', '检测到缺少/不可用的 npm · 正在补全工具链…');
        return NS.core.invoke('start_node_install').then(function () { return NS.stepNodeWait('npm'); });
      });
    }
    // 工具链快照已由 readEnv 在读取那一刻写好（本函数不再自行登记版本字段）。
    return NS.stepNodeDone();
  }

  // kind = 本次触发安装的缺失项（'node' | 'npm'），仅用于**失败文案前缀**：
  //   同一段等待逻辑要能如实说出是「Node 没补上」还是「npm 没补上」，否则用户无法判断该重试什么。
  function stepNodeWait(kind) {
    NS.phase('node');
    return new Promise(function (resolve) {
      var done = false;
      // 同样包超时：等待安装完成期间也不能因单次查询无响应而永久静默。
      var t = setInterval(function () {
        NS.readEnv().then(function (st) {
          if (st.__timeout) return;   // 下次 tick 重试
          // 失败前置检查（SSOT §3.1 不变量 T-5）：安装器报错后它不再 busy，若只看 busy 会一路
          //   轮询到兜底超时并被当作成功、直奔内核步骤 —— 而 npm 仍缺失，装内核必失败。
          if (!st.busy && st.error) { if (!done) { done = true; clearInterval(t); resolve(failOnMissingNpm(kind, st.error)); } return; }
          // 就绪 = node **且** npm **且**达门槛（npm 缺失时安装器可能先出 node，必须继续等）。
          if (!st.busy && st.installed && st.minOk !== false && st.npmOk === true) { if (!done) { done = true; clearInterval(t); resolve(NS.stepNodeDone()); } }
        }).catch(function () {});
      }, 700);
      // 兜底：Node 安装可能长达数分钟。此处**绝不**默认成功（SSOT §3.1 不变量 T-5）——
      //   旧实现在此直接 stepNodeDone()，于是「npm 没补上」也会进入内核步骤，用不存在的 npm 去装内核。
      //   改为最后一次查询确认缺失项仍缺即如实失败，仅当节点确实已就绪才放行。
      setTimeout(function () {
        if (done) return;
        clearInterval(t);
        NS.readEnv().then(function (st) {
          if (!st.busy && st.installed && st.minOk !== false && st.npmOk === true) { done = true; resolve(NS.stepNodeDone()); return; }
          if (!done) { done = true; resolve(failOnMissingNpm(kind, st.error || '安装超时未完成')); }
        }).catch(function () { if (!done) { done = true; resolve(failOnMissingNpm(kind, '安装超时未完成')); } });
      }, 600000);
    });
  }

  // 安装失败/超时且 node 或 npm 仍缺失：**唯一**出口是既有失败面板（NS.fail），
  //   返回 null 给调用链，确保**不进内核步骤**（SSOT §3.1 不变量 T-5：不得用不存在的 npm 装内核）。
  //   为什么按 kind 分辨文案：补 npm 失败与补 node 失败的可操作结论不同，笼统一句「安装失败」会让用户无从下手。
  function failOnMissingNpm(kind, error) {
    var target = kind === 'npm' ? 'npm' : 'Node.js';
    NS.fail('运行环境安装失败：' + target + ' 未能就绪' + (error ? '（' + error + '）' : '') + ' · 未进入内核安装，请重试或手动安装 Node 官方分发包');
    return null;
  }

  function stepNodeDone() {
    NS.setStep(0);
    NS.status(toolchainLine());
    // 环境就绪后**才**进入桌面版本（网络步骤，带超时与跳过出口）
    return NS.wait(350).then(NS.stepShellUpdate);
  }

  // ── 导出到 NS（跨模块可调用）──
  NS.readEnv = readEnv;
  NS.stepEnv = stepEnv;
  NS.failEnvTimeout = failEnvTimeout;
  NS.afterEnv = afterEnv;
  NS.stepNodeWait = stepNodeWait;
  NS.failOnMissingNpm = failOnMissingNpm;
  NS.stepNodeDone = stepNodeDone;
})(window.__BOOT_NS);
