# Remote Desktop App — Development Plan

Document status: M0 foundation and an initial Ubuntu X11 LAN snapshot prototype are implemented. QUIC/TLS 1.3 mutually pinned device authentication, local consent and a bounded PNG snapshot have passed loopback tests. Windows qualification, Android secure storage, H.264, continuous video and OS input remain open; M0 multi-platform acceptance and M1 are not complete. See [README](README.md), [foundation decisions](docs/decisions/0001-foundation.md) and [LAN prototype](docs/lan-prototype.md) for implemented behavior and remaining gates.

## Delivery milestones and release gates

The phases below organize engineering work, not release promises. This table is the authoritative scope for each milestone; sections 26–30 expand its acceptance criteria. Security, bounded queues, and failure handling begin with the first networked prototype.

| Milestone | Required scope | Exit gate |
|---|---|---|
| M0 — Foundation | Workspace, minimal Flutter shell, versioned protocol, device identity, permission model, CI | Windows/Ubuntu builds and Android compile smoke check; authentication and protocol rejection checks pass |
| M1 — LAN technical milestone | Windows and Ubuntu X11 in all four desktop directions; IP connection; one display; H.264; mouse/keyboard; attended access | Functional, security, recovery, and measured performance gates in section 29 |
| M2 — Internet desktop beta | M1 plus Device ID, signaling, IPv4/IPv6 candidate racing, P2P, one relay deployment, TCP/TLS fallback, authenticated reconnect | Desktop matrix passes direct, relay, and UDP-blocked tests; no plaintext media or input at relay |
| M3 — Product MVP | M2 plus usable Flutter UI, Android controller, device list, qualified Ubuntu Wayland attended support | Six controller/host combinations pass; permission denial and unsupported host states are explicit |
| M4 — Post-MVP capabilities | Unattended access, Wake & Connect, seamless route switching, multiple relay regions, clipboard, files, audio, multiple displays, advanced quality tuning | Each feature has its own platform and regression gates before release |

Initial test baselines: Windows 11 x64, Ubuntu 22.04 LTS x64 with an Xorg desktop session for M1, and GNOME Wayland on the same Ubuntu release for M3. The current development workstation is Ubuntu 22.04.5 LTS x86_64 running GNOME on X11. Record exact OS build, compositor, portal, driver, Android device/API level, and toolchain versions in the M0 compatibility manifest. These are qualification targets, not claims that untested versions work. Headless hosts, login screens, and elevated/secure desktops are outside M1–M3.

Build and smoke-test Linux artifacts on Ubuntu 22.04 from M0 onward. Verify selected Rust/Flutter toolchains, native dependencies, codec backends, and packaged runtime dependencies against this baseline before pinning them. Additional Ubuntu releases require separate qualification; newer portal features must be detected and must not be assumed available on the baseline.

Before a public release, complete the applicable production requirements in section 25, including signed distribution, update delivery, operational monitoring, and abuse controls. Deferring advanced features does not defer these release requirements.

---

## 1. Project Goal

Build a simple, reliable cross-platform remote desktop application with a UX similar to AnyDesk, focused on:

- Simple device discovery and connection
- Minimal setup for end users
- Wake sleeping devices when possible (post-MVP)
- Stable remote control across Windows, Ubuntu, and Android
- Low-latency LAN connections
- Internet P2P connections with relay fallback
- Unattended access for trusted devices (post-MVP)

### Supported connection matrix

| Controller | Remote Windows | Remote Ubuntu |
|---|---:|---:|
| Windows | ✅ | ✅ |
| Ubuntu | ✅ | ✅ |
| Android | ✅ | ✅ |

This includes:

- Windows → Windows
- Windows → Ubuntu
- Ubuntu → Windows
- **Ubuntu → Ubuntu**
- Android → Windows
- Android → Ubuntu

This is the M3 product matrix. Android is initially a controller/viewer only. Remote access *into* Android is outside the MVP scope. Ubuntu support means the qualified desktop sessions listed in the delivery milestones, not every compositor or a headless server.

---

# 2. Recommended Technology Stack

| Component | Technology |
|---|---|
| Core remote engine | Rust |
| Desktop UI | Flutter / Dart |
| Android UI | Flutter / Dart |
| Flutter ↔ Rust bridge | flutter_rust_bridge |
| Async runtime | Tokio |
| Backend API | Rust + Axum |
| Database | PostgreSQL |
| Presence/cache | Redis |
| Serialization | Protobuf |
| Main transport | QUIC / UDP |
| Fallback transport | TCP / TLS |
| Video codec | H.264 |
| Audio codec | Opus |
| Windows capture | Windows Graphics Capture / Desktop Duplication |
| Ubuntu X11 capture | X11 APIs |
| Ubuntu Wayland capture | PipeWire + XDG Desktop Portal |
| Authentication | Public/private device keys |
| Wake-on-LAN | UDP Magic Packet |

These are architecture choices, not a requirement to deploy every component at M0. M1 runs without a backend account, PostgreSQL, or Redis. Choose and pin concrete libraries, codec backends, and build dependencies through the M0/M1 integration spike; record the decisions and supported fallback behavior.

---

# 3. High-Level Architecture

```text
                    CONTROL SERVER
              ┌─────────────────────┐
              │ Auth                │
              │ Device discovery    │
              │ Signaling           │
              │ Presence            │
              │ Relay discovery     │
              └──────────┬──────────┘
                         │
                 Rust backend
                         │
          ┌──────────────┴──────────────┐
          │                             │
       Direct P2P                    Relay
          │                             │
          ▼                             ▼

┌──────────────────┐           ┌──────────────────┐
│ Windows / Ubuntu │◄─────────►│ Windows / Ubuntu │
│                  │           │                  │
│ Flutter UI       │           │ Flutter UI       │
│       ↕          │           │       ↕          │
│ Rust Core        │           │ Rust Core        │
└──────────────────┘           └──────────────────┘
          ▲
          │
          │
┌──────────────────┐
│ Android          │
│ Flutter UI       │
│       ↕          │
│ Rust Core        │
└──────────────────┘
```

Core principle:

```text
Flutter = UI / user interaction

Rust = remote engine
       networking
       capture
       video
       keyboard/mouse
       clipboard
       wake-on-LAN
       security
```

---

# 4. Repository Structure

