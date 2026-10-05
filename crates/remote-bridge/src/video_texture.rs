//! Stable C ABI for the Flutter Linux texture. No pixels cross the Dart bridge.
use std::sync::Arc;

#[repr(C)]
pub struct VideoPixels {
    pub data: *const u8,
    pub width: u32,
    pub height: u32,
    pub generation: u32,
    pub handle: *const remote_video::Frame,
}

#[unsafe(no_mangle)]
pub extern "C" fn beodesk_video_generation(session: u32) -> u32 {
    crate::live_client::pixels(session).map_or(0, |(generation, _)| generation)
}

/// # Safety
/// `output` must point to writable VideoPixels. Every successful acquisition
/// must be paired with exactly one release, after the renderer finishes using it.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn beodesk_video_acquire(session: u32, output: *mut VideoPixels) -> bool {
    if output.is_null() {
        return false;
    }
    let Some((generation, frame)) = crate::live_client::pixels(session) else {
        return false;
    };
    let pixels = VideoPixels {
        data: frame.rgba.as_ptr(),
        width: frame.width,
        height: frame.height,
        generation,
        handle: Arc::into_raw(frame),
    };
    unsafe {
        output.write(pixels);
    }
    true
}

/// # Safety
/// `handle` must be an unreleased handle returned by beodesk_video_acquire.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn beodesk_video_release(handle: *const remote_video::Frame) {
    if !handle.is_null() {
        drop(unsafe { Arc::from_raw(handle) });
    }
}
