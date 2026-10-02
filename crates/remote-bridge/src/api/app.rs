use anyhow::Result;

pub struct EngineInfo {
    pub version: String,
    pub platform: String,
    pub architecture: String,
    pub protocol_major: u32,
    pub protocol_minor: u32,
    pub can_connect: bool,
    pub can_host: bool,
}

pub struct DeviceInfo {
    pub fingerprint: String,
    pub storage_description: String,
}

#[flutter_rust_bridge::frb(sync)]
pub fn engine_info() -> EngineInfo {
    EngineInfo {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        platform: std::env::consts::OS.to_owned(),
        architecture: std::env::consts::ARCH.to_owned(),
        protocol_major: remote_protocol::PROTOCOL_MAJOR,
        protocol_minor: remote_protocol::PROTOCOL_MINOR,
        can_connect: true,
        can_host: cfg!(target_os = "linux")
            && std::env::var("XDG_SESSION_TYPE").as_deref() == Ok("x11"),
    }
}

/// Runs off the UI thread; the OS credential store may request an unlock.
pub fn initialize_device() -> Result<DeviceInfo> {
    let identity = crate::storage::load_device()?;
    Ok(DeviceInfo {
        fingerprint: identity.fingerprint(),
        storage_description: if cfg!(target_os = "linux") {
            "GNOME Secret Service".into()
        } else {
            "Windows Credential Manager".into()
        },
    })
}

#[flutter_rust_bridge::frb(init)]
pub fn init_app() {
    flutter_rust_bridge::setup_default_user_utils();
}
