use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;
use sqids::Sqids;
use serde::{Serialize, Deserialize};
use tokio::sync::broadcast;
use crate::game_manager::GameManager;
use crate::matchmaking::MatchResult;
use crate::GameTransition;
use crate::TimerConfig;

/// A seat position in a 4-player spades game. Teams: A+C vs B+D.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Seat {
    A,
    B,
    C,
    D,
}

impl Seat {
    pub fn to_index(self) -> usize {
        match self {
            Seat::A => 0,
            Seat::B => 1,
            Seat::C => 2,
            Seat::D => 3,
        }
    }

    pub fn from_index(i: usize) -> Option<Seat> {
        match i {
            0 => Some(Seat::A),
            1 => Some(Seat::B),
            2 => Some(Seat::C),
            3 => Some(Seat::D),
            _ => None,
        }
    }

    pub const ALL: [Seat; 4] = [Seat::A, Seat::B, Seat::C, Seat::D];
}

fn sqids_instance() -> Sqids {
    Sqids::builder()
        .min_length(6)
        .build()
        .expect("valid sqids config")
}

pub fn uuid_to_short_id(uuid: Uuid) -> String {
    let bytes = uuid.as_bytes();
    let high = u64::from_be_bytes(bytes[0..8].try_into().unwrap());
    let low = u64::from_be_bytes(bytes[8..16].try_into().unwrap());
    sqids_instance().encode(&[high, low]).expect("sqids encode")
}

pub fn short_id_to_uuid(short_id: &str) -> Option<Uuid> {
    let nums = sqids_instance().decode(short_id);
    if nums.len() != 2 {
        return None;
    }
    let mut bytes = [0u8; 16];
    bytes[0..8].copy_from_slice(&nums[0].to_be_bytes());
    bytes[8..16].copy_from_slice(&nums[1].to_be_bytes());
    Some(Uuid::from_bytes(bytes))
}

impl FromStr for Seat {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "A" => Ok(Seat::A),
            "B" => Ok(Seat::B),
            "C" => Ok(Seat::C),
            "D" => Ok(Seat::D),
            _ => Err(()),
        }
    }
}

impl fmt::Display for Seat {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Seat::A => write!(f, "A"),
            Seat::B => write!(f, "B"),
            Seat::C => write!(f, "C"),
            Seat::D => write!(f, "D"),
        }
    }
}

/// Configuration for creating a challenge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChallengeConfig {
    #[serde(default = "default_max_points")]
    pub max_points: i32,
    #[serde(default)]
    pub timer_config: Option<TimerConfig>,
    #[serde(default)]
    pub creator_seat: Option<Seat>,
    #[serde(default)]
    pub creator_name: Option<String>,
    #[serde(default = "default_expiry_secs")]
    pub expiry_secs: u64,
}

fn default_max_points() -> i32 {
    500
}

fn default_expiry_secs() -> u64 {
    86400
}

/// Per-seat snapshot sent in SSE events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeatInfo {
    pub seat: Seat,
    pub player_id: Uuid,
    pub name: Option<String>,
}

/// SSE events broadcast to challenge subscribers.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChallengeEvent {
    ChallengeCreated {
        challenge_id: Uuid,
        creator_player_id: Option<Uuid>,
        seats: [Option<SeatInfo>; 4],
        join_urls: HashMap<String, String>,
        expires_at_epoch_secs: u64,
    },
    SeatUpdate {
        challenge_id: Uuid,
        seats: [Option<SeatInfo>; 4],
    },
    GameStart(MatchResult),
    Cancelled {
        challenge_id: Uuid,
        reason: String,
    },
}

/// Status kind of a challenge.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ChallengeStatusKind {
    Open,
    Started { game_id: Uuid },
    Cancelled,
    Expired,
}

/// Full challenge status response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChallengeStatus {
    pub challenge_id: Uuid,
    pub max_points: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timer_config: Option<TimerConfig>,
    pub seats: [Option<SeatInfo>; 4],
    #[serde(flatten)]
    pub status: ChallengeStatusKind,
    pub expires_at_epoch_secs: u64,
}

/// Summary for listing open challenges.
#[derive(Debug, Serialize, Deserialize)]
pub struct ChallengeSummary {
    pub challenge_id: Uuid,
    pub max_points: i32,
    pub seats_filled: usize,
    pub seats: [Option<SeatInfo>; 4],
}

