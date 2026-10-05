//! Bounded, explicitly requested desktop snapshots.
use anyhow::{Context, Result, ensure};
use std::io::{Cursor, Write};

pub const MAX_PIXELS: usize = 3840 * 2160;
pub const MAX_PNG_BYTES: usize = 8 * 1024 * 1024;

pub struct Snapshot {
    pub width: u32,
    pub height: u32,
    pub png: Vec<u8>,
}

#[cfg(target_os = "linux")]
mod x11;

pub struct RgbaFrame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

pub struct DesktopCapture {
    #[cfg(target_os = "linux")]
    inner: x11::Capture,
}
impl DesktopCapture {
    pub fn open() -> Result<Self> {
        #[cfg(target_os = "linux")]
        {
            ensure!(
                std::env::var("XDG_SESSION_TYPE").as_deref() == Ok("x11"),
                "Live capture requires X11"
            );
            ensure_desktop_unlocked()?;
            Ok(Self {
                inner: x11::Capture::open()?,
            })
        }
        #[cfg(not(target_os = "linux"))]
        anyhow::bail!("Live capture is not supported on this platform yet")
    }
    pub fn capture(&mut self) -> Result<RgbaFrame> {
        #[cfg(target_os = "linux")]
        {
            self.inner.capture()
        }
        #[cfg(not(target_os = "linux"))]
        anyhow::bail!("Live capture is not supported on this platform yet")
    }
}

pub fn capture() -> Result<Snapshot> {
    #[cfg(target_os = "linux")]
    {
        ensure!(
            std::env::var("XDG_SESSION_TYPE").as_deref() == Ok("x11"),
            "Screen snapshots currently require an X11 desktop session"
        );
        ensure_desktop_unlocked()?;
        x11::capture()
    }
    #[cfg(not(target_os = "linux"))]
    {
        anyhow::bail!("Screen capture is not implemented for this platform yet")
    }
}

pub fn ensure_desktop_unlocked() -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        let bus = dbus::blocking::Connection::new_session()
            .context("Cannot verify the desktop lock state")?;
        let proxy = bus.with_proxy(
            "org.gnome.ScreenSaver",
            "/org/gnome/ScreenSaver",
            std::time::Duration::from_secs(1),
        );
        let (active,): (bool,) = proxy
            .method_call("org.gnome.ScreenSaver", "GetActive", ())
            .context("Cannot verify the GNOME desktop lock state")?;
        ensure!(
            !active,
            "The desktop is locked; screen sharing is unavailable"
        );
        Ok(())
    }
    #[cfg(not(target_os = "linux"))]
    anyhow::bail!("Host capture is not supported on this platform yet")
}

/// Validate remote PNG metadata before handing encoded bytes to a native decoder.
pub fn validate_png(bytes: &[u8]) -> Result<(u32, u32)> {
    ensure!(
        !bytes.is_empty() && bytes.len() <= MAX_PNG_BYTES,
        "Snapshot is too large"
    );
    let decoder = png::Decoder::new_with_limits(
        Cursor::new(bytes),
        png::Limits {
            bytes: 64 * 1024 * 1024,
        },
    );
    let mut reader = decoder.read_info().context("Invalid PNG snapshot")?;
    let info = reader.info();
    let (width, height) = (info.width, info.height);
    ensure!(
        width > 0
            && height > 0
            && (width as usize)
                .checked_mul(height as usize)
                .is_some_and(|count| count <= MAX_PIXELS),
        "Snapshot dimensions exceed limits"
    );
    ensure!(
        info.color_type == png::ColorType::Rgba
            && info.bit_depth == png::BitDepth::Eight
            && !info.interlaced
            && info.animation_control.is_none(),
        "Unsupported snapshot format"
    );
    let size = reader
        .output_buffer_size()
        .context("PNG output size overflow")?;
    ensure!(size <= MAX_PIXELS * 4, "PNG output exceeds limits");
    let mut decoded = vec![0; size];
    reader
        .next_frame(&mut decoded)
        .context("Damaged PNG snapshot")?;
    Ok((width, height))
}

pub fn encode_rgba(width: u32, height: u32, pixels: &[u8]) -> Result<Snapshot> {
    let count = (width as usize)
        .checked_mul(height as usize)
        .context("Capture dimensions overflow")?;
    ensure!(
        count > 0 && count <= MAX_PIXELS && pixels.len() == count * 4,
        "Invalid capture dimensions or pixel buffer"
    );
    let mut output = LimitedOutput(Vec::new());
    {
        let mut encoder = png::Encoder::new(&mut output, width, height);
        encoder.set_compression(png::Compression::Fast);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header()?;
        writer.write_image_data(pixels)?;
        writer.finish()?;
    }
    Ok(Snapshot {
        width,
        height,
        png: output.0,
    })
}

struct LimitedOutput(Vec<u8>);
impl Write for LimitedOutput {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.0.len().saturating_add(bytes.len()) > MAX_PNG_BYTES {
            return Err(std::io::Error::other("Encoded snapshot exceeds 8 MiB"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn png_round_trip_has_bounded_dimensions() {
        let image = encode_rgba(2, 1, &[255, 0, 0, 255, 0, 255, 0, 255]).unwrap();
        assert_eq!(validate_png(&image.png).unwrap(), (2, 1));
        assert!(encode_rgba(3841, 2160, &[]).is_err());
        assert!(validate_png(&[]).is_err());
        assert!(validate_png(b"not a png").is_err());
        assert!(validate_png(&image.png[..image.png.len() / 2]).is_err());
    }
}
