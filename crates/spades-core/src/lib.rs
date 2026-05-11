//! This crate provides an implementation of the four person card game, [spades](https://www.pagat.com/auctionwhist/spades.html). 
//! ## Example usage
//! ```
//! use spades::{Game, GameTransition, State};
//! use rand::seq::SliceRandom;
//! use rand::thread_rng;
//!
//! let mut g = Game::new(
//!     uuid::Uuid::new_v4(),
//!     [uuid::Uuid::new_v4(); 4],
//!     100,
//!     None,
//! );
//! g.play(GameTransition::Start).unwrap();
//! let mut rng = thread_rng();
//! while *g.get_state() != State::Completed {
//!     if let State::Trick(_) = *g.get_state() {
//!         let legal = g.get_legal_cards().unwrap();
//!         let card = *legal.choose(&mut rng).unwrap();
//!         g.play(GameTransition::Card(card)).unwrap();
//!     } else {
//!         g.play(GameTransition::Bet(3)).unwrap();
//!     }
//! }
//! assert_eq!(*g.get_state(), State::Completed);
//! ```

#![allow(clippy::large_enum_variant)]

mod scoring;
mod game_state;
mod cards;
mod result;
pub mod ai;

#[cfg(test)]
mod tests;

use std::sync::OnceLock;
use uuid::Uuid;
use sqids::Sqids;
pub use result::*;
pub use cards::*;
pub use game_state::*;

fn sqids_instance() -> &'static Sqids {
    static SQIDS: OnceLock<Sqids> = OnceLock::new();
    SQIDS.get_or_init(|| {
        Sqids::builder()
            .min_length(6)
            .build()
            .expect("valid sqids config")
    })
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

pub fn encode_player_url(game_id: Uuid, player_id: Uuid) -> String {
    let gb = game_id.as_bytes();
    let pb = player_id.as_bytes();
    sqids_instance().encode(&[
        u64::from_be_bytes(gb[0..8].try_into().unwrap()),
        u64::from_be_bytes(gb[8..16].try_into().unwrap()),
        u64::from_be_bytes(pb[0..8].try_into().unwrap()),
        u64::from_be_bytes(pb[8..16].try_into().unwrap()),
    ]).expect("sqids encode")
}

pub fn decode_player_url(s: &str) -> Option<(Uuid, Uuid)> {
    let nums = sqids_instance().decode(s);
    if nums.len() != 4 {
        return None;
    }
    let mut gb = [0u8; 16];
    gb[0..8].copy_from_slice(&nums[0].to_be_bytes());
    gb[8..16].copy_from_slice(&nums[1].to_be_bytes());
    let mut pb = [0u8; 16];
    pb[0..8].copy_from_slice(&nums[2].to_be_bytes());
    pb[8..16].copy_from_slice(&nums[3].to_be_bytes());
    Some((Uuid::from_bytes(gb), Uuid::from_bytes(pb)))
}

/// The primary way to interface with a spades game. Used as an argument to [Game::play](struct.Game.html#method.play).
pub enum GameTransition {
    Bet(i32),
    Card(Card),
    Start,
}

/// Fischer increment timer configuration (X+Y: X minutes initial, Y seconds increment per move).
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "openapi", derive(oasgen::OaSchema))]
pub struct TimerConfig {
    pub initial_time_secs: u64,
    pub increment_secs: u64,
}

/// Remaining clock time for each player in milliseconds.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PlayerClocks {
    pub remaining_ms: [u64; 4],
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct Player {
    id: Uuid,
    hand: Vec<Card>,
    #[serde(default)]
    name: Option<String>,
}

impl Player {
    pub fn new(id: Uuid) -> Player {
        Player {
            id,
            hand: vec![],
            name: None,
        }
    }
}

