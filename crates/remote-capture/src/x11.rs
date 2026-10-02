use crate::{MAX_PIXELS, Snapshot, encode_rgba};
use anyhow::{Context, Result, ensure};
use x11rb::{
    connection::Connection,
    protocol::xproto::{ConnectionExt, ImageFormat, ImageOrder, VisualClass},
};

pub fn capture() -> Result<Snapshot> {
    let (connection, screen_number) =
        x11rb::connect(None).context("Cannot connect to the X11 display")?;
    let setup = connection.setup();
    let screen = &setup.roots[screen_number];
    let geometry = connection.get_geometry(screen.root)?.reply()?;
    let width = geometry.width as usize;
    let height = geometry.height as usize;
    ensure!(
        width > 0 && height > 0 && width * height <= MAX_PIXELS,
        "Selected desktop exceeds the snapshot pixel limit"
    );
    let visual = screen
        .allowed_depths
        .iter()
        .flat_map(|depth| &depth.visuals)
        .find(|visual| visual.visual_id == screen.root_visual)
        .context("Missing X11 root visual")?;
    let format = setup
        .pixmap_formats
        .iter()
        .find(|format| format.depth == geometry.depth)
        .context("Missing X11 pixel format")?;
    ensure!(
        visual.class == VisualClass::TRUE_COLOR,
        "Only TrueColor X11 desktops are supported"
    );
    ensure!(
        matches!(format.bits_per_pixel, 24 | 32) && matches!(format.scanline_pad, 8 | 16 | 32),
        "Unsupported X11 pixel format"
    );
    let image = connection
        .get_image(
            ImageFormat::Z_PIXMAP,
            screen.root,
            0,
            0,
            geometry.width,
            geometry.height,
            u32::MAX,
        )?
        .reply()?;
    let rgba = convert_pixels(
        &image.data,
        width,
        height,
        format.bits_per_pixel,
        format.scanline_pad,
        setup.image_byte_order == ImageOrder::LSB_FIRST,
        [visual.red_mask, visual.green_mask, visual.blue_mask],
    )?;
    encode_rgba(width as u32, height as u32, &rgba)
}

fn convert_pixels(
    data: &[u8],
    width: usize,
    height: usize,
    bpp: u8,
    pad: u8,
    little_endian: bool,
    masks: [u32; 3],
) -> Result<Vec<u8>> {
    ensure!(
        matches!(bpp, 24 | 32) && matches!(pad, 8 | 16 | 32),
        "Unsupported X11 pixel layout"
    );
    ensure!(
        width > 0
            && height > 0
            && width
                .checked_mul(height)
                .is_some_and(|count| count <= MAX_PIXELS),
        "Invalid image dimensions"
    );
    ensure!(
        masks.iter().all(|mask| *mask != 0)
            && masks[0] & masks[1] == 0
            && masks[0] & masks[2] == 0
            && masks[1] & masks[2] == 0,
        "Invalid color masks"
    );
    for mask in masks {
        let normalized = mask >> mask.trailing_zeros();
        ensure!(
            normalized.count_ones() == normalized.trailing_ones(),
            "Noncontiguous color masks"
        );
    }
    let stride = (width * bpp as usize).div_ceil(pad as usize) * (pad as usize / 8);
    ensure!(
        data.len() == stride * height,
        "Unexpected X11 image buffer length"
    );
    let mut output = Vec::with_capacity(width * height * 4);
    for row in data.chunks_exact(stride) {
        for bytes in row[..width * (bpp as usize / 8)].chunks_exact(bpp as usize / 8) {
            let mut pixel = 0_u32;
            if little_endian {
                for (index, byte) in bytes.iter().enumerate() {
                    pixel |= (*byte as u32) << (index * 8);
                }
            } else {
                for byte in bytes {
                    pixel = (pixel << 8) | *byte as u32;
                }
            }
            for mask in masks {
                let shift = mask.trailing_zeros();
                let maximum = mask >> shift;
                output.push((((pixel & mask) >> shift) as u64 * 255 / maximum as u64) as u8);
            }
            output.push(255);
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn converts_little_endian_bgrx_and_padded_big_endian_rgb() {
        let masks = [0xff0000, 0x00ff00, 0x0000ff];
        assert_eq!(
            convert_pixels(&[3, 2, 1, 0], 1, 1, 32, 32, true, masks).unwrap(),
            [1, 2, 3, 255]
        );
        assert_eq!(
            convert_pixels(&[1, 2, 3, 0], 1, 1, 24, 32, false, masks).unwrap(),
            [1, 2, 3, 255]
        );
        assert!(convert_pixels(&[1, 2], 1, 1, 32, 32, true, masks).is_err());
    }
}
