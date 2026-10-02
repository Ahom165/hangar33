#!/usr/bin/env bash
# Suite GPU complète : tests offscreen + smoke-test de l'app sous Xvfb.
# (nécessite scripts/dev-env.sh — lavapipe en espace utilisateur)
set -e
cd "$(dirname "$0")/.."
source scripts/dev-env.sh

echo "=== [1/2] Test rendu offscreen (wgpu + lavapipe) ==="
cargo test -p h33-render --test offscreen -- --nocapture

echo ""
echo "=== [2/2] Smoke-test app (fenêtre Xvfb + lavapipe) ==="
mkdir -p docs/screenshots
xvfb-run -a -s "-screen 0 1280x800x24" \
  cargo run -p h33-app -- \
    --smoke-test \
    --screenshot docs/screenshots/smoke-test.png

echo ""
echo "=== GPU SUITE : OK — vérifie docs/screenshots/*.png ==="
