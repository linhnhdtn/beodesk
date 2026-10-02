//! Explicit local smoke check; uses the user's OS credential store.
fn main() -> anyhow::Result<()> {
    let first = remote_bridge::api::app::initialize_device()?;
    let second = remote_bridge::api::app::initialize_device()?;
    anyhow::ensure!(
        first.fingerprint == second.fingerprint,
        "device identity changed on reload"
    );
    println!("OS credential storage: identity persisted and reloaded successfully.");
    Ok(())
}
