# ADR 0002: Mutually pinned QUIC and one consented snapshot

Status: accepted for the LAN prototype; not M1 completion.

## Authentication

Use Quinn 0.11 and Rustls 0.23 with the ring crypto provider, pinned by the workspace lockfile. TLS 1.3 proves possession of each device's persistent Ed25519 key; the same key generates a short-lived self-signed X.509 certificate. ALPN is `beodesk-lan-snapshot/1`; 0-RTT is disabled. There is no application-defined key exchange or plaintext transport.

The custom verifier requires a single bounded certificate, an Ed25519 SPKI key of 32 bytes, current certificate validity and SHA-256 of that public key equal to the complete user-configured fingerprint. DNS/CA identity is replaced by this explicit out-of-band device pin. Crucially, Rustls still verifies the CertificateVerify signature using its crypto provider; pin matching alone never supplies a signature-verification assertion. See the [Rustls server verifier contract](https://docs.rs/rustls/0.23.45/rustls/client/danger/trait.ServerCertVerifier.html) and [client verifier contract](https://docs.rs/rustls/0.23.45/rustls/server/danger/trait.ClientCertVerifier.html).

Client certificates are mandatory. The public key repeated in Protobuf must equal the authenticated TLS peer key; it is not used as authentication evidence by itself. The host only binds an explicitly entered unicast interface IP after verifying its desktop is unlocked. Both peers must have previously compared their full fingerprints through a trusted channel. Persisted peer trust, revocation UX and independent security review remain release gates.

## Consent and framing

Run one request on one reliable bidirectional QUIC stream: bounded Protobuf handshake, bounded approval/denial status, then at most one PNG. Validate every 4-byte big-endian length prefix before allocating: 64 KiB for control and 8 MiB for PNG. The receiver fully decodes the bounded RGBA8 PNG before handing it to Flutter. This wire profile is for a snapshot only; it does not replace the planned video datagram format.

The existing host session state machine supplies a 60-second local-consent deadline and 2-second capture lease. A disconnected viewer cancels pending consent promptly. Capture is invoked only after local approval, with authorization/connection checks after encoding. A locked or unverifiable GNOME desktop is rejected. The capture closure is never invoked on denied consent or disconnect while awaiting consent. Reliable stream I/O has a 10-second timeout and the client waits up to 65 seconds for the consent response.

Pending approval and cancellation are process-local Rust state, exposed through polling APIs to Flutter. A request can be answered once; stale request IDs fail closed. Closing the host aborts its worker and closes the QUIC endpoint. No input injection, persistent unattended grant or automatic retry is introduced.

## Validation and follow-up

Real localhost UDP tests exercise two pinned ephemeral devices, rejection of either incorrect pin, framing limits, one approved snapshot, denied capture and cancellation on disconnect. A separate native smoke test uses the actual GNOME identity/lock APIs and captures only Xvfb. These prove the prototype path on the current Ubuntu host, not performance or cross-platform compatibility.

Next add platform capture adapters, H.264/texture rendering, recurring authorized-session lease and input cleanup integration. Keep QUIC pin authentication and consent enforcement in that path; do not treat successful snapshot viewing as a grant for control.
