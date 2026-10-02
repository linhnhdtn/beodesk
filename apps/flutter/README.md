# BeoDesk Flutter app

Linux/Windows desktop shell and Android controller target. See the [root README](../../README.md) for setup and current limitations.

From the repository root, run `bash scripts/dev.sh run`. Rust bindings are generated from `crates/remote-bridge/src/api`; regenerate with `bash scripts/dev.sh generate` after changing the API.

The app displays the securely stored device fingerprint and can request one PNG snapshot over mutually pinned QUIC/TLS. Ubuntu GNOME X11 hosts require local approval for each snapshot. Continuous video and remote input remain unimplemented. See the [LAN prototype guide](../../docs/lan-prototype.md).
