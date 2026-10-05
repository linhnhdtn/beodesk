//! Attended LAN sessions. Media and input use opposite directions of a QUIC
//! stream, so backpressure on screen frames cannot block input or its lease.
use crate::{
    PeerPin, snapshot,
    transport::{self, AuthenticatedConnection, DeviceCertificate},
    wire,
};
use anyhow::{Context, Result, ensure};
use rand::RngCore;
use remote_core::{
    identity::DeviceIdentity,
    session::{CONSENT_TIMEOUT, HostSession, INPUT_LEASE, Permission, SessionState},
};
use remote_protocol::{MAX_CONTROL_BYTES, PROTOCOL_MAJOR, PROTOCOL_MINOR, wire as proto};
use std::{
    future::Future,
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::sync::{Semaphore, mpsc, watch};

/// Each implementation owns one session's injected input. close must disable
/// injection and release held input, including when a blocking call is in flight.
pub trait Desktop: Send + Sync + 'static {
    fn capture(&self) -> Result<Vec<u8>>;
    fn feedback(&self, _delay: Duration) -> Result<()> {
        Ok(())
    }
    fn check_permission(&self) -> Result<()>;
    fn inject(&self, input: proto::InputEvent) -> Result<()>;
    fn release(&self);
    fn close(&self);
}

struct DesktopGuard<D: Desktop>(Arc<D>);
impl<D: Desktop> Drop for DesktopGuard<D> {
    fn drop(&mut self) {
        self.0.close();
    }
}

struct ConnectionGuard(quinn::Connection);
impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        self.0.close(0_u32.into(), b"Session ended");
    }
}

