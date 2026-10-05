//! Loopback-only native smoke test. Run under Xvfb to capture only a virtual display.
use remote_bridge::api::{app, lan};
use std::{
    net::UdpSocket,
    time::{Duration, Instant},
};

fn main() -> anyhow::Result<()> {
    println!(
        "Loopback snapshot smoke test; local request is explicitly auto-approved by this test."
    );
    let device = app::initialize_device()?;
    let socket = UdpSocket::bind("127.0.0.1:0")?;
    let address = socket.local_addr()?.to_string();
    drop(socket);
    lan::start_snapshot_host(address.clone())?;
    let client_pin = device.fingerprint;
    let client = std::thread::spawn(move || lan::fetch_snapshot(address, client_pin));
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let status = lan::snapshot_host_status()?;
        if status.request_id != 0 {
            lan::respond_to_view_request(status.request_id, true)?;
            break;
        }
        anyhow::ensure!(
            Instant::now() < deadline,
            "No authenticated viewing request received"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    let result = client
        .join()
        .map_err(|_| anyhow::anyhow!("Client worker failed"))?;
    lan::stop_snapshot_host()?;
    let image = result?;
    let (width, height) = remote_capture::validate_png(&image)?;
    std::fs::create_dir_all(".local")?;
    std::fs::write(".local/lan-snapshot.png", image)?;
    println!("Authenticated, consented PNG received over QUIC: {width} x {height}");
    Ok(())
}