/// Errors from challenge operations.
#[derive(Debug, Serialize, Deserialize)]
pub enum ChallengeError {
    NotFound,
    SeatTaken,
    NotOpen,
    NotCreator,
    InvalidSeat,
    LockError,
    GameCreationFailed(String),
}

impl fmt::Display for ChallengeError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ChallengeError::NotFound => write!(f, "Challenge not found"),
            ChallengeError::SeatTaken => write!(f, "Seat is already taken"),
            ChallengeError::NotOpen => write!(f, "Challenge is not open"),
            ChallengeError::NotCreator => write!(f, "Only the creator can cancel this challenge"),
            ChallengeError::InvalidSeat => write!(f, "Invalid seat"),
            ChallengeError::LockError => write!(f, "Internal lock error"),
            ChallengeError::GameCreationFailed(msg) => write!(f, "Game creation failed: {}", msg),
        }
    }
}

struct SeatOccupant {
    player_id: Uuid,
    name: Option<String>,
}

struct Challenge {
    challenge_id: Uuid,
    creator_id: Option<Uuid>,
    max_points: i32,
    timer_config: Option<TimerConfig>,
    seats: [Option<SeatOccupant>; 4],
    status: ChallengeStatusKindInternal,
    broadcast_tx: broadcast::Sender<ChallengeEvent>,
    expires_at_epoch_secs: u64,
    expiry_handle: Option<tokio::task::JoinHandle<()>>,
}

#[derive(Clone)]
enum ChallengeStatusKindInternal {
    Open,
    Started { game_id: Uuid },
    Cancelled,
    Expired,
}

impl Challenge {
    fn seats_snapshot(&self) -> [Option<SeatInfo>; 4] {
        [
            self.seats[0].as_ref().map(|o| SeatInfo { seat: Seat::A, player_id: o.player_id, name: o.name.clone() }),
            self.seats[1].as_ref().map(|o| SeatInfo { seat: Seat::B, player_id: o.player_id, name: o.name.clone() }),
            self.seats[2].as_ref().map(|o| SeatInfo { seat: Seat::C, player_id: o.player_id, name: o.name.clone() }),
            self.seats[3].as_ref().map(|o| SeatInfo { seat: Seat::D, player_id: o.player_id, name: o.name.clone() }),
        ]
    }

    fn seats_filled(&self) -> usize {
        self.seats.iter().filter(|s| s.is_some()).count()
    }

    fn to_status_kind(&self) -> ChallengeStatusKind {
        match &self.status {
            ChallengeStatusKindInternal::Open => ChallengeStatusKind::Open,
            ChallengeStatusKindInternal::Started { game_id } => ChallengeStatusKind::Started { game_id: *game_id },
            ChallengeStatusKindInternal::Cancelled => ChallengeStatusKind::Cancelled,
            ChallengeStatusKindInternal::Expired => ChallengeStatusKind::Expired,
        }
    }
}

fn epoch_secs_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Manages challenge links ("join game by link").
#[derive(Clone)]
pub struct ChallengeManager {
    game_manager: GameManager,
    challenges: Arc<RwLock<HashMap<Uuid, Challenge>>>,
}

impl ChallengeManager {
    pub fn new(game_manager: GameManager) -> Self {
        ChallengeManager {
            game_manager,
            challenges: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a new challenge. Returns (challenge_id, creator_player_id, broadcast_receiver).
    pub fn create_challenge(
        &self,
        config: ChallengeConfig,
    ) -> Result<(Uuid, Option<Uuid>, broadcast::Receiver<ChallengeEvent>), ChallengeError> {
        let challenge_id = Uuid::new_v4();
        let (broadcast_tx, broadcast_rx) = broadcast::channel(16);

        let expires_at = epoch_secs_now() + config.expiry_secs;

        let mut seats: [Option<SeatOccupant>; 4] = [None, None, None, None];
        let creator_id = config.creator_seat.map(|seat| {
            let player_id = Uuid::new_v4();
            seats[seat.to_index()] = Some(SeatOccupant {
                player_id,
                name: config.creator_name.clone(),
            });
            player_id
        });

        let challenge = Challenge {
            challenge_id,
            creator_id,
            max_points: config.max_points,
            timer_config: config.timer_config,
            seats,
            status: ChallengeStatusKindInternal::Open,
            broadcast_tx,
            expires_at_epoch_secs: expires_at,
            expiry_handle: None,
        };

        let mut challenges = self.challenges.write().map_err(|_| ChallengeError::LockError)?;
        challenges.insert(challenge_id, challenge);
        drop(challenges);

        // Spawn expiry timer
        let mgr = self.clone();
        let handle = tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_secs(config.expiry_secs)).await;
            mgr.expire_challenge(challenge_id);
        });