/// Primary game state. Internally manages player rotation, scoring, and cards.
#[derive(Debug, serde::Serialize)]
pub struct Game {
    id: Uuid,
    state: State,
    scoring: scoring::Scoring,
    current_player_index: usize,
    deck: Vec<cards::Card>,
    hands_played: Vec<[Option<cards::Card>; 4]>,
    leading_suit: Option<Suit>,
    players: [Player; 4],
    #[serde(skip_serializing_if = "Option::is_none")]
    timer_config: Option<TimerConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    player_clocks: Option<PlayerClocks>,
    #[serde(skip_serializing_if = "Option::is_none")]
    turn_started_at_epoch_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_trick_winner: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_completed_trick: Option<[cards::Card; 4]>,
    #[serde(default)]
    spades_broken: bool,
}

impl<'de> serde::Deserialize<'de> for Game {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error as _;
        #[derive(serde::Deserialize)]
        struct GameShim {
            id: Uuid,
            state: State,
            scoring: scoring::Scoring,
            current_player_index: usize,
            deck: Vec<cards::Card>,
            hands_played: Vec<[Option<cards::Card>; 4]>,
            leading_suit: Option<Suit>,
            #[serde(default)]
            players: Option<[Player; 4]>,
            #[serde(default)]
            player_a: Option<Player>,
            #[serde(default)]
            player_b: Option<Player>,
            #[serde(default)]
            player_c: Option<Player>,
            #[serde(default)]
            player_d: Option<Player>,
            #[serde(default)]
            timer_config: Option<TimerConfig>,
            #[serde(default)]
            player_clocks: Option<PlayerClocks>,
            #[serde(default)]
            turn_started_at_epoch_ms: Option<u64>,
            #[serde(default)]
            last_trick_winner: Option<usize>,
            #[serde(default)]
            last_completed_trick: Option<[cards::Card; 4]>,
            #[serde(default)]
            spades_broken: bool,
        }
        let s = GameShim::deserialize(d)?;
        let players = if let Some(ps) = s.players {
            ps
        } else {
            [
                s.player_a.ok_or_else(|| D::Error::missing_field("players or player_a"))?,
                s.player_b.ok_or_else(|| D::Error::missing_field("player_b"))?,
                s.player_c.ok_or_else(|| D::Error::missing_field("player_c"))?,
                s.player_d.ok_or_else(|| D::Error::missing_field("player_d"))?,
            ]
        };
        Ok(Game {
            id: s.id,
            state: s.state,
            scoring: s.scoring,
            current_player_index: s.current_player_index,
            deck: s.deck,
            hands_played: s.hands_played,
            leading_suit: s.leading_suit,
            players,
            timer_config: s.timer_config,
            player_clocks: s.player_clocks,
            turn_started_at_epoch_ms: s.turn_started_at_epoch_ms,
            last_trick_winner: s.last_trick_winner,
            last_completed_trick: s.last_completed_trick,
            spades_broken: s.spades_broken,
        })
    }
}

impl Game {
    pub fn new(id: Uuid, player_ids: [Uuid; 4], max_points: i32, timer_config: Option<TimerConfig>) -> Game {
        let player_clocks = timer_config.map(|tc| PlayerClocks {
            remaining_ms: [tc.initial_time_secs * 1000; 4],
        });
        Game {
            id,
            state: State::NotStarted,
            scoring: scoring::Scoring::new(max_points),
            hands_played: vec![[None; 4]],
            deck: cards::new_deck(),
            current_player_index: 0,
            leading_suit: None,
            players: [
                Player::new(player_ids[0]),
                Player::new(player_ids[1]),
                Player::new(player_ids[2]),
                Player::new(player_ids[3]),
            ],
            timer_config,
            player_clocks,
            turn_started_at_epoch_ms: None,
            last_trick_winner: None,
            last_completed_trick: None,
            spades_broken: false,
        }
    }

    pub fn get_id(&self) -> &Uuid {
        &self.id
    }
    
    /// See [`State`](enum.State.html)
    pub fn get_state(&self) -> &State {
        &self.state
    }

    pub fn get_team_a_score(&self) -> Result<&i32, GetError> {
        match self.state {
            State::NotStarted => Err(GetError::GameNotStarted),
            _ => Ok(&self.scoring.team_a.cumulative_points),
        }
    }

