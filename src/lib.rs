//! This crate provides an implementation of the four person card game, [spades](https://www.pagat.com/auctionwhist/spades.html). 
//! ## Example usage
//! ```
//! use std::{io};
//! use spades::{Game, GameTransition, State};
//! use rand::seq::SliceRandom;
//! use rand::thread_rng;
//! 
//! let mut g = Game::new(uuid::Uuid::new_v4(),
//!    [uuid::Uuid::new_v4(),
//!     uuid::Uuid::new_v4(),
//!     uuid::Uuid::new_v4(),
//!     uuid::Uuid::new_v4()],
//!     500, None);
//! 
//! 
//! g.play(GameTransition::Start);
//! 
//! while *g.get_state() != State::Completed {
//!     let mut stdin = io::stdin();
//!     let input = &mut String::new();
//!     let mut rng = thread_rng();
//!     if let State::Trick(_playerindex) = *g.get_state() {
//!         assert!(g.get_current_hand().is_ok());
//!         let hand = g.get_current_hand().ok().unwrap().clone();
//! 
//!         let random_card = hand.as_slice().choose(&mut rng).unwrap();
//!         
//!         g.play(GameTransition::Card(random_card.clone()));
//!     } else {
//!         g.play(GameTransition::Bet(3));
//!     }
//! }
//! assert_eq!(*g.get_state(), State::Completed);
//! ```

mod scoring;
mod game_state;
mod cards;
mod result;

#[cfg(feature = "server")]
pub mod game_manager;

#[cfg(feature = "server")]
pub mod matchmaking;

#[cfg(feature = "server")]
pub mod sqlite_store;

#[cfg(feature = "server")]
pub mod validation;

#[cfg(feature = "server")]
pub mod challenges;

#[cfg(test)]
mod tests;

use uuid::Uuid;
use sqids::Sqids;
pub use result::*;
pub use cards::*;
pub use game_state::*;

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

/// The primary way to interface with a spades game. Used as an argument to [Game::play](struct.Game.html#method.play).
pub enum GameTransition {
    Bet(i32),
    Card(Card),
    Start,
}

/// Fischer increment timer configuration (X+Y: X minutes initial, Y seconds increment per move).
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
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
struct Player{
    id: Uuid,
    hand: Vec<Card>,
    #[serde(default)]
    name: Option<String>,
}

impl Player {
    pub fn new(id: Uuid) -> Player {
        Player {
            id: id,
            hand: vec![],
            name: None,
        }
    }
}

