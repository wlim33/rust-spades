//! Spades as a `trick_engine::Ruleset`. Trump (spades) and the spades-broken
//! lead rule live in `legal_plays`/`trick_winner`; bags/nil/termination live in
//! the `scoring` module, owned here as serialized state.

use serde::{Deserialize, Serialize};
use trick_engine::{BidSpec, PlayContext, RoundOutcome, Ruleset, Seat, TeamId};
use trick_notation::Card as TnCard;

use crate::cards::{Card, Suit, from_tn, get_trick_winner, new_deck, to_tn};
use crate::scoring::Scoring;

#[derive(Serialize, Deserialize)]
pub(crate) struct Spades {
    scoring: Scoring,
    #[serde(default)]
    spades_broken: bool,
}

impl Spades {
    pub(crate) fn new(max_points: i32) -> Spades {
        Spades {
            scoring: Scoring::new(max_points),
            spades_broken: false,
        }
    }
    pub(crate) fn scoring(&self) -> &Scoring {
        &self.scoring
    }
    /// Mutable scoring access. Crate-internal escape hatch used by tests to seed
    /// terminal scores when exercising `get_winner_ids` (the public API has no
    /// way to set a score directly).
    #[cfg(test)]
    pub(crate) fn scoring_mut(&mut self) -> &mut Scoring {
        &mut self.scoring
    }
}

#[typetag::serde]
impl Ruleset for Spades {
    fn seat_count(&self) -> usize {
        4
    }
    fn team_of(&self, seat: Seat) -> TeamId {
        TeamId(seat % 2)
    }
    fn build_deck(&self) -> Vec<TnCard> {
        new_deck().into_iter().map(to_tn).collect()
    }
    fn hand_size(&self, _round: usize) -> usize {
        13
    }
    fn first_leader(&self, _round: usize) -> Seat {
        0
    }
    fn bid_phase(&self) -> Option<BidSpec> {
        Some(BidSpec { min: 0, max: 13 })
    }
    fn bid_is_legal(&self, _seat: Seat, bid: i32) -> bool {
        (0..=13).contains(&bid)
    }
    fn legal_plays(&self, ctx: &PlayContext) -> Vec<TnCard> {
        let hand: Vec<Card> = ctx.hand.iter().filter_map(from_tn).collect();
        let leading: Option<Suit> = ctx.table[ctx.leader]
            .as_ref()
            .and_then(from_tn)
            .map(|c| c.suit);
        let legal: Vec<Card> = match leading {
            None => {
                // Leading the trick: can't lead a spade until broken, unless
                // only spades remain.
                if !self.spades_broken && hand.iter().any(|c| c.suit != Suit::Spade) {
                    hand.iter()
                        .filter(|c| c.suit != Suit::Spade)
                        .copied()
                        .collect()
                } else {
                    hand.clone()
                }
            }
            Some(ls) => {
                let following: Vec<Card> = hand.iter().filter(|c| c.suit == ls).copied().collect();
                if following.is_empty() {
                    hand.clone()
                } else {
                    following
                }
            }
        };
        legal.into_iter().map(to_tn).collect()
    }
    fn trick_winner(&self, leader: Seat, played: &[TnCard]) -> Seat {
        let cards: [Card; 4] = std::array::from_fn(|i| from_tn(&played[i]).expect("spades card"));
        get_trick_winner(leader, &cards)
    }
    fn score_round(&mut self, outcome: &RoundOutcome) {
        self.scoring
            .finalize_round(&outcome.tricks_won, &outcome.bids);
        self.spades_broken = false;
    }
    fn is_over(&self) -> bool {
        self.scoring.is_over
    }
    fn scores(&self) -> Vec<i32> {
        vec![
            self.scoring.team_a.cumulative_points,
            self.scoring.team_b.cumulative_points,
        ]
    }
    fn after_play(&mut self, _seat: Seat, card: &TnCard) {
        if matches!(from_tn(card), Some(c) if c.suit == Suit::Spade) {
            self.spades_broken = true;
        }
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{Card, Rank, Suit};
    use trick_engine::Ruleset;

    fn tn(c: Card) -> trick_notation::Card {
        crate::cards::to_tn(c)
    }

    #[test]
    fn spade_trumps_led_suit() {
        let rules = Spades::new(500);
        // Leader (seat 0) leads a high heart; seat 2 plays a low spade and wins.
        let played = vec![
            tn(Card {
                suit: Suit::Heart,
                rank: Rank::Ace,
            }),
            tn(Card {
                suit: Suit::Heart,
                rank: Rank::Two,
            }),
            tn(Card {
                suit: Suit::Spade,
                rank: Rank::Two,
            }),
            tn(Card {
                suit: Suit::Heart,
                rank: Rank::King,
            }),
        ];
        assert_eq!(rules.trick_winner(0, &played), 2);
    }

    #[test]
    fn bid_range_enforced() {
        let rules = Spades::new(500);
        assert!(rules.bid_is_legal(0, 0));
        assert!(rules.bid_is_legal(0, 13));
        assert!(!rules.bid_is_legal(0, 14));
        assert!(!rules.bid_is_legal(0, -1));
    }

    #[test]
    fn leading_spade_blocked_until_broken() {
        let mut rules = Spades::new(500);
        // Construct a PlayContext where actor is LEADING (table has no card for leader yet).
        // Hand has one spade and one heart.
        let spade = tn(Card {
            suit: Suit::Spade,
            rank: Rank::Ace,
        });
        let heart = tn(Card {
            suit: Suit::Heart,
            rank: Rank::King,
        });
        let hand = vec![spade.clone(), heart.clone()];
        // 4-slot table, all None (leader is seat 0, hasn't played yet)
        let table: Vec<Option<TnCard>> = vec![None, None, None, None];
        let ctx = PlayContext {
            hand: &hand,
            table: &table,
            leader: 0,
            round: 0,
        };

        // Before spades broken: only heart is legal
        let legal = rules.legal_plays(&ctx);
        assert_eq!(legal.len(), 1);
        assert_eq!(legal[0], heart);

        // After a spade is played (via after_play hook): spade becomes legal too
        rules.after_play(0, &spade);
        let legal2 = rules.legal_plays(&ctx);
        assert_eq!(legal2.len(), 2);
        assert!(legal2.contains(&spade));
        assert!(legal2.contains(&heart));
    }
}
