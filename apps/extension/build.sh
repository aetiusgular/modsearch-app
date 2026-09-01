#!/usr/bin/env bash
# Build the web SPA and stage it into the extension as the full-page app.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"     # aurasearch-app
pnpm -C "$ROOT/apps/web" build
rm -rf "$HERE/app"
cp -R "$ROOT/apps/web/dist" "$HERE/app"
echo "Staged web build into $HERE/app"
