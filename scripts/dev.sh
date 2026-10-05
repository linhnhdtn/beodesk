#!/usr/bin/env bash
set -euo pipefail
source "$(dirname -- "${BASH_SOURCE[0]}")/env.sh"
cd "$BEODESK_ROOT"

case "${1:-run}" in
  doctor)
    rustc --version
    cargo --version
    flutter doctor -v
    ;;
  generate)
    cd apps/flutter
    flutter_rust_bridge_codegen generate
    ;;
  test)
    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets --locked -- -D warnings
    cargo test --workspace --locked
    cd apps/flutter
    flutter pub get --enforce-lockfile
    dart format --output=none --set-exit-if-changed lib test integration_test
    flutter analyze --no-pub
    flutter test --no-pub
    ;;
  build)
    cd apps/flutter
    flutter build linux --release
    ;;
  run)
    cd apps/flutter
    flutter run -d linux
    ;;
  smoke)
    cd apps/flutter
    xvfb-run -a flutter test integration_test/bridge_test.dart -d linux
    ;;
  snapshot-smoke)
    XDG_SESSION_TYPE=x11 xvfb-run -a -s '-screen 0 1280x720x24' cargo run -p remote_bridge --example lan_snapshot_probe --locked
    ;;
  lan-smoke)
    cd apps/flutter
    XDG_SESSION_TYPE=x11 xvfb-run -a -s '-screen 0 1280x900x24' flutter test integration_test/lan_snapshot_test.dart -d linux --no-pub
    ;;
  live-smoke)
    BEODESK_VIRTUAL_DISPLAY=1 XDG_SESSION_TYPE=x11 xvfb-run -a -s '-screen 0 1920x1080x24' cargo run --release -p remote_bridge --example lan_live_probe --locked
    ;;
  live-gui-smoke)
    cd apps/flutter
    XDG_SESSION_TYPE=x11 xvfb-run -a -s '-screen 0 1280x900x24' flutter test integration_test/lan_live_test.dart -d linux --no-pub
    ;;
  *)
    echo 'Usage: bash scripts/dev.sh {doctor|generate|test|build|run|smoke|snapshot-smoke|lan-smoke|live-smoke|live-gui-smoke}' >&2
    exit 2
    ;;
esac
