//! Session-owned X11 input. Never instantiate until the local user grants control.
#[cfg(target_os = "linux")]
mod x11;
#[cfg(target_os = "linux")]
pub use x11::InputController;

#[cfg(not(target_os = "linux"))]
pub struct InputController;
#[cfg(not(target_os = "linux"))]
impl InputController {
    pub fn open() -> anyhow::Result<Self> {
        anyhow::bail!("Remote input requires an X11 host")
    }
    pub fn inject(&mut self, _: remote_protocol::wire::InputEvent) -> anyhow::Result<()> {
        anyhow::bail!("Remote input requires an X11 host")
    }
    pub fn release_all(&mut self) {}
}
