use anyhow::Result;

pub struct HostStatus {
    pub listening: bool,
    pub address: String,
    pub request_id: u32,
    pub peer_fingerprint: String,
    pub device_name: String,
    pub live: bool,
    pub control: bool,
    pub active: bool,
}

pub fn start_snapshot_host(address: String) -> Result<HostStatus> {
    crate::lan::start_host(&address)
}

pub fn snapshot_host_status() -> Result<HostStatus> {
    crate::lan::host_status()
}
pub fn respond_to_view_request(request_id: u32, approved: bool) -> Result<()> {
    crate::lan::respond(request_id, approved)
}
pub fn stop_snapshot_host() -> Result<()> {
    crate::lan::stop_host()
}
pub fn fetch_snapshot(address: String, host_fingerprint: String) -> Result<Vec<u8>> {
    crate::lan::fetch(&address, &host_fingerprint)
}
pub fn cancel_snapshot_request() -> Result<()> {
    crate::lan::cancel_client()
}

#[derive(Clone)]
pub struct LiveFrame {
    pub frame_id: u32,
    pub width: u32,
    pub height: u32,
    pub closed: bool,
    pub error: String,
}

pub fn start_live_session(address: String, host_fingerprint: String) -> Result<u32> {
    crate::live_client::start(&address, &host_fingerprint)
}

pub fn poll_live_frame(session_id: u32) -> Result<LiveFrame> {
    crate::live_client::poll(session_id)
}

/// kind: 0 motion, 1 button, 2 key, 3 wheel, 4 release held input.
pub fn send_live_input(
    session_id: u32,
    kind: u8,
    x: f32,
    y: f32,
    code: u32,
    pressed: bool,
    delta: i32,
) -> Result<()> {
    crate::live_client::input(session_id, kind, x, y, code, pressed, delta)
}

pub fn stop_live_session(session_id: u32) -> Result<()> {
    crate::live_client::stop(session_id)
}
