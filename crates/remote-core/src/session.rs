//! Authorization state machine. Transport authentication is a separate prerequisite.
//!
//! A future transport adapter must verify the peer cryptographically before calling
//! `on_authenticated_transport`. A protobuf public key is never authentication.
use remote_protocol::{
    ProtocolError, validate,
    wire::{self, envelope::Body, input_event::Event},
};
use std::{
    collections::BTreeSet,
    time::{Duration, Instant},
};
use thiserror::Error;

pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
pub const CONSENT_TIMEOUT: Duration = Duration::from_secs(60);
pub const INPUT_LEASE: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission {
    ViewOnly,
    Control,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseReason {
    Denied,
    Timeout,
    Disconnected,
    Revoked,
    PermissionLost,
    PeerMismatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    Authenticating,
    AwaitingConsent,
    Active(Permission),
    Closed(CloseReason),
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SessionError {
    #[error("session transition is not permitted")]
    InvalidState,
    #[error("authenticated peer does not match the paired device")]
    PeerMismatch,
    #[error("remote control has not been authorized")]
    NotAuthorized,
    #[error("wrong session, stale transport epoch, or replayed sequence")]
    Replay,
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ReleasedInput {
    pub keys: Vec<u32>,
    pub buttons: Vec<u32>,
}

pub struct HostSession {
    state: SessionState,
    paired_key: [u8; 32],
    session_id: [u8; 16],
    epoch: u64,
    sequence: u64,
    deadline: Instant,
    keys: BTreeSet<u32>,
    buttons: BTreeSet<u32>,
    released: ReleasedInput,
}

impl HostSession {
    pub fn new(
        paired_key: [u8; 32],
        session_id: [u8; 16],
        epoch: u64,
        now: Instant,
    ) -> Result<Self, SessionError> {
        if epoch == 0 || session_id == [0; 16] {
            return Err(SessionError::Replay);
        }
        Ok(Self {
            state: SessionState::Authenticating,
            paired_key,
            session_id,
            epoch,
            sequence: 0,
            deadline: now + CONNECT_TIMEOUT,
            keys: BTreeSet::new(),
            buttons: BTreeSet::new(),
            released: ReleasedInput::default(),
        })
    }

    pub fn state(&self) -> SessionState {
        self.state
    }

    /// Trusted adapter callback ONLY, after TLS/QUIC identity verification.
    /// This checks the verified identity against the local pin; it is not a handshake.
    pub fn on_authenticated_transport(
        &mut self,
        verified_peer_key: [u8; 32],
        now: Instant,
    ) -> Result<(), SessionError> {
        self.tick(now);
        if self.state != SessionState::Authenticating {
            return Err(SessionError::InvalidState);
        }
        if verified_peer_key != self.paired_key {
            self.close(CloseReason::PeerMismatch);
            return Err(SessionError::PeerMismatch);
        }
        self.state = SessionState::AwaitingConsent;
        self.deadline = now + CONSENT_TIMEOUT;
        Ok(())
    }

    /// Called only by the local host UI, never directly from a remote message.
    pub fn approve(&mut self, permission: Permission, now: Instant) -> Result<(), SessionError> {
        self.tick(now);
        if self.state != SessionState::AwaitingConsent {
            return Err(SessionError::InvalidState);
        }
        self.state = SessionState::Active(permission);
        self.deadline = now + INPUT_LEASE;
        Ok(())
    }

    pub fn can_capture(&mut self, now: Instant) -> bool {
        self.tick(now);
        matches!(self.state, SessionState::Active(_))
    }

    /// Reliable input/control stream only. Pointer datagrams need their own sequence space.
    /// Returns an authorized input event for the OS adapter; never injects input itself.
    pub fn receive(
        &mut self,
        message: &wire::Envelope,
        now: Instant,
    ) -> Result<Option<wire::InputEvent>, SessionError> {
        self.tick(now);
        validate(message)?;
        let permission = match self.state {
            SessionState::Active(permission) => permission,
            _ => return Err(SessionError::NotAuthorized),
        };
        if message.session_id != self.session_id
            || message.transport_epoch != self.epoch
            || message.sequence <= self.sequence
        {
            return Err(SessionError::Replay);
        }
        let result = match message.body.as_ref() {
            Some(Body::Input(input)) => {
                if permission != Permission::Control {
                    return Err(SessionError::NotAuthorized);
                }
                match input.event.as_ref() {
                    Some(Event::Key(key)) => {
                        if key.pressed {
                            self.keys.insert(key.hid_usage);
                        } else {
                            self.keys.remove(&key.hid_usage);
                        }
                    }
                    Some(Event::PointerButton(button)) => {
                        if button.pressed {
                            self.buttons.insert(button.button);
                        } else {
                            self.buttons.remove(&button.button);
                        }
                    }
                    _ => {}
                }
                Some(*input)
            }
            Some(Body::Control(control)) => {
                match wire::Action::try_from(control.action) {
                    Ok(wire::Action::InputLease) => self.deadline = now + INPUT_LEASE,
                    Ok(wire::Action::Disconnect) => self.close(CloseReason::Disconnected),
                    Ok(wire::Action::ReleaseInput) => {
                        self.released.keys.extend(std::mem::take(&mut self.keys));
                        self.released
                            .buttons
                            .extend(std::mem::take(&mut self.buttons));
                    }
                    Ok(wire::Action::Ping | wire::Action::Pong | wire::Action::FrameReceived) => {}
                    _ => return Err(SessionError::Protocol(ProtocolError::InvalidPayload)),
                }
                None
            }
            _ => return Err(SessionError::InvalidState),
        };
        self.sequence = message.sequence;
        Ok(result)
    }

    /// The runtime must tick even when the peer sends no packets.
    pub fn tick(&mut self, now: Instant) {
        if !matches!(self.state, SessionState::Closed(_)) && now >= self.deadline {
            self.close(CloseReason::Timeout);
        }
    }

    pub fn close(&mut self, reason: CloseReason) {
        if matches!(self.state, SessionState::Closed(_)) {
            return;
        }
        self.state = SessionState::Closed(reason);
        self.released.keys.extend(std::mem::take(&mut self.keys));
        self.released
            .buttons
            .extend(std::mem::take(&mut self.buttons));
    }

    /// OS adapter must apply these releases after tick/receive/close, even on errors.
    pub fn take_released_input(&mut self) -> ReleasedInput {
        std::mem::take(&mut self.released)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use remote_protocol::{PROTOCOL_MAJOR, PROTOCOL_MINOR};

    fn session(now: Instant, permission: Permission) -> HostSession {
        let mut session = HostSession::new([2; 32], [3; 16], 1, now).unwrap();
        session.on_authenticated_transport([2; 32], now).unwrap();
        session.approve(permission, now).unwrap();
        session
    }
    fn key(sequence: u64) -> wire::Envelope {
        wire::Envelope {
            protocol_major: PROTOCOL_MAJOR,
            protocol_minor: PROTOCOL_MINOR,
            session_id: vec![3; 16],
            transport_epoch: 1,
            sequence,
            body: Some(Body::Input(wire::InputEvent {
                event: Some(Event::Key(wire::Key {
                    hid_usage: 4,
                    pressed: true,
                })),
            })),
        }
    }

    #[test]
    fn capture_and_input_require_both_identity_and_local_consent() {
        let now = Instant::now();
        let mut session = HostSession::new([2; 32], [3; 16], 1, now).unwrap();
        assert!(!session.can_capture(now));
        assert_eq!(
            session.approve(Permission::Control, now),
            Err(SessionError::InvalidState)
        );
        assert_eq!(
            session.receive(&key(1), now),
            Err(SessionError::NotAuthorized)
        );
        session.on_authenticated_transport([2; 32], now).unwrap();
        assert!(!session.can_capture(now));
        assert_eq!(
            session.receive(&key(1), now),
            Err(SessionError::NotAuthorized)
        );
        session.approve(Permission::Control, now).unwrap();
        assert!(session.can_capture(now));
        assert!(session.receive(&key(1), now).unwrap().is_some());
    }

    #[test]
    fn unknown_peer_and_expired_consent_fail_closed() {
        let now = Instant::now();
        let mut unknown = HostSession::new([2; 32], [3; 16], 1, now).unwrap();
        assert_eq!(
            unknown.on_authenticated_transport([9; 32], now),
            Err(SessionError::PeerMismatch)
        );
        assert!(!unknown.can_capture(now));
        let mut expired = HostSession::new([2; 32], [3; 16], 1, now).unwrap();
        expired.on_authenticated_transport([2; 32], now).unwrap();
        assert!(
            expired
                .approve(Permission::Control, now + CONSENT_TIMEOUT)
                .is_err()
        );
        assert!(!expired.can_capture(now + CONSENT_TIMEOUT));
    }

    #[test]
    fn rejects_view_only_input_replay_and_wrong_epoch() {
        let now = Instant::now();
        let mut view = session(now, Permission::ViewOnly);
        assert!(view.can_capture(now));
        assert_eq!(view.receive(&key(1), now), Err(SessionError::NotAuthorized));
        let mut control = session(now, Permission::Control);
        control.receive(&key(1), now).unwrap();
        assert_eq!(control.receive(&key(1), now), Err(SessionError::Replay));
        let mut stale = key(2);
        stale.transport_epoch = 2;
        assert_eq!(control.receive(&stale, now), Err(SessionError::Replay));
    }

    #[test]
    fn lease_timeout_releases_all_held_input_once() {
        let now = Instant::now();
        let mut session = session(now, Permission::Control);
        session.receive(&key(1), now).unwrap();
        let mut button = key(2);
        button.body = Some(Body::Input(wire::InputEvent {
            event: Some(Event::PointerButton(wire::PointerButton {
                button: 1,
                pressed: true,
                x: 0.5,
                y: 0.5,
            })),
        }));
        session.receive(&button, now).unwrap();
        session.tick(now + INPUT_LEASE);
        assert_eq!(session.state(), SessionState::Closed(CloseReason::Timeout));
        assert_eq!(
            session.take_released_input(),
            ReleasedInput {
                keys: vec![4],
                buttons: vec![1]
            }
        );
        session.tick(now + INPUT_LEASE);
        assert_eq!(session.take_released_input(), ReleasedInput::default());
    }

    #[test]
    fn lease_renewal_works_but_cannot_revive_a_closed_session() {
        let now = Instant::now();
        let mut session = session(now, Permission::Control);
        let mut lease = key(1);
        lease.body = Some(Body::Control(wire::SessionControl {
            action: wire::Action::InputLease as i32,
        }));
        session
            .receive(&lease, now + Duration::from_secs(1))
            .unwrap();
        assert!(session.can_capture(now + INPUT_LEASE));
        session.close(CloseReason::Revoked);
        lease.sequence = 2;
        assert_eq!(
            session.receive(&lease, now + INPUT_LEASE),
            Err(SessionError::NotAuthorized)
        );
    }

    #[test]
    fn denial_and_permission_loss_stop_capture() {
        let now = Instant::now();
        for reason in [
            CloseReason::Denied,
            CloseReason::PermissionLost,
            CloseReason::Disconnected,
            CloseReason::Revoked,
        ] {
            let mut session = session(now, Permission::Control);
            session.receive(&key(1), now).unwrap();
            session.close(reason);
            assert!(!session.can_capture(now));
            assert_eq!(session.take_released_input().keys, vec![4]);
            assert!(session.receive(&key(2), now).is_err());
        }
    }
}
