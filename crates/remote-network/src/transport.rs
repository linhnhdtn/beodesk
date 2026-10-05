use crate::{
    PeerPin,
    verifier::{ClientVerifier, PinnedVerifier, certificate_key},
};
use anyhow::{Context, Result, ensure};
use quinn::{
    Endpoint,
    crypto::rustls::{QuicClientConfig, QuicServerConfig},
};
use remote_core::identity::DeviceIdentity;
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};
use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};

const ALPN: &[u8] = b"beodesk-lan-snapshot/1";
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

pub struct DeviceCertificate {
    cert: CertificateDer<'static>,
    key: PrivatePkcs8KeyDer<'static>,
}

impl DeviceCertificate {
    pub fn from_identity(identity: &DeviceIdentity) -> Result<Self> {
        let secret = identity.tls_key_der()?;
        let key = PrivatePkcs8KeyDer::from(secret.to_vec());
        let key_pair = rcgen::KeyPair::from_pkcs8_der_and_sign_algo(&key, &rcgen::PKCS_ED25519)?;
        let mut params = rcgen::CertificateParams::new(vec!["beodesk.local".into()])?;
        params.not_before = time::OffsetDateTime::now_utc() - time::Duration::days(1);
        params.not_after = time::OffsetDateTime::now_utc() + time::Duration::days(30);
        let cert = params.self_signed(&key_pair)?.der().clone();
        Ok(Self { cert, key })
    }
}

pub struct AuthenticatedConnection {
    pub connection: quinn::Connection,
    pub peer_key: [u8; 32],
    pub peer_pin: PeerPin,
}

fn authenticated(connection: quinn::Connection) -> Result<AuthenticatedConnection> {
    let identity = connection
        .peer_identity()
        .context("Transport did not authenticate the peer")?
        .downcast::<Vec<CertificateDer<'static>>>()
        .map_err(|_| anyhow::anyhow!("Unexpected peer identity type"))?;
    ensure!(identity.len() == 1, "Unexpected peer certificate chain");
    let peer_key = certificate_key(&identity[0])?;
    Ok(AuthenticatedConnection {
        connection,
        peer_key,
        peer_pin: PeerPin::from_public_key(&peer_key),
    })
}

fn transport_config() -> Arc<quinn::TransportConfig> {
    let mut config = quinn::TransportConfig::default();
    config.max_concurrent_bidi_streams(1_u32.into());
    config.max_concurrent_uni_streams(0_u32.into());
    config.max_idle_timeout(Some(
        Duration::from_secs(75)
            .try_into()
            .expect("valid idle timeout"),
    ));
    config.keep_alive_interval(Some(Duration::from_secs(5)));
    config.receive_window((16_u32 * 1024 * 1024).into());
    config.stream_receive_window((8_u32 * 1024 * 1024 + 65536).into());
    Arc::new(config)
}

pub fn server(
    bind: SocketAddr,
    certificate: &DeviceCertificate,
    peer: PeerPin,
) -> Result<Endpoint> {
    server_with_peer(bind, certificate, Some(peer))
}

/// Authenticate any presented Ed25519 client key; local consent in live/snapshot
/// is still required before opening capture or injecting input.
pub fn server_attended(bind: SocketAddr, certificate: &DeviceCertificate) -> Result<Endpoint> {
    server_with_peer(bind, certificate, None)
}

fn server_with_peer(
    bind: SocketAddr,
    certificate: &DeviceCertificate,
    peer: Option<PeerPin>,
) -> Result<Endpoint> {
    ensure!(
        !bind.ip().is_unspecified()
            && !bind.ip().is_multicast()
            && bind.ip() != IpAddr::V4(Ipv4Addr::BROADCAST),
        "Select a specific local interface address"
    );
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut tls = rustls::ServerConfig::builder_with_provider(provider.clone())
        .with_protocol_versions(&[&rustls::version::TLS13])?
        .with_client_cert_verifier(Arc::new(ClientVerifier {
            pin: peer,
            provider,
        }))
        .with_single_cert(
            vec![certificate.cert.clone()],
            certificate.key.clone_key().into(),
        )?;
    tls.alpn_protocols = vec![ALPN.to_vec()];
    tls.max_early_data_size = 0;
    let mut config = quinn::ServerConfig::with_crypto(Arc::new(QuicServerConfig::try_from(tls)?));
    config.transport_config(transport_config());
    config.max_incoming(8);
    config.incoming_buffer_size(64 * 1024);
    config.incoming_buffer_size_total(512 * 1024);
    Ok(Endpoint::server(config, bind)?)
}