    pub fn get_team_b_score(&self) -> Result<&i32, GetError> {
        match self.state {
            State::NotStarted => Err(GetError::GameNotStarted),
            _ => Ok(&self.scoring.team_b.cumulative_points),
        }
    }

    pub fn get_team_a_bags(&self) -> Result<&i32, GetError> {
        match self.state {
            State::NotStarted => Err(GetError::GameNotStarted),
            _ => Ok(&self.scoring.team_a.bags),
        }
    }

    pub fn get_team_b_bags(&self) -> Result<&i32, GetError> {
        match self.state {
            State::NotStarted => Err(GetError::GameNotStarted),
            _ => Ok(&self.scoring.team_b.bags),
        }
    }
    
    /// Returns `GetError` when the current game is not in the Betting or Trick stages.
    pub fn get_current_player_id(&self) -> Result<&Uuid, GetError> {
        match self.state {
            State::NotStarted => Err(GetError::GameNotStarted),
            State::Completed | State::Aborted => Err(GetError::GameCompleted),
            State::Betting(_) | State::Trick(_) => Ok(&self.players[self.current_player_index].id),
        }
    }

    /// Returns a `GetError::InvalidUuid` if the game does not contain a player with the given `Uuid`.
    pub fn get_hand_by_player_id(&self, player_id: Uuid) -> Result<&Vec<Card>, GetError> {
        self.players
            .iter()
            .find(|p| p.id == player_id)
            .map(|p| &p.hand)
            .ok_or(GetError::InvalidUuid)
    }

    pub fn get_current_hand(&self) -> Result<&Vec<Card>, GetError> {
        match self.state {
            State::NotStarted => Err(GetError::GameNotStarted),
            State::Completed | State::Aborted => Err(GetError::GameCompleted),
            State::Betting(_) | State::Trick(_) => Ok(&self.players[self.current_player_index].hand),
        }
    }

    pub fn get_leading_suit(&self) -> Result<Option<Suit>, GetError> {
        match &self.state {
            State::NotStarted => Err(GetError::GameNotStarted),
            State::Completed => Err(GetError::GameCompleted),
            State::Trick(_) => Ok(self.leading_suit),
            _ => Err(GetError::Unknown),
        }
    }

    /// Returns the cards currently on the table; each slot is `None` if that player
    /// hasn't yet played this trick. Only available in the Trick stage.
    pub fn get_current_trick_cards(&self) -> Result<&[Option<cards::Card>; 4], GetError> {
        match self.state {
            State::NotStarted => Err(GetError::GameNotStarted),
            State::Completed | State::Aborted => Err(GetError::GameCompleted),
            State::Betting(_) => Err(GetError::Unknown),
            State::Trick(_) => Ok(self.hands_played.last().unwrap()),
        }
    }

    #[deprecated(since="1.0.0", note="Please use `get_current_hand` or `get_hand_by_player_id`")]
    pub fn get_hand(&self, player: usize) -> Result<&Vec<Card>, GetError> {
        self.players.get(player).map(|p| &p.hand).ok_or(GetError::InvalidUuid)
    }

    pub fn get_winner_ids(&self) -> Result<(&Uuid, &Uuid), GetError> {
        match self.state {
            State::Completed => {
                if self.scoring.team_a.cumulative_points > self.scoring.team_b.cumulative_points {
                    Ok((&self.players[0].id, &self.players[2].id))
                } else if self.scoring.team_b.cumulative_points > self.scoring.team_a.cumulative_points {
                    Ok((&self.players[1].id, &self.players[3].id))
                } else {
                    // Unreachable: Scoring keeps is_over = false on a tie at max_points,
                    // so the game never transitions to State::Completed with equal scores.
                    Err(GetError::GameNotCompleted)
                }
            },
            _ => {
                Err(GetError::GameNotCompleted)
            }
        }
    }

