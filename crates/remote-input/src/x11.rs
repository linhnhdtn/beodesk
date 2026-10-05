use anyhow::{Context, Result, ensure};
use remote_protocol::wire::{InputEvent, input_event::Event};
use std::collections::{BTreeMap, BTreeSet};
use x11rb::{
    connection::Connection,
    protocol::{
        xkb,
        xproto::{self, ConnectionExt},
        xtest,
    },
    rust_connection::RustConnection,
};

pub struct InputController {
    connection: RustConnection,
    root: u32,
    keycodes: BTreeMap<u32, u8>,
    keys: BTreeSet<u8>,
    buttons: BTreeSet<u8>,
}

impl InputController {
    pub fn open() -> Result<Self> {
        let (connection, screen) = x11rb::connect(None).context("Cannot open X11 input display")?;
        xtest::get_version(&connection, 2, 2)?
            .reply()
            .context("XTEST input extension is unavailable")?;
        ensure!(
            xkb::use_extension(&connection, 1, 0)?.reply()?.supported,
            "XKB is unavailable"
        );
        let names = xkb::get_names(
            &connection,
            xkb::ID::USE_CORE_KBD.into(),
            xkb::NameDetail::KEY_NAMES,
        )?
        .reply()?;
        let mut keycodes = BTreeMap::new();
        for (offset, name) in names
            .value_list
            .key_names
            .context("XKB key names unavailable")?
            .iter()
            .enumerate()
        {
            for usage in 4..=231 {
                if key_name(usage).is_some_and(|value| padded(value) == name.name) {
                    keycodes.insert(usage, names.first_key + offset as u8);
                }
            }
        }
        ensure!(
            keycodes.contains_key(&4) && keycodes.contains_key(&225),
            "Unsupported XKB keyboard mapping"
        );
        let root = connection.setup().roots[screen].root;
        Ok(Self {
            connection,
            root,
            keycodes,
            keys: BTreeSet::new(),
            buttons: BTreeSet::new(),
        })
    }

    fn fake(&self, kind: u8, detail: u8, x: i16, y: i16) -> Result<()> {
        xtest::fake_input(
            &self.connection,
            kind,
            detail,
            x11rb::CURRENT_TIME,
            self.root,
            x,
            y,
            0,
        )?
        .check()?;
        Ok(())
    }

    fn motion(&self, x: f32, y: f32) -> Result<()> {
        ensure!(
            x.is_finite() && y.is_finite() && (0.0..=1.0).contains(&x) && (0.0..=1.0).contains(&y),
            "Invalid pointer position"
        );
        let geometry = self.connection.get_geometry(self.root)?.reply()?;
        ensure!(
            geometry.width > 0
                && geometry.height > 0
                && geometry.width <= i16::MAX as u16
                && geometry.height <= i16::MAX as u16,
            "Unsupported desktop geometry"
        );
        let x = (x * f32::from(geometry.width - 1)).round() as i16;
        let y = (y * f32::from(geometry.height - 1)).round() as i16;
        self.fake(xproto::MOTION_NOTIFY_EVENT, 0, x, y)
    }

    pub fn inject(&mut self, input: InputEvent) -> Result<()> {
        match input.event.context("Missing input event")? {
            Event::PointerMove(point) => self.motion(point.x, point.y)?,
            Event::PointerButton(button) => {
                ensure!((1..=3).contains(&button.button), "Invalid pointer button");
                self.motion(button.x, button.y)?;
                let button_id = button.button as u8;
                if button.pressed {
                    if self.buttons.insert(button_id) {
                        self.fake(xproto::BUTTON_PRESS_EVENT, button_id, 0, 0)?;
                    }
                } else if self.buttons.contains(&button_id) {
                    self.fake(xproto::BUTTON_RELEASE_EVENT, button_id, 0, 0)?;
                    self.buttons.remove(&button_id);
                }
            }
            Event::Key(key) => {
                // Unsupported consumer/IME keys are ignored, never guessed.
                if let Some(&code) = self.keycodes.get(&key.hid_usage) {
                    if key.pressed {
                        if self.keys.insert(code) {
                            self.fake(xproto::KEY_PRESS_EVENT, code, 0, 0)?;
                        }
                    } else if self.keys.contains(&code) {
                        self.fake(xproto::KEY_RELEASE_EVENT, code, 0, 0)?;
                        self.keys.remove(&code);
                    }
                }
            }
            Event::Wheel(wheel) => {
                ensure!((-1200..=1200).contains(&wheel.delta), "Invalid wheel delta");
                let button = if wheel.delta < 0 { 4 } else { 5 };
                for _ in 0..wheel.delta.unsigned_abs().div_ceil(120) {
                    self.buttons.insert(button);
                    self.fake(xproto::BUTTON_PRESS_EVENT, button, 0, 0)?;
                    self.fake(xproto::BUTTON_RELEASE_EVENT, button, 0, 0)?;
                    self.buttons.remove(&button);
                }
            }
        }
        self.connection.flush()?;
        Ok(())
    }

