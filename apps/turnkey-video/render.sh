#!/usr/bin/env bash
# Renders the setup video into out/turnkey.mp4 (for posts and docs: the landing page shows the
# same steps as an interactive walkthrough instead, content/landing.json hero.walkthrough), and
# its step-6 frame as the built-in site's link-preview picture, engine/hub/site/default/assets/preview.png.
#
#   apps/turnkey-video/render.sh            # CRF=14 by default: about 1.7 MB
#
# Needs Node 22. Remotion downloads its own Chromium unless one is found: set
# REMOTION_BROWSER, or have Playwright's headless shell installed. Keep the MP4 at or under
# about 4 MB (1280x720, 30 fps): raise CRF if it grows past that, lower it while it fits.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
out="$here/out"
assets="$here/../../engine/hub/site/default/assets"
mkdir -p "$out"
cd "$here"
npm ci --no-audit --no-fund
browser="${REMOTION_BROWSER:-}"
if [ -z "$browser" ]; then
  for dir in "${PLAYWRIGHT_BROWSERS_PATH:-}" "$HOME/.cache/ms-playwright" /opt/pw-browsers; do
    [ -n "$dir" ] || continue
    found="$(ls -d "$dir"/chromium_headless_shell-*/chrome-linux/headless_shell 2>/dev/null | sort | tail -n 1 || true)"
    if [ -n "$found" ]; then browser="$found"; break; fi
  done
fi
args=()
if [ -n "$browser" ]; then args+=(--browser-executable "$browser"); fi
npx remotion render src/index.ts Turnkey "$out/turnkey.mp4" --codec h264 --crf "${CRF:-14}" \
  --x264-preset veryslow --pixel-format yuv420p --image-format png --color-space bt709 --overwrite "${args[@]}"
# The preview picture is step 6: the approval on the phone while the AI waits for it. Move this
# frame with the scene lengths in src/Turnkey.tsx.
npx remotion still src/index.ts Turnkey "$assets/preview.png" --frame 1476 --overwrite "${args[@]}"
ls -l "$out/turnkey.mp4" "$assets/preview.png"