pub async fn accept(endpoint: &Endpoint) -> Result<AuthenticatedConnection> {
    let incoming = endpoint.accept().await.context("Listener stopped")?;
    let connection = tokio::time::timeout(HANDSHAKE_TIMEOUT, incoming)
        .await
        .context("Peer authentication timed out")??;
    authenticated(connection)
}

/// The returned endpoint must live as long as the authenticated connection.
pub async fn connect(
    address: SocketAddr,
    certificate: &DeviceCertificate,
    peer: PeerPin,
) -> Result<(Endpoint, AuthenticatedConnection)> {
    ensure!(
        address.port() != 0 && !address.ip().is_unspecified() && !address.ip().is_multicast(),
        "Invalid remote endpoint"
    );
    let bind = SocketAddr::new(
        if address.is_ipv4() {
            IpAddr::V4(Ipv4Addr::UNSPECIFIED)
        } else {
            IpAddr::V6(Ipv6Addr::UNSPECIFIED)
        },
        0,
    );
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut tls = rustls::ClientConfig::builder_with_provider(provider.clone())
        .with_protocol_versions(&[&rustls::version::TLS13])?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(PinnedVerifier {
            pin: peer,
            provider,
        }))
        .with_client_auth_cert(
            vec![certificate.cert.clone()],
            certificate.key.clone_key().into(),
        )?;
    tls.alpn_protocols = vec![ALPN.to_vec()];
    tls.enable_early_data = false;
    let mut config = quinn::ClientConfig::new(Arc::new(QuicClientConfig::try_from(tls)?));
    config.transport_config(transport_config());
    let mut endpoint = Endpoint::client(bind)?;
    endpoint.set_default_client_config(config);
    let connection = tokio::time::timeout(
        HANDSHAKE_TIMEOUT,
        endpoint.connect(address, "beodesk.local")?,
    )
    .await
    .context("Peer authentication timed out")??;
    Ok((endpoint, authenticated(connection)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use remote_core::identity::{IdentityError, IdentityStore};
    use zeroize::Zeroizing;

    #[tokio::test]
    async fn attended_listener_still_requires_a_tls_client_certificate() {
        struct Store;
        impl IdentityStore for Store {
            fn load(&self) -> Result<Option<Zeroizing<Vec<u8>>>, IdentityError> {
                Ok(None)
            }
            fn save(&self, _: &[u8]) -> Result<(), IdentityError> {
                Ok(())
            }
        }
        let host = DeviceIdentity::load_or_create(&Store).unwrap();
        let server = server_attended(
            "127.0.0.1:0".parse().unwrap(),
            &DeviceCertificate::from_identity(&host).unwrap(),
        )
        .unwrap();
        let address = server.local_addr().unwrap();
        let accepting = tokio::spawn(async move { accept(&server).await.is_err() });
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let mut tls = rustls::ClientConfig::builder_with_provider(provider.clone())
            .with_protocol_versions(&[&rustls::version::TLS13])
            .unwrap()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(PinnedVerifier {
                pin: PeerPin::from_public_key(&host.public_key()),
                provider,
            }))
            .with_no_client_auth();
        tls.alpn_protocols = vec![ALPN.to_vec()];
        let mut endpoint = Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        endpoint.set_default_client_config(quinn::ClientConfig::new(Arc::new(
            QuicClientConfig::try_from(tls).unwrap(),
        )));
        if let Ok(connection) = endpoint.connect(address, "beodesk.local").unwrap().await {
            // Client completion may precede the server's certificate requirement.
            tokio::time::timeout(Duration::from_secs(3), connection.closed())
                .await
                .unwrap();
        }
        assert!(
            tokio::time::timeout(Duration::from_secs(3), accepting)
                .await
                .unwrap()
                .unwrap()
        );
    }
}