```text
remote-desktop/
│
├── apps/
│   └── flutter/
│       ├── lib/
│       │   ├── home/
│       │   ├── devices/
│       │   ├── remote/
│       │   └── settings/
│       ├── android/
│       ├── windows/
│       └── linux/
│
├── crates/
│   ├── remote-core/
│   ├── remote-network/
│   ├── remote-codec/
│   ├── remote-capture/
│   │   ├── windows/
│   │   └── linux/
│   ├── remote-input/
│   │   ├── windows/
│   │   └── linux/
│   ├── remote-protocol/
│   ├── remote-clipboard/
│   ├── remote-files/
│   ├── remote-audio/
│   └── remote-wol/
│
├── server/
│   ├── api/
│   ├── signaling/
│   ├── rendezvous/
│   └── relay/
│
└── Cargo.toml
```

---

# 5. Phase 0 — Project Foundation

## Goal

Create a clean monorepo and common protocol definitions.

## Tasks

- Create Rust workspace
- Create Flutter app
- Define OS abstraction layers
- Define transport interfaces
- Define common input events
- Define frame/video packet format
- Add basic logging
- Add configuration handling
- Add CI for Windows and Ubuntu
- Add Android compile smoke checks and a pinned compatibility/toolchain manifest
- Define protocol major/minor version and capability negotiation from the first prototype
- Generate device identities and define pairing, consent, disconnect, and trust-revocation flows
- Choose a reviewed transport authentication mechanism; bind peer identity to the encrypted session
- Define queue/memory/message limits and a session state machine with typed failure reasons
- Record architecture decisions for capture, codec, rendering, transport, and platform permissions

## Initial protocol types

```text
Handshake
VideoFrame
MouseEvent
KeyboardEvent
Ping
Pong
SessionState
Disconnect
```

## Definition of Done

The repository builds successfully on:

- Windows
- Ubuntu
- Android Flutter target

Authentication rejects unknown/unapproved peers, malformed or oversized messages are rejected, and incompatible protocol majors fail with an actionable error. These checks are required before a prototype accepts remote input.

---

# 6. Phase 1 — Windows → Windows LAN Prototype

## Goal

Display the screen of one Windows machine on another Windows machine.

## Scope

```text
Windows A
    ↓
Windows B

LAN only
```

## Tasks

### Host

- Capture desktop
- Produce raw frames
- Listen for incoming connections

### Client

- Connect using local IP
- Receive frames
- Render frames

## First prototype

```text
Remote IP

192.168.1.20

[ Connect ]
```

The first networked prototype requires encryption, authenticated pairing, and explicit host consent before capture or input begins. Pairing must verify the peer key through a locally compared fingerprint or a reviewed short-code pairing flow; a display name or Device ID alone is not proof of identity. Bind the listener only to the selected test interface and show an active-session indicator with a local disconnect action. Do not ship an unauthenticated network mode.

## Definition of Done

An approved client can see the remote Windows desktop continuously. Rejected, expired, or unpaired requests receive no screen data. Closing the host consent dialog denies the request.

---

# 7. Phase 2 — Windows Mouse and Keyboard Control

## Goal

Control the remote Windows machine.

## Events

```text
MouseMove
MouseButton
MouseWheel
KeyDown
KeyUp
```

Example abstraction:

```rust
enum InputEvent {
    MouseMove {
        x: f32,
        y: f32,
    },
    MouseButton {
        button: MouseButton,
        pressed: bool,
    },
    MouseWheel {
        delta: i32,
    },
    Key {
        key: KeyCode,
        pressed: bool,
    },
}
```

## Definition of Done

Remote user can:

- Move mouse
- Left click
- Right click
- Scroll
- Type text
- Use keyboard shortcuts
- Open applications remotely

Define absolute pointer coordinates normalized to the selected display, coordinate clamping, scale/rotation mapping, and physical-key versus text-input semantics. M1 requires US-layout keys and modifiers; additional layouts, IME, and Android text entry need their own M3 tests. Every button event carries the pointer position used for that click so separately delivered motion cannot move the click target.

Track pressed keys/buttons per session and release them on disconnect, permission loss, or a 2-second missed input lease. Refresh the lease every 500 ms while control is active. Clear input state on reconnect; never replay buffered keystrokes from an interrupted session.

---

# 8. Phase 3 — Video Streaming

## Goal

Replace raw/JPEG-style frame transport with real video streaming.

## Pipeline

```text
Screen Capture
      ↓
Frame processing
      ↓
H.264 encoder
      ↓
Network
      ↓
H.264 decoder
      ↓
Renderer
```

## Initial target

```text
1920 × 1080
30 FPS
```

LAN latency stretch target (capture-to-display, measured as defined in section 30):

```text
< 50 ms when practical
```

M1 acceptance uses the 30 FPS and p95 latency gates in section 29; this stretch target is not an additional release gate.

## Pipeline decisions required for M1

- Prove one capture → H.264 encode → transport → decode → Flutter texture render path on each desktop platform before broadening features.
- Record pixel formats, timestamps, buffer ownership, and the Rust/native/Flutter boundary. Avoid sending each raw video frame through ordinary Dart object serialization.
- Negotiate H.264 profile/level and decoder capabilities; support resolution changes and codec configuration followed by a fresh keyframe.
- Keep at most two pending raw frames; replace stale pending frames before encoding. Bound encoded/reassembly queues by bytes and time as specified in section 13A.
- Provide a tested software codec path; qualify hardware acceleration per device instead of assuming availability. Record codec distribution dependencies before packaging.
- Add basic overload handling and bitrate/FPS reduction in M1/M2; Phase 15 adds tuning, not the first protection against queue growth.

## Later optimization

- Damage-region detection
- Adaptive bitrate
- Hardware encoding
- Hardware decoding
- 60 FPS mode

---

# 9. Phase 4 — Ubuntu Remote Engine

Ubuntu is a first-class platform, not an optional extension.

## Goal

Support all desktop-to-desktop combinations:

```text
Windows → Windows
Windows → Ubuntu
Ubuntu → Windows
Ubuntu → Ubuntu
```

## Architecture

```text
                 ScreenCapture
                      │
         ┌────────────┴────────────┐
         │                         │
 WindowsCapture               LinuxCapture
                                   │
                         ┌─────────┴─────────┐
                         │                   │
                        X11               Wayland
```

## Phase 4A — Ubuntu X11

Implement first:

- X11 screen capture
- X11 keyboard injection
- X11 mouse injection
- Clipboard later

Test:

```text
Windows → Ubuntu X11
Ubuntu X11 → Windows
Ubuntu X11 → Ubuntu X11
```

## Phase 4B — Ubuntu Wayland

Implement a permission/capture/input spike during M1; qualify attended Wayland support for M3 after X11 is stable. A failed spike must result in an explicit limitation or a revised release scope, not an untested support claim.

