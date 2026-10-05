use crate::PeerPin;
use rustls::{
    DigitallySignedStruct, DistinguishedName, Error, SignatureScheme,
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    pki_types::{CertificateDer, ServerName, UnixTime},
    server::danger::{ClientCertVerified, ClientCertVerifier},
};
use std::sync::Arc;
use x509_parser::prelude::{FromDer, X509Certificate};

#[derive(Debug)]
pub(crate) struct PinnedVerifier {
    pub pin: PeerPin,
    pub provider: Arc<rustls::crypto::CryptoProvider>,
}

pub(crate) fn certificate_key(cert: &CertificateDer<'_>) -> Result<[u8; 32], Error> {
    let (remaining, parsed) = X509Certificate::from_der(cert.as_ref())
        .map_err(|_| Error::General("Invalid peer certificate".into()))?;
    if !remaining.is_empty()
        || parsed.public_key().algorithm.algorithm.to_id_string() != "1.3.101.112"
        || parsed.public_key().subject_public_key.unused_bits != 0
    {
        return Err(Error::General(
            "Expected an Ed25519 device certificate".into(),
        ));
    }
    parsed
        .public_key()
        .subject_public_key
        .data
        .as_ref()
        .try_into()
        .map_err(|_| Error::General("Invalid Ed25519 public key length".into()))
}

// The host accepts a cryptographically proven device identity before asking
// its local user for consent. An optional pin is retained for restricted callers.
#[derive(Debug)]
pub(crate) struct ClientVerifier {
    pub pin: Option<PeerPin>,
    pub provider: Arc<rustls::crypto::CryptoProvider>,
}

impl PinnedVerifier {
    fn verify_pin(
        &self,
        cert: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        now: UnixTime,
    ) -> Result<(), Error> {
        verify_certificate(cert, intermediates, now, Some(self.pin))
    }

    fn verify_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
        tls13: bool,
    ) -> Result<HandshakeSignatureValid, Error> {
        // The reviewed TLS implementation verifies possession of the pinned key.
        if tls13 {
            rustls::crypto::verify_tls13_signature(
                message,
                cert,
                signature,
                &self.provider.signature_verification_algorithms,
            )
        } else {
            rustls::crypto::verify_tls12_signature(
                message,
                cert,
                signature,
                &self.provider.signature_verification_algorithms,
            )
        }
    }
}

fn verify_certificate(
    cert: &CertificateDer<'_>,
    intermediates: &[CertificateDer<'_>],
    now: UnixTime,
    pin: Option<PeerPin>,
) -> Result<(), Error> {
    if !intermediates.is_empty() || cert.len() > 16 * 1024 {
        return Err(Error::General("Unexpected device certificate chain".into()));
    }
    let key = certificate_key(cert)?;
    if pin.is_some_and(|pin| PeerPin::from_public_key(&key) != pin) {
        return Err(Error::General(
            "Peer device fingerprint does not match the verified pin".into(),
        ));
    }
    let (_, parsed) = X509Certificate::from_der(cert.as_ref())
        .map_err(|_| Error::General("Invalid peer certificate".into()))?;
    let current =
        i64::try_from(now.as_secs()).map_err(|_| Error::General("Invalid system time".into()))?;
    if current < parsed.validity().not_before.timestamp()
        || current > parsed.validity().not_after.timestamp()
    {
        return Err(Error::General(
            "Peer device certificate is not valid at current time".into(),
        ));
    }
    Ok(())
}

impl ServerCertVerifier for PinnedVerifier {
    fn verify_server_cert(
        &self,
        cert: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        // Device identity is the out-of-band verified fingerprint, not a DNS name.
        self.verify_pin(cert, intermediates, now)?;
        Ok(ServerCertVerified::assertion())
    }
    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        self.verify_signature(message, cert, signature, false)
    }
    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        self.verify_signature(message, cert, signature, true)
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![SignatureScheme::ED25519]
    }
}

impl ClientCertVerifier for ClientVerifier {
    fn offer_client_auth(&self) -> bool {
        true
    }
    fn client_auth_mandatory(&self) -> bool {
        true
    }
    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &[]
    }
    fn verify_client_cert(
        &self,
        cert: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        now: UnixTime,
    ) -> Result<ClientCertVerified, Error> {
        verify_certificate(cert, intermediates, now, self.pin)?;
        Ok(ClientCertVerified::assertion())
    }
    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            signature,
            &self.provider.signature_verification_algorithms,
        )
    }
    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            signature,
            &self.provider.signature_verification_algorithms,
        )
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![SignatureScheme::ED25519]
    }
}