        let mut challenges = self.challenges.write().map_err(|_| ChallengeError::LockError)?;
        if let Some(c) = challenges.get_mut(&challenge_id) {
            c.expiry_handle = Some(handle);
        }

        Ok((challenge_id, creator_id, broadcast_rx))
    }

    /// Join a specific seat in a challenge. Returns (player_id, broadcast_receiver).
    pub fn join_challenge(
        &self,
        challenge_id: Uuid,
        seat: Seat,
        name: Option<String>,
    ) -> Result<(Uuid, broadcast::Receiver<ChallengeEvent>), ChallengeError> {
        let mut challenges = self.challenges.write().map_err(|_| ChallengeError::LockError)?;
        let challenge = challenges.get_mut(&challenge_id).ok_or(ChallengeError::NotFound)?;

        if !matches!(challenge.status, ChallengeStatusKindInternal::Open) {
            return Err(ChallengeError::NotOpen);
        }

        let idx = seat.to_index();
        if challenge.seats[idx].is_some() {
            return Err(ChallengeError::SeatTaken);
        }

        let player_id = Uuid::new_v4();
        let rx = challenge.broadcast_tx.subscribe();
        challenge.seats[idx] = Some(SeatOccupant {
            player_id,
            name,
        });

        let seats_snapshot = challenge.seats_snapshot();
        let _ = challenge.broadcast_tx.send(ChallengeEvent::SeatUpdate {
            challenge_id,
            seats: seats_snapshot,
        });

        let all_filled = challenge.seats_filled() == 4;

        if all_filled {
            // Extract data needed for game creation before dropping lock
            let player_ids: [Uuid; 4] = [
                challenge.seats[0].as_ref().unwrap().player_id,
                challenge.seats[1].as_ref().unwrap().player_id,
                challenge.seats[2].as_ref().unwrap().player_id,
                challenge.seats[3].as_ref().unwrap().player_id,
            ];
            let player_names: [Option<String>; 4] = [
                challenge.seats[0].as_ref().unwrap().name.clone(),
                challenge.seats[1].as_ref().unwrap().name.clone(),
                challenge.seats[2].as_ref().unwrap().name.clone(),
                challenge.seats[3].as_ref().unwrap().name.clone(),
            ];
            let max_points = challenge.max_points;
            let timer_config = challenge.timer_config;
            let broadcast_tx = challenge.broadcast_tx.clone();

            // Abort expiry timer
            if let Some(handle) = challenge.expiry_handle.take() {
                handle.abort();
            }

            // Drop lock before calling game_manager
            drop(challenges);

            match self.game_manager.create_game_with_players(player_ids, max_points, timer_config) {
                Ok(response) => {
                    if self.game_manager.make_transition(response.game_id, GameTransition::Start).is_err() {
                        let _ = self.game_manager.remove_game(response.game_id);
                    } else {
                        // Set player names
                        for (i, name) in player_names.iter().enumerate() {
                            if name.is_some() {
                                let _ = self.game_manager.set_player_name(response.game_id, player_ids[i], name.clone());
                            }
                        }

                        // Update challenge status
                        if let Ok(mut challenges) = self.challenges.write() {
                            if let Some(c) = challenges.get_mut(&challenge_id) {
                                c.status = ChallengeStatusKindInternal::Started { game_id: response.game_id };
                            }
                        }

                        let _ = broadcast_tx.send(ChallengeEvent::GameStart(MatchResult {
                            game_id: response.game_id,
                            player_id: Uuid::nil(),
                            player_ids,
                            player_names,
                        }));
                    }
                }
                Err(_) => {
                    // Game creation failed — challenge stays open
                }
            }
        }

        Ok((player_id, rx))
    }

    /// Vacate a seat on disconnect. Called by the drop guard.
    pub fn vacate_seat(&self, challenge_id: Uuid, seat: Seat, player_id: Uuid) {
        let mut challenges = match self.challenges.write() {
            Ok(c) => c,
            Err(_) => return,
        };
        let challenge = match challenges.get_mut(&challenge_id) {
            Some(c) => c,
            None => return,
        };

        if !matches!(challenge.status, ChallengeStatusKindInternal::Open) {
            return;
        }

        let idx = seat.to_index();
        if let Some(occupant) = &challenge.seats[idx] {
            if occupant.player_id == player_id {
                challenge.seats[idx] = None;
                let seats_snapshot = challenge.seats_snapshot();
                let _ = challenge.broadcast_tx.send(ChallengeEvent::SeatUpdate {
                    challenge_id,
                    seats: seats_snapshot,
                });
            }
        }
    }

    /// Cancel a challenge. Only the creator can cancel it.
    pub fn cancel_challenge(&self, challenge_id: Uuid, requester_id: Uuid) -> Result<(), ChallengeError> {
        let mut challenges = self.challenges.write().map_err(|_| ChallengeError::LockError)?;
        let challenge = challenges.get_mut(&challenge_id).ok_or(ChallengeError::NotFound)?;

        if !matches!(challenge.status, ChallengeStatusKindInternal::Open) {
            return Err(ChallengeError::NotOpen);
        }

        match challenge.creator_id {
            Some(cid) if cid == requester_id => {}
            _ => return Err(ChallengeError::NotCreator),
        }

        challenge.status = ChallengeStatusKindInternal::Cancelled;
        if let Some(handle) = challenge.expiry_handle.take() {
            handle.abort();
        }

        let _ = challenge.broadcast_tx.send(ChallengeEvent::Cancelled {
            challenge_id,
            reason: "Cancelled by creator".to_string(),
        });

        Ok(())
    }

    /// Get the full status of a challenge.
    pub fn get_status(&self, challenge_id: Uuid) -> Result<ChallengeStatus, ChallengeError> {
        let challenges = self.challenges.read().map_err(|_| ChallengeError::LockError)?;
        let challenge = challenges.get(&challenge_id).ok_or(ChallengeError::NotFound)?;

        Ok(ChallengeStatus {
            challenge_id: challenge.challenge_id,
            max_points: challenge.max_points,
            timer_config: challenge.timer_config,
            seats: challenge.seats_snapshot(),
            status: challenge.to_status_kind(),
            expires_at_epoch_secs: challenge.expires_at_epoch_secs,
        })
    }

    /// List all open challenges.
    pub fn list_challenges(&self) -> Vec<ChallengeSummary> {
        let challenges = match self.challenges.read() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        challenges
            .values()
            .filter(|c| matches!(c.status, ChallengeStatusKindInternal::Open))
            .map(|c| ChallengeSummary {
                challenge_id: c.challenge_id,
                max_points: c.max_points,
                seats_filled: c.seats_filled(),
                seats: c.seats_snapshot(),
            })
            .collect()
    }

    /// Called by the expiry timer task.
    fn expire_challenge(&self, challenge_id: Uuid) {
        let mut challenges = match self.challenges.write() {
            Ok(c) => c,
            Err(_) => return,
        };
        let challenge = match challenges.get_mut(&challenge_id) {
            Some(c) => c,
            None => return,
        };

        if !matches!(challenge.status, ChallengeStatusKindInternal::Open) {
            return;
        }

        challenge.status = ChallengeStatusKindInternal::Expired;
        let _ = challenge.broadcast_tx.send(ChallengeEvent::Cancelled {
            challenge_id,
            reason: "Challenge expired".to_string(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_manager() -> ChallengeManager {
        ChallengeManager::new(GameManager::new())
    }

    #[tokio::test]
    async fn test_create_challenge_basic() {
        let cm = make_manager();
        let config = ChallengeConfig {
            max_points: 500,
            timer_config: None,
            creator_seat: None,
            creator_name: None,
            expiry_secs: 3600,
        };

        let (challenge_id, creator_id, _rx) = cm.create_challenge(config).unwrap();
        assert_ne!(challenge_id, Uuid::nil());
        assert!(creator_id.is_none());

        let status = cm.get_status(challenge_id).unwrap();
        assert!(matches!(status.status, ChallengeStatusKind::Open));
        assert_eq!(status.max_points, 500);
        assert_eq!(status.seats.iter().filter(|s| s.is_some()).count(), 0);
    }

    #[tokio::test]
    async fn test_create_challenge_with_creator_seat() {
        let cm = make_manager();
        let config = ChallengeConfig {
            max_points: 500,
            timer_config: None,
            creator_seat: Some(Seat::A),
            creator_name: Some("Alice".to_string()),
            expiry_secs: 3600,
        };

        let (challenge_id, creator_id, _rx) = cm.create_challenge(config).unwrap();
        assert!(creator_id.is_some());

        let status = cm.get_status(challenge_id).unwrap();
        assert!(status.seats[0].is_some());
        assert_eq!(status.seats[0].as_ref().unwrap().name.as_deref(), Some("Alice"));
        assert_eq!(status.seats[0].as_ref().unwrap().seat, Seat::A);
        assert!(status.seats[1].is_none());
    }

    #[tokio::test]
    async fn test_join_specific_seat() {
        let cm = make_manager();
        let config = ChallengeConfig {
            max_points: 500,
            timer_config: None,
            creator_seat: None,
            creator_name: None,
            expiry_secs: 3600,
        };

        let (challenge_id, _, _rx) = cm.create_challenge(config).unwrap();

        let result = cm.join_challenge(challenge_id, Seat::B, Some("Bob".to_string()));
        assert!(result.is_ok());

        let status = cm.get_status(challenge_id).unwrap();
        assert!(status.seats[1].is_some());
        assert_eq!(status.seats[1].as_ref().unwrap().name.as_deref(), Some("Bob"));
    }

    #[tokio::test]
    async fn test_duplicate_seat_error() {
        let cm = make_manager();
        let config = ChallengeConfig {
            max_points: 500,
            timer_config: None,
            creator_seat: Some(Seat::A),
            creator_name: Some("Alice".to_string()),
            expiry_secs: 3600,
        };

        let (challenge_id, _, _rx) = cm.create_challenge(config).unwrap();

        let result = cm.join_challenge(challenge_id, Seat::A, Some("Bob".to_string()));
        assert!(matches!(result, Err(ChallengeError::SeatTaken)));
    }

    #[tokio::test]
    async fn test_all_4_seats_triggers_game_start() {
        let cm = make_manager();
        let config = ChallengeConfig {
            max_points: 500,
            timer_config: None,
            creator_seat: Some(Seat::A),
            creator_name: Some("Alice".to_string()),
            expiry_secs: 3600,
        };

        let (challenge_id, _, mut creator_rx) = cm.create_challenge(config).unwrap();

        let mut _rxs = Vec::new();
        for (seat, name) in [(Seat::B, "Bob"), (Seat::C, "Carol"), (Seat::D, "Dave")] {
            let (_pid, rx) = cm.join_challenge(challenge_id, seat, Some(name.to_string())).unwrap();
            _rxs.push(rx);
        }

        // Creator should receive game_start via broadcast
        loop {
            let event = creator_rx.recv().await.unwrap();
            match event {
                ChallengeEvent::GameStart(result) => {
                    assert_eq!(result.player_ids.len(), 4);
                    break;
                }
                ChallengeEvent::SeatUpdate { .. } => continue,
                _ => panic!("Unexpected event"),
            }
        }

        let status = cm.get_status(challenge_id).unwrap();
        assert!(matches!(status.status, ChallengeStatusKind::Started { .. }));
    }

    #[tokio::test]
    async fn test_cancel_by_creator() {
        let cm = make_manager();
        let config = ChallengeConfig {
            max_points: 500,
            timer_config: None,
            creator_seat: Some(Seat::A),
            creator_name: None,
            expiry_secs: 3600,
        };

        let (challenge_id, creator_id, _rx) = cm.create_challenge(config).unwrap();

        let result = cm.cancel_challenge(challenge_id, creator_id.unwrap());
        assert!(result.is_ok());

        let status = cm.get_status(challenge_id).unwrap();
        assert!(matches!(status.status, ChallengeStatusKind::Cancelled));
    }

    #[tokio::test]
    async fn test_cancel_by_non_creator_fails() {
        let cm = make_manager();
        let config = ChallengeConfig {
            max_points: 500,
            timer_config: None,
            creator_seat: Some(Seat::A),
            creator_name: None,
            expiry_secs: 3600,
        };

        let (challenge_id, _, _rx) = cm.create_challenge(config).unwrap();

        let result = cm.cancel_challenge(challenge_id, Uuid::new_v4());
        assert!(matches!(result, Err(ChallengeError::NotCreator)));
    }

    #[tokio::test]
    async fn test_vacate_seat_on_disconnect() {
        let cm = make_manager();
        let config = ChallengeConfig {
            max_points: 500,
            timer_config: None,
            creator_seat: None,
            creator_name: None,
            expiry_secs: 3600,
        };

        let (challenge_id, _, _rx) = cm.create_challenge(config).unwrap();

        let (player_id, _join_rx) = cm.join_challenge(challenge_id, Seat::B, Some("Bob".to_string())).unwrap();

        cm.vacate_seat(challenge_id, Seat::B, player_id);

        let status = cm.get_status(challenge_id).unwrap();
        assert!(status.seats[1].is_none());
    }

    #[tokio::test]
    async fn test_join_cancelled_challenge_fails() {
        let cm = make_manager();
        let config = ChallengeConfig {
            max_points: 500,
            timer_config: None,
            creator_seat: Some(Seat::A),
            creator_name: None,
            expiry_secs: 3600,
        };

        let (challenge_id, creator_id, _rx) = cm.create_challenge(config).unwrap();
        cm.cancel_challenge(challenge_id, creator_id.unwrap()).unwrap();

        let result = cm.join_challenge(challenge_id, Seat::B, None);
        assert!(matches!(result, Err(ChallengeError::NotOpen)));
    }

    #[tokio::test]
    async fn test_list_challenges() {
        let cm = make_manager();

        let config1 = ChallengeConfig {
            max_points: 500,
            timer_config: None,
            creator_seat: None,
            creator_name: None,
            expiry_secs: 3600,
        };
        let config2 = ChallengeConfig {
            max_points: 300,
            timer_config: None,
            creator_seat: Some(Seat::A),
            creator_name: Some("Alice".to_string()),
            expiry_secs: 3600,
        };

        let _ = cm.create_challenge(config1).unwrap();
        let _ = cm.create_challenge(config2).unwrap();

        let list = cm.list_challenges();
        assert_eq!(list.len(), 2);
    }

    #[tokio::test]
    async fn test_join_nonexistent_challenge() {
        let cm = make_manager();
        let result = cm.join_challenge(Uuid::new_v4(), Seat::A, None);
        assert!(matches!(result, Err(ChallengeError::NotFound)));
    }

    #[tokio::test]
    async fn test_expire_challenge() {
        let cm = make_manager();
        let config = ChallengeConfig {
            max_points: 500,
            timer_config: None,
            creator_seat: None,
            creator_name: None,
            expiry_secs: 0, // expires immediately
        };

        let (challenge_id, _, _rx) = cm.create_challenge(config).unwrap();

        // Give the expiry timer a moment to fire
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        let status = cm.get_status(challenge_id).unwrap();
        assert!(matches!(status.status, ChallengeStatusKind::Expired));
    }

    #[tokio::test]
    async fn test_vacate_seat_wrong_player_no_op() {
        let cm = make_manager();
        let config = ChallengeConfig {
            max_points: 500,
            timer_config: None,
            creator_seat: None,
            creator_name: None,
            expiry_secs: 3600,
        };

        let (challenge_id, _, _rx) = cm.create_challenge(config).unwrap();
        let (_player_id, _join_rx) = cm.join_challenge(challenge_id, Seat::B, Some("Bob".to_string())).unwrap();

        // Try to vacate with wrong player_id — should be a no-op
        cm.vacate_seat(challenge_id, Seat::B, Uuid::new_v4());

        let status = cm.get_status(challenge_id).unwrap();
        assert!(status.seats[1].is_some()); // seat B still occupied
    }

    #[tokio::test]
    async fn test_vacate_seat_non_open_no_op() {
        let cm = make_manager();
        let config = ChallengeConfig {
            max_points: 500,
            timer_config: None,
            creator_seat: Some(Seat::A),
            creator_name: None,
            expiry_secs: 3600,
        };

        let (challenge_id, creator_id, _rx) = cm.create_challenge(config).unwrap();
        cm.cancel_challenge(challenge_id, creator_id.unwrap()).unwrap();

        // Vacating on a cancelled challenge is a no-op
        cm.vacate_seat(challenge_id, Seat::A, creator_id.unwrap());

        let status = cm.get_status(challenge_id).unwrap();
        assert!(matches!(status.status, ChallengeStatusKind::Cancelled));
        // Seat should still show the occupant (challenge is just cancelled, not cleared)
        assert!(status.seats[0].is_some());
    }

    #[tokio::test]
    async fn test_cancel_already_cancelled_not_open() {
        let cm = make_manager();
        let config = ChallengeConfig {
            max_points: 500,
            timer_config: None,
            creator_seat: Some(Seat::A),
            creator_name: None,
            expiry_secs: 3600,
        };

        let (challenge_id, creator_id, _rx) = cm.create_challenge(config).unwrap();
        cm.cancel_challenge(challenge_id, creator_id.unwrap()).unwrap();

        // Cancelling again should fail with NotOpen
        let result = cm.cancel_challenge(challenge_id, creator_id.unwrap());
        assert!(matches!(result, Err(ChallengeError::NotOpen)));
    }

    #[test]
    fn test_seat_from_index_out_of_range() {
        assert!(Seat::from_index(4).is_none());
        assert!(Seat::from_index(100).is_none());
    }

    #[test]
    fn test_seat_from_index_valid() {
        assert_eq!(Seat::from_index(0), Some(Seat::A));
        assert_eq!(Seat::from_index(1), Some(Seat::B));
        assert_eq!(Seat::from_index(2), Some(Seat::C));
        assert_eq!(Seat::from_index(3), Some(Seat::D));
    }

    #[test]
    fn test_seat_from_str_invalid() {
        assert!("X".parse::<Seat>().is_err());
        assert!("".parse::<Seat>().is_err());
        assert!("AB".parse::<Seat>().is_err());
    }

    #[test]
    fn test_seat_from_str_valid() {
        assert_eq!("A".parse::<Seat>(), Ok(Seat::A));
        assert_eq!("b".parse::<Seat>(), Ok(Seat::B));
        assert_eq!("c".parse::<Seat>(), Ok(Seat::C));
        assert_eq!("D".parse::<Seat>(), Ok(Seat::D));
    }

    #[test]
    fn test_seat_display_all() {
        assert_eq!(format!("{}", Seat::A), "A");
        assert_eq!(format!("{}", Seat::B), "B");
        assert_eq!(format!("{}", Seat::C), "C");
        assert_eq!(format!("{}", Seat::D), "D");
    }

    #[test]
    fn test_seat_to_index() {
        assert_eq!(Seat::A.to_index(), 0);
        assert_eq!(Seat::B.to_index(), 1);
        assert_eq!(Seat::C.to_index(), 2);
        assert_eq!(Seat::D.to_index(), 3);
    }

    #[test]
    fn test_challenge_error_display() {
        assert_eq!(format!("{}", ChallengeError::NotFound), "Challenge not found");
        assert_eq!(format!("{}", ChallengeError::SeatTaken), "Seat is already taken");
        assert_eq!(format!("{}", ChallengeError::NotOpen), "Challenge is not open");
        assert_eq!(format!("{}", ChallengeError::NotCreator), "Only the creator can cancel this challenge");
        assert_eq!(format!("{}", ChallengeError::InvalidSeat), "Invalid seat");
        assert_eq!(format!("{}", ChallengeError::LockError), "Internal lock error");
        assert_eq!(format!("{}", ChallengeError::GameCreationFailed("oops".to_string())), "Game creation failed: oops");
    }

    #[tokio::test]
    async fn test_get_status_not_found() {
        let cm = make_manager();
        let result = cm.get_status(Uuid::new_v4());
        assert!(matches!(result, Err(ChallengeError::NotFound)));
    }

    #[test]
    fn test_uuid_short_id_roundtrip() {
        let uuid = Uuid::new_v4();
        let short = uuid_to_short_id(uuid);
        assert!(short.len() >= 6);
        assert!(short.len() < 36);
        let decoded = short_id_to_uuid(&short).unwrap();
        assert_eq!(uuid, decoded);
    }

    #[test]
    fn test_short_id_invalid_returns_none() {
        assert!(short_id_to_uuid("!!!invalid!!!").is_none());
    }
}
