# BeoDesk Flutter app

Linux/Windows desktop shell and Android controller target. See the [root README](../../README.md) for setup and current limitations.

From the repository root, run `bash scripts/dev.sh run`. Rust bindings are generated from `crates/remote-bridge/src/api`; regenerate with `bash scripts/dev.sh generate` after changing the API.

The app displays the securely stored device fingerprint and supports continuous H.264 video using a Linux native texture plus mouse/physical-keyboard control over QUIC/TLS with the host fingerprint pinned by the viewer. The host no longer needs the viewer fingerprint in advance; its consent dialog shows the TLS-proven requesting identity. Ubuntu GNOME X11 hosts require explicit local consent for each live session. The viewer releases held input on focus loss and can switch to viewing only or disconnect. The original one-snapshot action remains available. Windows/Android native texture rendering and Windows host capture/input remain open. See the [LAN prototype guide](../../docs/lan-prototype.md).
