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

/// Game state. Internally manages player rotation, scoring, and cards.
#[derive(Debug)]
pub struct Game {
    id: Uuid,
    state: State,
    scoring: scoring::Scoring,
    rotation_status: usize,
    deck: Vec<cards::Card>,
    hands_played: Vec<[cards::Card; 4]>,
    bets_placed: Vec<[i32; 4]>,
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
            rotation_status: 0,
            player_a: Player::new(player_ids[0]),
            player_b: Player::new(player_ids[1]),
            player_c: Player::new(player_ids[2]),
            player_d: Player::new(player_ids[3]),
        }
    }

    pub fn get_id(&self) -> &Uuid {
        &self.id
    }
    
    pub fn get_state(&self) -> &State {
        &self.state
    }

    pub fn get_current_player_id(&self) -> Result<&Uuid, GetError>{
        match self.state {
            State::NotStarted => {Err(GetError::GameNotStarted)},
            State::Completed => {Err(GetError::GameCompleted)},
            State::Betting(0) | State::Trick(0) => Ok(&self.player_a.id),
            State::Betting(1) | State::Trick(1) => Ok(&self.player_b.id),
            State::Betting(2) | State::Trick(2) => Ok(&self.player_c.id),
            State::Betting(3) | State::Trick(3) => Ok(&self.player_d.id),
            _ => {Err(GetError::Unknown)}
        }
    }

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
        match self.state {
            State::NotStarted => {Err(GetError::GameNotStarted)},
            State::Completed => {Err(GetError::GameCompleted)},
            State::Betting(0) | State::Trick(0) => Ok(&self.player_a.hand),
            State::Betting(1) | State::Trick(1) => Ok(&self.player_b.hand),
            State::Betting(2) | State::Trick(2) => Ok(&self.player_c.hand),
            State::Betting(3) | State::Trick(3) => Ok(&self.player_d.hand),
            _ => {Err(GetError::Unknown)}

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
                    State::Trick(_index) => {
                        return Err(TransitionError::BetInTrickStage);
                    },
                    State::Completed => {
                        return Err(TransitionError::CompletedGame);
                    },
                    State::Betting(index) => {
                        self.bets_placed.last_mut().unwrap()[index] = bet;
                        self.rotation_status = (self.rotation_status + 1) % 4;
                        if self.rotation_status == 0 {
                            self.scoring.bet(*self.bets_placed.last().unwrap());
                            self.bets_placed.push([0;4]);
                            self.state = State::Trick(0);
                            return Ok(TransitionSuccess::BetComplete);
                        } else {
                            self.state = State::Betting((index + 1) % 4);
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
                    State::Betting(_index) => {
                        return Err(TransitionError::CardInBettingStage)
                    },
                    State::Trick(index) => {
                        {
                            let player_hand = &mut match index {
                                0 => &mut self.player_a,
                                1 => &mut self.player_b,
                                2 => &mut self.player_c,
                                3 => &mut self.player_d,
                                _ => &mut self.player_d,
                            }.hand;

                            if !player_hand.contains(&card) {
                                return Err(TransitionError::CardNotInHand);
                            }

                            let card_index = player_hand.iter().position(|x| x == &card).unwrap();
                            self.deck.push(player_hand.remove(card_index));
                        }
                        
                        self.hands_played.last_mut().unwrap()[index] = card;
                        self.rotation_status = (self.rotation_status + 1) % 4;
                        
                        if self.rotation_status == 0 {
                            let winner = self.scoring.trick(index, self.hands_played.last().unwrap());
                            if self.scoring.is_over {
                                self.state = State::Completed;
                                return Ok(TransitionSuccess::GameOver);
                            }
                            if self.scoring.in_betting_stage {
                                self.state = State::Betting(0);
                                self.deal_cards();
                            } else {
                                self.state = State::Trick(winner);
                                self.hands_played.push(new_pot());
                            }
                            return Ok(TransitionSuccess::Trick);
                        } else {
                            self.state = State::Trick((index + 1) % 4);
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

    pub fn get_winner_ids(&self) -> Result<[&Uuid; 2], GetError> {
        match self.state {
            State::Completed => {
                if self.scoring.team_a.cumulative_points <= self.scoring.team_b.cumulative_points {
                    return Ok([&self.player_a.id, &self.player_c.id]);
                } else {
                    return Ok([&self.player_b.id, &self.player_d.id]);
                }
            },
            _ => {
                Err(GetError::GameNotCompleted)
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
    }
}