Use:

```text
XDG Desktop Portal
       +
PipeWire
```

Wayland may require portal permissions for screen capture and input control depending on compositor/security model.

Use the [XDG RemoteDesktop portal](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.RemoteDesktop.html) session and permission lifecycle, with the ScreenCast/PipeWire path for video. Test grant, denial, cancellation, and session closure on the exact supported GNOME/portal versions. Restore tokens and persistence depend on available portal support and user decisions; they do not establish blanket unattended permission.

Test:

```text
Windows → Ubuntu Wayland
Ubuntu Wayland → Windows
Ubuntu Wayland → Ubuntu Wayland
```

## Definition of Done

These flows work reliably in LAN on the qualified host display system (X11 for M1; supported Wayland configurations added in M3):

- Windows → Windows
- Windows → Ubuntu
- Ubuntu → Windows
- **Ubuntu → Ubuntu**

Ubuntu → Ubuntu must be explicitly included in regression testing.

---

# 10. Phase 5 — Stable Internal Protocol

## Goal

Stabilize the versioned protocol introduced in M0. Do not wait until this phase to define compatibility, identity binding, or message limits.

Suggested messages:

```text
Handshake
ProtocolVersion
Capabilities
VideoPacket
MouseEvent
KeyboardEvent
Ping
Pong
QualityChange
SessionState
Disconnect
```

Recommended:

```text
Protobuf
```

Include feature negotiation:

```text
supports_h264
supports_h265
supports_av1
supports_clipboard
supports_audio
supports_file_transfer
supports_wol
supports_wayland
```

Specify major-version rejection, compatible minor-version behavior, unknown optional fields, message size limits, session IDs, transport epochs, monotonic sequence numbers, codec negotiation, and error codes. Capabilities describe what the current session is permitted and able to do, not just what the binary implements. M1 must reject unsupported requests safely.

---

# 11. Phase 6 — Signaling Server

## Goal

Allow devices to discover each other without entering IP addresses.

Architecture:

```text
Device A
    │
    │ register
    ▼
Signaling Server
    ▲
    │ register
    │
Device B
```

The server handles:

- Device registration
- Device presence
- Session negotiation
- Endpoint exchange
- Public-key exchange
- Relay discovery

It should **not** process video when direct P2P works.

---

# 12. Phase 7 — Device ID

Each installation receives a user-friendly ID.

Example:

```text
847 291 302
```

Optional alias:

```text
ubuntu-office
gaming-pc
```

UI:

```text
Connect to another device

┌────────────────────────┐
│ 847 291 302            │
└────────────────────────┘

[ Connect ]
```

Backend maps:

```text
Device ID
    ↓
Device identity
    ↓
Current endpoint
```

---

# 13. Phase 8 — Internet P2P and Connection Racing

## Goal

Connect devices over the Internet with the lowest practical latency, regardless of whether the best path is IPv4 or IPv6.

The application must **not prefer IPv4 or IPv6 permanently**.

Instead, it should discover multiple candidates, race their authenticated connectivity checks, and start on the first healthy path. Longer quality measurements refine the choice after startup; they must not block the initial connection.

## Connection preference

Prefer lower measured cost, using direct routes as the starting preference when quality is similar. This is not a serial timeout chain:

```text
LAN direct
    ↓
Internet P2P
    ↓
Relay
```

But IPv4 and IPv6 candidates should be **raced**, not tried strictly one after another.

Example of ongoing quality measurement after the initial path is connected:

```text
Client A                        Device B

LAN IPv4     ────────────────► test
LAN IPv6     ────────────────► test
Public IPv4  ────────────────► test
Public IPv6  ────────────────► test

              ↓

Measure:
RTT
packet loss
jitter
handshake time

              ↓

Retain best measured candidate for reconnect / M4 switching
```

This avoids waiting several seconds for a broken or slow IPv6 path before falling back to IPv4, or vice versa.

## Candidate model

Suggested Rust model:

```rust
enum CandidateType {
    LanV4,
    LanV6,
    DirectV4,
    DirectV6,
    RelayV4,
    RelayV6,
}
```

```rust
struct ConnectionCandidate {
    candidate_type: CandidateType,
    address: std::net::SocketAddr,

    rtt_ms: u32,
    jitter_ms: u32,
    loss_percent: f32, // 0.0..=100.0; 1.0 means 1% loss
    connect_time_ms: u32,
}
```

Measurement fields apply only after sampling. In implementation, represent missing measurements explicitly (for example with `Option<PathMetrics>`); an unmeasured path must not receive a zero-cost score. Record sample count, measurement age, and transport type alongside each path.

## Candidate scoring

Do not select a route only because it is IPv6 or IPv4.

Use measured quality.

Initial tunable scoring model (policy cost, not a measured latency prediction):

```text
score_ms =
    rtt_ms
  + 2 × jitter_ms
  + 100 × loss_percent
  + relay_penalty_ms
```

Lower score wins.

Use RTT and jitter in milliseconds, loss in percentage points from 0 to 100, and an initial relay penalty of 10 score units. Thus 1% loss adds 100 units. These weights are starting policy values to validate under controlled network tests, not universal quality constants.

Compare paths using the same sampling window and estimator: initially 10 seconds, at least 20 acknowledged/expired probes, median RTT, mean absolute successive RTT difference for jitter, and missing replies after the declared probe deadline for loss. Mark low-sample estimates as provisional; do not claim precise 1% loss from a small probe set. Account for probe overhead and cap concurrent checks.

Example:

```text
Direct IPv4

RTT      18 ms
Jitter    2 ms
Loss      0%

Score ≈ 22
```

```text
Direct IPv6

RTT      45 ms
Jitter   10 ms
Loss      1%

Score ≈ 165
```

Select IPv4.

On another network IPv6 may win. The application should always use measured performance instead of assuming one family is faster.

## Connection algorithm

```text
Resolve target Device ID
        ↓
Get authorized connection candidates
        ↓
Start connectivity checks
        ↓
┌────────────┬────────────┬────────────┬────────────┐
│ LAN IPv4   │ LAN IPv6   │ P2P IPv4  │ P2P IPv6  │
└────────────┴────────────┴────────────┴────────────┘
        ↓
Race authenticated handshakes with a bounded delay
        ↓
Start on first healthy authorized path
        ↓
Monitor quality; evaluate relay if direct is absent or degraded
```

Do not wait for every candidate to fully timeout before testing the next one.

