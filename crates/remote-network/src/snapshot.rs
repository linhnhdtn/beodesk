//! One consented PNG snapshot, using a bounded reliable QUIC stream for the prototype.
use crate::{
    PeerPin,
    transport::{self, AuthenticatedConnection, DeviceCertificate},
    wire,
};
use anyhow::{Context, Result, ensure};
use rand::RngCore;
use remote_core::{
    identity::DeviceIdentity,
    session::{CONSENT_TIMEOUT, HostSession, Permission},
};
use remote_protocol::{MAX_CONTROL_BYTES, PROTOCOL_MAJOR, PROTOCOL_MINOR, wire as proto};
use std::{future::Future, net::SocketAddr, time::Instant};

pub struct ViewRequest {
    pub peer_fingerprint: String,
    pub device_name: String,
    pub live: bool,
    pub control: bool,
}

pub async fn serve<A, AFuture, C>(
    peer: AuthenticatedConnection,
    authorize: A,
    capture: C,
) -> Result<()>
where
    A: FnOnce(ViewRequest) -> AFuture,
    AFuture: Future<Output = bool>,
    C: FnOnce() -> Result<Vec<u8>> + Send + 'static,
{
    let (send, mut recv) =
        tokio::time::timeout(wire::IO_TIMEOUT, peer.connection.accept_bi()).await??;
    let hello = wire::read_control(&mut recv).await?;
    serve_request(peer, send, hello, authorize, capture).await
}

pub(crate) async fn serve_request<A, AFuture, C>(
    peer: AuthenticatedConnection,
    mut send: quinn::SendStream,
    hello: proto::Envelope,
    authorize: A,
    capture: C,
) -> Result<()>
where
    A: FnOnce(ViewRequest) -> AFuture,
    AFuture: Future<Output = bool>,
    C: FnOnce() -> Result<Vec<u8>> + Send + 'static,
{
    ensure!(
        hello.transport_epoch == 1 && hello.sequence == 1,
        "Invalid initial session metadata"
    );
    let Some(proto::envelope::Body::Handshake(handshake)) = hello.body.as_ref() else {
        anyhow::bail!("Expected a session handshake");
    };
    ensure!(
        handshake.public_key == peer.peer_key,
        "Handshake identity differs from authenticated transport"
    );
    ensure!(
        handshake
            .capabilities
            .contains(&(proto::Capability::View as i32))
            && handshake
                .capabilities
                .contains(&(proto::Capability::PngSnapshot as i32)),
        "Peer does not support snapshot viewing"
    );
    let session_id: [u8; 16] = hello.session_id.as_slice().try_into()?;
    let mut session = HostSession::new(peer.peer_key, session_id, 1, Instant::now())?;
    session.on_authenticated_transport(peer.peer_key, Instant::now())?;
    let approved = tokio::select! {
        result = tokio::time::timeout(
            CONSENT_TIMEOUT,
            authorize(ViewRequest {
                peer_fingerprint: peer.peer_pin.display(),
                device_name: handshake.device_name.clone(),
                live: false,
                control: false,
            }),
        ) => result.unwrap_or(false),
        _ = peer.connection.closed() => false,
    };
    if !approved || peer.connection.close_reason().is_some() {
        let denial = status(&hello, proto::Action::ViewDenied);
        wire::write_control(&mut send, &denial).await?;
        send.finish()?;
        let _ = tokio::time::timeout(wire::IO_TIMEOUT, send.stopped()).await;
        return Ok(());
    }
    session.approve(Permission::ViewOnly, Instant::now())?;
    ensure!(
        session.can_capture(Instant::now()),
        "Capture permission expired"
    );
    let captured = tokio::task::spawn_blocking(capture)
        .await
        .context("Capture worker failed")?;
    let bytes = match captured {
        Ok(bytes)
            if session.can_capture(Instant::now()) && peer.connection.close_reason().is_none() =>
        {
            bytes
        }
        _ => {
            wire::write_control(&mut send, &status(&hello, proto::Action::ViewUnavailable)).await?;
            send.finish()?;
            let _ = tokio::time::timeout(wire::IO_TIMEOUT, send.stopped()).await;
            return Ok(());
        }
    };
    ensure!(
        !bytes.is_empty() && bytes.len() <= wire::MAX_SNAPSHOT_BYTES,
        "Snapshot exceeds negotiated limit"
    );
    wire::write_control(&mut send, &status(&hello, proto::Action::ViewApproved)).await?;
    wire::write_bytes(&mut send, &bytes, wire::MAX_SNAPSHOT_BYTES).await?;
    send.finish()?;
    let _ = tokio::time::timeout(wire::IO_TIMEOUT, send.stopped()).await;
    Ok(())
}

pub async fn fetch(
    address: SocketAddr,
    identity: &DeviceIdentity,
    expected_host: PeerPin,
) -> Result<Vec<u8>> {
    let certificate = DeviceCertificate::from_identity(identity)?;
    let (_endpoint, peer) = transport::connect(address, &certificate, expected_host)
        .await
        .with_context(|| format!("Authenticating LAN host at {address}"))?;
    let (mut send, mut recv) = peer
        .connection
        .open_bi()
        .await
        .context("Opening snapshot request stream")?;
    let mut session_id = [0; 16];
    rand::rngs::OsRng.fill_bytes(&mut session_id);
    let hello = proto::Envelope {
        protocol_major: PROTOCOL_MAJOR,
        protocol_minor: PROTOCOL_MINOR,
        session_id: session_id.to_vec(),
        transport_epoch: 1,
        sequence: 1,
        body: Some(proto::envelope::Body::Handshake(proto::Handshake {
            public_key: identity.public_key().to_vec(),
            device_name: "BeoDesk viewer".into(),
            capabilities: vec![
                proto::Capability::View as i32,
                proto::Capability::PngSnapshot as i32,
            ],
            max_control_bytes: MAX_CONTROL_BYTES as u32,
        })),
    };
    wire::write_control(&mut send, &hello)
        .await
        .context("Sending snapshot request to host")?;
    send.finish().context("Finishing snapshot request")?;
    let response = wire::read_consent(&mut recv)
        .await
        .context("Waiting for the sharing machine to allow this request")?;
    ensure!(
        response.session_id == hello.session_id
            && response.transport_epoch == 1
            && response.sequence == 1,
        "Invalid response session metadata"
    );
    match response.body {
        Some(proto::envelope::Body::Control(control))
            if control.action == proto::Action::ViewApproved as i32 => {}
        Some(proto::envelope::Body::Control(control))
            if control.action == proto::Action::ViewDenied as i32 =>
        {
            anyhow::bail!("Host denied or timed out the viewing request")
        }
        Some(proto::envelope::Body::Control(control))
            if control.action == proto::Action::ViewUnavailable as i32 =>
        {
            anyhow::bail!(
                "Host cannot capture its desktop; check X11 session and desktop lock state"
            )
        }
        _ => anyhow::bail!("Unexpected host response"),
    }
    let png = wire::read_bytes(&mut recv, wire::MAX_SNAPSHOT_BYTES)
        .await
        .context("Receiving approved snapshot")?;
    peer.connection.close(0_u32.into(), b"Snapshot received");
    Ok(png)
}

fn status(hello: &proto::Envelope, action: proto::Action) -> proto::Envelope {
    proto::Envelope {
        protocol_major: PROTOCOL_MAJOR,
        protocol_minor: PROTOCOL_MINOR,
        session_id: hello.session_id.clone(),
        transport_epoch: 1,
        sequence: 1,
        body: Some(proto::envelope::Body::Control(proto::SessionControl {
            action: action as i32,
        })),
    }
}