    /// The primary function used to progress the game state. The first `GameTransition` argument must always be 
    /// [`GameTransition::Start`](enum.GameTransition.html#variant.Start). The stages and player rotations are managed
    /// internally. The order of `GameTransition` arguments should be:
    /// 
    /// Start -> Bet * 4 -> Card * 13 -> Bet * 4 -> Card * 13 -> Bet * 4 -> ...
    pub fn play(&mut self, entry: GameTransition) -> Result<TransitionSuccess, TransitionError> {
        self.last_completed_trick = None;
        match entry {
            GameTransition::Bet(bet) => {
                match self.state {
                    State::NotStarted => {
                        Err(TransitionError::NotStarted)
                    },
                    State::Trick(_rotation_status) => {
                        Err(TransitionError::BetInTrickStage)
                    },
                    State::Completed | State::Aborted => {
                        Err(TransitionError::CompletedGame)
                    },
                    State::Betting(rotation_status) => {
                        self.scoring.add_bet(self.current_player_index,bet);
                        if rotation_status == 3 {
                            self.scoring.bet();
                            self.state = State::Trick((rotation_status + 1) % 4);
                            self.current_player_index = 0;
                            return Ok(TransitionSuccess::BetComplete);
                        } else {
                            self.current_player_index = (self.current_player_index + 1) % 4;
                            self.state = State::Betting((rotation_status + 1) % 4);
                        }

                        Ok(TransitionSuccess::Bet)
                    },
                }
            },
            GameTransition::Card(card) => {
                match self.state {
                    State::NotStarted => {
                        Err(TransitionError::NotStarted)
                    },
                    State::Completed | State::Aborted => {
                        Err(TransitionError::CompletedGame)
                    },
                    State::Betting(_rotation_status) => {
                        Err(TransitionError::CardInBettingStage)
                    },
                    State::Trick(rotation_status) => {
                        {
                            let player_hand = &mut self.players[self.current_player_index].hand;

                            if !player_hand.contains(&card) {
                                return Err(TransitionError::CardNotInHand);
                            }
                            if rotation_status == 0
                                && card.suit == Suit::Spade
                                && !self.spades_broken
                                && player_hand.iter().any(|c| c.suit != Suit::Spade)
                            {
                                return Err(TransitionError::SpadesNotBroken);
                            }
                            if rotation_status == 0 {
                                self.leading_suit = Some(card.suit);
                            } else if let Some(ls) = self.leading_suit
                                && ls != card.suit
                                && player_hand.iter().any(|x| x.suit == ls)
                            {
                                return Err(TransitionError::CardIncorrectSuit);
                            }

                            let card_index = player_hand.iter().position(|x| x == &card).unwrap();
                            self.deck.push(player_hand.remove(card_index));
                        }

                        if card.suit == Suit::Spade {
                            self.spades_broken = true;
                        }
                        self.hands_played.last_mut().unwrap()[self.current_player_index] = Some(card);

                        if rotation_status == 3 {
                            let trick = self.hands_played.last().unwrap();
                            let played: [Card; 4] = [
                                trick[0].unwrap(),
                                trick[1].unwrap(),
                                trick[2].unwrap(),
                                trick[3].unwrap(),
                            ];
                            let winner = self.scoring.trick(self.current_player_index, &played);
                            self.last_trick_winner = Some(winner);
                            self.last_completed_trick = Some(played);
                            if self.scoring.is_over {
                                self.state = State::Completed;
                                return Ok(TransitionSuccess::GameOver);
                            }
                            if self.scoring.in_betting_stage {
                                self.last_trick_winner = None;
                                self.current_player_index = 0;
                                self.state = State::Betting((rotation_status + 1) % 4);
                                self.spades_broken = false;
                                self.leading_suit = None;
                                self.deal_cards();
                            } else {
                                self.current_player_index = winner;
                                self.state = State::Trick((rotation_status + 1) % 4);
                                self.leading_suit = None;
                                self.hands_played.push([None; 4]);
                            }
                            Ok(TransitionSuccess::Trick)
                        } else {
                            self.current_player_index = (self.current_player_index + 1) % 4;
                            self.state = State::Trick((rotation_status + 1) % 4);
                            Ok(TransitionSuccess::PlayCard)
                        }
                    }
                }
            },
            GameTransition::Start => {
                if self.state != State::NotStarted {
                    return Err(TransitionError::AlreadyStarted);
                }
                self.deal_cards();
                self.state = State::Betting(0);
                Ok(TransitionSuccess::Start)
            }
        }
    }

