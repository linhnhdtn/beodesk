#[cfg(any(target_os = "linux", target_os = "windows"))]
use anyhow::Context;
use anyhow::Result;
use remote_core::identity::DeviceIdentity;

#[cfg(any(target_os = "linux", target_os = "windows"))]
pub fn load_device() -> Result<DeviceIdentity> {
    use directories::ProjectDirs;
    use fs2::FileExt;
    use remote_core::identity::{IdentityError, IdentityStore};
    use std::fs::{self, OpenOptions};
    use zeroize::Zeroizing;

    struct CredentialStore(keyring::Entry);
    impl IdentityStore for CredentialStore {
        fn load(&self) -> Result<Option<Zeroizing<Vec<u8>>>, IdentityError> {
            match self.0.get_secret() {
                Ok(secret) => Ok(Some(Zeroizing::new(secret))),
                Err(keyring::Error::NoEntry) => Ok(None),
                Err(error) => Err(IdentityError::Storage(error.to_string())),
            }
        }
        fn save(&self, secret: &[u8]) -> Result<(), IdentityError> {
            self.0
                .set_secret(secret)
                .map_err(|error| IdentityError::Storage(error.to_string()))
        }
    }

    let dirs = ProjectDirs::from("com", "beodesk", "BeoDesk")
        .context("Cannot locate app data directory")?;
    fs::create_dir_all(dirs.data_local_dir()).context("Cannot create app data directory")?;
    // The file contains no secret. The OS releases the lock if a process crashes.
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(dirs.data_local_dir().join("identity.lock"))?;
    lock.lock_exclusive()
        .context("Cannot lock device identity initialization")?;
    let entry = keyring::Entry::new("com.beodesk.desktop", "device-identity-v1")?;
    let identity = DeviceIdentity::load_or_create(&CredentialStore(entry))?;
    Ok(identity)
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
pub fn load_device() -> Result<DeviceIdentity> {
    anyhow::bail!("Secure device storage is not implemented for this platform yet")
}
