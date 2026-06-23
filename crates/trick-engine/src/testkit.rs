//! A minimal `Ruleset` used only to exercise the generic engine independently of
//! any real game. `HighCard`: 4 seats, every seat its own team, french-52 deck,
//! 13-card hands, no bidding, must-follow-led-suit, highest rank of the led suit
//! wins, fixed 1-round game.

use serde::{Deserialize, Serialize};
use trick_notation::{Card, Deck};

use crate::ruleset::Ruleset;
use crate::types::{BidSpec, PlayContext, RoundOutcome, Seat, TeamId};

#[derive(Default, Serialize, Deserialize)]
pub struct HighCard {
    #[serde(default)]
    pub rounds_played: usize,
}

const RANK_ORDER: [&str; 13] = [
    "2", "3", "4", "5", "6", "7", "8", "9", "T", "J", "Q", "K", "A",
];

fn rank_value(rank: &str) -> usize {
    RANK_ORDER.iter().position(|r| *r == rank).unwrap_or(0)
}

fn suit_of(card: &Card) -> Option<&str> {
    match card {
        Card::Suited { suit, .. } => Some(suit),
        Card::Special { .. } => None,
    }
}

#[typetag::serde]
impl Ruleset for HighCard {
    fn seat_count(&self) -> usize {
        4
    }
    fn team_of(&self, seat: Seat) -> TeamId {
        TeamId(seat)
    }
    fn build_deck(&self) -> Vec<Card> {
        Deck::french52().cards()
    }
    fn hand_size(&self, _round: usize) -> usize {
        13
    }
    fn first_leader(&self, _round: usize) -> Seat {
        0
    }
    fn bid_phase(&self) -> Option<BidSpec> {
        None
    }
    fn bid_is_legal(&self, _seat: Seat, _bid: i32) -> bool {
        false
    }
    fn legal_plays(&self, ctx: &PlayContext) -> Vec<Card> {
        let led = ctx.table[ctx.leader].as_ref().and_then(suit_of);
        match led {
            Some(led_suit) => {
                let following: Vec<Card> = ctx
                    .hand
                    .iter()
                    .filter(|c| suit_of(c) == Some(led_suit))
                    .cloned()
                    .collect();
                if following.is_empty() {
                    ctx.hand.to_vec()
                } else {
                    following
                }
            }
            None => ctx.hand.to_vec(),
        }
    }
    fn trick_winner(&self, leader: Seat, played: &[Card]) -> Seat {
        let led_suit = suit_of(&played[leader]);
        let mut best = leader;
        for (i, card) in played.iter().enumerate() {
            if suit_of(card) == led_suit
                && rank_value(rank_of(card)) > rank_value(rank_of(&played[best]))
            {
                best = i;
            }
        }
        best
    }
    fn score_round(&mut self, _outcome: &RoundOutcome) {
        self.rounds_played += 1;
    }
    fn is_over(&self) -> bool {
        self.rounds_played >= 1
    }
    fn scores(&self) -> Vec<i32> {
        vec![0; 4]
    }
}

fn rank_of(card: &Card) -> &str {
    match card {
        Card::Suited { rank, .. } => rank,
        Card::Special { .. } => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highcard_serializes_with_tag() {
        let r: Box<dyn Ruleset> = Box::new(HighCard::default());
        let j = serde_json::to_string(&r).unwrap();
        assert!(j.contains("HighCard"), "tagged json: {j}");
        let back: Box<dyn Ruleset> = serde_json::from_str(&j).unwrap();
        assert_eq!(back.seat_count(), 4);
    }
}
