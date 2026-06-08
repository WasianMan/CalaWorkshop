#!/usr/bin/env bash
#
# Package extension/ into a versioned dist/CalaWorkshop-vX.Y.Z.c7s.zip archive (Linux/CI).
# `zip -r` stores directory entries, which the panel's installer requires.
#
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SRC="$ROOT/extension"
DIST="$ROOT/dist"
VERSION="$(sed -nE 's/^version[[:space:]]*=[[:space:]]*"([^"]+)".*/\1/p' "$SRC/backend/Cargo.toml" | head -n1)"
if [ -z "$VERSION" ]; then
  echo "extension backend version missing" >&2
  exit 1
fi
OUT="$DIST/CalaWorkshop-v$VERSION.c7s.zip"

[ -f "$SRC/Metadata.toml" ] || { echo "extension/Metadata.toml missing" >&2; exit 1; }

mkdir -p "$DIST"
rm -f "$DIST"/*.c7s.zip

( cd "$SRC" && zip -rq "$OUT" . -x '*/node_modules/*' '*/target/*' '*.DS_Store' )

echo "Built $OUT"