    pub fn set_player_name(&mut self, player_id: Uuid, name: Option<String>) -> Result<(), GetError> {
        let p = self
            .players
            .iter_mut()
            .find(|p| p.id == player_id)
            .ok_or(GetError::InvalidUuid)?;
        p.name = name;
        Ok(())
    }

    pub fn get_player_names(&self) -> [(Uuid, Option<&str>); 4] {
        std::array::from_fn(|i| (self.players[i].id, self.players[i].name.as_deref()))
    }

    pub fn get_timer_config(&self) -> Option<&TimerConfig> {
        self.timer_config.as_ref()
    }

    pub fn get_player_clocks(&self) -> Option<&PlayerClocks> {
        self.player_clocks.as_ref()
    }

    pub fn get_player_clocks_mut(&mut self) -> Option<&mut PlayerClocks> {
        self.player_clocks.as_mut()
    }

    pub fn get_current_player_index_num(&self) -> usize {
        self.current_player_index
    }

    /// Returns true if the game is in the first round's betting phase (round 0, Betting state).
    pub fn is_first_round_betting(&self) -> bool {
        self.scoring.round == 0 && matches!(self.state, State::Betting(_))
    }

    pub fn get_turn_started_at_epoch_ms(&self) -> Option<u64> {
        self.turn_started_at_epoch_ms
    }

    pub fn set_turn_started_at_epoch_ms(&mut self, epoch_ms: Option<u64>) {
        self.turn_started_at_epoch_ms = epoch_ms;
    }

    pub fn get_player_bets(&self) -> Option<[i32; 4]> {
        match self.state {
            State::NotStarted => None,
            _ => Some(self.scoring.bets_placed[self.scoring.round]),
        }
    }

    pub fn get_player_tricks_won(&self) -> Option<[i32; 4]> {
        match self.state {
            State::NotStarted => None,
            _ => Some(self.scoring.player_tricks_won),
        }
    }

    pub fn get_last_trick_winner_id(&self) -> Option<Uuid> {
        self.last_trick_winner.map(|idx| self.players[idx.min(3)].id)
    }

    pub fn get_last_completed_trick(&self) -> Option<&[cards::Card; 4]> {
        self.last_completed_trick.as_ref()
    }

    /// Set the game state directly (used by GameManager for abort).
    pub fn set_state(&mut self, state: State) {
        self.state = state;
    }

    /// Returns the list of legal cards the current player can play.
    /// Only valid in the Trick state.
    pub fn get_legal_cards(&self) -> Result<Vec<Card>, GetError> {
        match &self.state {
            State::Trick(rotation_status) => {
                let hand = self.get_current_hand()?;
                if *rotation_status == 0 {
                    if !self.spades_broken {
                        let non_spades: Vec<Card> =
                            hand.iter().filter(|c| c.suit != Suit::Spade).copied().collect();
                        if !non_spades.is_empty() {
                            return Ok(non_spades);
                        }
                    }
                    Ok(hand.clone())
                } else if let Some(ls) = self.leading_suit {
                    let has_leading_suit = hand.iter().any(|c| c.suit == ls);
                    if has_leading_suit {
                        Ok(hand.iter().filter(|c| c.suit == ls).copied().collect())
                    } else {
                        Ok(hand.clone())
                    }
                } else {
                    Ok(hand.clone())
                }
            }
            _ => Err(GetError::Unknown),
        }
    }

    fn deal_cards(&mut self) {
        cards::shuffle(&mut self.deck);
        let mut hands = cards::deal_four_players(&mut self.deck);
        for i in (0..4).rev() {
            self.players[i].hand = hands.pop().unwrap();
            self.players[i].hand.sort();
        }
    }
}