Use a Happy-Eyeballs-style staggered race so the first healthy low-latency path wins quickly.

Initial policy, to be verified during M2:

- Stagger candidate attempts by 50 ms, alternate available address families, and cap active attempts at four. Use previous successful measurements when available without permanently preferring a family.
- Start relay checks after 300 ms if no direct handshake succeeds; do not wait for all direct candidates to time out. Bound each attempt to 3 seconds and the overall connection attempt to 10 seconds, excluding the separately shown host-consent wait (60 seconds).
- Validate peer identity and host authorization before starting capture or input. Connectivity probes alone do not authorize a session.
- If direct quality violates the configured latency/loss budget for two consecutive 10-second windows, probe relay routes even when direct still works. Start with median RTT above 150 ms or measured loss above 3%, then tune using M2 test evidence. Compare end-to-end controller-to-host measurements through each relay, not just controller-to-relay RTT.
- In M2/M3, score available routes on startup and reconnect. Switching an active healthy session is M4: require a score improvement of at least `max(10, 20% of current score)` for two windows and a 30-second switch cooldown. A dead path triggers immediate reconnect without waiting for quality windows.
- Cancel losing attempts, expire relay allocations, and report the selected route and failure reason in diagnostic logs without exposing session secrets.

## NAT traversal

Tasks:

- Public endpoint discovery
- IPv4 NAT traversal
- IPv6 direct connectivity
- UDP hole punching
- NAT mapping discovery
- Connection racing
- Timeout handling
- Relay fallback

## Preferred transport

```text
QUIC / UDP
```

Avoid putting real-time video, mouse movement, and keyboard traffic into one blocking TCP stream.

M2 must also pass an outbound-TCP-only test. Specify a relay tunnel over TLS/TCP port 443 with separate control/input and media connections, both bound to the same authenticated application session. Use a vetted end-to-end encrypted session layer inside the relay tunnel; TLS terminated at the relay alone does not meet the confidentiality requirement. Make the concrete library/protocol choice and review it before M2 implementation; do not invent a key exchange.

Fallback keeps the same message semantics, bounded media queues, and input-reset rules. Drop stale media before writing to TCP; bytes already written cannot be selectively withdrawn. Lower video quality under congestion and document that fallback cannot promise QUIC-equivalent latency. Failure of either required connection pauses control and triggers authenticated reconnect. Proxy authentication and networks that also block this TLS tunnel remain explicit unsupported cases until separately qualified.

---

# 13A. QUIC Channel Design

Use one encrypted QUIC connection with different traffic classes.

```text
QUIC Session
│
├── Reliable control stream
│   ├── session state
│   ├── authentication
│   └── clipboard metadata
│
├── Reliable input stream
│   ├── keyboard key-down / key-up
│   ├── mouse buttons / wheel
│   └── input lease renewal
│
├── Low-latency input path
│   └── mouse movement
│
├── Video path
│   └── H.264 packets
│
└── Audio path
    └── Opus packets
```

Mouse movement should not require retransmission of stale positions.

Example:

```text
x=300
x=305
x=312
x=320
```

If `x=305` is lost but `x=320` arrives, retransmitting the older position is unnecessary.

For remote desktop, freshness is usually more important than perfect delivery for high-frequency state updates.

## Wire semantics and bounded buffering