    /// Release only this session's tracked input, even if the desktop is locked.
    pub fn release_all(&mut self) {
        for code in std::mem::take(&mut self.keys) {
            let _ = self.fake(xproto::KEY_RELEASE_EVENT, code, 0, 0);
        }
        for button in std::mem::take(&mut self.buttons) {
            let _ = self.fake(xproto::BUTTON_RELEASE_EVENT, button, 0, 0);
        }
        let _ = self.connection.flush();
    }
}
impl Drop for InputController {
    fn drop(&mut self) {
        self.release_all();
    }
}

fn padded(name: &str) -> [u8; 4] {
    let mut bytes = [0; 4];
    bytes[..name.len()].copy_from_slice(name.as_bytes());
    bytes
}

// USB HID keyboard-page usages → XKB physical names (independent of keycodes
// and the active layout). The host's layout/modifiers determine the text.
fn key_name(usage: u32) -> Option<&'static str> {
    const LETTERS: [&str; 26] = [
        "AC01", "AB05", "AB03", "AC03", "AD03", "AC04", "AC05", "AC06", "AD08", "AC07", "AC08",
        "AC09", "AB07", "AB06", "AD09", "AD10", "AD01", "AD04", "AC02", "AD05", "AD07", "AB04",
        "AD02", "AB02", "AD06", "AB01",
    ];
    const DIGITS: [&str; 10] = [
        "AE01", "AE02", "AE03", "AE04", "AE05", "AE06", "AE07", "AE08", "AE09", "AE10",
    ];
    const FUNCTION: [&str; 12] = [
        "FK01", "FK02", "FK03", "FK04", "FK05", "FK06", "FK07", "FK08", "FK09", "FK10", "FK11",
        "FK12",
    ];
    Some(match usage {
        4..=29 => LETTERS[(usage - 4) as usize],
        30..=39 => DIGITS[(usage - 30) as usize],
        40 => "RTRN",
        41 => "ESC",
        42 => "BKSP",
        43 => "TAB",
        44 => "SPCE",
        45 => "AE11",
        46 => "AE12",
        47 => "AD11",
        48 => "AD12",
        49 | 50 => "BKSL",
        51 => "AC10",
        52 => "AC11",
        53 => "TLDE",
        54 => "AB08",
        55 => "AB09",
        56 => "AB10",
        57 => "CAPS",
        58..=69 => FUNCTION[(usage - 58) as usize],
        70 => "PRSC",
        71 => "SCLK",
        72 => "PAUS",
        73 => "INS",
        74 => "HOME",
        75 => "PGUP",
        76 => "DELE",
        77 => "END",
        78 => "PGDN",
        79 => "RGHT",
        80 => "LEFT",
        81 => "DOWN",
        82 => "UP",
        83 => "NMLK",
        84 => "KPDV",
        85 => "KPMU",
        86 => "KPSU",
        87 => "KPAD",
        88 => "KPEN",
        89 => "KP1",
        90 => "KP2",
        91 => "KP3",
        92 => "KP4",
        93 => "KP5",
        94 => "KP6",
        95 => "KP7",
        96 => "KP8",
        97 => "KP9",
        98 => "KP0",
        99 => "KPDL",
        100 => "LSGT",
        101 => "MENU",
        224 => "LCTL",
        225 => "LFSH",
        226 => "LALT",
        227 => "LWIN",
        228 => "RCTL",
        229 => "RTSH",
        230 => "RALT",
        231 => "RWIN",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn physical_keys_include_modifiers_navigation_and_printable_rows() {
        assert_eq!(key_name(4), Some("AC01"));
        assert_eq!(key_name(29), Some("AB01"));
        assert_eq!(key_name(39), Some("AE10"));
        assert_eq!(key_name(225), Some("LFSH"));
        assert_eq!(key_name(82), Some("UP"));
        assert_eq!(padded("ESC"), *b"ESC\0");
        assert!(key_name(200).is_none());
    }
}
