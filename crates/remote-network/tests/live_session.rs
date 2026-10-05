use anyhow::{Result, ensure};
use remote_core::identity::{DeviceIdentity, IdentityError, IdentityStore};
use remote_network::{
    PeerPin,
    live::{self, Command, Desktop, Viewer},
    transport::{self, DeviceCertificate},
};
use remote_protocol::wire::{InputEvent, Key, input_event::Event};
use std::{
    collections::BTreeSet,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::{sync::mpsc, task::JoinHandle};
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

#[derive(Default)]
struct State {
    opened: AtomicUsize,
    captures: AtomicUsize,
    closed: AtomicBool,
    locked: AtomicBool,
    held: Mutex<BTreeSet<u32>>,
    events: Mutex<Vec<InputEvent>>,
}
struct FakeDesktop(Arc<State>);
impl Desktop for FakeDesktop {
    fn capture(&self) -> Result<Vec<u8>> {
        self.check_permission()?;
        let sequence = self.0.captures.fetch_add(1, Ordering::SeqCst);
        Ok(vec![(sequence % 256) as u8])
    }
    fn check_permission(&self) -> Result<()> {
        ensure!(!self.0.locked.load(Ordering::SeqCst), "Desktop locked");
        Ok(())
    }
    fn inject(&self, input: InputEvent) -> Result<()> {
        self.0.events.lock().unwrap().push(input);
        if let Some(Event::Key(key)) = input.event {
            let mut keys = self.0.held.lock().unwrap();
            if key.pressed {
                keys.insert(key.hid_usage);
            } else {
                keys.remove(&key.hid_usage);
            }
        }
        Ok(())
    }
    fn release(&self) {
        self.0.held.lock().unwrap().clear();
    }
    fn close(&self) {
        self.release();
        self.0.closed.store(true, Ordering::SeqCst);
    }
}

async fn open(
    approve: bool,
    control: bool,
) -> (Result<Viewer>, Arc<State>, JoinHandle<Result<()>>) {
    let host = identity();
    let client = identity();
    let endpoint = transport::server_attended(
        "127.0.0.1:0".parse().unwrap(),
        &DeviceCertificate::from_identity(&host).unwrap(),
    )
    .unwrap();
    let address = endpoint.local_addr().unwrap();
    let state = Arc::new(State::default());
    let worker = state.clone();
    let expected_pin = PeerPin::from_public_key(&client.public_key()).display();
    let task = tokio::spawn(async move {
        let peer = transport::accept(&endpoint).await?;
        live::serve(
            peer,
            move |request| async move {
                assert_eq!(request.peer_fingerprint, expected_pin);
                assert!(request.live);
                assert_eq!(request.control, control);
                approve
            },
            || panic!("Live request must not use snapshot capture"),
            move || {
                worker.opened.fetch_add(1, Ordering::SeqCst);
                Ok(FakeDesktop(worker))
            },
        )
        .await
    });
    let viewer = live::connect(
        address,
        &client,
        PeerPin::from_public_key(&host.public_key()),
        control,
    )
    .await;
    (viewer, state, task)
}

fn key(pressed: bool) -> Command {
    Command::Input(InputEvent {
        event: Some(Event::Key(Key {
            hid_usage: 225,
            pressed,
        })),
    })
}

async fn wait_for(mut condition: impl FnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(4), async {
        while !condition() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("condition was not reached");
}

fn run(
    mut viewer: Viewer,
) -> (
    mpsc::Sender<Command>,
    mpsc::Receiver<Vec<u8>>,
    JoinHandle<Result<()>>,
) {
    let (commands, receiver) = mpsc::channel(16);
    let (frames, received) = mpsc::channel(64);
    let task = tokio::spawn(async move {
        viewer
            .run(receiver, move |bytes| {
                let _ = frames.try_send(bytes);
                Ok(())
            })
            .await
    });
    (commands, received, task)
}

#[tokio::test]
async fn denied_live_request_never_opens_capture_or_input() {
    let (viewer, state, task) = open(false, true).await;
    assert!(viewer.err().unwrap().to_string().contains("denied"));
    task.await.unwrap().unwrap();
    assert_eq!(state.opened.load(Ordering::SeqCst), 0);
    assert_eq!(state.captures.load(Ordering::SeqCst), 0);
    assert!(state.events.lock().unwrap().is_empty());
}

#[tokio::test]
async fn streams_changing_frames_and_releases_keys_on_focus_loss_and_disconnect() {
    let (viewer, state, host) = open(true, true).await;
    let (commands, mut frames, client) = run(viewer.unwrap());
    let first = frames.recv().await.unwrap();
    let second = frames.recv().await.unwrap();
    assert_ne!(first, second);
    commands.send(key(true)).await.unwrap();
    wait_for(|| state.held.lock().unwrap().contains(&225)).await;
    commands.send(Command::Release).await.unwrap();
    wait_for(|| state.held.lock().unwrap().is_empty()).await;
    commands.send(key(true)).await.unwrap();
    wait_for(|| state.held.lock().unwrap().contains(&225)).await;
    client.abort();
    let _ = client.await;
    wait_for(|| state.closed.load(Ordering::SeqCst)).await;
    assert!(state.held.lock().unwrap().is_empty());
    let _ = host.await.unwrap();
}

#[tokio::test]
async fn stalled_viewer_expires_the_lease_even_while_frames_continue() {
    let (viewer, state, host) = open(true, true).await;
    let (commands, _frames, client) = run(viewer.unwrap());
    commands.send(key(true)).await.unwrap();
    wait_for(|| state.held.lock().unwrap().contains(&225)).await;
    wait_for(|| state.closed.load(Ordering::SeqCst)).await;
    assert!(state.held.lock().unwrap().is_empty());
    assert!(state.captures.load(Ordering::SeqCst) > 2);
    assert!(host.await.unwrap().is_err());
    let _ = client.await.unwrap();
}

#[tokio::test]
async fn renewing_lease_survives_two_seconds_and_host_abort_releases_input() {
    let (viewer, state, host) = open(true, true).await;
    let (commands, _frames, client) = run(viewer.unwrap());
    commands.send(key(true)).await.unwrap();
    for _ in 0..6 {
        tokio::time::sleep(Duration::from_millis(500)).await;
        commands.send(Command::Lease).await.unwrap();
    }
    assert!(!state.closed.load(Ordering::SeqCst));
    assert!(state.held.lock().unwrap().contains(&225));
    host.abort();
    let _ = host.await;
    assert!(state.closed.load(Ordering::SeqCst));
    assert!(state.held.lock().unwrap().is_empty());
    let _ = client.await.unwrap();
}

#[tokio::test]
async fn lock_stops_capture_and_releases_input() {
    let (viewer, state, host) = open(true, true).await;
    let (commands, _frames, client) = run(viewer.unwrap());
    commands.send(key(true)).await.unwrap();
    wait_for(|| state.held.lock().unwrap().contains(&225)).await;
    state.locked.store(true, Ordering::SeqCst);
    wait_for(|| state.closed.load(Ordering::SeqCst)).await;
    assert!(state.held.lock().unwrap().is_empty());
    assert!(host.await.unwrap().is_err());
    let _ = client.await.unwrap();
}

#[tokio::test]
async fn view_permission_cannot_inject_input() {
    let (viewer, state, host) = open(true, false).await;
    let (commands, _frames, client) = run(viewer.unwrap());
    commands.send(key(true)).await.unwrap();
    wait_for(|| state.closed.load(Ordering::SeqCst)).await;
    assert!(state.events.lock().unwrap().is_empty());
    assert!(host.await.unwrap().is_err());
    let _ = client.await.unwrap();
}

#[tokio::test]
async fn slow_receiver_allows_only_one_outstanding_capture() {
    let (viewer, state, host) = open(true, true).await;
    let viewer = viewer.unwrap();
    wait_for(|| state.captures.load(Ordering::SeqCst) == 1).await;
    tokio::time::sleep(Duration::from_millis(250)).await;
    assert_eq!(state.captures.load(Ordering::SeqCst), 1);
    // Starting the decoder/ack loop releases the next capture immediately.
    let (_commands, mut frames, client) = run(viewer);
    assert_ne!(frames.recv().await.unwrap(), frames.recv().await.unwrap());
    client.abort();
    let _ = client.await;
    wait_for(|| state.closed.load(Ordering::SeqCst)).await;
    let _ = host.await;
}

#[tokio::test]
async fn first_time_viewer_waits_for_local_approval_before_desktop_access() {
    let host = identity();
    let client = identity();
    let endpoint = transport::server_attended(
        "127.0.0.1:0".parse().unwrap(),
        &DeviceCertificate::from_identity(&host).unwrap(),
    )
    .unwrap();
    let address = endpoint.local_addr().unwrap();
    let pin = PeerPin::from_public_key(&host.public_key());
    let expected_viewer = PeerPin::from_public_key(&client.public_key()).display();
    let state = Arc::new(State::default());
    let worker = state.clone();
    let (requested, request) = tokio::sync::oneshot::channel();
    let (approve, approved) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let peer = transport::accept(&endpoint).await?;
        live::serve(
            peer,
            move |request| async move {
                assert_eq!(request.peer_fingerprint, expected_viewer);
                requested.send(()).unwrap();
                approved.await.unwrap()
            },
            || panic!("No snapshot requested"),
            move || {
                worker.opened.fetch_add(1, Ordering::SeqCst);
                Ok(FakeDesktop(worker))
            },
        )
        .await
    });
    let connecting = tokio::spawn(async move { live::connect(address, &client, pin, true).await });
    tokio::time::timeout(Duration::from_secs(3), request)
        .await
        .unwrap()
        .unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(!connecting.is_finished());
    assert_eq!(state.opened.load(Ordering::SeqCst), 0);
    assert_eq!(state.captures.load(Ordering::SeqCst), 0);
    assert!(state.events.lock().unwrap().is_empty());
    approve.send(true).unwrap();
    let viewer = connecting.await.unwrap().unwrap();
    let (commands, mut frames, viewing) = run(viewer);
    assert!(frames.recv().await.is_some());
    commands.send(key(true)).await.unwrap();
    wait_for(|| state.held.lock().unwrap().contains(&225)).await;
    viewing.abort();
    let _ = viewing.await;
    wait_for(|| state.closed.load(Ordering::SeqCst)).await;
    assert!(state.held.lock().unwrap().is_empty());
    let _ = server.await;
}
