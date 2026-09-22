#!/usr/bin/env node
// 注释纪律门禁 CS-1 到 CS-6。规范出处：docs/DEVELOPMENT-TRACK.md 第 4 节。
//
// 契约字面量（被其它门禁 grep 的注释原文，批量改写前先 grep 全部引用点）：
// 见 CS_CONTRACT_LITERALS，由 CS-7 反向校验它们仍在 src 内存在。
'use strict';
const fs = require('fs');
const path = require('path');

const ROOT = path.resolve(__dirname, '..');
const CS_TYPOGRAPHIC = '‘’“”–—…';

function allowed(ch) {
  const p = ch.codePointAt(0);
  if (p === 9 || p === 10 || p === 13 || (p >= 0x20 && p <= 0x7e)) return true;
  if (p >= 0x3000 && p <= 0x303f) return true;
  if (p >= 0x3040 && p <= 0x30ff) return true;
  if (p >= 0x4e00 && p <= 0x9fff) return true;
  if (p >= 0xff00 && p <= 0xffef) return true;
  return CS_TYPOGRAPHIC.indexOf(ch) >= 0;
}

const CS_NARRATIVE = [
  [/第\s*\d+\s*批/g, '批次号'],
  [/批\s*\d+/g, '批次号'],
  [/run\s*`?\d{6,}/g, 'CI run 号'],
  [/AUDIT-\d{4}-\d{2}-\d{2}/g, '审计报告引用'],
  [/\u00a7/g, '章节交叉引用'],
  [/勘误/g, '勘误叙述'],
  [/全绿|复绿|CI\s*全红|假绿|判红/g, 'CI 红绿叙述'],
  [/20\d{2}-\d{2}(?!\d)(?:-\d{2}(?!\d))?/g, '日期戳'],
];

const CS_EXEMPT = [];

// 契约字面量：src-tauri/tests 里的判据用 contains()/find() 搜这些串，而它们目前**只存在于注释原文**。
// 压缩注释时不得删掉；删掉之前先把钉它们的判据重锚到代码符号。CS-7 反向校验它们仍在。
const CS_CONTRACT_LITERALS = [
  { lit: 'CORE_APPLY_BUDGET_MS', files: ['src-tauri/src/bridge.rs'] },
  { lit: 'cmd.output()', files: ['src-tauri/src/core.rs'] },
  { lit: 'struct BoundedOutput', files: ['src-tauri/src/core.rs'] },
  { lit: 'RunAtLoad', files: ['src-tauri/src/platform/macos.rs'] },
  { lit: 'probeMirrorThen', files: ['src-tauri/src/mirror.rs'] },
  { lit: 'npmOk !== true', files: ['src-tauri/bootstrap/js/20-env.js'] },
  { lit: 'Test-NetConnection', files: ['src-tauri/src/domain/cli.rs', 'src-tauri/src/platform/windows.rs'] },
  { lit: '唯一来源', files: ['src-tauri/src/commands/mod.rs'] },
  { lit: 'env_status', files: ['src-tauri/src/main.rs'] },
  { lit: 'minisign', files: ['src-tauri/src/commands/mod.rs', 'src-tauri/src/main.rs'] },
  { lit: '3100', files: ['src-tauri/src/domain/guardctl.rs', 'src-tauri/src/env.rs'] },
  // 排除镜像的记录没有代码路径可锚（被排除的东西不出现在预设里），只能由注释承载。
  { lit: '已排除', files: ['src-tauri/src/mirror.rs'] },
];

/** Rust：返回与 src 等长的掩码串（注释字符保留，其余置空格，换行保留）。 */
function maskRs(src) {
  const out = new Array(src.length).fill(' ');
  let i = 0, n = src.length;
  while (i < n) {
    const c = src[i], d = src[i + 1];
    if (c === '/' && d === '/') {
      while (i < n && src[i] !== '\n') { out[i] = src[i]; i++; }
      continue;
    }
    if (c === '/' && d === '*') {
      let depth = 0;
      while (i < n) {
        if (src[i] === '/' && src[i + 1] === '*') { depth++; out[i] = '/'; out[i + 1] = '*'; i += 2; continue; }
        if (src[i] === '*' && src[i + 1] === '/') { depth--; out[i] = '*'; out[i + 1] = '/'; i += 2; if (depth === 0) break; continue; }
        out[i] = src[i] === '\n' ? '\n' : (src[i] === ' ' ? ' ' : src[i]);
        i++;
      }
      continue;
    }
    if (c === 'r' && (/^r(#*)"/.test(src.slice(i)))) {
      const m = /^r(#*)"/.exec(src.slice(i)); const hashes = m[1];
      const end = '"' + hashes;
      i += m[0].length;
      const stop = src.indexOf(end, i);
      i = stop < 0 ? n : stop + end.length;
      continue;
    }
    if (c === '"') {
      i++;
      while (i < n) { if (src[i] === '\\') { i += 2; continue; } if (src[i] === '"') { i++; break; } if (src[i] === '\n') break; i++; }
      continue;
    }
    if (c === "'") {
      const close = src.indexOf("'", i + 1);
      if (close > i && close - i <= 4 && src[close - 1] !== '\\') i = close + 1; else i++;
      continue;
    }
    if (c === '\n') out[i] = '\n';
    i++;
  }
  return out.join('');
}

/** JS/TS：字符串、模板、正则按词法区分；注释字符保留。 */
function maskJs(src) {
  const out = new Array(src.length).fill(' ');
  let i = 0, n = src.length, prev = '';
  while (i < n) {
    const c = src[i], d = src[i + 1];
    if (c === '/' && d === '/') { while (i < n && src[i] !== '\n') { out[i] = src[i]; i++; } continue; }
    if (c === '/' && d === '*') {
      while (i < n) {
        if (src[i] === '*' && src[i + 1] === '/') { out[i] = '*'; out[i + 1] = '/'; i += 2; break; }
        out[i] = src[i]; i++;
      }
      continue;
    }
    if (c === '"' || c === "'" || c === '`') {
      const q = c; i++;
      while (i < n) { if (src[i] === '\\') { i += 2; continue; } if (src[i] === q) { i++; break; } i++; }
      prev = q; continue;
    }
    if (c === '/' && /[=(,:[!&|?{};\s]|^/.test(prev)) {
      i++; let cls = false;
      while (i < n) {
        if (src[i] === '\\') { i += 2; continue; }
        if (src[i] === '[') cls = true; else if (src[i] === ']') cls = false;
        else if (src[i] === '/' && !cls) { i++; break; }
        else if (src[i] === '\n') break;
        i++;
      }
      while (i < n && /[a-z]/.test(src[i])) i++;
      prev = '/'; continue;
    }
    if (c === '\n') out[i] = '\n';
    if (!/\s/.test(c)) prev = c;
    i++;
  }
  return out.join('');
}

/** HTML：只认 <!-- --> 块。 */
function maskHtml(src) {
  const out = new Array(src.length).fill(' ');
  let i = 0;
  while (i < src.length) {
    if (src.startsWith('<!--', i)) {
      const stop = src.indexOf('-->', i);
      const end = stop < 0 ? src.length : stop + 3;
      for (let k = i; k < end; k++) out[k] = src[k] === '\n' ? '\n' : (src[k] === ' ' ? ' ' : src[k]);
      i = end; continue;
    }
    if (src[i] === '\n') out[i] = '\n';
    i++;
  }
  return out.join('');
}

/** 整行注释的判定按语言分：Rust 的 `#[cfg(...)]` 是代码不是注释，`#` 只对 sh/py 生效。 */
function isCommentLead(rel, line) {
  if (/\.rs$/.test(rel)) return /^[ \t]*(?:\/\/|\/\*)/.test(line);
  if (/\.(?:js|cjs|mjs|ts|tsx)$/.test(rel)) return /^[ \t]*(?:\/\/|\/\*)/.test(line);
  if (/\.html$/.test(rel)) return /^[ \t]*<!--/.test(line);
  if (/\.(?:sh|ps1|py)$/.test(rel)) return /^[ \t]*#(?!!)/.test(line);
  return false;
}

/** SH：只认整行 `#`（shebang 除外）。 */
function maskSh(src) {
  return String(src).split('\n').map((l) => (/^[ \t]*#(?!!)/.test(l) ? l : l.replace(/[^\s]/g, ' '))).join('\n');
}

function maskFor(rel, src) {  if (/\.rs$/.test(rel)) return maskRs(src);
  if (/\.(?:js|cjs|mjs|ts|tsx)$/.test(rel)) return maskJs(src);
  if (/\.html$/.test(rel)) return maskHtml(src);
  if (/\.(?:sh|ps1|py)$/.test(rel)) return maskSh(src);
  return null;
}

function walk(dir, acc) {
  for (const name of fs.readdirSync(dir)) {
    if (['node_modules', 'target', 'dist', '.git'].includes(name)) continue;
    const p = path.join(dir, name);
    if (fs.statSync(p).isDirectory()) walk(p, acc);
    else if (maskFor('', name) !== null || /\.(rs|js|html|sh)$/.test(name)) acc.push(p);
  }
  return acc;
}

function scan(rel, src) {
  const masked = maskFor(rel, src);
  const res = { icons: [], narrative: [], blocks: [], ratio: null, commentLines: 0, codeLines: 0 };
  if (masked === null) return res;
  const lines = src.split('\n'), mlines = masked.split('\n');
  let run = 0, runStart = 0;
  for (let i = 0; i < lines.length; i++) {
    const ml = mlines[i] || '';
    const original = lines[i] || '';
    const commentChars = ml.replace(/[ \t]/g, '').length;
    const isFullLineComment = isCommentLead(rel, original);
    if (commentChars > 0) res.commentLines++;
    if (original.trim() !== '' && commentChars === 0) res.codeLines++;
    if (commentChars > 0) {
      for (const ch of ml) {
        if (ch === ' ' || ch === '\t' || ch === '\n') continue;
        if (!allowed(ch)) res.icons.push({ line: i + 1, ch, cp: ch.codePointAt(0).toString(16) });
      }
      for (const [re, label] of CS_NARRATIVE) {
        re.lastIndex = 0;
        let m;
        while ((m = re.exec(ml))) res.narrative.push({ line: i + 1, label, text: m[0] });
      }
    }
    if (isFullLineComment) { if (run === 0) runStart = i + 1; run++; }
    else { if (run > 4) res.blocks.push({ start: runStart, len: run }); run = 0; }
  }
  if (run > 4) res.blocks.push({ start: runStart, len: run });
  if (/\.rs$/.test(rel) && res.codeLines > 0 && res.commentLines > res.codeLines) {
    res.ratio = { commentLines: res.commentLines, codeLines: res.codeLines };
  }
  return res;
}

// --- CS-3 / CS-4：门禁自身完整性（合成样本，永远硬失败）---
function selfcheck() {
  const fails = [];
  const ck = (name, ok, note) => { if (!ok) fails.push(name + ' —— ' + note); };
  const dirty = '// 结论 -> 见 §6.1\n// 2026-09-13 那次 CI 全绿 ✅\nlet x = 1; // 第 4 批\n';
  const dm = maskFor('a.rs', dirty);
  const dIcons = [...dm].filter((c) => c !== ' ' && c !== '\n' && !allowed(c));
  ck('CS-3 注释内的图标必被抓到', dIcons.length >= 1, JSON.stringify(dIcons));
  const dNarr = CS_NARRATIVE.reduce((a, [re, label]) => { re.lastIndex = 0; const m = dm.match(re); return m ? a.concat(label) : a; }, []);
  ck('CS-3 叙事标记逐类被抓到', dNarr.includes('章节交叉引用') && dNarr.includes('日期戳') && dNarr.includes('CI 红绿叙述') && dNarr.includes('批次号'), dNarr.join(','));
  const clean = '// 端口由 ports.json 单点下发，改这里要同步 contract\nfn f() { let s = "https://registry.npmjs.org/-/ping"; }\n';
  const cm = maskFor('b.rs', clean);
  ck('CS-3 白名单注释零命中（防恒真）', [...cm].every((c) => c === ' ' || c === '\n' || allowed(c)), 'clean 样本');
  ck('CS-3 字符串内同形字符不误报', !cm.includes('registry.npmjs.org'), 'URL 在字符串字面量里，须被置空');
  const block = '/* 说明 -> 箭头在块注释内也算 */\nfn g() {}\n';
  ck('CS-3 块注释同样在覆盖面内', maskFor('c.rs', block).includes('->'), '块注释未被抽取');
  const sh = '#!/bin/sh\n# 整行注释 -> 违规\ntrue # 行尾注释不覆盖\n';
  const sm = maskSh(sh);
  ck('CS-3 shell 整行注释被抓到、shebang 不报', sm.split('\n')[1].includes('->') && !sm.split('\n')[0].includes('!'), '掩码结果不符');
  return fails;
}

function main() {
  const only = process.argv.slice(2).filter((a) => !a.startsWith('-'));
  const dirs = ['src-tauri/src', 'src-tauri/bootstrap', 'scripts', 'ci', 'shell-release'];
  let files = [];
  for (const d of dirs) { const abs = path.join(ROOT, d); if (fs.existsSync(abs)) walk(abs, files); }
  if (only.length) files = files.filter((f) => only.some((o) => path.relative(ROOT, f).includes(o)));
  let icons = [], narrative = [], blocks = [], ratios = [], commentLines = 0;
  for (const f of files) {
    const rel = path.relative(ROOT, f);
    if (CS_EXEMPT.some((e) => e.file === rel)) continue;
    const r = scan(rel, fs.readFileSync(f, 'utf8'));
    icons = icons.concat(r.icons.map((v) => ({ ...v, rel })));
    narrative = narrative.concat(r.narrative.map((v) => ({ ...v, rel })));
    blocks = blocks.concat(r.blocks.map((v) => ({ ...v, rel })));
    if (r.ratio) ratios.push({ rel, ...r.ratio });
    commentLines += r.commentLines;
  }
  const fails = selfcheck();
  if (!only.length) {
    if (files.length < 25) fails.push(`CS-4 覆盖面：扫描文件数 ${files.length} < 25（抽取失效？）`);
    if (commentLines < 400) fails.push(`CS-4 覆盖面：注释行数 ${commentLines} < 400（抽取失效？）`);
  }
  const lost = CS_CONTRACT_LITERALS.filter((e) => !e.files.some((f) => {
    const p = path.join(ROOT, f);
    return fs.existsSync(p) && fs.readFileSync(p, 'utf8').includes(e.lit);
  }));
  if (lost.length) fails.push(`CS-7 契约字面量已从注释原文消失（钉它的判据会失配）：` + lost.map((e) => `${e.lit} <- ${e.files.join(',')}`).join(' | '));
  if (icons.length) fails.push(`CS-1 注释含白名单外字符 ${icons.length} 处：` + icons.slice(0, 20).map((v) => `${v.rel}:${v.line} U+${v.cp}`).join(' | '));
  if (narrative.length) fails.push(`CS-2 注释含过程叙事 ${narrative.length} 处：` + narrative.slice(0, 20).map((v) => `${v.rel}:${v.line} ${v.label}(${v.text})`).join(' | '));
  if (blocks.length) fails.push(`CS-5 连续注释块 >4 行 共 ${blocks.length} 块：` + blocks.slice(0, 20).map((v) => `${v.rel}:${v.start}(${v.len} 行)`).join(' | '));
  if (ratios.length) fails.push(`CS-6 注释行数超过代码行数的文件 ${ratios.length} 个：` + ratios.map((v) => `${v.rel} 注释${v.commentLines}/代码${v.codeLines}`).join(' | '));
  console.log(`CS 扫描：${files.length} 个文件，注释 ${commentLines} 行`);
  console.log(`  CS-1 图标 ${icons.length} | CS-2 叙事 ${narrative.length} | CS-5 超块 ${blocks.length} | CS-6 配比失衡 ${ratios.length}`);
  if (fails.length) { fails.forEach((f) => console.log('CS FAIL ' + f)); process.exit(1); }
  console.log('CS PASS 注释纪律全部合规');
}

if (require.main === module) main();
module.exports = { maskRs, maskJs, maskHtml, maskSh, maskFor, allowed, scan, CS_NARRATIVE };
