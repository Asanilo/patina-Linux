#!/usr/bin/env bash
set -e

cd "$(dirname "$0")"

OUT="dist/patina-web-sync-unsigned.xpi"
VERSION=$(node -p "require('./manifest.json').version")

mkdir -p dist

echo "Packaging patina-web-sync@${VERSION}..."

zip -r "$OUT" \
  manifest.json \
  background.js \
  options.js \
  options.html \
  popup.html \
  popup.js \
  icons/ \
  -x "icons/*.xcf" \
  -x "*.DS_Store" \
  -x "__MACOSX/*"

echo "Created: $OUT"
echo ""
echo "To install:"
echo "  1. Open Firefox/Zen → about:debugging#/runtime/this-firefox"
echo "  2. Click 'Load Temporary Add-on...'"
echo "  3. Select: $OUT"
echo ""
echo "Or for AMO unlisted signing:"
echo "  1. Submit to https://addons.mozilla.org/developers/addons"
echo "  2. Download signed .xpi and distribute"
