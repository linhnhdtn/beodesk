use remote_core::identity::{DeviceIdentity, IdentityError, IdentityStore};
use remote_network::{
    PeerPin,
    transport::{self, DeviceCertificate},
    wire,
};
use remote_protocol::{PROTOCOL_MAJOR, PROTOCOL_MINOR, wire as proto};
use std::{net::SocketAddr, time::Duration};
use zeroize::Zeroizing;

fn identity() -> DeviceIdentity {
    struct EphemeralTestStore;
    impl IdentityStore for EphemeralTestStore {
        fn load(&self) -> Result<Option<Zeroizing<Vec<u8>>>, IdentityError> {
            Ok(None)
        }
        fn save(&self, _: &[u8]) -> Result<(), IdentityError> {
            Ok(())
        }
    }
    DeviceIdentity::load_or_create(&EphemeralTestStore).unwrap()
}

fn packet() -> proto::Envelope {
    proto::Envelope {
        protocol_major: PROTOCOL_MAJOR,
        protocol_minor: PROTOCOL_MINOR,
        session_id: vec![7; 16],
        transport_epoch: 1,
        sequence: 1,
        body: Some(proto::envelope::Body::Control(proto::SessionControl {
            action: proto::Action::Ping as i32,
        })),
    }
}

#[tokio::test]
async fn two_pinned_devices_exchange_control_over_real_encrypted_udp() {
    let host = identity();
    let client = identity();
    let endpoint = transport::server(
        "127.0.0.1:0".parse().unwrap(),
        &DeviceCertificate::from_identity(&host).unwrap(),
        PeerPin::from_public_key(&client.public_key()),
    )
    .unwrap();
    let address = endpoint.local_addr().unwrap();
    let expected_client = client.public_key();
    let task = tokio::spawn(async move {
        let accepted = transport::accept(&endpoint).await.unwrap();
        assert_eq!(accepted.peer_key, expected_client);
        let (mut send, mut recv) = accepted.connection.accept_bi().await.unwrap();
        let message = wire::read_control(&mut recv).await.unwrap();
        wire::write_control(&mut send, &message).await.unwrap();
        send.finish().unwrap();
        let _ = send.stopped().await;
    });
    let (_endpoint, peer) = transport::connect(
        address,
        &DeviceCertificate::from_identity(&client).unwrap(),
        PeerPin::from_public_key(&host.public_key()),
    )
    .await
    .unwrap();
    assert_eq!(peer.peer_key, host.public_key());
    let (mut send, mut recv) = peer.connection.open_bi().await.unwrap();
    wire::write_control(&mut send, &packet()).await.unwrap();
    send.finish().unwrap();
    assert_eq!(wire::read_control(&mut recv).await.unwrap(), packet());
    task.await.unwrap();
}

#[tokio::test]
async fn wrong_host_pin_is_rejected_during_tls_authentication() {
    let host = identity();
    let client = identity();
    let wrong = identity();
    let endpoint = transport::server_attended(
        "127.0.0.1:0".parse().unwrap(),
        &DeviceCertificate::from_identity(&host).unwrap(),
    )
    .unwrap();
    let address = endpoint.local_addr().unwrap();
    let task = tokio::spawn(async move { transport::accept(&endpoint).await.is_err() });
    assert!(
        transport::connect(
            address,
            &DeviceCertificate::from_identity(&client).unwrap(),
            PeerPin::from_public_key(&wrong.public_key())
        )
        .await
        .is_err()
    );
    assert!(
        tokio::time::timeout(Duration::from_secs(12), task)
            .await
            .unwrap()
            .unwrap()
    );
}

#[tokio::test]
async fn untrusted_controller_is_rejected_even_when_it_trusts_the_host() {
    let host = identity();
    let client = identity();
    let trusted = identity();
    let endpoint = transport::server(
        "127.0.0.1:0".parse().unwrap(),
        &DeviceCertificate::from_identity(&host).unwrap(),
        PeerPin::from_public_key(&trusted.public_key()),
    )
    .unwrap();
    let address = endpoint.local_addr().unwrap();
    let task = tokio::spawn(async move { transport::accept(&endpoint).await.is_err() });
    let result = transport::connect(
        address,
        &DeviceCertificate::from_identity(&client).unwrap(),
        PeerPin::from_public_key(&host.public_key()),
    )
    .await;
    // QUIC client completion may precede the server receiving its client certificate.
    // The server must reject before accepting any application stream.
    if let Ok((_endpoint, peer)) = result {
        assert!(
            peer.connection.closed().await.to_string().contains("error")
                || peer.connection.close_reason().is_some()
        );
    }
    assert!(task.await.unwrap());
}

#[tokio::test]
async fn oversized_length_is_rejected_without_allocating_or_waiting_for_body() {
    let host = identity();
    let client = identity();
    let endpoint = transport::server(
        "127.0.0.1:0".parse().unwrap(),
        &DeviceCertificate::from_identity(&host).unwrap(),
        PeerPin::from_public_key(&client.public_key()),
    )
    .unwrap();
    let address = endpoint.local_addr().unwrap();
    let task = tokio::spawn(async move {
        let accepted = transport::accept(&endpoint).await.unwrap();
        let (_, mut recv) = accepted.connection.accept_bi().await.unwrap();
        assert!(
            wire::read_control(&mut recv)
                .await
                .unwrap_err()
                .to_string()
                .contains("size limit")
        );
    });
    let (_endpoint, peer) = transport::connect(
        address,
        &DeviceCertificate::from_identity(&client).unwrap(),
        PeerPin::from_public_key(&host.public_key()),
    )
    .await
    .unwrap();
    let (mut send, _) = peer.connection.open_bi().await.unwrap();
    send.write_all(&((remote_protocol::MAX_CONTROL_BYTES + 1) as u32).to_be_bytes())
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap();
}

#[test]
fn fingerprint_parser_rejects_short_or_non_hex_pins() {
    let device = identity();
    let pin = PeerPin::from_public_key(&device.public_key());
    assert_eq!(PeerPin::parse(&pin.display()).unwrap(), pin);
    assert!(PeerPin::parse("12345678").is_err());
    assert!(PeerPin::parse(&"G".repeat(64)).is_err());
    let invalid_bind: SocketAddr = "0.0.0.0:4433".parse().unwrap();
    assert!(
        transport::server(
            invalid_bind,
            &DeviceCertificate::from_identity(&device).unwrap(),
            pin
        )
        .is_err()
    );
}
