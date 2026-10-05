//! Low-delay baseline H.264. Encoded dependencies are never dropped; callers
//! bound in-flight frames and skip capture while the receiver is behind.
use anyhow::{Context, Result, ensure};
use openh264::{
    OpenH264API,
    encoder::{
        BitRate, Complexity, Encoder, EncoderConfig, FrameRate, IntraFramePeriod, Profile,
        RateControlMode, UsageType,
    },
    formats::{RgbaSliceU8, YUVBuffer, YUVSource},
};
use std::time::{Duration, Instant};

pub const MAX_PACKET: usize = 2 * 1024 * 1024;
pub const MAX_WIDTH: usize = 1920;
pub const MAX_HEIGHT: usize = 1080;

pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

pub struct ScreenEncoder {
    encoder: Encoder,
    yuv: YUVBuffer,
    scaled: Vec<u8>,
    tier: usize,
    changed: Instant,
    delay_ms: f64,
}

fn encoder(tier: usize) -> Result<Encoder> {
    Ok(Encoder::with_api_config(
        OpenH264API::from_source(),
        EncoderConfig::new()
            .usage_type(UsageType::ScreenContentRealTime)
            .adaptive_quantization(false)
            .background_detection(false)
            .profile(Profile::Baseline)
            .complexity(Complexity::Low)
            .bitrate(BitRate::from_bps([1_500_000, 3_000_000, 6_000_000][tier]))
            .rate_control_mode(RateControlMode::Bitrate)
            .skip_frames(true)
            .max_frame_rate(FrameRate::from_hz(30.0))
            .num_threads(2)
            .intra_frame_period(IntraFramePeriod::from_num_frames(300)),
    )?)
}

impl ScreenEncoder {
    pub fn new() -> Result<Self> {
        Ok(Self {
            encoder: encoder(2)?,
            yuv: YUVBuffer::new(2, 2),
            scaled: Vec::new(),
            tier: 2,
            changed: Instant::now(),
            delay_ms: 0.0,
        })
    }
    /// Apply sustained receiver backpressure, with hysteresis to avoid repeated
    /// encoder resets/keyframe bursts. A reset starts a new IDR + parameter sets.
    pub fn feedback(&mut self, delay: Duration) -> Result<()> {
        self.delay_ms = self.delay_ms * 0.9 + delay.as_secs_f64() * 100.0;
        let next = if self.delay_ms > 80.0 {
            self.tier.saturating_sub(1)
        } else if self.delay_ms < 30.0 {
            (self.tier + 1).min(2)
        } else {
            self.tier
        };
        if next != self.tier && self.changed.elapsed() > Duration::from_secs(3) {
            self.encoder = encoder(next)?;
            self.tier = next;
            self.changed = Instant::now();
        }
        Ok(())
    }
    pub fn encode(&mut self, width: usize, height: usize, rgba: &[u8]) -> Result<Vec<u8>> {
        ensure!(
            width > 0
                && height > 0
                && width <= 8192
                && height <= 8192
                && width.checked_mul(height).is_some_and(|n| n <= 3840 * 2160)
                && rgba.len() == width * height * 4,
            "Invalid capture dimensions"
        );
        let ratio = (MAX_WIDTH as f64 / width as f64)
            .min(MAX_HEIGHT as f64 / height as f64)
            .min(1.0);
        let w = ((width as f64 * ratio) as usize / 2 * 2).max(2);
        let h = ((height as f64 * ratio) as usize / 2 * 2).max(2);
        if self.yuv.dimensions() != (w, h) {
            self.yuv = YUVBuffer::new(w, h);
        }
        let pixels = if (w, h) == (width, height) {
            rgba
        } else {
            self.scaled.resize(w * h * 4, 0);
            for y in 0..h {
                for x in 0..w {
                    let source = ((y * height / h) * width + x * width / w) * 4;
                    self.scaled[(y * w + x) * 4..(y * w + x + 1) * 4]
                        .copy_from_slice(&rgba[source..source + 4]);
                }
            }
            &self.scaled
        };
        self.yuv.read_rgba8(RgbaSliceU8::new(pixels, (w, h)));
        let packet = self.encoder.encode(&self.yuv)?.to_vec();
        ensure!(
            packet.len() <= MAX_PACKET,
            "Encoded frame exceeds video limit"
        );
        Ok(packet) // Empty means rate control intentionally skipped this capture.
    }
}

