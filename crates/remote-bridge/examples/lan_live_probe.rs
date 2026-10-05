//! End-to-end live session smoke test. The script launches a private Xvfb;
//! this example auto-approves only its own loopback identity and test window.
#[cfg(target_os = "linux")]
fn main() -> anyhow::Result<()> {
    linux::run()
}
#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("This probe requires Linux Xvfb");
}

#[cfg(target_os = "linux")]
mod linux {
    use anyhow::{Result, ensure};
    use remote_bridge::api::{app, lan};
    use std::{
        net::UdpSocket,
        time::{Duration, Instant},
    };
    use x11rb::{
        connection::Connection,
        protocol::{
            Event,
            xproto::{self, ConnectionExt},
        },
        rust_connection::RustConnection,
    };

    fn until(mut predicate: impl FnMut() -> Result<bool>) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if predicate()? {
                return Ok(());
            }
            ensure!(Instant::now() < deadline, "Live probe timed out");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn connect(address: &str, fingerprint: &str) -> Result<u32> {
        let (address, fingerprint) = (address.to_owned(), fingerprint.to_owned());
        let client = std::thread::spawn(move || lan::start_live_session(address, fingerprint));
        until(|| {
            let status = lan::snapshot_host_status()?;
            if status.request_id == 0 {
                return Ok(false);
            }
            ensure!(status.live && status.control, "Wrong consent type");
            lan::respond_to_view_request(status.request_id, true)?;
            Ok(true)
        })?;
        client
            .join()
            .map_err(|_| anyhow::anyhow!("Client worker panicked"))?
    }

    fn frame(id: u32) -> Result<Vec<u8>> {
        let mut bytes = Vec::new();
        until(|| {
            let next = lan::poll_live_frame(id)?;
            ensure!(!next.closed, "Live session closed: {}", next.error);
            if next.frame_id != 0 {
                let mut pixels = std::mem::MaybeUninit::uninit();
                // Own one native frame handle for the duration of this copy.
                if unsafe {
                    remote_bridge::video_texture::beodesk_video_acquire(id, pixels.as_mut_ptr())
                } {
                    let pixels = unsafe { pixels.assume_init() };
                    bytes = unsafe {
                        std::slice::from_raw_parts(
                            pixels.data,
                            pixels.width as usize * pixels.height as usize * 4,
                        )
                    }
                    .to_vec();
                    unsafe {
                        remote_bridge::video_texture::beodesk_video_release(pixels.handle);
                    }
                }
            }
            Ok(!bytes.is_empty())
        })?;
        Ok(bytes)
    }

    fn events(connection: &RustConnection) -> Result<Vec<Event>> {
        let mut received = Vec::new();
        while let Some(event) = connection.poll_for_event()? {
            received.push(event);
        }
        Ok(received)
    }

