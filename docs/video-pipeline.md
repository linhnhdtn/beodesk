# Linux live video — 2026-10-05

Live sessions now capture X11 pixels, encode H.264, carry access units over QUIC/TLS with a viewer-pinned host and locally approved viewer identity, decode once in Rust, and publish RGBA frames directly to a Flutter Linux pixel-buffer texture. Dart polls only dimensions, generation and session status every 100 ms; it does not copy or decode live image bytes. The separate one-snapshot action still uses PNG.

## Codec and capture

- `remote-video` uses pinned `openh264` / `openh264-sys2` 0.9.8, compiling bundled source. The deployed app needs neither FFmpeg nor a separate codec installation. NASM is used at build time for x86 assembly; encoding and decoding are software, not hardware accelerated.
- One persistent X11 connection per host session captures the root desktop. The common BGRX layout has a direct conversion path. Capture remains bounded to 3840×2160 total pixels. Larger or odd dimensions are scaled to even dimensions within 1920×1080, preserving the desktop's proportions to within rounding. This version uses nearest-neighbor downscaling.
- The encoder uses baseline profile, screen-content real-time mode, low complexity, two threads and a maximum 30 fps. SPS/PPS and an IDR accompany startup and encoder resets; there are no B frames. Resolution changes reconfigure the encoder.
- Each encoded access unit is limited to 2 MiB. Before native decode, the receiver checks packet/NAL bounds and baseline SPS coded dimensions (at most 1920×1088, allowing macroblock padding), progressive mode and at most four references. Decoded display size is separately limited to 1920×1080. This is bounded validation, not a complete H.264 verifier or decoder sandbox.

## Backpressure and input

`H264_STREAM` explicitly identifies the new live wire format. Both peers must be updated; old PNG live clients do not receive H.264 disguised as PNG. Snapshot compatibility remains.

Only one encoded access unit may be outstanding. The receiver acknowledges after decoding and replacing its latest frame. The host waits for that acknowledgement before capturing again, so a slow receiver cannot accumulate seconds of old frames. Captures are skipped before encoding; dependent H.264 frames are never discarded midstream. Input, frame acknowledgements and UI lease messages share the opposite direction of the QUIC stream, with monotonically validated control sequence numbers. Media acknowledgements do not renew the input lease.

Receiver acknowledgement delay feeds an EWMA. Sustained delay above 80 ms steps bitrate down from 6 to 3 to 1.5 Mbit/s; below 30 ms steps it back up. Changes are separated by at least three seconds and restart at a keyframe. Frame skipping is enabled inside rate control. This is a simple LAN adaptation policy, not a full bandwidth estimator.

The one-frame window trades throughput on high-RTT links for bounded backlog. Reliable QUIC still retransmits lost media and can stall on packet loss. Datagram fragmentation, loss recovery, hardware codecs and WAN qualification remain future work; the application's long-term datagram design is not claimed implemented here.

Local consent, desktop lock checks, physical key mapping, focus release, host stop and the 500 ms UI lease / 2 s expiry are retained. Desktop capture checks lock state before and after encoding, and the independent permission watchdog still runs.

## Frame ownership and rendering

Rust stores one latest `Arc<Frame>`. The Linux runner resolves three C ABI entry points from the bundled `libremote_bridge.so`: generation, acquire and release. A raster callback acquires a frame reference and holds it until the next callback or texture finalization, so replacing the latest frame cannot invalidate GPU upload memory. The platform thread checks for a changed generation every 16 ms and marks the texture available. Intermediate decoded frames may be replaced before presentation without affecting decoder reference history.

Flutter unregisters the texture after removing its widget; late creation after the widget closes is disposed immediately. Engine shutdown removes the timer/channel and unregisters remaining textures. Decoded-frame ownership is independent of the viewer task, so an acquired reference stays valid while a session stops.

The native renderer currently ships for Linux. Windows/Android live start reports that limitation before requesting host consent; their native texture backends remain to be implemented. Windows host capture and Android secure storage were already incomplete.

## Validation

38 Rust tests and 34 Flutter widget tests pass, with clean Clippy and Flutter analysis. The Linux release bundle and both native live smoke tests pass.

- Rust unit/network tests cover successive frames and colors, resolution changes, downscaling 4K, bitrate reset/keyframe recovery, invalid SPS bounds, one outstanding capture, consent/authentication and input cleanup.
- Flutter widget tests cover native texture disposal including delayed creation and creation failure, coordinate mapping, keyboard modifiers, focus loss, consent races and resize preservation.
- `bash scripts/dev.sh live-gui-smoke`: actual Flutter consent → authenticated QUIC → H.264 decode → repeated native raster callbacks → disconnect on private Xvfb. A delayed consent status does not close the approved host page.
- `bash scripts/dev.sh live-smoke`: release build, private 1920×1080 Xvfb, changing red/blue frames and actual XTEST click/drag/wheel/Shift+A; verifies X11 input state after focus release, lease expiry, reconnect and host stop.

On this workstation the release native probe measured **28.9 decoded frames/s over 3.01 seconds at 1920×1080**, on a simple mostly static Xvfb scene, over localhost. This measures capture/encode/transport/decode throughput, not physical display presentation rate or input-to-photon latency. The separate GUI test confirms texture callbacks render successive frames. No claim is made yet for two-machine performance, scrolling documents, video playback, CPU usage or impaired networks.
