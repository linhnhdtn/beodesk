# ADR 0003: Host fingerprint plus local consent

Status: implemented for attended Linux sessions, 2026-10-05.

The controller previously had to send its fingerprint back to the sharing machine before the sharing listener could start. This duplicated pairing work. The application now asks the controller only for the sharing machine's IP and full fingerprint. A bare IP uses port 4433. The sharing machine starts without a controller pin and approves each request locally.

The viewer's `PinnedVerifier` continues to require the exact host fingerprint. The host's `ClientVerifier` still mandates a single bounded, currently valid Ed25519 certificate and verifies TLS CertificateVerify through Rustls. In attended mode it does not compare that client key to a preconfigured fingerprint. The request identity shown in the consent dialog comes from the proven TLS peer key, and application handshakes must match that key. A device name is self-reported, not a verified human identity.

`server_attended` makes this mode explicit; the restricted `server` entry point retains client pin checking for existing transport callers and tests. The bridge uses attended mode for live video and snapshots. HostSession binds the approved session to the authenticated requesting key. No capture or input controller opens until local consent; denied, cancelled and expired requests grant no access. Input leases, disconnect cleanup and lock checks are unchanged. There is no automatic approval, persisted viewer allowlist or unattended access.

The host UI removes the viewer fingerprint field, paste button and configured-viewer status. It automatically detects its own LAN address, offers its own connection details before and after starting sharing, and shows the requesting device fingerprint at consent. Errors from a legacy host that still requires a viewer pin direct users to update that host.

Validation covers first-time viewers without a configured pin, no desktop access while consent is pending, denial, view-only restrictions, missing TLS client certificate, incorrect host pin, input cleanup, bare IPv4/IPv6 default ports and the simplified Flutter flow. Native integration tests exercise actual consent and H.264 texture rendering on Xvfb. Cross-machine acceptance remains a separate check.

Final local validation: 40 Rust tests, 36 Flutter widget tests, clean Clippy/Flutter analysis, live GUI integration, snapshot GUI integration, release native XTEST probe and Linux release build passed. The native probe used a private 1080p Xvfb display; no two-machine result is claimed.