pub struct ScreenDecoder(openh264::decoder::Decoder);
impl ScreenDecoder {
    pub fn new() -> Result<Self> {
        Ok(Self(openh264::decoder::Decoder::new()?))
    }
    pub fn decode(&mut self, bytes: &[u8]) -> Result<Frame> {
        validate_packet(bytes)?;
        let yuv = self
            .0
            .decode(bytes)?
            .context("H.264 packet did not produce a frame")?;
        let (w, h) = yuv.dimensions();
        ensure!(
            w > 0 && h > 0 && w <= MAX_WIDTH && h <= MAX_HEIGHT,
            "Decoded frame exceeds limits"
        );
        let mut rgba = vec![0; w * h * 4];
        yuv.write_rgba8(&mut rgba);
        Ok(Frame {
            width: w as u32,
            height: h as u32,
            rgba,
        })
    }
}

// Reject oversized SPS before entering the native decoder, including a forged
// stream with tiny transport metadata but huge coded dimensions/reference count.
fn validate_packet(bytes: &[u8]) -> Result<()> {
    ensure!(
        !bytes.is_empty() && bytes.len() <= MAX_PACKET,
        "Invalid video packet size"
    );
    let mut starts = Vec::new();
    let mut i = 0;
    while i + 3 < bytes.len() {
        if bytes[i..].starts_with(&[0, 0, 1]) {
            starts.push(i + 3);
            i += 3;
        } else {
            i += 1;
        }
    }
    ensure!(
        !starts.is_empty() && starts[0] <= 4,
        "Missing H.264 start code"
    );
    ensure!(starts.len() <= 512, "Too many video NAL units");
    for (index, &start) in starts.iter().enumerate() {
        let end = starts.get(index + 1).map_or(bytes.len(), |n| n - 3);
        let nal = &bytes[start..end];
        ensure!(!nal.is_empty() && nal[0] & 0x80 == 0, "Invalid video NAL");
        match nal[0] & 31 {
            7 => validate_sps(&nal[1..])?,
            1 | 5 | 6 | 8 | 9 | 12 => {}
            _ => anyhow::bail!("Unsupported H.264 NAL type"),
        }
    }
    Ok(())
}

