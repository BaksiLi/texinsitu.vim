#!/bin/sh
# Kernel and renderer tests, then the screen tests in a real terminal Vim.
set -eu
repo=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
# $VIM is Vim's own runtime root, not an executable selector.
vim_bin=${TEXINSITU_VIM:-vim}
cargo test --quiet --release --manifest-path "$repo/kernel/Cargo.toml"
cargo build --quiet --release --manifest-path "$repo/kernel/Cargo.toml"
python3 "$repo/tests/renderer.py"

work=$(mktemp -d)
trap 'rm -f "$work/result" "$work/log"; rmdir "$work"' EXIT
# screenstring() needs a real screen, not Ex mode.
TEXINSITU_RESULT=$work/result "$vim_bin" -Nu "$repo/tests/screen.vim" -i NONE -n -T xterm \
  >"$work/log" 2>&1 || true
if [ "$(head -n 1 "$work/result" 2>/dev/null)" != PASS ]; then
  echo 'screen tests: FAIL'
  cat "$work/result" 2>/dev/null || tail -n 20 "$work/log"
  exit 1
fi
echo 'screen tests: PASS'
