# Development environment

## Pinned tools

| Component | Version / baseline |
|---|---|
| Ubuntu | 22.04.5 LTS, x86_64, GNOME X11 |
| Rust | 1.98.1, pinned in `rust-toolchain.toml` |
| Flutter | 3.47.5 stable, pinned in `.fvmrc` |
| Flutter revision | `6a19cca56475dbfba1478ee68d7bd0c2ef891da1` |
| Dart | 3.13.4, bundled with Flutter |
| flutter_rust_bridge runtime/codegen | 2.11.1 |
| Clang | Ubuntu 14.0.0 |
| CMake / Ninja | 3.22.1 / 1.10.1 |
| Java | OpenJDK 17 |
| Android compile SDK / minimum SDK | 36 / 24 |
| Android NDK | 28.2.13676358 |
| Android Gradle Plugin / Gradle | 9.1.0 / 9.3.1 |

Commit the root `Cargo.lock` and `apps/flutter/pubspec.lock`. The bridge is a member of the root Rust workspace and has no independent Cargo lockfile. Cargo/Flutter build products, local SDKs, and credentials are excluded from version control.

## Fresh Ubuntu 22.04 workstation

Install native build requirements:

```bash
sudo apt install clang cmake ninja-build pkg-config libgtk-3-dev liblzma-dev libstdc++-12-dev libdbus-1-dev libxtst-dev curl git unzip xz-utils nasm xvfb
```

For the local-SDK layout used by `scripts/env.sh`, from the repository root:

```bash
mkdir -p .tools
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs -o /tmp/beodesk-rustup.sh
CARGO_HOME="$PWD/.tools/cargo" RUSTUP_HOME="$PWD/.tools/rustup" sh /tmp/beodesk-rustup.sh -y --no-modify-path --profile minimal --default-toolchain 1.98.1
git clone --depth 1 --branch 3.47.5 https://github.com/flutter/flutter.git .tools/flutter
source scripts/env.sh
flutter --version
flutter config --no-analytics
cargo install flutter_rust_bridge_codegen --version 2.11.1 --locked
cd apps/flutter
flutter pub get --enforce-lockfile
```

`scripts/env.sh` puts Cargo, rustup, pub and Gradle caches under `.tools/`. Flutter itself may retain normal user-level settings. Follow the official [Rust installation guide](https://rust-lang.org/tools/install/) and [Flutter Linux setup](https://docs.flutter.dev/platform-integration/linux/setup) for system-specific issues. Bridge generation follows the [flutter_rust_bridge quickstart](https://cjycode.com/flutter_rust_bridge/quickstart); the runtime and codegen versions must match.

Use `bash scripts/dev.sh doctor` to validate toolchains. Tests need permission to bind localhost sockets. Native integration uses Xvfb; it does not require or test a real remote host.

## Validation boundaries

- Core tests exercise malformed/oversized messages, version handling, consent, wrong pinned peer, replay, stale epoch, lease expiry and held-input cleanup.
- Widget tests cover pending initialization, startup recovery, unavailable secure storage, a narrow viewport, explicit host denial, expired prompts, viewer cancellation and clipboard connection details. Clipboard tests use a mocked platform channel to verify copying the local identity and pasting the peer identity into the correct fields. Separate format tests reject short fingerprints and invalid endpoints before applying either field.
- Real UDP loopback tests cover attended connections without a viewer pin, mandatory TLS client credentials, local consent before desktop access, the retained restricted transport mode, incorrect host/controller pins, oversized length prefixes, approved/denied capture and disconnect during pending consent. They use temporary in-memory identities and do not need an OS keyring.
- Video tests cover successive H.264 frames, resolution changes, 4K downscaling, bitrate resets and SPS bounds before native decoding. Capture tests cover PNG bounds and X11 pixel conversion. `snapshot-smoke` additionally exercises the real OS identity, GNOME lock-state check, Xvfb capture and QUIC transfer. Its automatic approval is limited to its own loopback test; it is not included in unattended CI.
- Native bridge integration loads the bundled Rust shared library and renders engine status. It intentionally avoids the interactive OS credential store so it can run unattended in CI.
- `lan-smoke` is a separate native GUI integration test requiring the real unlocked GNOME keyring. It uses the IP-detection button to bind one of this machine's assigned IPv4 addresses, starts both peers on that same machine, submits and approves a request through Flutter, and asserts the received image is displayed and loading has stopped. Run it under the script's Xvfb display; it is excluded from unattended CI.
- `identity_probe` separately exercises real credential storage. It creates/reuses the `com.beodesk.desktop` / `device-identity-v1` entry; it prints no private key.
- `live-smoke` exercises native continuous capture and real XTEST click, drag, wheel and Shift+A events against a private Xvfb window. It checks actual X11 key/button state after release, a missed input lease, reconnection and host stop. `live-gui-smoke` tests explicit control consent, successive H.264 frames reaching the native texture raster callback and viewer disconnect. Both require the unlocked OS keyring and are excluded from unattended CI.
- Live network tests cover denied consent without opening desktop access, view-only input rejection, changing frames, release requests, transport loss, lease expiration/renewal, host-task cancellation and permission loss. Widget tests cover normalized click/drag coordinates, physical modifiers, ignoring duplicated key repeats, focus loss and switching off control.
- State-machine tests alone do not prove network authentication. The separate loopback tests now exercise real QUIC/TLS; cross-machine behavior and performance remain unqualified.
- A Windows build needs a Windows runner with Visual Studio C++ tools. Android needs its SDK/NDK; a successful APK compile alone does not qualify controller behavior or key storage.

## Next implementation gate

The Linux path now uses bundled OpenH264 and Flutter pixel-buffer textures. Qualify cross-machine FPS/latency and add Windows texture rendering/capture/input. The current X11 path has attended control, physical-key mapping, periodic input leases, a single latest frame in the bridge and native frame reference ownership and texture disposal. Add cross-machine pairing persistence/revocation and Windows validation. The viewer pins the host key; the attended listener requires TLS proof of the requesting device key before presenting consent without a preconfigured viewer pin; a protobuf public-key field or UI approval alone cannot authenticate a connection.

H.264 builds use pinned `openh264` / `openh264-sys2` 0.9.8 from source. NASM enables x86 codec assembly; install it before the first build. This checkout also supports a local NASM under `.tools/nasm/usr/bin`. No FFmpeg executable or separately installed codec is needed at runtime. See [video pipeline](video-pipeline.md) for ownership, flow control and measurements.