/// Also accepts the original one-snapshot request without changing its consent.
pub async fn serve<A, F, C, D, O>(
    peer: AuthenticatedConnection,
    authorize: A,
    snapshot_capture: C,
    open: O,
) -> Result<()>
where
    A: FnOnce(snapshot::ViewRequest) -> F,
    F: Future<Output = bool>,
    C: FnOnce() -> Result<Vec<u8>> + Send + 'static,
    D: Desktop,
    O: FnOnce() -> Result<D> + Send + 'static,
{
    let (mut send, mut recv) =
        tokio::time::timeout(wire::IO_TIMEOUT, peer.connection.accept_bi()).await??;
    let hello = wire::read_control(&mut recv).await?;
    let Some(proto::envelope::Body::Handshake(handshake)) = hello.body.as_ref() else {
        anyhow::bail!("Expected a session handshake");
    };
    if !handshake
        .capabilities
        .contains(&(proto::Capability::H264Stream as i32))
    {
        return snapshot::serve_request(peer, send, hello, authorize, snapshot_capture).await;
    }
    let _connection = ConnectionGuard(peer.connection.clone());
    ensure!(
        hello.transport_epoch == 1 && hello.sequence == 1 && handshake.public_key == peer.peer_key,
        "Invalid authenticated session handshake"
    );
    ensure!(
        handshake
            .capabilities
            .contains(&(proto::Capability::View as i32)),
        "Peer must support viewing"
    );
    let control = handshake
        .capabilities
        .contains(&(proto::Capability::Control as i32));
    let mut session = HostSession::new(
        peer.peer_key,
        hello.session_id.as_slice().try_into()?,
        1,
        Instant::now(),
    )?;
    session.on_authenticated_transport(peer.peer_key, Instant::now())?;
    let approved = tokio::select! {
        result = tokio::time::timeout(CONSENT_TIMEOUT, authorize(snapshot::ViewRequest {
            peer_fingerprint: peer.peer_pin.display(), device_name: handshake.device_name.clone(), live: true, control,
        })) => result.unwrap_or(false),
        _ = peer.connection.closed() => false,
    };
    if !approved || peer.connection.close_reason().is_some() {
        wire::write_control(
            &mut send,
            &message(&hello, 1, action(proto::Action::ViewDenied)),
        )
        .await?;
        send.finish()?;
        let _ = tokio::time::timeout(wire::IO_TIMEOUT, send.stopped()).await;
        return Ok(());
    }
    session.approve(
        if control {
            Permission::Control
        } else {
            Permission::ViewOnly
        },
        Instant::now(),
    )?;
    let (lease_sender, mut lease_receiver) =
        watch::channel(tokio::time::Instant::now() + INPUT_LEASE);
    let desktop = Arc::new(tokio::task::spawn_blocking(open).await??);
    let _desktop = DesktopGuard(desktop.clone());
    ensure!(
        session.can_capture(Instant::now()) && peer.connection.close_reason().is_none(),
        "Session expired while opening desktop"
    );
    wire::write_control(
        &mut send,
        &message(
            &hello,
            1,
            action(if control {
                proto::Action::ControlApproved
            } else {
                proto::Action::ViewApproved
            }),
        ),
    )
    .await?;
    let credit = Semaphore::new(0);
    let outstanding = AtomicBool::new(false);
    let media = async {
        loop {
            let started = tokio::time::Instant::now();
            let worker = desktop.clone();
            let bytes = tokio::task::spawn_blocking(move || worker.capture()).await??;
            if !bytes.is_empty() {
                // Only one access unit can be outstanding. A slow receiver causes
                // us to skip captures, never discard dependent H.264 P frames.
                outstanding.store(true, Ordering::Release);
                let sent = Instant::now();
                wire::write_bytes(&mut send, &bytes, 2 * 1024 * 1024).await?;
                tokio::time::timeout(INPUT_LEASE, credit.acquire())
                    .await??
                    .forget();
                desktop.feedback(sent.elapsed())?;
            }
            tokio::time::sleep_until(started + Duration::from_micros(33_333)).await;
        }
        #[allow(unreachable_code)]
        Ok::<(), anyhow::Error>(())
    };
    let inputs = async {
        loop {
            let incoming = wire::read_control(&mut recv).await?;
            ensure!(incoming.sequence > 1, "Replayed session handshake sequence");
            let input = session.receive(&incoming, Instant::now())?;
            if matches!(session.state(), SessionState::Closed(_)) {
                return Ok::<(), anyhow::Error>(());
            }
            if let Some(input) = input {
                let worker = desktop.clone();
                tokio::task::spawn_blocking(move || worker.inject(input)).await??;
            }
            if let Some(proto::envelope::Body::Control(control)) = incoming.body {
                if control.action == proto::Action::FrameReceived as i32 {
                    ensure!(
                        outstanding.swap(false, Ordering::AcqRel),
                        "Unexpected frame acknowledgement"
                    );
                    credit.add_permits(1);
                }
                if control.action == proto::Action::InputLease as i32 {
                    lease_sender.send_replace(tokio::time::Instant::now() + INPUT_LEASE);
                }
                if control.action == proto::Action::ReleaseInput as i32 {
                    let worker = desktop.clone();
                    tokio::task::spawn_blocking(move || worker.release()).await?;
                    session.take_released_input();
                }
            }
        }
    };
    let lease = async {
        loop {
            let deadline = *lease_receiver.borrow_and_update();
            tokio::select! {
                _ = tokio::time::sleep_until(deadline) => anyhow::bail!("Input lease expired"),
                result = lease_receiver.changed() => { result.context("Input lease ended")?; },
            }
        }
        #[allow(unreachable_code)]
        Ok::<(), anyhow::Error>(())
    };
    let permission = async {
        loop {
            let worker = desktop.clone();
            tokio::task::spawn_blocking(move || worker.check_permission()).await??;
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        #[allow(unreachable_code)]
        Ok::<(), anyhow::Error>(())
    };
    tokio::select! {
        result = media => result,
        result = inputs => result,
        result = permission => result,
        result = lease => result,
        _ = peer.connection.closed() => Ok(()),
    }
}

#[derive(Debug)]
pub enum Command {
    Input(proto::InputEvent),
    Release,
    Lease,
}

pub struct Viewer {
    _endpoint: quinn::Endpoint,
    connection: quinn::Connection,
    send: quinn::SendStream,
    recv: quinn::RecvStream,
    hello: proto::Envelope,
}

impl Drop for Viewer {
    fn drop(&mut self) {
        self.connection.close(0_u32.into(), b"Viewer disconnected");
    }
}

pub async fn connect(
    address: SocketAddr,
    identity: &DeviceIdentity,
    host: PeerPin,
    control: bool,
) -> Result<Viewer> {
    let certificate = DeviceCertificate::from_identity(identity)?;
    let (endpoint, peer) = transport::connect(address, &certificate, host)
        .await
        .with_context(|| format!("Authenticating LAN host at {address}"))?;
    let (mut send, mut recv) = peer.connection.open_bi().await?;
    let mut id = [0; 16];
    rand::rngs::OsRng.fill_bytes(&mut id);
    let mut capabilities = vec![
        proto::Capability::View as i32,
        proto::Capability::H264Stream as i32,
        proto::Capability::H264 as i32,
    ];
    if control {
        capabilities.push(proto::Capability::Control as i32);
    }
    let hello = proto::Envelope {
        protocol_major: PROTOCOL_MAJOR,
        protocol_minor: PROTOCOL_MINOR,
        session_id: id.to_vec(),
        transport_epoch: 1,
        sequence: 1,
        body: Some(proto::envelope::Body::Handshake(proto::Handshake {
            public_key: identity.public_key().to_vec(),
            device_name: "BeoDesk viewer".into(),
            capabilities,
            max_control_bytes: MAX_CONTROL_BYTES as u32,
        })),
    };
    wire::write_control(&mut send, &hello).await?;
    let response = wire::read_consent(&mut recv).await.context(
        "Waiting for host approval; update both apps and allow this request on the sharing machine",
    )?;
    ensure!(
        response.session_id == hello.session_id
            && response.transport_epoch == 1
            && response.sequence == 1,
        "Invalid response session metadata"
    );
    let expected = if control {
        proto::Action::ControlApproved
    } else {
        proto::Action::ViewApproved
    };
    match response.body {
        Some(proto::envelope::Body::Control(status)) if status.action == expected as i32 => {}
        Some(proto::envelope::Body::Control(status))
            if status.action == proto::Action::ViewDenied as i32 =>
        {
            anyhow::bail!("Host denied or timed out the viewing request")
        }
        _ => anyhow::bail!(
            "Host did not approve the requested live session; update BeoDesk on both machines"
        ),
    }
    Ok(Viewer {
        _endpoint: endpoint,
        connection: peer.connection,
        send,
        recv,
        hello,
    })
}

impl Viewer {
    /// The UI supplies leases while it is alive. A stalled UI must not keep
    /// remotely pressed keys held merely because the Rust runtime is still alive.
    pub async fn run<F>(
        &mut self,
        mut commands: mpsc::Receiver<Command>,
        mut frame: F,
    ) -> Result<()>
    where
        F: FnMut(Vec<u8>) -> Result<()>,
    {
        let send = &mut self.send;
        let recv = &mut self.recv;
        let hello = &self.hello;
        let (acks, mut ack_receiver) = mpsc::channel::<()>(1);
        let inputs = async {
            let mut sequence = 2;
            wire::write_control(
                send,
                &message(hello, sequence, action(proto::Action::InputLease)),
            )
            .await?;
            loop {
                let command = tokio::select! {
                    ack = ack_receiver.recv() => { ack.context("Video acknowledgement ended")?; None },
                    command = commands.recv() => Some(command.context("Viewer UI ended")?),
                };
                sequence += 1;
                let body = match command {
                    Some(Command::Input(input)) => proto::envelope::Body::Input(input),
                    Some(Command::Release) => action(proto::Action::ReleaseInput),
                    Some(Command::Lease) => action(proto::Action::InputLease),
                    None => action(proto::Action::FrameReceived),
                };
                wire::write_control(send, &message(hello, sequence, body)).await?;
            }
            #[allow(unreachable_code)]
            Ok::<(), anyhow::Error>(())
        };
        let media = async {
            loop {
                frame(wire::read_bytes(recv, 2 * 1024 * 1024).await?)?;
                acks.send(()).await?;
            }
            #[allow(unreachable_code)]
            Ok::<(), anyhow::Error>(())
        };
        tokio::select! {
            result = inputs => result,
            result = media => result,
            _ = self.connection.closed() => anyhow::bail!("Remote session disconnected"),
        }
    }
}

fn action(action: proto::Action) -> proto::envelope::Body {
    proto::envelope::Body::Control(proto::SessionControl {
        action: action as i32,
    })
}

fn message(hello: &proto::Envelope, sequence: u64, body: proto::envelope::Body) -> proto::Envelope {
    proto::Envelope {
        protocol_major: PROTOCOL_MAJOR,
        protocol_minor: PROTOCOL_MINOR,
        session_id: hello.session_id.clone(),
        transport_epoch: 1,
        sequence,
        body: Some(body),
    }
}
