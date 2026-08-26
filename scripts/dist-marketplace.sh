#!/usr/bin/env bash
# Clean marketplace slice for directory-source registration.
#
# ZCode's directory-source marketplace mirrors the WHOLE target directory —
# pointing it at the repo root copies build artifacts too (a local target/
# weighed in at 13GB). GitHub-source installs are unaffected (git clone
# carries tracked files only, and target/ is gitignored), so this slice is
# only needed for local marketplace testing. Regenerate after plugin/ or
# marketplace.json changes, then refresh the marketplace in the app.
set -euo pipefail
cd "$(dirname "$0")/.."
rm -rf dist/marketplace
mkdir -p dist/marketplace
cp marketplace.json dist/marketplace/
cp -R plugin dist/marketplace/
echo "[OK] dist/marketplace ready ($(du -sh dist/marketplace | cut -f1)) — register this path as the directory marketplace, not the repo root"
