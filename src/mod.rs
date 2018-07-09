extern crate uuid;

use game::uuid::Uuid;
use game::player::Player;
use game::error::*;
use game::deck::*;

pub mod deck;
pub mod player;
pub mod scoring;
pub mod error;

#[cfg(test)]
mod tests;

pub enum GameTransition {
    Bet(i32),
    Card(Card),
    Start,
}

#[derive(Debug)]
pub struct Game {
    id: Uuid,
    pub scoring: scoring::ScoringState,
    pub current_player: usize,
    pub rotation_status: usize,
    pub deck: Vec<deck::Card>,
    pub hands_played: Vec<[deck::Card; 4]>,
    pub bets_placed: Vec<[i32; 4]>,
    player_a: Player,
    player_b: Player,
    player_c: Player,
    player_d: Player,
    game_started: bool
}

impl Game {
    pub fn new(id: Uuid, player_ids: [Uuid; 4], max_points: i32) -> Game {
        Game {
            id: id,
            scoring: scoring::ScoringState::new(max_points),
            hands_played: vec![new_pot()],
            bets_placed: vec![[0;4]],
            deck: deck::new(),
            current_player: 0,
            rotation_status: 0,
            player_a: Player::new(player_ids[0]),
            player_b: Player::new(player_ids[1]),
            player_c: Player::new(player_ids[2]),
            player_d: Player::new(player_ids[3]),
            game_started: false
        }
    }
    #[cfg(test)]
    pub fn test_helper_set_hand(&self, player: usize, hand: Vec<Card> ) {
        let mut h = match player {
            0 => & self.player_a.hand,
            1 => & self.player_b.hand,
            2 => & self.player_c.hand,
            3 => & self.player_d.hand,
            _ => & self.player_d.hand,
        };
        h = &hand;
    }

    pub fn get_hand(&self, player: usize) -> &Vec<Card> {
        match player {
            0 => & self.player_a.hand,
            1 => & self.player_b.hand,
            2 => & self.player_c.hand,
            3 => & self.player_d.hand,
            _ => & self.player_d.hand,
        }
    }

    fn deal_cards(&mut self) {
        deck::shuffle(&mut self.deck);
        let mut hands = deck::deal_four_players(&mut self.deck);

        self.player_a.hand = hands.pop().unwrap();
        self.player_b.hand = hands.pop().unwrap();
        self.player_c.hand = hands.pop().unwrap();
        self.player_d.hand = hands.pop().unwrap();
    }

    pub fn play(&mut self, entry: GameTransition) -> Result<Success, TransitionError> {
        match entry {
            GameTransition::Bet(bet) => {
                if self.scoring.is_over {
                    return Err(TransitionError::CompletedGame);
                }
                if !self.scoring.in_betting_stage {
                    return Err(TransitionError::BetInTrickStage);
                }
 
                self.bets_placed.last_mut().unwrap()[self.current_player] = bet;
                
                self.current_player = (self.current_player + 1) % 4;
                self.rotation_status = (self.rotation_status + 1) % 4;
                if self.rotation_status == 0 {
                    self.scoring.bet(*self.bets_placed.last().unwrap());
                    self.bets_placed.push([0;4]);
                    return Ok(Success::BetComplete);
                }

                return Ok(Success::Bet);
            },
            GameTransition::Card(card) => {
                if self.scoring.is_over {
                    return Err(TransitionError::CompletedGame);
                }
                if self.scoring.in_betting_stage {
                    return Err(TransitionError::CardInBettingStage);
                }


                let play_card_result = self.play_card(&card);

                if let Ok(Success::PlayCard) = play_card_result {
                    self.hands_played.last_mut().unwrap()[self.current_player] = card;
                    self.current_player = (self.current_player + 1) % 4;
                    self.rotation_status = (self.rotation_status + 1) % 4;
                    
                    if self.rotation_status == 0 {
                        let winner = self.scoring.trick(self.current_player, self.hands_played.last().unwrap());
                        if self.scoring.is_over { 
                            return Ok(Success::GameOver);
                        }
                        if self.scoring.in_betting_stage {
                            self.deal_cards();
                        } else {
                            self.current_player = winner;
                            self.hands_played.push(new_pot());
                        }
                        print!("new current player: {}\n", self.current_player);

                        return Ok(Success::Trick);
                    }
                    return Ok(Success::PlayCard);
                };
                return play_card_result;
            },
            GameTransition::Start => {
                if self.game_started {
                    return Err(TransitionError::Start);
                }
                self.deal_cards();
                self.game_started = true;
                return Ok(Success::Start);
            }
        }
    }

    fn play_card(&mut self, card: &Card) -> Result<Success, TransitionError> {
        let player_hand = &mut match self.current_player {
            0 => &mut self.player_a,
            1 => &mut self.player_b,
            2 => &mut self.player_c,
            3 => &mut self.player_d,
            _ => &mut self.player_d,
        }.hand;

        if !player_hand.contains(card) {
            return Err(TransitionError::CardNotInHand);
        }

        let card_index = player_hand.iter().position(|x| x == card).unwrap();
        self.deck.push(player_hand.remove(card_index));

        return Ok(Success::PlayCard);
    }
}
