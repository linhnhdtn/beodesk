# ADR 0001: Foundation boundaries

Status: accepted; QUIC/TLS snapshot transport is recorded in ADR 0002, continuous-video codec integration remains open.

## Structure and bridge

Use Rust workspace crates for protocol, core and Flutter bridge, plus networking and capture for the working snapshot path. Add codec crates when they contain a working vertical slice. Flutter platforms are generated for Linux, Windows and Android; Rust is bundled using flutter_rust_bridge 2.11.1 and its Cargokit integration. Keep generated bindings and Cargokit license/build files in source control.

Two local Cargokit compatibility changes must be preserved when replacing the vendored template: the Gradle task uses injected `ExecOperations` because [Gradle 9 removes `Project.exec`](https://docs.gradle.org/current/userguide/upgrading_version_8.html#deprecated_project_exec), and the Rust builder honors `RUSTUP_TOOLCHAIN` supplied by our environment/CI instead of forcing the moving `stable` channel. `cargokit.yaml` enforces the workspace lockfile for every build profile. These are build integration changes, not runtime features.

## Protocol

Protobuf is generated from one schema at build time. Version 1.0 starts with handshake metadata, input events and session controls. Control messages are capped at 64 KiB before parsing; session IDs are 16 bytes and epoch/sequence values must be nonzero. New minor versions may carry unknown optional fields, but unknown required actions and unsupported capability values fail closed until a negotiation rule is implemented.

The current envelope is a **reliable control/input format**. It is not a video packet format or a datagram receiver. Motion datagrams will need their own monotonic sequence space, periodic position refresh and click-position ordering. Transport code must check the length prefix before allocating its read buffer.

## Device identity

Use an Ed25519 device key; expose only its public fingerprint to Flutter. Store the seed in Linux Secret Service or Windows Credential Manager. A process-held file lock serializes read/create across app instances; the lock file contains no secret. In-memory secret buffers are zeroized where owned by this code. Missing entries can be created; unreadable or corrupt existing entries must not be overwritten.

There is no plaintext fallback. Android secure storage is deliberately unavailable until Android Keystore integration is built and tested. The SHA-256 fingerprint is not the future numeric Device ID.

## Authorization

The core models authentication pending → host consent pending → active view/control → closed. A 10-second transport timeout, 60-second consent timeout and 2-second active-session lease are enforced by a caller-driven monotonic clock. A future runtime must tick the session without relying on incoming packets and apply queued key/button releases after every operation, including errors.

`on_authenticated_transport` is a **trusted adapter callback**, not a cryptographic verifier. The network adapter authenticates possession of the pinned key via TLS CertificateVerify before calling it. The current listener is opt-in and limited to a specific IP and one configured peer fingerprint. Local UI approval must never be accepted as a remote protocol command.

Revocation, disconnect, permission loss and lease expiry close the session. Closed sessions cannot be resumed by replaying an input/lease; the future resumption protocol must establish a new authenticated transport epoch.

## Remaining release gates

- Independent review of custom certificate pin policy, first-pairing workflow and persisted peer trust/revocation. Real encrypted loopback handshakes are now implemented and tested.
- Windows build/credential-store validation on Windows; Android Keystore and device testing.
- Typed persistent settings and sanitized structured runtime diagnostics as actual services are added.
- Continuous X11/Windows capture, codec, native texture rendering, network scheduling and OS input adapters. The PNG snapshot prototype does not meet the video gate.
- M1 cross-machine performance and recovery report. Foundation tests do not satisfy these gates.
