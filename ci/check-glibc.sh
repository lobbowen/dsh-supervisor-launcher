#!/usr/bin/env bash
# glibc 基座门禁（跨平台审计 F1 的防线）：断言 Linux 产物不要求高于允许上限的 glibc 符号。
# 用法: ci/check-glibc.sh <binary> [max=2.35]
# 不变量：产物引用的最高 GLIBC 符号不得超过 max（glibc 前向兼容；超限则旧发行版无法运行）。
set -euo pipefail
BIN="${1:?用法: check-glibc.sh <binary> [max]}"; MAX="${2:-2.35}"
[ -f "$BIN" ] || { echo "错误：找不到 $BIN"; exit 2; }

# 提取该二进制引用的所有 GLIBC_x.y 版本（取最高）
extract() {
  if command -v objdump >/dev/null 2>&1; then
    objdump -T "$BIN" 2>/dev/null | grep -oE "GLIBC_[0-9]+\.[0-9]+" || true
  else
    readelf --dyn-syms -W "$BIN" 2>/dev/null | grep -oE "GLIBC_[0-9]+\.[0-9]+" || true
  fi
}
VERS="$(extract | sed "s/^GLIBC_//" | sort -uV)"
[ -n "$VERS" ] || { echo "警告：未能从 $BIN 提取 glibc 符号（非动态链接？）"; exit 0; }
HIGHEST="$(printf "%s\n" "$VERS" | tail -1)"

# semver 比较（仅比较 x.y）
vercmp() { [ "$1" = "$2" ] && { echo 0; return; }; printf "%s\n%s\n" "$1" "$2" | sort -V | tail -1 | grep -qx "$1" && echo 1 || echo -1; }

echo "== glibc 门禁: $BIN =="
echo "  要求的最高符号: GLIBC_$HIGHEST   允许上限: $MAX"
if [ "$(vercmp "$HIGHEST" "$MAX")" -gt 0 ]; then
  echo "  失败：该产物要求 GLIBC_$HIGHEST > $MAX"
  echo "     发行版兼容性将受限（例如 Ubuntu 22.04 仅有 glibc 2.35）。"
  echo "     修复：在更老的基座上构建（推荐 GitHub runner: ubuntu-22.04）。"
  exit 1
fi
echo "  通过：可在 glibc >= $MAX 的发行版运行"
