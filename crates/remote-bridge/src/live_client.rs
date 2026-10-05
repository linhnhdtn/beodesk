use crate::{api::lan::LiveFrame, lan};
use anyhow::{Context, Result, ensure};
use remote_network::{
    PeerPin,
    live::{self, Command},
};
use remote_protocol::wire::{self as proto, input_event::Event};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicU32, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::sync::mpsc;

static VIEWER: Mutex<Option<Viewer>> = Mutex::new(None);
static IDS: AtomicU32 = AtomicU32::new(1);

struct Viewer {
    id: u32,
    commands: mpsc::Sender<Command>,
    latest: Arc<Mutex<VideoState>>,
    task: tokio::task::JoinHandle<()>,
    lease: Instant,
}

struct VideoState {
    info: LiveFrame,
    pixels: Option<Arc<remote_video::Frame>>,
}

pub(crate) fn pixels(id: u32) -> Option<(u32, Arc<remote_video::Frame>)> {
    let viewer = VIEWER.lock().ok()?;
    let state = viewer.as_ref().filter(|v| v.id == id)?.latest.lock().ok()?;
    if state.info.closed {
        return None;
    }
    Some((state.info.frame_id, state.pixels.clone()?))
}

pub fn start(address: &str, fingerprint: &str) -> Result<u32> {
    let address = address.parse().context("Use an IP address and port")?;
    let pin = PeerPin::parse(fingerprint)?;
    let identity = crate::storage::load_device()?;
    let (guard, mut cancel) = lan::reserve_client()?;
    let mut connection = lan::runtime().block_on(async {
        tokio::select! {
            result = live::connect(address, &identity, pin, true) => result,
            _ = &mut cancel => anyhow::bail!("Viewing request cancelled"),
        }
    })?;
    let id = IDS.fetch_add(1, Ordering::Relaxed).max(1);
    let (sender, receiver) = mpsc::channel(128);
    let latest = Arc::new(Mutex::new(VideoState {
        info: LiveFrame {
            frame_id: 0,
            width: 0,
            height: 0,
            closed: false,
            error: String::new(),
        },
        pixels: None,
    }));
    let frames = latest.clone();
    let mut decoder = remote_video::ScreenDecoder::new()?;
    let task = lan::runtime().spawn(async move {
        let _guard = guard;
        let result = tokio::select! {
            result = connection.run(receiver, |packet| {
                let pixels = Arc::new(decoder.decode(&packet)?);
                let mut frame = frames.lock().map_err(|_| anyhow::anyhow!("Frame state unavailable"))?;
                frame.info.frame_id = frame.info.frame_id.wrapping_add(1).max(1);
                frame.info.width = pixels.width;
                frame.info.height = pixels.height;
                frame.pixels = Some(pixels);
                Ok(())
            }) => result,
            _ = &mut cancel => Ok(()),
        };
        if let Ok(mut frame) = frames.lock() {
            frame.info.closed = true;
            frame.pixels = None;
            frame.info.error = result.err().map(|e| format!("{e:#}")).unwrap_or_default();
        }
    });
    *VIEWER
        .lock()
        .map_err(|_| anyhow::anyhow!("Viewer unavailable"))? = Some(Viewer {
        id,
        commands: sender,
        latest,
        task,
        lease: Instant::now(),
    });
    Ok(id)
}

pub fn poll(id: u32) -> Result<LiveFrame> {
    let mut viewer = VIEWER
        .lock()
        .map_err(|_| anyhow::anyhow!("Viewer unavailable"))?;
    let viewer = viewer
        .as_mut()
        .filter(|v| v.id == id)
        .context("Session has ended")?;
    let frame = viewer
        .latest
        .lock()
        .map_err(|_| anyhow::anyhow!("Frame state unavailable"))?;
    if !frame.info.closed && viewer.lease.elapsed() >= Duration::from_millis(500) {
        viewer
            .commands
            .try_send(Command::Lease)
            .context("Input queue unavailable; reconnect")?;
        viewer.lease = Instant::now();
    }
    Ok(frame.info.clone())
}

pub fn input(
    id: u32,
    kind: u8,
    x: f32,
    y: f32,
    code: u32,
    pressed: bool,
    delta: i32,
) -> Result<()> {
    let command = if kind == 4 {
        Command::Release
    } else {
        let event = match kind {
            0 => Event::PointerMove(proto::PointerMove { x, y }),
            1 => Event::PointerButton(proto::PointerButton {
                button: code,
                pressed,
                x,
                y,
            }),
            2 => Event::Key(proto::Key {
                hid_usage: code,
                pressed,
            }),
            3 => Event::Wheel(proto::Wheel { delta }),
            _ => anyhow::bail!("Invalid input kind"),
        };
        let input = proto::InputEvent { event: Some(event) };
        remote_protocol::validate(&proto::Envelope {
            protocol_major: remote_protocol::PROTOCOL_MAJOR,
            protocol_minor: remote_protocol::PROTOCOL_MINOR,
            session_id: vec![1; 16],
            transport_epoch: 1,
            sequence: 1,
            body: Some(proto::envelope::Body::Input(input)),
        })?;
        Command::Input(input)
    };
    let viewer = VIEWER
        .lock()
        .map_err(|_| anyhow::anyhow!("Viewer unavailable"))?;
    let viewer = viewer
        .as_ref()
        .filter(|v| v.id == id)
        .context("Session has ended")?;
    ensure!(!viewer.task.is_finished(), "Session has ended");
    viewer
        .commands
        .try_send(command)
        .context("Input queue is full or closed; reconnect")?;
    Ok(())
}

pub fn stop(id: u32) -> Result<()> {
    let mut viewer = VIEWER
        .lock()
        .map_err(|_| anyhow::anyhow!("Viewer unavailable"))?;
    if viewer.as_ref().is_some_and(|v| v.id == id) {
        let viewer = viewer.take().expect("checked viewer");
        // The task owns ClientGuard and the QUIC connection; await its drop so a
        // new session cannot race the previous session's client-slot cleanup.
        viewer.task.abort();
        drop(viewer.commands);
        drop(viewer.latest);
        drop(lan::runtime().block_on(viewer.task));
    }
    Ok(())
}
