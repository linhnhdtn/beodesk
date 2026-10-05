use anyhow::{Result, ensure};
use remote_input::InputController;
use remote_network::live::Desktop;
use remote_protocol::wire::InputEvent;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

pub(crate) struct HostDesktop {
    input: Mutex<InputController>,
    video: Mutex<(remote_capture::DesktopCapture, remote_video::ScreenEncoder)>,
    active: AtomicBool,
    host_active: Arc<AtomicBool>,
}

impl HostDesktop {
    pub fn open(active: Arc<AtomicBool>) -> Result<Self> {
        remote_capture::ensure_desktop_unlocked()?;
        let input = InputController::open()?;
        let video = (
            remote_capture::DesktopCapture::open()?,
            remote_video::ScreenEncoder::new()?,
        );
        active.store(true, Ordering::Release);
        Ok(Self {
            input: Mutex::new(input),
            video: Mutex::new(video),
            active: AtomicBool::new(true),
            host_active: active,
        })
    }
}

impl Desktop for HostDesktop {
    fn capture(&self) -> Result<Vec<u8>> {
        ensure!(self.active.load(Ordering::Acquire), "Session has ended");
        self.check_permission()?;
        let mut video = self
            .video
            .lock()
            .map_err(|_| anyhow::anyhow!("Video state unavailable"))?;
        let (capture, encoder) = &mut *video;
        let frame = capture.capture()?;
        let bytes = encoder.encode(frame.width as usize, frame.height as usize, &frame.rgba)?;
        self.check_permission()?;
        Ok(bytes)
    }
    fn feedback(&self, delay: std::time::Duration) -> Result<()> {
        self.video
            .lock()
            .map_err(|_| anyhow::anyhow!("Video state unavailable"))?
            .1
            .feedback(delay)
    }
    fn check_permission(&self) -> Result<()> {
        ensure!(self.active.load(Ordering::Acquire), "Session has ended");
        remote_capture::ensure_desktop_unlocked()
    }
    fn inject(&self, input: InputEvent) -> Result<()> {
        // Check before taking the lock so cleanup never waits for DBus.
        self.check_permission()?;
        let mut controller = self
            .input
            .lock()
            .map_err(|_| anyhow::anyhow!("Input state unavailable"))?;
        ensure!(self.active.load(Ordering::Acquire), "Session has ended");
        controller.inject(input)
    }
    fn release(&self) {
        self.input
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .release_all();
    }
    fn close(&self) {
        if self.active.swap(false, Ordering::AcqRel) {
            self.host_active.store(false, Ordering::Release);
            self.release();
        }
    }
}

impl Drop for HostDesktop {
    fn drop(&mut self) {
        self.close();
    }
}
