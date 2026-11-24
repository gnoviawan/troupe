#!/usr/bin/env bash
# Runs the live-driver scenario on a real `hello` window: start it with
# RUNE_DRIVE_SOCK, wait for the button, click it, wait for the "Pressed 1"
# label, read it back and save a frame. Needs a display (xvfb-run on CI) and a
# hello built with `--features tooling`.
#   scripts/rune/drive-hello.sh <hello binary> <png path>
set -euo pipefail

bin=$1
png=$(realpath -m "$2")
dir=$(mktemp -d)
sock="$dir/drive.sock"

RUNE_DRIVE_SOCK="$sock" "$bin" &
app=$!
trap 'kill "$app" 2>/dev/null || true; rm -rf "$dir"' EXIT

node "$(dirname "$0")/drive.mjs" "$sock" \
  '{"cmd":"wait","key":"press","timeout_ms":60000}' \
  '{"cmd":"click","key":"press"}' \
  '{"cmd":"wait","text":"Pressed 1","timeout_ms":10000}' \
  '{"cmd":"find","key":"press-label"}' \
  "{\"cmd\":\"snap\",\"path\":\"$png\"}"
