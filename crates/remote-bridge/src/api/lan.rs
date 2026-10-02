use anyhow::Result;

pub struct HostStatus {
    pub listening: bool,
    pub address: String,
    pub request_id: u32,
    pub peer_fingerprint: String,
    pub device_name: String,
}

pub fn start_snapshot_host(address: String, peer_fingerprint: String) -> Result<HostStatus> {
    crate::lan::start_host(&address, &peer_fingerprint)
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