    pub fn run() -> Result<()> {
        ensure!(
            std::env::var("BEODESK_VIRTUAL_DISPLAY").as_deref() == Ok("1"),
            "Run bash scripts/dev.sh live-smoke to isolate input on Xvfb"
        );
        let (connection, screen_number) = x11rb::connect(None)?;
        let screen = &connection.setup().roots[screen_number];
        let window = connection.generate_id()?;
        connection
            .create_window(
                screen.root_depth,
                window,
                screen.root,
                0,
                0,
                screen.width_in_pixels,
                screen.height_in_pixels,
                0,
                xproto::WindowClass::INPUT_OUTPUT,
                screen.root_visual,
                &xproto::CreateWindowAux::new()
                    .background_pixel(0xff0000)
                    .event_mask(
                        xproto::EventMask::KEY_PRESS
                            | xproto::EventMask::KEY_RELEASE
                            | xproto::EventMask::BUTTON_PRESS
                            | xproto::EventMask::BUTTON_RELEASE
                            | xproto::EventMask::POINTER_MOTION,
                    ),
            )?
            .check()?;
        connection.map_window(window)?.check()?;
        connection
            .set_input_focus(xproto::InputFocus::PARENT, window, x11rb::CURRENT_TIME)?
            .check()?;
        connection.flush()?;
        let device = app::initialize_device()?;
        let socket = UdpSocket::bind("127.0.0.1:0")?;
        let address = socket.local_addr()?.to_string();
        drop(socket);
        lan::start_snapshot_host(address.clone())?;
        struct StopHost;
        impl Drop for StopHost {
            fn drop(&mut self) {
                let _ = lan::cancel_snapshot_request();
                let _ = lan::stop_snapshot_host();
            }
        }
        let _stop = StopHost;
        let id = connect(&address, &device.fingerprint)?;
        ensure!(
            lan::snapshot_host_status()?.active,
            "Host did not show an active session"
        );
        let first = frame(id)?;
        let info = lan::poll_live_frame(id)?;
        let dimensions = (info.width, info.height);
        connection
            .change_window_attributes(
                window,
                &xproto::ChangeWindowAttributesAux::new().background_pixel(0x0000ff),
            )?
            .check()?;
        connection.clear_area(false, window, 0, 0, 0, 0)?.check()?;
        until(|| Ok(frame(id)? != first))?;
        println!(
            "Continuous capture verified: {} x {}, changing H.264 frames decoded to native RGBA",
            dimensions.0, dimensions.1
        );

        let began = Instant::now();
        let initial = lan::poll_live_frame(id)?.frame_id;
        while began.elapsed() < Duration::from_secs(3) {
            lan::poll_live_frame(id)?;
            std::thread::sleep(Duration::from_millis(20));
        }
        let count = lan::poll_live_frame(id)?.frame_id - initial;
        println!(
            "Native H.264 throughput: {:.1} fps over {:.2}s ({}x{})",
            count as f64 / began.elapsed().as_secs_f64(),
            began.elapsed().as_secs_f64(),
            dimensions.0,
            dimensions.1
        );
        ensure!(
            count >= 30,
            "Live throughput regressed below 10 fps on the virtual display"
        );

        events(&connection)?;
        lan::send_live_input(id, 1, 0.25, 0.75, 1, true, 0)?;
        lan::send_live_input(id, 0, 0.5, 0.5, 0, false, 0)?;
        lan::send_live_input(id, 1, 0.5, 0.5, 1, false, 0)?;
        lan::send_live_input(id, 2, 0.0, 0.0, 225, true, 0)?;
        lan::send_live_input(id, 2, 0.0, 0.0, 4, true, 0)?;
        lan::send_live_input(id, 2, 0.0, 0.0, 4, false, 0)?;
        lan::send_live_input(id, 2, 0.0, 0.0, 225, false, 0)?;
        lan::send_live_input(id, 3, 0.0, 0.0, 0, false, 120)?;
        let mut received = Vec::new();
        until(|| {
            lan::poll_live_frame(id)?;
            received.extend(events(&connection)?);
            Ok(received
                .iter()
                .any(|event| matches!(event, Event::ButtonRelease(event) if event.detail == 5)))
        })?;
        let key_presses = received
            .iter()
            .filter_map(|event| {
                if let Event::KeyPress(event) = event {
                    Some(event)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        ensure!(key_presses.len() == 2, "Expected Shift and A key presses");
        ensure!(
            key_presses[1].state.contains(xproto::KeyButMask::SHIFT),
            "A did not receive its Shift modifier"
        );
        let shift_code = key_presses[0].detail;
        let click = received
            .iter()
            .find_map(|event| match event {
                Event::ButtonPress(e) if e.detail == 1 => Some(e),
                _ => None,
            })
            .ok_or_else(|| anyhow::anyhow!("Missing mouse click"))?;
        ensure!(
            (f32::from(click.root_x) - f32::from(screen.width_in_pixels - 1) * 0.25).abs() <= 1.0,
            "Click x was not mapped to the captured desktop"
        );
        ensure!(
            (f32::from(click.root_y) - f32::from(screen.height_in_pixels - 1) * 0.75).abs() <= 1.0,
            "Click y was not mapped to the captured desktop"
        );
        ensure!(received.iter().any(|e| matches!(e, Event::MotionNotify(e) if e.state.contains(xproto::KeyButMask::BUTTON1))), "Missing drag with button held");
        println!("Real XTEST events verified: click, drag, wheel, Shift+A and key-up");

        let held = || -> Result<bool> {
            let pointer = connection.query_pointer(screen.root)?.reply()?;
            let keyboard = connection.query_keymap()?.reply()?;
            Ok(pointer.mask.contains(xproto::KeyButMask::BUTTON1)
                && keyboard.keys[(shift_code / 8) as usize] & (1 << (shift_code % 8)) != 0)
        };
        let released = || -> Result<bool> {
            let pointer = connection.query_pointer(screen.root)?.reply()?;
            let keyboard = connection.query_keymap()?.reply()?;
            Ok(!pointer.mask.contains(xproto::KeyButMask::BUTTON1)
                && keyboard.keys[(shift_code / 8) as usize] & (1 << (shift_code % 8)) == 0)
        };
        lan::send_live_input(id, 1, 0.5, 0.5, 1, true, 0)?;
        lan::send_live_input(id, 2, 0.0, 0.0, 225, true, 0)?;
        until(held)?;
        lan::send_live_input(id, 4, 0.0, 0.0, 0, false, 0)?;
        until(released)?;
        lan::send_live_input(id, 1, 0.5, 0.5, 1, true, 0)?;
        lan::send_live_input(id, 2, 0.0, 0.0, 225, true, 0)?;
        until(held)?;
        // Intentionally stop UI polling: no more input leases may be generated.
        until(|| Ok(!lan::snapshot_host_status()?.active))?;
        ensure!(released()?, "Expired session left X11 input held");
        lan::stop_live_session(id)?;
        println!("Focus release and missed-lease cleanup verified against X11 state");

        let next = connect(&address, &device.fingerprint)?;
        frame(next)?;
        lan::send_live_input(next, 1, 0.5, 0.5, 1, true, 0)?;
        lan::send_live_input(next, 2, 0.0, 0.0, 225, true, 0)?;
        until(held)?;
        lan::stop_snapshot_host()?;
        ensure!(released()?, "Host stop left input held");
        lan::stop_live_session(next)?;
        println!("Reconnect and host Stop sharing cleanup verified");
        Ok(())
    }
}
