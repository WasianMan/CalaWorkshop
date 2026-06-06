#!/usr/bin/env bash
#
# Package extension/ into dist/dev_wasian_calaworkshop.c7s.zip (Linux/CI).
# `zip -r` stores directory entries, which the panel's installer requires.
#
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SRC="$ROOT/extension"
DIST="$ROOT/dist"
OUT="$DIST/dev_wasian_calaworkshop.c7s.zip"

[ -f "$SRC/Metadata.toml" ] || { echo "extension/Metadata.toml missing" >&2; exit 1; }

mkdir -p "$DIST"
rm -f "$OUT"

( cd "$SRC" && zip -rq "$OUT" . -x '*/node_modules/*' '*/target/*' '*.DS_Store' )

echo "Built $OUT"