/// Primary game state. Internally manages player rotation, scoring, and cards.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Game {
    id: Uuid,
    state: State,
    scoring: scoring::Scoring,
    current_player_index: usize,
    deck: Vec<cards::Card>,
    hands_played: Vec<[cards::Card; 4]>,
    leading_suit: Suit,
    player_a: Player,
    player_b: Player,
    player_c: Player,
    player_d: Player,
    #[serde(default)]
    timer_config: Option<TimerConfig>,
    #[serde(default)]
    player_clocks: Option<PlayerClocks>,
    #[serde(default)]
    turn_started_at_epoch_ms: Option<u64>,
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
            hands_played: vec![new_pot()],
            deck: cards::new_deck(),
            current_player_index: 0,
            leading_suit: Suit::Blank,
            player_a: Player::new(player_ids[0]),
            player_b: Player::new(player_ids[1]),
            player_c: Player::new(player_ids[2]),
            player_d: Player::new(player_ids[3]),
            timer_config,
            player_clocks,
            turn_started_at_epoch_ms: None,
        }
    }

    pub fn get_id(&self) -> &Uuid {
        &self.id
    }
    
    /// See [`State`](enum.State.html)
    pub fn get_state(&self) -> &State {
        &self.state
    }

    pub fn get_team_a_score(&self) ->  Result<&i32, GetError> {
        match (&self.state, self.current_player_index) {
            (State::NotStarted, _) => {Err(GetError::GameNotStarted)},
            _ => {Ok(&self.scoring.team_a.cumulative_points)}
        }
    }

    pub fn get_team_b_score(&self) ->  Result<&i32, GetError> {
        match (&self.state, self.current_player_index) {
            (State::NotStarted, _) => {Err(GetError::GameNotStarted)},
            _ => {Ok(&self.scoring.team_b.cumulative_points)}
        }
    }

    pub fn get_team_a_bags(&self) -> Result<&i32, GetError> {
        match self.state {
            State::NotStarted => {Err(GetError::GameNotStarted)},
            _ => {Ok(&self.scoring.team_a.bags)}
        }
    }

    pub fn get_team_b_bags(&self) -> Result<&i32, GetError> {
        match self.state {
            State::NotStarted => {Err(GetError::GameNotStarted)},
            _ => {Ok(&self.scoring.team_b.bags)}
        }
    }
    
    /// Returns `GetError` when the current game is not in the Betting or Trick stages.
    pub fn get_current_player_id(&self) -> Result<&Uuid, GetError>{
        match (&self.state, self.current_player_index) {
            (State::NotStarted, _) => {Err(GetError::GameNotStarted)},
            (State::Completed, _) => {Err(GetError::GameCompleted)},
            (State::Betting(_), 0) | (State::Trick(_), 0) => Ok(&self.player_a.id),
            (State::Betting(_), 1) | (State::Trick(_), 1) => Ok(&self.player_b.id),
            (State::Betting(_), 2) | (State::Trick(_), 2) => Ok(&self.player_c.id),
            (State::Betting(_), 3) | (State::Trick(_), 3) => Ok(&self.player_d.id),
            _ => {Err(GetError::Unknown)}
        }
    }

    /// Returns a `GetError::InvalidUuid` if the game does not contain a player with the given `Uuid`.
    pub fn get_hand_by_player_id(&self, player_id: Uuid) -> Result<&Vec<Card>, GetError> {
        if player_id == self.player_a.id {
            return Ok(&self.player_a.hand);
        }
        if player_id == self.player_b.id {
            return Ok(&self.player_b.hand);
        }        
        if player_id == self.player_c.id {
            return Ok(&self.player_c.hand);
        }        
        if player_id == self.player_d.id {
            return Ok(&self.player_d.hand);
        }

        return Err(GetError::InvalidUuid);
    }
    
    pub fn get_current_hand(&self) -> Result<&Vec<Card>, GetError> {
        match (&self.state, self.current_player_index) {
            (State::NotStarted, _) => {Err(GetError::GameNotStarted)},
            (State::Completed, _) => {Err(GetError::GameCompleted)},
            (State::Betting(_), 0) | (State::Trick(_), 0) => Ok(&self.player_a.hand),
            (State::Betting(_), 1) | (State::Trick(_), 1) => Ok(&self.player_b.hand),
            (State::Betting(_), 2) | (State::Trick(_), 2) => Ok(&self.player_c.hand),
            (State::Betting(_), 3) | (State::Trick(_), 3) => Ok(&self.player_d.hand),
            _ => {Err(GetError::Unknown)}
        }
    }

    pub fn get_leading_suit(&self) -> Result<&Suit, GetError> {
        match &self.state {
            State::NotStarted => {Err(GetError::GameNotStarted)},
            State::Completed => {Err(GetError::GameCompleted)},
            State::Trick(_) => Ok(&self.leading_suit),
            _ => {Err(GetError::Unknown)}
        }
    }

    /// Returns an array with (only if in the trick stage).
    pub fn get_current_trick_cards(&self) -> Result<&[cards::Card; 4], GetError> {
        match self.state {
            State::NotStarted => {Err(GetError::GameNotStarted)},
            State::Completed | State::Aborted => {Err(GetError::GameCompleted)},
            State::Betting(_) => {Err(GetError::GameCompleted)},
            State::Trick(_) => {Ok(self.hands_played.last().unwrap())},
        }
    }

    #[deprecated(since="1.0.0", note="Please use `get_current_hand` or `get_hand_by_player_id`")]
    pub fn get_hand(&self, player: usize) -> Result<&Vec<Card>, GetError> {
        match player {
            0 => Ok(&self.player_a.hand),
            1 => Ok(&self.player_b.hand),
            2 => Ok(&self.player_c.hand),
            3 => Ok(&self.player_d.hand),
            _ => Ok(&self.player_d.hand),
        }
    }

    pub fn get_winner_ids(&self) -> Result<(&Uuid, &Uuid), GetError> {
        match self.state {
            State::Completed => {
                if self.scoring.team_a.cumulative_points > self.scoring.team_b.cumulative_points {
                    return Ok((&self.player_a.id, &self.player_c.id));
                } else if self.scoring.team_b.cumulative_points > self.scoring.team_a.cumulative_points {
                    return Ok((&self.player_b.id, &self.player_d.id));
                } else {
                    // Tie should not happen (is_over prevents it), but guard against it
                    return Err(GetError::GameNotCompleted);
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
        match entry {
            GameTransition::Bet(bet) => {
                match self.state {
                    State::NotStarted => {
                        return Err(TransitionError::NotStarted);
                    },
                    State::Trick(_rotation_status) => {
                        return Err(TransitionError::BetInTrickStage);
                    },
                    State::Completed | State::Aborted => {
                        return Err(TransitionError::CompletedGame);
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

                        return Ok(TransitionSuccess::Bet);
                    },
                };
            },
            GameTransition::Card(card) => {
                match self.state {
                    State::NotStarted => {
                        return Err(TransitionError::NotStarted);
                    },
                    State::Completed | State::Aborted => {
                        return Err(TransitionError::CompletedGame);
                    },
                    State::Betting(_rotation_status) => {
                        return Err(TransitionError::CardInBettingStage)
                    },
                    State::Trick(rotation_status) => {
                        {
                            let player_hand = &mut match self.current_player_index {
                                0 => &mut self.player_a,
                                1 => &mut self.player_b,
                                2 => &mut self.player_c,
                                3 => &mut self.player_d,
                                _ => &mut self.player_d,
                            }.hand;

                            if !player_hand.contains(&card) {
                                return Err(TransitionError::CardNotInHand);
                            }
                            let leading_suit = self.leading_suit;
                            if rotation_status == 0 {
                                self.leading_suit = card.suit;
                            }
                            if self.leading_suit != card.suit && player_hand.iter().any(|ref x| x.suit == leading_suit) {
                                return Err(TransitionError::CardIncorrectSuit);
                            }

                            let card_index = player_hand.iter().position(|x| x == &card).unwrap();
                            self.deck.push(player_hand.remove(card_index));
                        }
                        
                        self.hands_played.last_mut().unwrap()[self.current_player_index] = card;
                        
                        if rotation_status == 3 {
                            let winner = self.scoring.trick(self.current_player_index, self.hands_played.last().unwrap());
                            if self.scoring.is_over {
                                self.state = State::Completed;
                                return Ok(TransitionSuccess::GameOver);
                            }
                            if self.scoring.in_betting_stage {
                                self.current_player_index = 0;
                                self.state = State::Betting((rotation_status + 1) % 4);
                                self.deal_cards();
                            } else {
                                self.current_player_index = winner;
                                self.state = State::Trick((rotation_status + 1) % 4);
                                self.hands_played.push(new_pot());
                            }
                            return Ok(TransitionSuccess::Trick);
                        } else {
                            self.current_player_index = (self.current_player_index + 1) % 4;
                            self.state = State::Trick((rotation_status + 1) % 4);
                            return Ok(TransitionSuccess::PlayCard);
                        }
                    }
                };
            },
            GameTransition::Start => {
                if self.state != State::NotStarted {
                    return Err(TransitionError::AlreadyStarted);
                }
                self.deal_cards();
                self.state = State::Betting(0);
                return Ok(TransitionSuccess::Start);
            }
        }
    }

    pub fn set_player_name(&mut self, player_id: Uuid, name: Option<String>) -> Result<(), GetError> {
        if player_id == self.player_a.id {
            self.player_a.name = name;
        } else if player_id == self.player_b.id {
            self.player_b.name = name;
        } else if player_id == self.player_c.id {
            self.player_c.name = name;
        } else if player_id == self.player_d.id {
            self.player_d.name = name;
        } else {
            return Err(GetError::InvalidUuid);
        }
        Ok(())
    }

    pub fn get_player_names(&self) -> [(Uuid, Option<&str>); 4] {
        [
            (self.player_a.id, self.player_a.name.as_deref()),
            (self.player_b.id, self.player_b.name.as_deref()),
            (self.player_c.id, self.player_c.name.as_deref()),
            (self.player_d.id, self.player_d.name.as_deref()),
        ]
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
                    // First card in trick: any card is legal
                    Ok(hand.clone())
                } else {
                    // Must follow leading suit if possible
                    let has_leading_suit = hand.iter().any(|c| c.suit == self.leading_suit);
                    if has_leading_suit {
                        Ok(hand.iter().filter(|c| c.suit == self.leading_suit).cloned().collect())
                    } else {
                        Ok(hand.clone())
                    }
                }
            }
            _ => Err(GetError::Unknown),
        }
    }

    fn deal_cards(&mut self) {
        cards::shuffle(&mut self.deck);
        let mut hands = cards::deal_four_players(&mut self.deck);

        self.player_a.hand = hands.pop().unwrap();
        self.player_b.hand = hands.pop().unwrap();
        self.player_c.hand = hands.pop().unwrap();
        self.player_d.hand = hands.pop().unwrap();

        self.player_a.hand.sort();
        self.player_b.hand.sort();
        self.player_c.hand.sort();
        self.player_d.hand.sort();
    }
}
