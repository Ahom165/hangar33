#!/usr/bin/env bash
# ============================================================
#  HANGAR 33 — environnement de dev GPU sans carte graphique
#  (lavapipe = driver Vulkan logiciel de Mesa, espace utilisateur)
# ============================================================
# Usage :
#   source scripts/dev-env.sh
#   cargo test -p h33-render --test offscreen -- --nocapture
#   xvfb-run -a cargo run -p h33-app -- --smoke-test --screenshot docs/screenshots/smoke.png

# Driver Vulkan lavapipe extrait en espace utilisateur (debs dpkg -x).
export VK_ICD_FILENAMES=/home/z/.local/vulkan-lvp/root/usr/share/vulkan/icd.d/lvp_icd.json
export LD_LIBRARY_PATH=/home/z/.local/vulkan-lvp/root/usr/lib/x86_64-linux-gnu:${LD_LIBRARY_PATH:-}

# Force le backend Vulkan (lavapipe) pour wgpu.
export WGPU_BACKEND=vulkan
# Traces de chargement si besoin :
# export VK_LOADER_DEBUG=all

echo "[dev-env] lavapipe actif : $VK_ICD_FILENAMES"
