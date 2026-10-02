//! Bounded protobuf control messages. Decoding never grants session permissions.
use prost::Message;
use thiserror::Error;

pub mod wire {
    include!(concat!(env!("OUT_DIR"), "/beodesk.control.rs"));
}

pub const PROTOCOL_MAJOR: u32 = 1;
pub const PROTOCOL_MINOR: u32 = 0;
pub const MAX_CONTROL_BYTES: usize = 64 * 1024;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProtocolError {
    #[error("control message exceeds 64 KiB")]
    TooLarge,
    #[error("invalid protobuf message")]
    Malformed,
    #[error("unsupported protocol major version: {0}")]
    IncompatibleVersion(u32),
    #[error("invalid session ID, transport epoch, or sequence")]
    InvalidSession,
    #[error("invalid or missing message payload")]
    InvalidPayload,
}

/// The transport must check its length prefix before allocating/reading the body.
pub fn validate_length(length: usize) -> Result<(), ProtocolError> {
    if length > MAX_CONTROL_BYTES {
        Err(ProtocolError::TooLarge)
    } else if length == 0 {
        Err(ProtocolError::Malformed)
    } else {
        Ok(())
    }
}

pub fn decode(bytes: &[u8]) -> Result<wire::Envelope, ProtocolError> {
    validate_length(bytes.len())?;
    let envelope = wire::Envelope::decode(bytes).map_err(|_| ProtocolError::Malformed)?;
    validate(&envelope)?;
    Ok(envelope)
}

pub fn encode(envelope: &wire::Envelope) -> Result<Vec<u8>, ProtocolError> {
    validate(envelope)?;
    validate_length(envelope.encoded_len())?;
    Ok(envelope.encode_to_vec())
}

pub fn validate(envelope: &wire::Envelope) -> Result<(), ProtocolError> {
    use wire::{envelope::Body, input_event::Event};
    if envelope.protocol_major != PROTOCOL_MAJOR {
        return Err(ProtocolError::IncompatibleVersion(envelope.protocol_major));
    }
    if envelope.session_id.len() != 16
        || envelope.session_id.iter().all(|byte| *byte == 0)
        || envelope.transport_epoch == 0
        || envelope.sequence == 0
    {
        return Err(ProtocolError::InvalidSession);
    }
    let valid = match envelope.body.as_ref() {
        Some(Body::Handshake(hello)) => {
            hello.public_key.len() == 32
                && !hello.device_name.trim().is_empty()
                && hello.device_name.len() <= 128
                && !hello.device_name.chars().any(char::is_control)
                && (1024..=MAX_CONTROL_BYTES as u32).contains(&hello.max_control_bytes)
                && hello.capabilities.len() <= 16
                && hello.capabilities.iter().all(|value| {
                    wire::Capability::try_from(*value)
                        .is_ok_and(|cap| cap != wire::Capability::Unspecified)
                })
        }
        Some(Body::Input(input)) => match input.event.as_ref() {
            Some(Event::PointerMove(point)) => position_valid(point.x, point.y),
            Some(Event::PointerButton(button)) => {
                (1..=3).contains(&button.button) && position_valid(button.x, button.y)
            }
            Some(Event::Key(key)) => (4..=231).contains(&key.hid_usage),
            Some(Event::Wheel(wheel)) => (-1200..=1200).contains(&wheel.delta),
            None => false,
        },
        Some(Body::Control(control)) => wire::Action::try_from(control.action)
            .is_ok_and(|action| action != wire::Action::Unspecified),
        None => false,
    };
    if valid {
        Ok(())
    } else {
        Err(ProtocolError::InvalidPayload)
    }
}

fn position_valid(x: f32, y: f32) -> bool {
    x.is_finite() && y.is_finite() && (0.0..=1.0).contains(&x) && (0.0..=1.0).contains(&y)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packet() -> wire::Envelope {
        wire::Envelope {
            protocol_major: PROTOCOL_MAJOR,
            protocol_minor: PROTOCOL_MINOR,
            session_id: vec![1; 16],
            transport_epoch: 1,
            sequence: 1,
            body: Some(wire::envelope::Body::Control(wire::SessionControl {
                action: wire::Action::Ping as i32,
            })),
        }
    }

    #[test]
    fn rejects_untrusted_lengths_before_decoding() {
        assert_eq!(
            decode(&vec![0; MAX_CONTROL_BYTES + 1]),
            Err(ProtocolError::TooLarge)
        );
        assert_eq!(decode(&[]), Err(ProtocolError::Malformed));
        assert_eq!(decode(&[0xff]), Err(ProtocolError::Malformed));
    }

    #[test]
    fn versions_allow_new_minor_but_reject_new_major() {
        let mut message = packet();
        message.protocol_minor = 100;
        assert_eq!(decode(&encode(&message).unwrap()).unwrap(), message);
        message.protocol_major += 1;
        assert!(matches!(
            encode(&message),
            Err(ProtocolError::IncompatibleVersion(_))
        ));
    }

    #[test]
    fn unknown_optional_fields_are_ignored() {
        let message = packet();
        let mut bytes = encode(&message).unwrap();
        bytes.extend_from_slice(&[0xa0, 0x06, 0x01]); // field 100, varint 1
        assert_eq!(decode(&bytes).unwrap(), message);
    }

    #[test]
    fn rejects_non_finite_coordinates_and_unknown_actions() {
        let mut message = packet();
        message.body = Some(wire::envelope::Body::Input(wire::InputEvent {
            event: Some(wire::input_event::Event::PointerMove(wire::PointerMove {
                x: f32::NAN,
                y: 0.5,
            })),
        }));
        assert_eq!(encode(&message), Err(ProtocolError::InvalidPayload));
        message.body = Some(wire::envelope::Body::Control(wire::SessionControl {
            action: 999,
        }));
        assert_eq!(encode(&message), Err(ProtocolError::InvalidPayload));
    }

    #[test]
    fn rejects_missing_payload_and_session_metadata() {
        let mut message = packet();
        message.body = None;
        assert_eq!(encode(&message), Err(ProtocolError::InvalidPayload));
        message = packet();
        message.session_id.clear();
        assert_eq!(encode(&message), Err(ProtocolError::InvalidSession));
    }
}