struct Bits {
    data: Vec<u8>,
    pos: usize,
}
impl Bits {
    fn take(&mut self, count: usize) -> Result<u32> {
        ensure!(
            count <= 32 && self.pos + count <= self.data.len() * 8,
            "Truncated H.264 SPS"
        );
        let mut value = 0;
        for _ in 0..count {
            value = (value << 1) | ((self.data[self.pos / 8] >> (7 - self.pos % 8)) & 1) as u32;
            self.pos += 1;
        }
        Ok(value)
    }
    fn ue(&mut self) -> Result<u32> {
        let mut zeros = 0;
        while self.take(1)? == 0 {
            zeros += 1;
            ensure!(zeros < 24, "SPS integer exceeds limit");
        }
        Ok((1 << zeros) - 1 + self.take(zeros)?)
    }
}
fn validate_sps(data: &[u8]) -> Result<()> {
    ensure!(data.len() <= 1024, "SPS exceeds limit");
    let mut unescaped = Vec::new();
    let mut zeros = 0;
    for &byte in data {
        if zeros == 2 && byte == 3 {
            zeros = 0;
            continue;
        }
        unescaped.push(byte);
        zeros = if byte == 0 { zeros + 1 } else { 0 };
    }
    let mut b = Bits {
        data: unescaped,
        pos: 0,
    };
    ensure!(b.take(8)? == 66, "Only baseline H.264 is supported");
    b.take(16)?; // constraints and level
    ensure!(b.ue()? <= 31 && b.ue()? <= 12, "Invalid SPS identifiers");
    match b.ue()? {
        0 => {
            ensure!(b.ue()? <= 12, "Invalid POC width");
        }
        2 => {}
        _ => anyhow::bail!("Unsupported H.264 picture ordering"),
    }
    ensure!(b.ue()? <= 4, "Too many reference frames");
    b.take(1)?;
    let width = (b.ue()? + 1) * 16;
    let height = (b.ue()? + 1) * 16;
    ensure!(
        width <= 1920 && height <= 1088 && b.take(1)? == 1,
        "Coded dimensions exceed video limits"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn consecutive_frames_resize_and_compression() {
        let mut enc = ScreenEncoder::new().unwrap();
        let mut dec = ScreenDecoder::new().unwrap();
        for (w, h, color) in [
            (640, 360, [220, 20, 20, 255]),
            (640, 360, [20, 20, 220, 255]),
            (800, 450, [20, 220, 20, 255]),
        ] {
            let raw = color.repeat(w * h);
            let bytes = enc.encode(w, h, &raw).unwrap();
            assert!(bytes.len() < raw.len() / 20);
            let frame = dec.decode(&bytes).unwrap();
            assert_eq!((frame.width, frame.height), (w as u32, h as u32));
            for (got, want) in frame.rgba[0..3].iter().zip(&color[..3]) {
                assert!((*got as i16 - *want as i16).abs() < 15, "{got} != {want}");
            }
        }
    }
    #[test]
    fn bounds_sps_before_native_allocation() {
        fn sps(width_mbs: u32, height_mbs: u32, refs: u32) -> Vec<u8> {
            let mut bits = String::new();
            // id, frame_num width, POC type 2, references, gap flag, dimensions.
            for v in [0, 0, 2, refs] {
                let binary = format!("{:b}", v + 1);
                bits.push_str(&"0".repeat(binary.len() - 1));
                bits.push_str(&binary);
            }
            bits.push('0');
            for v in [width_mbs - 1, height_mbs - 1] {
                let binary = format!("{:b}", v + 1);
                bits.push_str(&"0".repeat(binary.len() - 1));
                bits.push_str(&binary);
            }
            bits.push('1'); // progressive
            while !bits.len().is_multiple_of(8) {
                bits.push('0');
            }
            let mut data = vec![66, 0, 31];
            data.extend(
                bits.as_bytes()
                    .chunks(8)
                    .map(|chunk| chunk.iter().fold(0, |n, b| (n << 1) | (b - b'0'))),
            );
            data
        }
        assert!(validate_sps(&sps(120, 68, 2)).is_ok());
        assert!(validate_sps(&sps(121, 68, 2)).is_err());
        assert!(validate_sps(&sps(120, 69, 2)).is_err());
        assert!(validate_sps(&sps(120, 68, 17)).is_err());
    }

    #[test]
    fn downscales_4k_and_recovers_after_bitrate_reset() {
        let mut enc = ScreenEncoder::new().unwrap();
        let mut dec = ScreenDecoder::new().unwrap();
        let raw = [30, 180, 30, 255].repeat(3840 * 2160);
        let first = dec.decode(&enc.encode(3840, 2160, &raw).unwrap()).unwrap();
        assert_eq!((first.width, first.height), (1920, 1080));
        enc.changed = Instant::now() - Duration::from_secs(4);
        enc.feedback(Duration::from_secs(1)).unwrap();
        assert_eq!(enc.tier, 1);
        let next = enc.encode(3840, 2160, &raw).unwrap();
        // The adapted encoder must send a new independently decodable keyframe.
        assert_eq!(
            ScreenDecoder::new().unwrap().decode(&next).unwrap().width,
            1920
        );
        assert_eq!(dec.decode(&next).unwrap().height, 1080);
    }

    #[test]
    fn rejects_invalid_unbounded_and_nonbaseline_video() {
        let mut dec = ScreenDecoder::new().unwrap();
        for bytes in [
            vec![],
            vec![0; MAX_PACKET + 1],
            vec![0, 0, 1, 0x67, 100, 0, 31],
            vec![0, 0, 1, 0x67, 66, 0, 31],
        ] {
            assert!(dec.decode(&bytes).is_err());
        }
        assert!(
            ScreenEncoder::new()
                .unwrap()
                .encode(10000, 10000, &[])
                .is_err()
        );
    }
}
