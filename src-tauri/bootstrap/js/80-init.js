// 80-init —— 按钮绑定、事件监听与启动（拆分自 bootstrap.html，2026-09-11）。
//
// 必须**最后加载**：它依赖前面所有模块已挂到 NS 上。
(function (NS) {
  NS.$('btnUpdRetry').addEventListener('click', function () {
    NS.hideUpdChoice();
    NS.status('正在重试桌面版本更新…');
    NS.stepShellApply().catch(function (e) { NS.showUpdChoice('重试异常：' + NS.errText(e)); });
  });
  NS.$('btnMirror').addEventListener('click', function () {
    var shown = NS.$('mirrorBox').style.display !== 'none';
    if (shown) { NS.$('mirrorBox').style.display = 'none'; } else { NS.showMirror(); }
  });
  NS.$('btnMirrorProbe').addEventListener('click', function () { NS.loadMirror(); });
  NS.$('btnMirrorSave').addEventListener('click', function () {
    var split = function (v) {
      return String(v || '').split(/[,\n]/).map(function (x) { return x.trim(); }).filter(Boolean);
    };
    var node = split(NS.$('mirrorNode').value);
    var npm = split(NS.$('mirrorNpm').value);
    if (!node.length && !npm.length) { NS.$('mirrorHint').textContent = '请至少填写一项镜像地址'; return; }
    NS.$('mirrorHint').textContent = '正在保存…';
    var jobs = [];
    if (node.length) jobs.push(NS.core.invoke('mirror_set', { kind: 'node', urls: node }));
    if (npm.length) jobs.push(NS.core.invoke('mirror_set', { kind: 'npm', urls: npm }));
    Promise.all(jobs).then(function () {
      NS.$('mirrorHint').textContent = '已保存，正在重新开始引导…';
      return NS.wait(500);
    }).then(function () {
      NS.hideFail();
      NS.$('mirrorBox').style.display = 'none';
      NS.boot();
    }).catch(function (e) {
      NS.$('mirrorHint').textContent = '保存失败：' + e;
    });
  });
  NS.$('btnRetry').addEventListener('click', function () {
    NS.$('btnForceNode').style.display = 'none';
    NS.boot();
  });
  NS.$('btnForceNode').addEventListener('click', function () {
    NS.hideFail();
    NS.$('btnForceNode').style.display = 'none';
    NS.$('mirrorBox').style.display = 'none';
    NS.setStep(0);
    NS.phase('node');
    NS.status('已跳过环境检测 · 正在准备安装 Node.js…');
    NS.probeMirrorThen(function () {
      return NS.core.invoke('start_node_install').then(function () { return NS.stepNodeWait(); });
    });
  });
  NS.$('btnDiag').addEventListener('click', function () {
    var t = NS.diagText();
    try { if (navigator.clipboard) navigator.clipboard.writeText(t); } catch (e) {}
    NS.status('诊断信息已复制：' + t);
  });
  if (NS.evt) {
    NS.evt.listen('guard_progress', function (e) {
      var p = e.payload || {};
      if (p.status) NS.status(p.status);
    });
  }
  // 安装/下载事件（SSOT §2.4 / §3.3）：node / npm / kernel / shell 在**三平台同一形态**，
  //   且本文件是**唯一**消费入口。为什么必须收敛到这里：过去环境与桌面壳各有一套旧进度事件，
  //   各自拼文案、各自决定是否画条，导致同一件「下载安装」在不同阶段面貌不一、进度条还会与真实进度脱节。
  //   现在一律转交 NS.install.*，本处不拼任何下载/安装文案，也不触碰进度条。
  //   旧事件名已按规范删除，无兼容层（门禁 G-4）。
  if (NS.evt) {
    NS.evt.listen('install_progress', function (e) {
      var p = e.payload || {};
      NS.install.text(p.kind, p.status);
    });
    NS.evt.listen('install_done', function (e) {
      var p = e.payload || {};
      NS.install.done(p.kind, p.version);
    });
    NS.evt.listen('install_error', function (e) {
      var p = (e && e.payload) || {};
      NS.install.fail(p.kind, p.error || '未知');
    });
  }
  NS.boot();
})(window.__BOOT_NS);
