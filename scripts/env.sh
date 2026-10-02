#!/usr/bin/env bash
# Source this file from bash or zsh. SDKs/caches stay inside this checkout.
if [ -n "${BASH_VERSION:-}" ]; then
  BEODESK_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
elif [ -n "${ZSH_VERSION:-}" ]; then
  BEODESK_ROOT="$(cd -- "$(dirname -- "${(%):-%x}")/.." && pwd)"
else
  echo 'Use bash or zsh to source scripts/env.sh.' >&2
  return 1
fi
export BEODESK_ROOT
export CARGO_HOME="$BEODESK_ROOT/.tools/cargo"
export RUSTUP_HOME="$BEODESK_ROOT/.tools/rustup"
export RUSTUP_TOOLCHAIN="$(sed -n 's/^channel = "\(.*\)"$/\1/p' "$BEODESK_ROOT/rust-toolchain.toml")"
export PUB_CACHE="$BEODESK_ROOT/.tools/pub-cache"
export GRADLE_USER_HOME="$BEODESK_ROOT/.tools/gradle"
export PATH="$CARGO_HOME/bin:$BEODESK_ROOT/.tools/flutter/bin:$PATH"
