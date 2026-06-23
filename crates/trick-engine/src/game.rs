//! The generic state machine. Holds a boxed `Ruleset` and drives rounds of
//! deal → (bid) → trick* → score. Cards are opaque; legality and winners come
//! from the ruleset.

use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::ruleset::Ruleset;
use crate::types::{Card, Player, Seat, State};

/// A caller-supplied transition.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Action {
    Start,
    Bid(i32),
    Play(Card),
    Abort,
}

/// A successful transition's classification.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StepOutcome {
    Started,
    Bid,
    BidComplete,
    PlayCard,
    TrickComplete,
    RoundComplete,
    GameOver,
    Aborted,
}

/// A rejected transition. The facade maps `IllegalPlay`/`IllegalBid` onto its
/// own game-specific error variants.
#[derive(Clone, Copy, PartialEq, Eq, Debug, thiserror::Error)]
pub enum StepError {
    #[error("game not started")]
    NotStarted,
    #[error("game already started")]
    AlreadyStarted,
    #[error("action not valid in this phase")]
    WrongPhase,
    #[error("game already completed")]
    Completed,
    #[error("bid rejected by ruleset")]
    IllegalBid,
    #[error("card not in hand")]
    CardNotInHand,
    #[error("card is not a legal play")]
    IllegalPlay,
}

#[derive(Serialize, Deserialize)]
pub struct Game {
    id: Uuid,
    rules: Box<dyn Ruleset>,
    state: State,
    players: Vec<Player>,
    current_seat: Seat,
    trick_leader: Seat,
    deck: Vec<Card>,
    trick: Vec<Option<Card>>,
    history: Vec<Vec<Option<Card>>>,
    bids: Vec<i32>,
    round: usize,
}

impl Game {
    pub fn new(id: Uuid, player_ids: Vec<Uuid>, rules: Box<dyn Ruleset>) -> Game {
        let n = rules.seat_count();
        assert_eq!(player_ids.len(), n, "player count must equal seat_count");
        Game {
            id,
            rules,
            state: State::NotStarted,
            players: player_ids.into_iter().map(Player::new).collect(),
            current_seat: 0,
            trick_leader: 0,
            deck: vec![],
            trick: vec![None; n],
            history: vec![],
            bids: vec![0; n],
            round: 0,
        }
    }

    pub fn id(&self) -> &Uuid {
        &self.id
    }
    pub fn state(&self) -> &State {
        &self.state
    }
    pub fn current_seat(&self) -> Seat {
        self.current_seat
    }
    pub fn seat_count(&self) -> usize {
        self.rules.seat_count()
    }
    pub fn rules(&self) -> &dyn Ruleset {
        self.rules.as_ref()
    }
    pub fn hand(&self, seat: Seat) -> &[Card] {
        &self.players[seat].hand
    }
    pub fn player_id(&self, seat: Seat) -> Uuid {
        self.players[seat].id
    }
    pub fn current_trick(&self) -> &[Option<Card>] {
        &self.trick
    }
    pub fn trick_leader(&self) -> Seat {
        self.trick_leader
    }
    pub fn round(&self) -> usize {
        self.round
    }
    pub fn bids(&self) -> &[i32] {
        &self.bids
    }
    pub fn history(&self) -> &[Vec<Option<Card>>] {
        &self.history
    }
    pub fn player_mut(&mut self, seat: Seat) -> &mut Player {
        &mut self.players[seat]
    }

    fn deal(&mut self) {
        let n = self.rules.seat_count();
        let mut deck = self.rules.build_deck();
        deck.shuffle(&mut rand::rng());
        let hand_size = self.rules.hand_size(self.round);
        for seat in 0..n {
            let start = seat * hand_size;
            self.players[seat].hand = deck[start..start + hand_size].to_vec();
        }
        self.deck = deck;
        self.trick = vec![None; n];
        self.trick_leader = self.rules.first_leader(self.round);
        self.current_seat = self.trick_leader;
    }

    /// Drive the machine. Spades-specific meaning is entirely in `self.rules`.
    pub fn step(&mut self, action: Action) -> Result<StepOutcome, StepError> {
        match action {
            Action::Start => {
                if self.state != State::NotStarted {
                    return Err(StepError::AlreadyStarted);
                }
                self.deal();
                self.state = if self.rules.bid_phase().is_some() {
                    State::Bidding(0)
                } else {
                    State::Trick(0)
                };
                Ok(StepOutcome::Started)
            }
            Action::Abort => match self.state {
                State::Completed | State::Aborted => Err(StepError::Completed),
                _ => {
                    self.state = State::Aborted;
                    Ok(StepOutcome::Aborted)
                }
            },
            // Bid/Play implemented in Tasks 5 and 6.
            Action::Bid(_) | Action::Play(_) => Err(StepError::WrongPhase),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::HighCard;
    use uuid::Uuid;

    fn ids(n: usize) -> Vec<Uuid> {
        (0..n).map(|i| Uuid::from_u128(i as u128 + 1)).collect()
    }

    #[test]
    fn start_deals_and_enters_trick_when_no_bidding() {
        let mut g = Game::new(Uuid::from_u128(99), ids(4), Box::new(HighCard::default()));
        assert_eq!(*g.state(), State::NotStarted);
        let out = g.step(Action::Start).unwrap();
        assert_eq!(out, StepOutcome::Started);
        // HighCard has no bid phase, so we go straight to Trick(0).
        assert_eq!(*g.state(), State::Trick(0));
        // Each of the 4 seats holds 13 cards.
        for seat in 0..4 {
            assert_eq!(g.hand(seat).len(), 13);
        }
        assert_eq!(g.current_seat(), 0); // first_leader
    }

    #[test]
    fn double_start_is_rejected() {
        let mut g = Game::new(Uuid::from_u128(1), ids(4), Box::new(HighCard::default()));
        g.step(Action::Start).unwrap();
        assert_eq!(g.step(Action::Start), Err(StepError::AlreadyStarted));
    }
}
