'use strict';
// 桌面壳版本三处互锁自洽校验：Cargo.toml 的 [package].version、tauri.conf.json 的 version、
// Cargo.lock 的本包 version。Tauri 打包与更新清单读前者，lock 不一致会被 cargo 当作依赖变更。
const fs = require("node:fs");
const path = require("node:path");
const root = path.join(__dirname, "..");
const bad = [];
let cargo; let tauri; let lock;
const p = (...x) => path.join(root, "src-tauri", ...x);
try {
  cargo = fs.readFileSync(p("Cargo.toml"), "utf8").match(/^version\s*=\s*"([^"]+)"/m)?.[1];
} catch (e) { bad.push("读取 Cargo.toml 失败: " + e.message); }
try {
  tauri = JSON.parse(fs.readFileSync(p("tauri.conf.json"), "utf8")).version;
} catch (e) { bad.push("读取 tauri.conf.json 失败: " + e.message); }
try {
  const t = fs.readFileSync(p("Cargo.lock"), "utf8");
  lock = t.match(/\[\[package\]\]\nname = "dsh-supervisor-gui"\nversion = "([^"]+)"/)?.[1];
} catch { /* Cargo.lock 可选 */ }
if (!cargo) bad.push("Cargo.toml 缺 [package] version");
if (!tauri) bad.push("tauri.conf.json 缺 version");
if (cargo && tauri && cargo !== tauri) bad.push("tauri.conf.json " + tauri + " ≠ Cargo.toml " + cargo);
if (lock && cargo && lock !== cargo) bad.push("Cargo.lock " + lock + " ≠ Cargo.toml " + cargo);
if (tauri && !/^[0-9]+\.[0-9]+\.[0-9]+(-(BETA|RC)\.[0-9]+)?$/.test(tauri)) bad.push("壳版本非法: " + tauri);
if (bad.length) { console.error("壳版本校验失败:\n- " + bad.join("\n- ")); process.exit(1); }
console.log("壳版本自洽 OK: " + tauri + "（Cargo.toml = tauri.conf.json" + (lock ? " = Cargo.lock" : "") + "）");
