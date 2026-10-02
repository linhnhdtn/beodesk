//! Authenticated LAN transport. No host screen access is authorized by a handshake.
pub mod snapshot;
pub mod transport;
mod verifier;
pub mod wire;

use anyhow::{Result, ensure};
use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PeerPin([u8; 32]);

impl PeerPin {
    pub fn from_public_key(key: &[u8; 32]) -> Self {
        Self(Sha256::digest(key).into())
    }

    pub fn parse(text: &str) -> Result<Self> {
        // Only whitespace separators are accepted; no shortened fingerprints.
        let hex: String = text.chars().filter(|c| !c.is_ascii_whitespace()).collect();
        ensure!(
            hex.len() == 64 && hex.is_ascii(),
            "Fingerprint must contain all 64 hexadecimal digits"
        );
        let mut digest = [0; 32];
        for (index, byte) in digest.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16)
                .map_err(|_| anyhow::anyhow!("Fingerprint contains invalid hexadecimal digits"))?;
        }
        Ok(Self(digest))
    }

    pub fn display(&self) -> String {
        self.0
            .chunks(4)
            .map(|chunk| chunk.iter().map(|b| format!("{b:02X}")).collect::<String>())
            .collect::<Vec<_>>()
            .join(" ")
    }
}
