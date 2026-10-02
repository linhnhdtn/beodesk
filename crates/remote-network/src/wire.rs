use anyhow::{Context, Result, ensure};
use remote_protocol::{MAX_CONTROL_BYTES, wire::Envelope};
use std::time::Duration;

pub const MAX_SNAPSHOT_BYTES: usize = 8 * 1024 * 1024;
pub const IO_TIMEOUT: Duration = Duration::from_secs(10);

pub async fn write_control(send: &mut quinn::SendStream, envelope: &Envelope) -> Result<()> {
    let bytes = remote_protocol::encode(envelope)?;
    write_bytes(send, &bytes, MAX_CONTROL_BYTES).await
}

pub async fn read_control(recv: &mut quinn::RecvStream) -> Result<Envelope> {
    let bytes = read_bytes(recv, MAX_CONTROL_BYTES).await?;
    Ok(remote_protocol::decode(&bytes)?)
}

pub async fn read_consent(recv: &mut quinn::RecvStream) -> Result<Envelope> {
    let bytes = read_bytes_timeout(recv, MAX_CONTROL_BYTES, Duration::from_secs(65)).await?;
    Ok(remote_protocol::decode(&bytes)?)
}

pub async fn write_bytes(send: &mut quinn::SendStream, bytes: &[u8], limit: usize) -> Result<()> {
    ensure!(
        !bytes.is_empty() && bytes.len() <= limit,
        "Payload exceeds negotiated size limit"
    );
    tokio::time::timeout(IO_TIMEOUT, async {
        send.write_all(&(bytes.len() as u32).to_be_bytes()).await?;
        send.write_all(bytes).await?;
        Ok::<_, anyhow::Error>(())
    })
    .await
    .context("Write timed out")?
}

pub async fn read_bytes(recv: &mut quinn::RecvStream, limit: usize) -> Result<Vec<u8>> {
    read_bytes_timeout(recv, limit, IO_TIMEOUT).await
}

async fn read_bytes_timeout(
    recv: &mut quinn::RecvStream,
    limit: usize,
    wait: Duration,
) -> Result<Vec<u8>> {
    tokio::time::timeout(wait, async {
        let mut header = [0; 4];
        recv.read_exact(&mut header).await?;
        let length = u32::from_be_bytes(header) as usize;
        ensure!(
            length > 0 && length <= limit,
            "Payload exceeds negotiated size limit"
        );
        let mut bytes = vec![0; length];
        recv.read_exact(&mut bytes).await?;
        Ok(bytes)
    })
    .await
    .context("Read timed out")?
}
