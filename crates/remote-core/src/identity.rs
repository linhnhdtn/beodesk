use ed25519_dalek::SigningKey;
use ed25519_dalek::pkcs8::EncodePrivateKey;
use rand::rngs::OsRng;
use sha2::{Digest, Sha256};
use thiserror::Error;
use zeroize::Zeroizing;

#[derive(Debug, Error)]
pub enum IdentityError {
    #[error("secure device storage unavailable: {0}")]
    Storage(String),
    #[error("stored device key has an invalid length; refusing to replace identity")]
    CorruptKey,
}

/// Implementations must protect the key at rest and must not log its contents.
pub trait IdentityStore {
    fn load(&self) -> Result<Option<Zeroizing<Vec<u8>>>, IdentityError>;
    fn save(&self, secret: &[u8]) -> Result<(), IdentityError>;
}

// Deliberately no Debug/Serialize implementation for private identity material.
pub struct DeviceIdentity {
    key: SigningKey,
}

impl DeviceIdentity {
    /// Caller serializes initialization so two app instances cannot rotate keys.
    pub fn load_or_create(store: &impl IdentityStore) -> Result<Self, IdentityError> {
        let key = match store.load()? {
            Some(secret) => {
                let bytes: &[u8; 32] = secret
                    .as_slice()
                    .try_into()
                    .map_err(|_| IdentityError::CorruptKey)?;
                SigningKey::from_bytes(bytes)
            }
            None => {
                let key = SigningKey::generate(&mut OsRng);
                let bytes = Zeroizing::new(key.to_bytes());
                store.save(bytes.as_ref())?;
                key
            }
        };
        Ok(Self { key })
    }

    pub fn public_key(&self) -> [u8; 32] {
        self.key.verifying_key().to_bytes()
    }

    /// Rust-only export for the reviewed TLS implementation. Never expose through FFI.
    pub fn tls_key_der(&self) -> Result<Zeroizing<Vec<u8>>, IdentityError> {
        let document = self
            .key
            .to_pkcs8_der()
            .map_err(|error| IdentityError::Storage(error.to_string()))?;
        Ok(Zeroizing::new(document.as_bytes().to_vec()))
    }

    /// Full SHA-256 fingerprint, not the future server-assigned numeric Device ID.
    pub fn fingerprint(&self) -> String {
        let digest = Sha256::digest(self.public_key());
        digest
            .chunks(4)
            .map(|chunk| chunk.iter().map(|b| format!("{b:02X}")).collect::<String>())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[derive(Default)]
    struct MemoryStore(RefCell<Option<Vec<u8>>>);
    impl IdentityStore for MemoryStore {
        fn load(&self) -> Result<Option<Zeroizing<Vec<u8>>>, IdentityError> {
            Ok(self.0.borrow().clone().map(Zeroizing::new))
        }
        fn save(&self, key: &[u8]) -> Result<(), IdentityError> {
            *self.0.borrow_mut() = Some(key.to_vec());
            Ok(())
        }
    }

    #[test]
    fn identity_is_stable_across_reloads() {
        let store = MemoryStore::default();
        let first = DeviceIdentity::load_or_create(&store).unwrap();
        let second = DeviceIdentity::load_or_create(&store).unwrap();
        assert_eq!(first.public_key(), second.public_key());
        assert_eq!(first.fingerprint().len(), 71);
        let other = DeviceIdentity::load_or_create(&MemoryStore::default()).unwrap();
        assert_ne!(first.public_key(), other.public_key());
    }

    #[test]
    fn corrupt_keys_are_never_silently_regenerated() {
        let store = MemoryStore(RefCell::new(Some(vec![9; 10])));
        assert!(matches!(
            DeviceIdentity::load_or_create(&store),
            Err(IdentityError::CorruptKey)
        ));
        assert_eq!(store.0.borrow().as_ref().unwrap().len(), 10);
    }

    #[test]
    fn storage_failure_fails_closed() {
        struct Locked;
        impl IdentityStore for Locked {
            fn load(&self) -> Result<Option<Zeroizing<Vec<u8>>>, IdentityError> {
                Ok(None)
            }
            fn save(&self, _: &[u8]) -> Result<(), IdentityError> {
                Err(IdentityError::Storage("locked".into()))
            }
        }
        assert!(DeviceIdentity::load_or_create(&Locked).is_err());
    }
}
