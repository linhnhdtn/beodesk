use crate::api::lan::HostStatus;
use anyhow::{Context, Result, ensure};
use remote_network::{
    PeerPin, snapshot,
    transport::{self, DeviceCertificate},
};
use std::{
    net::SocketAddr,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicU32, Ordering},
    },
};
use tokio::{runtime::Runtime, sync::oneshot};

static RUNTIME: OnceLock<Runtime> = OnceLock::new();
static HOST: Mutex<Option<Host>> = Mutex::new(None);
static CLIENT: Mutex<Option<Option<oneshot::Sender<()>>>> = Mutex::new(None);
static REQUEST_IDS: AtomicU32 = AtomicU32::new(1);

struct Pending {
    id: u32,
    fingerprint: String,
    name: String,
    reply: Option<oneshot::Sender<bool>>,
}
struct Host {
    endpoint: quinn::Endpoint,
    task: tokio::task::JoinHandle<()>,
    pending: Arc<Mutex<Option<Pending>>>,
}

fn runtime() -> &'static Runtime {
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("create network runtime")
    })
}

pub fn start_host(address: &str, peer_fingerprint: &str) -> Result<HostStatus> {
    let address: SocketAddr = address
        .parse()
        .context("Use an IP address and port, for example 192.168.1.20:4433")?;
    ensure!(address.port() != 0, "Choose a nonzero listening port");
    let pin = PeerPin::parse(peer_fingerprint)?;
    ensure!(
        cfg!(target_os = "linux") && std::env::var("XDG_SESSION_TYPE").as_deref() == Ok("x11"),
        "Sharing currently requires Ubuntu GNOME on X11"
    );
    remote_capture::ensure_desktop_unlocked()?;
    let identity = crate::storage::load_device()?;
    let cert = DeviceCertificate::from_identity(&identity)?;
    let mut host = HOST
        .lock()
        .map_err(|_| anyhow::anyhow!("Host state unavailable"))?;
    ensure!(
        host.is_none(),
        "A screen-sharing listener is already active"
    );
    let endpoint = runtime().block_on(async { transport::server(address, &cert, pin) })?;
    let pending = Arc::new(Mutex::new(None));
    let worker_endpoint = endpoint.clone();
    let worker_pending = pending.clone();
    let task = runtime().spawn(async move {
        loop {
            let accepted = match transport::accept(&worker_endpoint).await {
                Ok(peer) => peer,
                Err(error) => {
                    report_failure("peer authentication", &error);
                    // stop_host aborts this task and closes the endpoint.
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    continue;
                }
            };
            let queue = worker_pending.clone();
            let result = snapshot::serve(
                accepted,
                move |request| async move {
                    let (sender, receiver) = oneshot::channel();
                    let id = REQUEST_IDS.fetch_add(1, Ordering::Relaxed).max(1);
                    if let Ok(mut pending) = queue.lock() {
                        *pending = Some(Pending {
                            id,
                            fingerprint: request.peer_fingerprint,
                            name: request.device_name,
                            reply: Some(sender),
                        });
                    } else {
                        return false;
                    }
                    receiver.await.unwrap_or(false)
                },
                || {
                    let frame = remote_capture::capture()?;
                    remote_capture::ensure_desktop_unlocked()?;
                    Ok(frame.png)
                },
            )
            .await;
            if let Err(error) = result {
                report_failure("snapshot request", &error);
            }
            if let Ok(mut pending) = worker_pending.lock() {
                *pending = None;
            }
        }
    });
    *host = Some(Host {
        endpoint,
        task,
        pending,
    });
    drop(host);
    host_status()
}

fn report_failure(stage: &str, error: &anyhow::Error) {
    // Bounded, printable diagnostic; no identity seed or private certificate is logged.
    let details: String = format!("{error:#}")
        .chars()
        .filter(|character| !character.is_control())
        .take(2048)
        .collect();
    eprintln!("BeoDesk LAN {stage}: {details}");
}

pub fn host_status() -> Result<HostStatus> {
    let host = HOST
        .lock()
        .map_err(|_| anyhow::anyhow!("Host state unavailable"))?;
    let mut status = HostStatus {
        listening: false,
        address: String::new(),
        request_id: 0,
        peer_fingerprint: String::new(),
        device_name: String::new(),
    };
    if let Some(host) = host.as_ref() {
        status.listening = true;
        status.address = host.endpoint.local_addr()?.to_string();
        let pending = host
            .pending
            .lock()
            .map_err(|_| anyhow::anyhow!("Request state unavailable"))?;
        if let Some(request) = pending.as_ref().filter(|request| {
            request
                .reply
                .as_ref()
                .is_some_and(|reply| !reply.is_closed())
        }) {
            status.request_id = request.id;
            status.peer_fingerprint = request.fingerprint.clone();
            status.device_name = request.name.clone();
        }
    }
    Ok(status)
}

pub fn respond(id: u32, approved: bool) -> Result<()> {
    let host = HOST
        .lock()
        .map_err(|_| anyhow::anyhow!("Host state unavailable"))?;
    let host = host.as_ref().context("Listener is not active")?;
    let mut pending = host
        .pending
        .lock()
        .map_err(|_| anyhow::anyhow!("Request state unavailable"))?;
    let request = pending
        .as_mut()
        .filter(|request| request.id == id)
        .context("Viewing request expired")?;
    request
        .reply
        .take()
        .context("Viewing request already answered")?
        .send(approved)
        .map_err(|_| anyhow::anyhow!("Viewing request expired"))
}

pub fn stop_host() -> Result<()> {
    let host = HOST
        .lock()
        .map_err(|_| anyhow::anyhow!("Host state unavailable"))?
        .take();
    if let Some(host) = host {
        host.endpoint
            .close(0_u32.into(), b"Local user stopped sharing");
        host.task.abort();
    }
    Ok(())
}

struct ClientGuard;
impl Drop for ClientGuard {
    fn drop(&mut self) {
        if let Ok(mut active) = CLIENT.lock() {
            *active = None;
        }
    }
}

pub fn fetch(address: &str, host_fingerprint: &str) -> Result<Vec<u8>> {
    let address: SocketAddr = address
        .parse()
        .context("Use an IP address and port, for example 192.168.1.20:4433")?;
    let pin = PeerPin::parse(host_fingerprint)?;
    let identity = crate::storage::load_device()?;
    let (cancel_sender, cancel_receiver) = oneshot::channel();
    {
        let mut active = CLIENT
            .lock()
            .map_err(|_| anyhow::anyhow!("Client state unavailable"))?;
        ensure!(active.is_none(), "Another snapshot request is in progress");
        *active = Some(Some(cancel_sender));
    }
    let _guard = ClientGuard;
    let bytes = runtime().block_on(async {
        tokio::select! {
            result = snapshot::fetch(address, &identity, pin) => result,
            _ = cancel_receiver => Err(anyhow::anyhow!("Viewing request cancelled")),
        }
    })?;
    remote_capture::validate_png(&bytes)?;
    Ok(bytes)
}

pub fn cancel_client() -> Result<()> {
    if let Some(active) = CLIENT
        .lock()
        .map_err(|_| anyhow::anyhow!("Client state unavailable"))?
        .as_mut()
        && let Some(sender) = active.take()
    {
        let _ = sender.send(());
    }
    Ok(())
}
