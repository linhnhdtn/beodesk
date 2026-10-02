use remote_core::identity::{DeviceIdentity, IdentityError, IdentityStore};
use remote_network::{
    PeerPin, snapshot,
    transport::{self, DeviceCertificate},
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use zeroize::Zeroizing;

fn identity() -> DeviceIdentity {
    struct Store;
    impl IdentityStore for Store {
        fn load(&self) -> Result<Option<Zeroizing<Vec<u8>>>, IdentityError> {
            Ok(None)
        }
        fn save(&self, _: &[u8]) -> Result<(), IdentityError> {
            Ok(())
        }
    }
    DeviceIdentity::load_or_create(&Store).unwrap()
}

#[tokio::test]
async fn denial_sends_no_screen_bytes_and_never_calls_capture() {
    let host = identity();
    let client = identity();
    let endpoint = transport::server(
        "127.0.0.1:0".parse().unwrap(),
        &DeviceCertificate::from_identity(&host).unwrap(),
        PeerPin::from_public_key(&client.public_key()),
    )
    .unwrap();
    let address = endpoint.local_addr().unwrap();
    let captured = Arc::new(AtomicUsize::new(0));
    let count = captured.clone();
    let task = tokio::spawn(async move {
        let peer = transport::accept(&endpoint).await.unwrap();
        snapshot::serve(
            peer,
            |_| async { false },
            move || {
                count.fetch_add(1, Ordering::SeqCst);
                Ok(vec![1])
            },
        )
        .await
        .unwrap();
    });
    let result = snapshot::fetch(
        address,
        &client,
        PeerPin::from_public_key(&host.public_key()),
    )
    .await;
    assert!(result.unwrap_err().to_string().contains("denied"));
    task.await.unwrap();
    assert_eq!(captured.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn approved_request_delivers_exact_snapshot_once() {
    let host = identity();
    let client = identity();
    let endpoint = transport::server(
        "127.0.0.1:0".parse().unwrap(),
        &DeviceCertificate::from_identity(&host).unwrap(),
        PeerPin::from_public_key(&client.public_key()),
    )
    .unwrap();
    let address = endpoint.local_addr().unwrap();
    let expected_pin = PeerPin::from_public_key(&client.public_key()).display();
    let task = tokio::spawn(async move {
        let peer = transport::accept(&endpoint).await.unwrap();
        snapshot::serve(
            peer,
            move |request| async move {
                assert_eq!(request.peer_fingerprint, expected_pin);
                true
            },
            || Ok(b"fixture snapshot".to_vec()),
        )
        .await
        .unwrap();
    });
    let bytes = snapshot::fetch(
        address,
        &client,
        PeerPin::from_public_key(&host.public_key()),
    )
    .await
    .unwrap();
    assert_eq!(bytes, b"fixture snapshot");
    task.await.unwrap();
}

#[tokio::test]
async fn disconnected_viewer_cancels_pending_consent_without_capture() {
    let host = identity();
    let client = identity();
    let endpoint = transport::server(
        "127.0.0.1:0".parse().unwrap(),
        &DeviceCertificate::from_identity(&host).unwrap(),
        PeerPin::from_public_key(&client.public_key()),
    )
    .unwrap();
    let address = endpoint.local_addr().unwrap();
    let host_pin = PeerPin::from_public_key(&host.public_key());
    let captured = Arc::new(AtomicUsize::new(0));
    let count = captured.clone();
    let (ready, pending) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let peer = transport::accept(&endpoint).await.unwrap();
        let _ = snapshot::serve(
            peer,
            move |_| async move {
                ready.send(()).unwrap();
                std::future::pending::<bool>().await
            },
            move || {
                count.fetch_add(1, Ordering::SeqCst);
                Ok(vec![1])
            },
        )
        .await;
    });
    let viewer = tokio::spawn(async move { snapshot::fetch(address, &client, host_pin).await });
    tokio::time::timeout(std::time::Duration::from_secs(3), pending)
        .await
        .unwrap()
        .unwrap();
    viewer.abort();
    let _ = viewer.await;
    tokio::time::timeout(std::time::Duration::from_secs(3), server)
        .await
        .expect("disconnect must release host consent without waiting 60 seconds")
        .unwrap();
    assert_eq!(captured.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn wrong_viewer_pin_fails_with_connection_diagnostics_before_consent() {
    let host = identity();
    let viewer = identity();
    let other_viewer = identity();
    let endpoint = transport::server(
        "127.0.0.1:0".parse().unwrap(),
        &DeviceCertificate::from_identity(&host).unwrap(),
        PeerPin::from_public_key(&other_viewer.public_key()),
    )
    .unwrap();
    let address = endpoint.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let error = transport::accept(&endpoint)
            .await
            .err()
            .expect("a viewer with the wrong key must not reach consent");
        let details = format!("{error:#}");
        assert!(details.contains("fingerprint does not match"), "{details}");
    });
    let error = snapshot::fetch(
        address,
        &viewer,
        PeerPin::from_public_key(&host.public_key()),
    )
    .await
    .unwrap_err();
    let details = format!("{error:#}");
    assert!(
        error.chain().count() > 1,
        "connection failure must retain its underlying cause: {details}"
    );
    assert!(details.contains("fingerprint does not match"), "{details}");
    server.await.unwrap();
}
