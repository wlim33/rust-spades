//! This crate provides an implementation of the four person card game, [spades](https://www.pagat.com/auctionwhist/spades.html). 
//! ## Example usage
//! ```
//! extern crate rand;
//! extern crate uuid;
//! extern crate spades;
//! 
//! use std::{io};
//! use spades::{Game, GameTransition, State};
//! use rand::{thread_rng, Rng};
//! 
//! let mut g = Game::new(uuid::Uuid::new_v4(), 
//!    [uuid::Uuid::new_v4(), 
//!     uuid::Uuid::new_v4(), 
//!     uuid::Uuid::new_v4(), 
//!     uuid::Uuid::new_v4()], 
//!     500);
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
//!         let random_card = rng.choose(hand.as_slice()).unwrap();
//!         
//!         g.play(GameTransition::Card(random_card.clone()));
//!     } else {
//!         g.play(GameTransition::Bet(3));
//!     }
//! }
//! assert_eq!(*g.get_state(), State::Completed);
//! ```

extern crate uuid;

mod scoring;
mod game_state;
mod cards;
mod result;

#[cfg(test)]
mod tests;

use uuid::Uuid;
pub use result::*;
pub use cards::*;
pub use game_state::*;

/// The primary way to interface with a spades game. Used as an argument to [Game::play](struct.Game.html#method.play).
pub enum GameTransition {
    Bet(i32),
    Card(Card),
    Start,
}

#[derive(Debug)]
struct Player{
    id: Uuid,
    hand: Vec<Card>
}

impl Player {
    pub fn new(id: Uuid) -> Player {
        Player {
            id: id,
            hand: vec![]
        }
    }
}

/// Primary game state. Internally manages player rotation, scoring, and cards.
#[derive(Debug)]
pub struct Game {
    id: Uuid,
    state: State,
    scoring: scoring::Scoring,
    current_player_index: usize,
    deck: Vec<cards::Card>,
    hands_played: Vec<[cards::Card; 4]>,
    bets_placed: Vec<[i32; 4]>,
    leading_suit: Suit,
    player_a: Player,
    player_b: Player,
    player_c: Player,
    player_d: Player,
}

impl Game {
    pub fn new(id: Uuid, player_ids: [Uuid; 4], max_points: i32) -> Game {
        Game {
            id: id,
            state: State::NotStarted,
            scoring: scoring::Scoring::new(max_points),
            hands_played: vec![new_pot()],
            bets_placed: vec![[0;4]],
            deck: cards::new_deck(),
            current_player_index: 0,
            leading_suit: Suit::Blank,
            player_a: Player::new(player_ids[0]),
            player_b: Player::new(player_ids[1]),
            player_c: Player::new(player_ids[2]),
            player_d: Player::new(player_ids[3]),
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
        if player_id == self.player_a.id {
            return Ok(&self.player_a.hand);
        }        
        if player_id == self.player_a.id {
            return Ok(&self.player_a.hand);
        }        
        if player_id == self.player_a.id {
            return Ok(&self.player_a.hand);
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
            State::Completed => {Err(GetError::GameCompleted)},
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
                if self.scoring.team_a.cumulative_points <= self.scoring.team_b.cumulative_points {
                    return Ok((&self.player_a.id, &self.player_c.id));
                } else {
                    return Ok((&self.player_b.id, &self.player_d.id));
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
                    State::Completed => {
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
                    State::Completed => {
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