QUIC reliable streams and the negotiated DATAGRAM extension serve different traffic needs. DATAGRAM delivery is not retransmitted and does not provide fragmentation; its usable payload depends on negotiated limits and path MTU. See [RFC 9221](https://www.rfc-editor.org/rfc/rfc9221.html).

| Traffic | M1/M2 delivery policy |
|---|---|
| Handshake, authorization, session state | Dedicated reliable ordered stream, length-delimited messages |
| Key/button/wheel events | Dedicated reliable ordered input stream; sequence number, input lease, and click position |
| Pointer motion | DATAGRAM absolute display position with sequence number; periodically repeat the latest position so a lost final update is repaired; discard older updates |
| H.264 | DATAGRAM application fragments with frame ID, fragment index/count, media epoch, capture timestamp, and keyframe/configuration flags |
| Clipboard/files, later | Separate reliable streams with lower scheduling priority and independent limits |
| Audio, later | DATAGRAM with sequence/timestamp and a bounded jitter buffer |

Use the QUIC library's current payload limit for fragment sizing. Reject impossible fragment counts, oversized declared frames, duplicates, and obsolete epochs before allocating buffers. Initial limits are 64 KiB per control message, 8 MiB per encoded frame, and 16 MiB of incomplete video reassembly per session; negotiate lower limits where needed. Expire incomplete frames 100 ms after the first fragment arrives. Refuse data beyond limits instead of allocating unbounded memory.

The sender retains at most two pending raw frames and bounds queued encoded media to 16 MiB and 100 ms of local queue age. Decoder work is bounded to two complete access units; overload flushes stale work and requests a keyframe when needed. Drop raw frames before encoding when possible. Do not assume arbitrary encoded H.264 reference frames can be dropped without affecting subsequent decoding: discard unusable dependent data and request a rate-limited keyframe (initially at most once per second). Send codec configuration at startup and with each recovery keyframe.

M1 requires negotiated DATAGRAM support and returns an explicit unsupported-transport error otherwise. M2 adds the authenticated reliable fallback above. Match advertised capabilities to the transport actually selected.

---

# 13B. Traffic Priority

Network scheduling should prioritize control responsiveness over visual quality.

Recommended priority:

```text
Priority 1
Keyboard / mouse buttons

Priority 2
Mouse movement

Priority 3
Audio

Priority 4
Video

Priority 5
Clipboard / file transfer
```

When bandwidth drops, reduce video quality first.

Do **not** allow file transfer or a large video queue to delay keyboard or mouse input.

Enforce priority in the application scheduler and bound each queue; separate QUIC streams alone are not a scheduling policy. M1/M2 include queue metrics and overload tests. Advanced bitrate tuning can remain in Phase 15.

---

# 13C. Connection Migration

The active path should be monitored continuously.

M4 target example:

```text
Android on Wi-Fi
       ↓
IPv6 P2P connection
       ↓
Wi-Fi disappears
       ↓
4G becomes active
       ↓
Discover candidates again
       ↓
IPv4 / IPv6 / Relay
       ↓
Move session to best available path
```

Seamless application-session continuity is an M4 goal. M2/M3 require visible reconnecting state, safe input reset, and a bounded authenticated reconnect when the network changes; they do not promise interruption-free switching.

Keep session identity separate from the current network path.

QUIC path validation/migration operates within its connection model; changing to a relay or TCP tunnel may require a new transport. Do not equate all route changes with native QUIC migration. See [RFC 9000, section 9](https://www.rfc-editor.org/rfc/rfc9000.html#section-9).

For application-session resumption, bind a short-lived, single-use resume credential to both peer identities and the existing permission grant. Revalidate revocation and expiry, increment the transport epoch, invalidate the old transport, reset keys/buttons and media buffers, and request a fresh keyframe. If the grant is no longer valid, require new host consent. Never replay stale input or accept simultaneous controllers through two transports.

---

# 14. Phase 9 — Relay Server

Use relay when no healthy direct path is ready within the startup budget, direct connectivity fails, or direct is clearly worse under the section 13 scoring policy. M2/M3 apply quality-based selection at startup/reconnect; seamless live switching is M4.

```text
Client A
    ↓
Relay
    ↓
Client B
```

Important design rule:

```text
Relay forwards encrypted packets.

Relay does not decode video.
Relay does not inspect remote input.
```

Deploy relay independently from signaling.

## Regional relay selection

M2/M3 begin with one region to validate correctness and operating cost. Multiple regional relays are M4; do not make three-region infrastructure a prerequisite for a usable beta.

Candidate regions for a later Asia-focused deployment (choose using measurements and hosting constraints):

```text
Vietnam
Singapore
Tokyo
```

Later:

```text
Frankfurt
US West
US East
```

When direct P2P fails or remains degraded, test available relay endpoints using the section 13 policy. When multiple regions exist, choose by measured controller-to-host cost through the relay, including both network legs and relay load.

Example:

```text
Relay Vietnam
End-to-end RTT 15 ms

Relay Singapore
End-to-end RTT 42 ms

Relay Tokyo
End-to-end RTT 78 ms

→ choose Vietnam
```

Relay selection should consider:

```text
RTT
packet loss
jitter
current relay load
```

IPv4 and IPv6 relay endpoints should both be supported and raced when available.

# 15. Security — M0 Onward; Phase 10 Hardening Gate

Security is a prerequisite for networked prototypes, not a feature first introduced at Phase 10. This phase reviews and hardens what M0/M1 already enforce; its gate must pass before the M2 Internet beta.

## Device identity

On install:

```text
Generate private key
        ↓
Keep private key on device

Generate public key
        ↓
Register public key with server
```

Generate and protect keys locally in M0; server registration starts in M2. Use OS-backed secret storage where available, define a protected fallback, and never write private keys or session tokens to logs. Device IDs and aliases are lookup labels, not authentication secrets.

## Connection

```text
Device A
   ↓ challenge

Device B
   ↓ signature

Device A
   ↓ verify
```

This is a conceptual illustration only, not a complete authentication protocol. Use a reviewed authenticated handshake that binds both peer identities, fresh challenges, negotiated capabilities, and the session transcript. Do not design a custom signature/key-exchange protocol from this diagram. Verify first-pairing fingerprints or use a reviewed pairing flow; a server-provided public key alone is insufficient to establish trust. Reject identity changes until explicitly re-paired.

## Requirements

- End-to-end encrypted remote stream
- Secure device identity
- Replay protection
- Session authentication
- Trusted-device revocation
- No plaintext remote credentials in backend database
- Host accept/deny with a 60-second timeout; capture and input remain disabled until permission is granted
- Explicit view-only/control permissions, visible session indicator, and immediate local disconnect
- Pairing-attempt limits, host request throttling, parser/message limits, and authenticated relay allocation before Internet exposure
- Reject replayed or expired session/resume credentials; disable 0-RTT for authorization and remote-control actions
- Revocation terminates active grants and prevents resumption; log session outcomes without screen content, clipboard content, or keystrokes

Release checks cover denied consent, unknown/changed keys, malicious signaling key substitution, relay confidentiality, replay, malformed messages, revoked devices, and abrupt transport loss. Trusted identity does not automatically grant unattended control in M1–M3.

---

# 16. Phase 11 — Flutter Product UI

Build the polished product UI after the core connection path is stable. The minimal Flutter render/consent/disconnect shell is already required for M1.

## Main screen

```text
REMOTE

Connect to a device

┌─────────────────────────────┐
│ Device ID / Alias       →   │
└─────────────────────────────┘


MY DEVICES

● Windows PC
  Windows 11
                       Connect

● Ubuntu Dev
  Ubuntu
                       Connect
```

Navigation:

```text
Home
Devices
Files
Settings
```

Avoid unnecessary dashboards.

---

# 17. Phase 12 — Android Controller

Android initially controls Windows and Ubuntu.

Support:

```text
Android → Windows
Android → Ubuntu
```

## Touch mode

```text
Tap
→ Left click

Long press
→ Right click

Two-finger swipe
→ Scroll
```

## Trackpad mode

```text
Swipe
→ Move pointer

Tap
→ Left click

Two-finger tap
→ Right click

Two-finger swipe
→ Scroll
```

Trackpad mode should be the default for desktop-style interaction.

---

# 18. Phase 13 — Wake-on-LAN

M4 feature; not required for the product MVP. Implement after the host lifecycle and unattended permission model are qualified.

## UX

User should see:

```text
Ubuntu Workstation

Sleeping

[ Wake & Connect ]
```

Not:

```text
Send WOL Magic Packet
```

## Flow

```text
User presses Wake & Connect
          ↓
Wake request
          ↓
LAN relay / available device
          ↓
Magic packet
          ↓
Sleeping PC wakes
          ↓
Agent reconnects to server
          ↓
Device becomes Online
          ↓
Authenticate and recheck host consent / unattended grant
          ↓
Remote session starts only when authorized
```

States:

```text
Online
Sleeping
Waking
Connecting
Offline
```

Support WOL for both Windows and Ubuntu when hardware/network configuration permits it.

Presence and wake capability are separate properties:

| State | Required evidence / behavior |
|---|---|
| Online | Authenticated host heartbeat within 30 seconds; send heartbeats every 10 seconds |
| Sleeping | Host announced planned sleep during an authenticated session; show last-confirmed time and expire this indication after 10 minutes |
| Offline / status unknown | Heartbeat expired without a recent sleep announcement, or sleep evidence expired; never infer sleep solely from missing heartbeats |
| Waking | An authorized wake request was accepted by an available helper; wait up to 90 seconds for a fresh host heartbeat |
| Connecting | Host is online and normal authenticated session setup is in progress |

Enable Wake & Connect only after wake capability and a currently available, explicitly trusted helper on the target LAN have been configured and tested. Store the target MAC/interface and broadcast scope as protected device configuration. A missing helper or failed wake probe produces an explanation, not a false success. Expire requests, rate-limit retries, and display timeout after 90 seconds. Waking a machine never bypasses remote-access authorization. On timeout return to Offline / status unknown.

---

# 19. Phase 14 — Unattended Access

M4: allow explicitly authorized trusted devices to connect without someone accepting locally. Identity trust established during pairing is separate from permission for unattended access, which is disabled by default.

Example:

```text
Trusted devices

✓ My Android
✓ Ubuntu Laptop
✓ Windows Laptop
```

Security requirements:

- Device public-key trust
- Optional PIN/password
- Revocation
- Session log
- Optional 2FA for account actions

## Host lifecycle and platform gates

- Split the background service/supervisor from the user-session capture/input worker. Use authenticated local IPC, least privilege, and explicit worker/session ownership.
- Qualify Windows service startup and the per-user worker, and Linux system/user service behavior. Define what happens on reboot, user switch, worker crash, logout, lock, suspend/resume, and portal-session closure.
- For M1–M3, lock/logout or loss of capture/input permission stops streaming and releases input. After unlock or login, require a valid session grant and any OS permission again; never claim login-screen or headless access.
- Elevated Windows UI and secure-desktop control require separate design and qualification. `SendInput` is subject to integrity-level restrictions; installing a service alone does not establish support. See [Microsoft SendInput documentation](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput).
- For Wayland, gate unattended support on the tested compositor/portal lifecycle and permissions. Unsupported combinations remain attended-only with an explicit UI explanation.
- Test reboot recovery, permission loss, trust revocation during a session, stale resume credentials, and local emergency disconnect before enabling unattended access on any platform.

---

# 20. Phase 15 — Adaptive Streaming and Latency Control

M1/M2 already require bounded queues, stale-frame handling, and basic overload adaptation. This phase adds advanced quality tuning and hardware-specific optimizations in M4.

Remote desktop should optimize for **freshness and responsiveness**, not perfect delivery of every frame.

Continuously measure:

```text
RTT
packet loss
jitter
available bandwidth
encode time
decode time
encoder queue depth
decoder queue depth
render delay
```

## Example quality adaptation

Strong connection:

```text
1080p
60 FPS
8 Mbps
```

Medium connection:

```text
1080p
30 FPS
3–5 Mbps
```

Weak connection:

```text
720p
30 FPS
1.5–2.5 Mbps
```

Prioritize responsiveness over image quality.

## Frame dropping

Do not allow a long encode queue.

Bad:

```text
Frame 100
Frame 101
Frame 102
Frame 103
Frame 104

encoder processes every frame
        ↓

beautiful video
but 500 ms behind reality
```

Preferred:

```text
Frame 100
Frame 101
Frame 102
Frame 103
Frame 104

encoder overloaded
        ↓

drop stale frames
        ↓

encode newest useful frame
```

For interactive remote desktop, showing the newest frame is usually more important than preserving every old frame.

## Hardware encoding

Prefer hardware acceleration when available.

Windows:

```text
NVIDIA → NVENC
Intel  → Quick Sync
AMD    → AMF
```

Ubuntu/Linux:

```text
NVIDIA → NVENC
Intel/AMD → VA-API where supported
```

Fallback:

```text
software H.264
```

## Damage-region optimization

Where practical, detect changed screen regions instead of treating every desktop frame as completely new.

This can reduce:

- encoder work
- bandwidth usage
- latency
- battery usage

## Keyframe policy

Avoid generating keyframes too frequently because they create large bandwidth spikes.

Request a new keyframe when:

- decoder loses synchronization
- packet loss damages the current reference chain
- resolution changes
- session resumes after a meaningful interruption

# 21. Phase 16 — Clipboard

Start with text only.

```text
Remote copy
    ↓
Encrypted clipboard event
    ↓
Local clipboard
```

Support:

```text
Windows ↔ Windows
Windows ↔ Ubuntu
Ubuntu ↔ Ubuntu
Android ↔ Windows
Android ↔ Ubuntu
```

Later:

- Images
- File clipboard

---

# 22. Phase 17 — File Transfer

Separate file transfer from video streaming.

Architecture:

```text
Session
├── Control channel
├── Video channel
├── Input channel
└── File channel
```

Features:

- Upload
- Download
- Cancel
- Resume
- Progress
- Integrity verification

---

# 23. Phase 18 — Remote Audio

Pipeline:

```text
Remote system audio
       ↓
Capture
       ↓
Opus encoder
       ↓
Network
       ↓
Opus decoder
       ↓
Local audio output
```

Audio is not required for the first MVP.

---

# 24. Phase 19 — Multiple Monitors

Host announces:

```text
Monitor 1
Monitor 2
Monitor 3
```

Client UI:

```text
Display

✓ Monitor 1
  Monitor 2
  Monitor 3
  All monitors
```

---

# 25. Phase 20 — Production Readiness

This is a release audit, not the first implementation phase for cross-cutting reliability. Encryption, permission enforcement, timeouts, input cleanup, and bounded queues begin in M0/M1; Internet rate limits and authenticated reconnect are M2 requirements.

Before public distribution, qualify signed artifacts and the update delivery process, regression coverage, crash/log privacy, monitoring, relay capacity limits, and backup/restore for deployed state. Account recovery and 2FA apply if accounts are introduced; autoscaling and multi-region deployment depend on measured load. Automatic update installation can follow a tested signed manual-update process.

Audit or add as applicable:

- Auto update
- Crash reporting
- Reconnect
- Connection timeout handling
- Structured logs
- Metrics
- Rate limiting
- Device revocation
- Account recovery
- 2FA
- Server monitoring
- Relay autoscaling
- Database backup
- Abuse protection
- Signed releases

---

# 26. Recommended Development Stages

## Stage 1 — Remote Core (M0–M1)

Build:

```text
Windows → Windows
Windows → Ubuntu
Ubuntu → Windows
Ubuntu → Ubuntu

LAN only
H.264
Mouse
Keyboard
Encrypted authenticated session
Explicit host consent
Bounded queues and input cleanup
```

This is the most important stage.

Do not move to Internet infrastructure until these combinations are stable.

---

## Stage 2 — Internet Connectivity (M2)

Add:

```text
Device ID
Signaling
P2P
NAT traversal
Relay fallback
IPv4/IPv6 candidate racing
TLS/TCP fallback for blocked UDP
Authenticated reconnect
Internet security hardening
```

Target matrix remains:

```text
Windows → Windows
Windows → Ubuntu
Ubuntu → Windows
Ubuntu → Ubuntu
```

---

## Stage 3 — Product Layer (M3 / product MVP)

Add:

```text
Flutter UI
Device list
Android controller
Qualified Wayland attended support
Permission and connection failure UX
```

Full target matrix:

| Controller | Windows | Ubuntu |
|---|---:|---:|
| Windows | ✅ | ✅ |
| Ubuntu | ✅ | ✅ |
| Android | ✅ | ✅ |

---

## Stage 4 — Post-MVP Capabilities (M4)

Add:

```text
Unattended access
Wake-on-LAN
Seamless application-session route switching
Multiple relay regions
Clipboard
File transfer
Audio
Multi-monitor
Advanced adaptive bitrate tuning
Auto-update
```

---

# 27. MVP Definition

The product MVP is M3. M1 is an earlier LAN engineering milestone, and M2 is an Internet desktop beta. The delivery table at the start of this document governs scope.

## Platforms

```text
Windows
Ubuntu
Android controller
```

## Required connection matrix

```text
Windows → Windows
Windows → Ubuntu
Ubuntu → Windows
Ubuntu → Ubuntu
Android → Windows
Android → Ubuntu
```

## Networking

```text
LAN direct
Internet P2P
Relay fallback
IPv4 + IPv6 dual-stack
IPv4/IPv6 connection racing
QUIC / UDP
TLS/TCP relay fallback
first healthy authenticated path at startup
quality measurement and route selection on reconnect
visible bounded reconnect when network changes
```

The MVP connection engine must not hard-code IPv4-first or IPv6-first behavior.

It should:

```text
discover candidates
        ↓
race IPv4 + IPv6
        ↓
start first healthy authorized path
        ↓
measure real quality
        ↓
monitor continuously
        ↓
reconnect safely on path failure
```

## Remote functionality

```text
1080p
30 FPS target
Mouse
Keyboard
One selected display
Attended host consent
```

## Device functionality

```text
Device ID
Online / Offline
Last seen
Connection / reconnect status
```

## Security

```text
Device keys
End-to-end encrypted sessions, including relay fallback
Verified pairing and explicit host consent
Revocation, replay protection, and local disconnect
```

## Not required for first MVP

```text
Unattended access
Wake & Connect
Seamless route switching
Multiple relay regions
Clipboard
Audio
File transfer
Multi-monitor
Chat
Session recording
Enterprise dashboard
Complex organization/user management
```

---

# 28. Testing Matrix

Apply the matrix to the feature set of each release: M1 requires the four desktop LAN rows on X11 hosts; M2 adds all desktop Internet/relay rows and TCP fallback; M3 adds Android and qualified Wayland host sessions. Later feature gates extend this matrix rather than blocking earlier milestones.

| Source | Destination | LAN | Internet P2P | Relay |
|---|---|---:|---:|---:|
| Windows | Windows | ✅ | ✅ | ✅ |
| Windows | Ubuntu | ✅ | ✅ | ✅ |
| Ubuntu | Windows | ✅ | ✅ | ✅ |
| **Ubuntu** | **Ubuntu** | ✅ | ✅ | ✅ |
| Android | Windows | ✅ | ✅ | ✅ |
| Android | Ubuntu | ✅ | ✅ | ✅ |

For every relevant network test, cover:

```text
IPv4 only
IPv6 only
Dual-stack IPv4 + IPv6
Broken/slow IPv6 with healthy IPv4
Broken/slow IPv4 with healthy IPv6
CGNAT IPv4
Direct IPv6
P2P failure → relay fallback
UDP blocked → TLS/TCP relay fallback
Wi-Fi → mobile-network authenticated reconnect (M2/M3)
Seamless route migration (M4)
```

The application should select the best working path without requiring user configuration.

Ubuntu testing should cover:

```text
X11 → X11
X11 → Wayland
Wayland → X11
Wayland → Wayland
```

where supported by the Ubuntu desktop environment and permission model.

Add cases for denied/cancelled permissions, changed peer keys, expired consent, revoked devices, oversized/malformed packets, packet duplication/reordering, host lock/logout, display resize, held keys during disconnect, and reconnect with a stale transport epoch. Test slow and unavailable direct routes, relay failure, both address families failing, and low-bandwidth/high-loss conditions with bounded memory. Record unsupported platform cases explicitly instead of marking them passed.

M3 includes Android touch/trackpad mapping, soft-keyboard/text entry, rotation, app background/foreground, and decoder recovery after network changes. M4 adds sleep/wake helper failures, service restart, and unattended-access lifecycle checks.

## M2/M3 release evidence

For each applicable controller/host pair, run 20 connection attempts on controlled healthy LAN-direct, Internet-direct, relay-UDP, and UDP-blocked relay-TCP profiles. Record bandwidth, RTT, jitter/loss injection, outcome, selected family/transport, and consent-excluded setup time. Require every attempt to succeed within the configured 10-second setup budget once the host is reachable and consent is available. Where topology intentionally prevents direct access, relay success is expected and must not be misreported as a successful direct test.

For each failure scenario, require either successful authenticated fallback/reconnect within 10 seconds after a usable network becomes available or the documented explicit failure for an intentionally unsupported network; no hang or unsafe input state is acceptable. Re-consent, if required, has its own visible 60-second timeout. Inspect relay-side test instrumentation to confirm it handles opaque end-to-end payloads without endpoint decryption keys. Publish the same functional/security evidence for M3 Android and qualified Wayland; unmet mandatory cases block that milestone.

---

# 29. M1 — First Development Target and Acceptance Criteria

The first useful technical milestone is a secure, attended LAN session with no backend dependency:

```text
Windows → Windows
Windows ↔ Ubuntu
Ubuntu ↔ Ubuntu

LAN only

1920 × 1080
30 FPS

Mouse ✅
Keyboard ✅
H.264 ✅

Account ❌
Wake-on-LAN ❌
File transfer ❌
Audio ❌
Clipboard ❌
```

Ubuntu hosting uses the qualified X11 baseline for this milestone. Use one active desktop display and a logged-in host user. A minimal Flutter connection/consent/rendering shell is sufficient; product UI polish is M3.

## Implementation order

1. Complete M0: pinned build manifest, versioned message envelope, authenticated pairing, consent, and typed failure states.
2. Prove Windows capture-to-Flutter rendering with H.264 over an encrypted LAN connection; record capture/codec/render backends and buffer ownership.
3. Add mouse/keyboard, click-position semantics, input leases, local disconnect, and failure cleanup.
4. Implement Ubuntu X11 capture/input and validate all four desktop directions. Run the Wayland permission spike without making it an M1 release gate.
5. Measure the complete pipeline, enforce queue limits, test overload/disconnect/lock, and publish a reproducible acceptance report.

## Definition of Done

Use two physical computers on wired gigabit LAN with measured p95 RTT at most 5 ms, one selected 1920×1080 display per host at at least 60 Hz, and no deliberate loss. Record CPU/GPU/RAM, exact OS/driver versions, codec backend/profile/bitrate, capture API, and client renderer. Software codec fallback must pass functional tests; performance support is claimed only for the tested reference hardware/configuration.

| Gate | Pass condition |
|---|---|
| Connectivity | All four desktop directions establish an authorized session by IP; p95 consent-to-first-frame at most 3 seconds over 20 attempts per direction |
| Video | A fixed 5-minute moving/scrolling workload averages at least 28 rendered FPS at 1080p with a 30 FPS configuration after a 30-second warmup |
| Latency | Capture-to-display p95 at most 100 ms and input-to-visible-response p95 at most 150 ms, measured using section 30 with at least 200 samples per direction |
| Input | Move, both mouse buttons, wheel, text and modifiers work; coordinate tests cover display corners and scaling; no stuck keys/buttons after failures |
| Consent/security | Denied/expired consent, wrong keys, revoked grants, incompatible major versions, and invalid messages cannot enable capture/input |
| Stability | One 30-minute session per desktop direction without a crash; configured frame/queue limits hold. Record host/client process memory after 60 seconds of warmup; within 60 seconds after overload ends, each process settles within baseline plus the larger of 64 MiB or 20% of baseline. Report measured peaks and investigate sustained growth |
| Recovery | Abrupt disconnect releases keys/buttons within 2 seconds; capture permission loss or host lock stops capture/input immediately when detected; a new authorized connection succeeds without restarting either app |
| Overload | Under CPU throttling and an injected 20 ms RTT / 1% loss / 5 Mbps link, queues stay bounded, quality reduces, and input cleanup still works; normal-LAN FPS/latency gates do not apply during this stress case |

Store the manifest, test procedure, latency samples, summary percentiles, sanitized logs, and known limitations with the milestone report. Missing test hardware is an unverified gate, not a pass. Once M1 passes, proceed to M2 signaling/P2P/relay, then the M3 product layer; Wake-on-LAN remains M4.

---

# 30. Performance Targets

Section 29 defines M1 acceptance thresholds. The values below are longer-term optimization targets, not additional MVP promises.

## Measurement definitions

- Network RTT: application probe round trip on the selected route; report p50/p95/p99, loss sample count, and window duration separately from video latency.
- Capture-to-display: elapsed time from host frame acquisition to presentation of that frame at the client. Cross-machine subtraction requires measured clock offset/error; otherwise use an instrumented visual/high-speed-camera method and report its uncertainty. Do not subtract unsynchronized timestamps.
- Input-to-visible-response: elapsed time from the controller receiving an input event to presentation of the corresponding host UI response on the controller. Use a test application that echoes a correlated input marker into the captured frame, measured with the controller's monotonic clock; document that physical device latency is excluded.
- Local pipeline stages: instrument capture, encode, queued time, decode, and presentation using local monotonic clocks. CPU-side texture submission alone must not be reported as physical display presentation; name any proxy measurement explicitly.
- Record warmup, workload, display refresh rate, codec settings, route, hardware, dropped frames, bitrate, queue depths, CPU/GPU usage, and peak memory. Compare runs with the same conditions. M2/M3 acceptance reports define their tested bandwidth/RTT/loss profiles; Internet figures below are stretch targets.

## LAN

```text
Network RTT        < 5–10 ms where network allows
Input-to-visible   < 30 ms stretch target
Video              1080p / 60 FPS on capable hardware
```

## Internet — same region

```text
Network RTT        < 30–50 ms preferred
Input-to-visible   < 70 ms stretch target
Video              1080p / 30–60 FPS depending on bandwidth
```

## Poor networks

Prefer:

```text
lower bitrate
lower resolution
lower FPS
drop stale video frames
```

instead of:

```text
delaying keyboard
delaying mouse
building long video queues
```

The remote session should feel responsive even when image quality must temporarily decrease.

---

# 31. Connect Engine Summary

The M2/M3 connection engine follows this flow; the optional M4 extension is identified explicitly:

```text
User selects device
        ↓
Resolve Device ID
        ↓
Check presence / report offline
        ↓
Exchange authorized candidates
        ↓
Race LAN / P2P IPv4 + IPv6 authenticated handshakes
        ↓
No direct ready after 300 ms? → also race available relay paths
        ↓
UDP unavailable? → authenticated TLS/TCP relay tunnel
        ↓
First healthy path ready within setup budget?
        ├── no → explicit timeout / failure reason
        └── yes
             ↓
Verify peer identity + obtain / revalidate host consent
        ├── denied / expired → close; no capture or input
        └── authorized
             ↓
Start end-to-end encrypted video + input
        ↓
Monitor RTT / loss / jitter and bounded pipeline queues
        ↓
Direct degraded? → probe relay and retain measurements
Path failed? → release input; show Reconnecting; re-race routes
               → validate resume grant or obtain new consent
               → reset media/input epoch and request keyframe

M4 only: switch a live session to a materially better validated
path using quality windows, hysteresis, and cooldown (section 13)
```

User-facing UI remains simple:

```text
Connecting...
```

The user should never need to choose:

```text
IPv4
IPv6
UDP
QUIC
P2P
Relay
```

All routing decisions belong inside `remote-network`; session permissions remain enforced by the core session state machine. M1 connects by entered LAN IP and uses the same identity/consent checks without Device ID lookup.

---

# 32. Core Product Principle

The end user should never need to understand:

```text
IP addresses
Ports
NAT
QUIC
Relay servers
STUN
Wake-on-LAN packets
X11 / Wayland internals
```

The eventual product can present the following simple device list. Sleeping and Wake & Connect appear only after M4 wake qualification; M3 shows Online, Offline/last seen, and connection status. A device name does not imply headless-host support.

```text
My Devices

● Windows PC
  Online
  [ Connect ]

◐ Ubuntu PC
  Sleeping
  [ Wake & Connect ]

● Ubuntu Laptop
  Online
  [ Connect ]
```

The technical complexity stays inside the remote engine.

The user experience stays:

> Find the device → click Connect → work.
