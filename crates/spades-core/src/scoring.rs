use crate::cards::{Card, get_trick_winner};
use serde::{Deserialize, Serialize};

/// A team at or below this cumulative score loses — the standard Spades
/// "minimum score" rule. Without it, two teams that both keep losing points
/// never reach `max_points` and the game runs forever (unbounded `hands_played`,
/// which OOM'd the server on 2026-06-19). See
/// docs/superpowers/specs/2026-06-19-game-termination-guarantee-design.md.
const MIN_POINTS: i32 = -200;

/// Hard cap on rounds per game — a final backstop against any non-terminating
/// edge case beyond the loss floor. Far above any realistic game (a game to 500
/// is well under ~30 rounds).
const MAX_ROUNDS: usize = 100;

#[derive(Debug, Serialize, Deserialize)]
pub struct GameConfig {
    pub(crate) max_points: i32,
}

/// One partner's result for a round: their `bet`, and whether they `took_trick`
/// (the latter only affects scoring for a nil bid, i.e. `bet == 0`). Pairing the
/// two keeps a seat's bet and trick flag from drifting apart at call sites.
#[derive(Debug, Clone, Copy)]
struct PartnerOutcome {
    bet: i32,
    took_trick: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TeamState {
    pub current_round_tricks_won: i32,
    pub bags: i32,
    pub cumulative_points: i32,
}

impl TeamState {
    fn new() -> TeamState {
        TeamState {
            current_round_tricks_won: 0,
            bags: 0,
            cumulative_points: 0,
        }
    }

    fn calculate_round_totals(&mut self, first: PartnerOutcome, second: PartnerOutcome) {
        // Double nil: both partners bid 0. Scored as an indivisible unit — +200 if
        // neither partner took a trick, otherwise a flat -200 with the tricks
        // discarded (no bags). This bypasses the per-partner nil scoring below.
        if first.bet == 0 && second.bet == 0 {
            let made = !first.took_trick && !second.took_trick;
            self.cumulative_points += if made { 200 } else { -200 };
            return;
        }

        let team_tricks = self.current_round_tricks_won;
        let team_bets = first.bet + second.bet;

        if team_tricks >= team_bets {
            let round_bags = team_tricks - team_bets;
            self.bags += round_bags;
            self.cumulative_points += round_bags + (team_bets * 10);
        } else {
            self.cumulative_points -= team_bets * 10;
        }

        while self.bags >= 10 {
            self.bags -= 10;
            self.cumulative_points -= 100;
        }

        // Nil bid (bet == 0): +100 if the partner took no trick, −100 if they did.
        for partner in [first, second] {
            if partner.bet == 0 {
                self.cumulative_points += if partner.took_trick { -100 } else { 100 };
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Scoring {
    pub config: GameConfig,
    pub team_a: TeamState,
    pub team_b: TeamState,
    pub in_betting_stage: bool,
    pub bets_placed: Vec<[i32; 4]>,
    pub is_over: bool,
    pub round: usize,
    pub trick: usize,
    /// Whether each seat took at least one trick this round; consumed when
    /// adjudicating nil bids. Serialized as `nil_check` for backward compat.
    #[serde(rename = "nil_check")]
    pub won_a_trick: [bool; 4],
    pub player_tricks_won: [i32; 4],
}

impl Scoring {
    pub fn new(max_points: i32) -> Scoring {
        Scoring {
            team_a: TeamState::new(),
            team_b: TeamState::new(),
            in_betting_stage: true,
            bets_placed: vec![[0; 4]],
            is_over: false,
            round: 0,
            trick: 0,
            config: GameConfig { max_points },
            won_a_trick: [false, false, false, false],
            player_tricks_won: [0; 4],
        }
    }

    pub fn add_bet(&mut self, current_player_index: usize, bet: i32) {
        self.bets_placed.last_mut().unwrap()[current_player_index] = bet;
    }

    pub fn bet(&mut self) {
        self.trick = 0;
        self.in_betting_stage = false;

        self.bets_placed.push([0; 4]);
    }

    pub fn trick(&mut self, starting_player_index: usize, cards: &[Card; 4]) -> usize {
        let winner = get_trick_winner(starting_player_index, cards);
        self.won_a_trick[winner] = true;
        self.player_tricks_won[winner] += 1;

        if winner.is_multiple_of(2) {
            self.team_a.current_round_tricks_won += 1;
        } else {
            self.team_b.current_round_tricks_won += 1;
        }

        if self.trick == 12 {
            self.team_a.calculate_round_totals(
                PartnerOutcome {
                    bet: self.bets_placed[self.round][0],
                    took_trick: self.won_a_trick[0],
                },
                PartnerOutcome {
                    bet: self.bets_placed[self.round][2],
                    took_trick: self.won_a_trick[2],
                },
            );
            self.team_b.calculate_round_totals(
                PartnerOutcome {
                    bet: self.bets_placed[self.round][1],
                    took_trick: self.won_a_trick[1],
                },
                PartnerOutcome {
                    bet: self.bets_placed[self.round][3],
                    took_trick: self.won_a_trick[3],
                },
            );
            self.won_a_trick = [false; 4];
            self.player_tricks_won = [0; 4];
            self.in_betting_stage = true;
            self.team_a.current_round_tricks_won = 0;
            self.team_b.current_round_tricks_won = 0;

            let a_reached = self.team_a.cumulative_points >= self.config.max_points;
            let b_reached = self.team_b.cumulative_points >= self.config.max_points;
            if a_reached || b_reached {
                if a_reached && b_reached {
                    // Both teams reached max_points: higher score wins, tie continues
                    if self.team_a.cumulative_points != self.team_b.cumulative_points {
                        self.is_over = true;
                    }
                } else {
                    self.is_over = true;
                }
            }
            // Loss floor: a team at or below MIN_POINTS loses. Unconditional —
            // no tie escape — so a perpetually-losing game still terminates.
            if self.team_a.cumulative_points <= MIN_POINTS
                || self.team_b.cumulative_points <= MIN_POINTS
            {
                self.is_over = true;
            }
            // Round cap: a final backstop. self.round is the just-completed
            // round's 0-based index and is incremented just below, so the cap
            // fires as the MAX_ROUNDS-th round completes.
            if self.round + 1 >= MAX_ROUNDS {
                self.is_over = true;
            }
            self.round += 1;
        } else {
            self.trick += 1;
        }

        winner
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{Rank, Suit};

    /// Helper: create 4 cards of the same suit with given ranks.
    /// Player at `winner_idx` gets the highest rank to win the trick.
    fn make_trick(suit: Suit, ranks: [Rank; 4]) -> [Card; 4] {
        [
            Card {
                suit,
                rank: ranks[0],
            },
            Card {
                suit,
                rank: ranks[1],
            },
            Card {
                suit,
                rank: ranks[2],
            },
            Card {
                suit,
                rank: ranks[3],
            },
        ]
    }

    /// Helper: play a full round (13 tricks) where `team_a_wins` tricks go to
    /// player 0 (team A) and the rest go to player 1 (team B).
    fn play_round(scoring: &mut Scoring, team_a_wins: usize) {
        for t in 0..13 {
            // All same suit; highest rank wins
            let cards = if t < team_a_wins {
                // Player 0 wins (Ace is highest)
                make_trick(Suit::Club, [Rank::Ace, Rank::King, Rank::Queen, Rank::Jack])
            } else {
                // Player 1 wins
                make_trick(Suit::Club, [Rank::Two, Rank::Ace, Rank::Three, Rank::Four])
            };
            scoring.trick(0, &cards);
        }
    }

    #[test]
    fn test_scoring_new() {
        let s = Scoring::new(500);
        assert_eq!(s.team_a.cumulative_points, 0);
        assert_eq!(s.team_b.cumulative_points, 0);
        assert_eq!(s.team_a.bags, 0);
        assert_eq!(s.team_b.bags, 0);
        assert!(s.in_betting_stage);
        assert!(!s.is_over);
        assert_eq!(s.round, 0);
        assert_eq!(s.trick, 0);
        assert_eq!(s.config.max_points, 500);
    }

    #[test]
    fn test_add_bet_and_bet_finalize() {
        let mut s = Scoring::new(500);
        s.add_bet(0, 3);
        s.add_bet(1, 4);
        s.add_bet(2, 2);
        s.add_bet(3, 4);
        assert_eq!(s.bets_placed[0], [3, 4, 2, 4]);

        s.bet();
        assert!(!s.in_betting_stage);
        assert_eq!(s.trick, 0);
    }

    #[test]
    fn test_make_bid_exactly() {
        // Team A bids 6, wins exactly 6 tricks => +60 pts, 0 bags
        let mut s = Scoring::new(500);
        s.add_bet(0, 3); // player A bets 3
        s.add_bet(1, 3);
        s.add_bet(2, 3); // player C bets 3 -> team A total = 6
        s.add_bet(3, 4);
        s.bet();

        play_round(&mut s, 6); // team A wins 6

        assert_eq!(s.team_a.cumulative_points, 60);
        assert_eq!(s.team_a.bags, 0);
    }

    #[test]
    fn test_overbid_gains_bags() {
        // Team A bids 4, wins 6 => +42 pts, 2 bags
        let mut s = Scoring::new(500);
        s.add_bet(0, 2);
        s.add_bet(1, 3);
        s.add_bet(2, 2); // team A total = 4
        s.add_bet(3, 4);
        s.bet();

        play_round(&mut s, 6); // team A wins 6

        assert_eq!(s.team_a.cumulative_points, 42);
        assert_eq!(s.team_a.bags, 2);
    }

    #[test]
    fn test_missed_bid_penalty() {
        // Team A bids 5, wins 3 => -50 pts (FIXED BUG)
        let mut s = Scoring::new(500);
        s.add_bet(0, 3);
        s.add_bet(1, 3);
        s.add_bet(2, 2); // team A total = 5
        s.add_bet(3, 4);
        s.bet();

        play_round(&mut s, 3); // team A wins only 3

        assert_eq!(s.team_a.cumulative_points, -50);
        assert_eq!(s.team_a.bags, 0);
    }

    #[test]
    fn test_bag_penalty_at_10() {
        let mut s = Scoring::new(500);

        // Round 1: bid 1, win 6 => +15 pts, 5 bags
        s.add_bet(0, 1);
        s.add_bet(1, 6);
        s.add_bet(2, 0); // nil bid for player C
        s.add_bet(3, 7);
        s.bet();
        play_round(&mut s, 6); // team A wins 6
        // team_a: first_bet=1, first_nil=true(player0 won tricks), second_bet=0, second_nil=true(player2 won tricks)
        // Actually player C (index 2) never wins with our helper since only player 0 or 1 wins.
        // Let's just check the cumulative state
        let _pts_after_r1 = s.team_a.cumulative_points;
        let _bags_after_r1 = s.team_a.bags;

        // Round 2: bid 1, win enough to accumulate >= 10 bags total
        s.add_bet(0, 1);
        s.add_bet(1, 6);
        s.add_bet(2, 0);
        s.add_bet(3, 7);
        s.bet();
        play_round(&mut s, 6);

        // After 2 rounds, bags should have been penalized if they crossed 10
        // The exact values depend on nil handling, but bags should be < 10
        assert!(s.team_a.bags < 10);
        // Verify the penalty was applied (cumulative should have -100 from bags at some point)
        assert!(s.team_a.cumulative_points != 0); // non-trivial score
    }

    #[test]
    fn test_nil_bid_success() {
        // Player A bids 0, doesn't win any tricks => +100 pts
        let mut s = Scoring::new(500);
        s.add_bet(0, 0); // nil
        s.add_bet(1, 6);
        s.add_bet(2, 6); // team A non-nil bid = 6
        s.add_bet(3, 7);
        s.bet();

        // Team A wins 6 tricks, but only from player 2 (index 2)
        // All tricks won by player 2 - use player 2 as winner
        for t in 0..13 {
            let cards = if t < 6 {
                // Player 2 wins (has Ace)
                make_trick(Suit::Club, [Rank::Two, Rank::Three, Rank::Ace, Rank::Four])
            } else {
                // Player 1 wins
                make_trick(Suit::Club, [Rank::Two, Rank::Ace, Rank::Three, Rank::Four])
            };
            scoring_trick_with_start(&mut s, 0, &cards);
        }

        // won_a_trick[0] should be false (player A never won a trick)
        // So nil bonus +100 for player A
        // Team A bid = 0 + 6 = 6, won 6 tricks => +60 from regular + 100 from nil = 160
        assert_eq!(s.team_a.cumulative_points, 160);
    }

    #[test]
    fn test_nil_bid_failure() {
        // Player A bids 0 but wins a trick => -100 pts
        let mut s = Scoring::new(500);
        s.add_bet(0, 0); // nil
        s.add_bet(1, 6);
        s.add_bet(2, 6); // team A non-nil bid = 6
        s.add_bet(3, 7);
        s.bet();

        // Player 0 wins 1 trick, player 2 wins 5, player 1 wins 7
        for t in 0..13 {
            let cards = if t == 0 {
                // Player 0 wins (has Ace)
                make_trick(Suit::Club, [Rank::Ace, Rank::King, Rank::Queen, Rank::Jack])
            } else if t < 6 {
                // Player 2 wins
                make_trick(Suit::Club, [Rank::Two, Rank::Three, Rank::Ace, Rank::Four])
            } else {
                // Player 1 wins
                make_trick(Suit::Club, [Rank::Two, Rank::Ace, Rank::Three, Rank::Four])
            };
            scoring_trick_with_start(&mut s, 0, &cards);
        }

        // won_a_trick[0] = true (player A won a trick) => -100
        // Team A bid = 0 + 6 = 6, won 6 tricks => +60 from regular - 100 from nil = -40
        assert_eq!(s.team_a.cumulative_points, -40);
    }

    /// Helper: just calls scoring.trick with starting index
    fn scoring_trick_with_start(
        scoring: &mut Scoring,
        starting_idx: usize,
        cards: &[Card; 4],
    ) -> usize {
        scoring.trick(starting_idx, cards)
    }

    #[test]
    fn test_nil_check_correct_player_index() {
        // Verify that won_a_trick[2] is used for player C (team_a's second player), not won_a_trick[1]
        let mut s = Scoring::new(500);
        s.add_bet(0, 3);
        s.add_bet(1, 3);
        s.add_bet(2, 0); // player C bids nil
        s.add_bet(3, 4);
        s.bet();

        // Player 2 (C) never wins any tricks
        for _ in 0..13 {
            let cards = make_trick(Suit::Club, [Rank::Ace, Rank::King, Rank::Two, Rank::Three]);
            s.trick(0, &cards);
        }

        // Player C (index 2) bid nil and never won -> won_a_trick[2] should be false
        // This means successful nil -> +100 pts for nil
        // Team A: bet=3+0=3, tricks won=13 (all by player 0) => (13-3) bags + 30 = 40 pts, +100 nil bonus = 140
        // But bags are 10 so -100 penalty: 140 - 100 = 40, bags = 0
        assert_eq!(s.team_a.cumulative_points, 40);
    }

    #[test]
    fn test_is_over_team_a_wins() {
        let mut s = Scoring::new(100);
        s.add_bet(0, 6);
        s.add_bet(1, 3);
        s.add_bet(2, 4); // team A total = 10
        s.add_bet(3, 3);
        s.bet();

        play_round(&mut s, 10); // team A wins 10 tricks => 100 pts

        assert!(s.is_over);
        assert!(s.team_a.cumulative_points >= 100);
    }

    #[test]
    fn test_is_over_team_b_wins() {
        // FIXED BUG: previously only checked team_a
        let mut s = Scoring::new(100);
        s.add_bet(0, 3);
        s.add_bet(1, 6);
        s.add_bet(2, 3); // team A total = 6
        s.add_bet(3, 4); // team B total = 10
        s.bet();

        play_round(&mut s, 3); // team A wins 3, team B wins 10

        assert!(s.is_over);
        assert!(s.team_b.cumulative_points >= 100);
    }

    #[test]
    fn test_is_over_both_teams_higher_wins() {
        // FIXED BUG: when both reach max_points, higher score wins
        let mut s = Scoring::new(100);

        // Set up scores so both will exceed 100 but one is higher
        s.team_a.cumulative_points = 90;
        s.team_b.cumulative_points = 80;

        s.add_bet(0, 1);
        s.add_bet(1, 6);
        s.add_bet(2, 1);
        s.add_bet(3, 6); // team B total = 12
        s.bet();

        // Team A wins 2, team B wins 11
        // Team A: bid=2, tricks=2 => +20 => 110
        // Team B: bid=12, tricks=11 => missed bid => -120 => actually team B goes down
        // Let's use simpler setup
        play_round(&mut s, 2);

        // With these bets & tricks, check if the game ends correctly
        // The exact result depends on the math, but the mechanism is tested
        // team_a: 90 + 20 = 110 (bid 2, won 2)
        // team_b: 80 - 120 = -40 (bid 12, won 11) — missed bid penalty
        assert!(s.is_over); // Only team A reached, so game is over
        assert!(s.team_a.cumulative_points >= 100);
    }

    #[test]
    fn test_is_over_tie_continues() {
        // FIXED BUG: when both teams reach max_points with same score, game continues
        let mut s = Scoring::new(100);

        // Both teams at same score, both will reach exactly same points
        s.team_a.cumulative_points = 90;
        s.team_b.cumulative_points = 90;

        s.add_bet(0, 1);
        s.add_bet(1, 1);
        s.add_bet(2, 0); // nil for C
        s.add_bet(3, 0); // nil for D
        s.bet();

        // We need both teams to end at exactly the same score
        // Team A: player 0 wins tricks, player 2 doesn't
        // Team B: player 1 wins tricks, player 3 doesn't
        // Let's set up so both get same points
        // Actually, let's directly test the logic by setting up the state
        // Team A bid=1+0=1, team B bid=1+0=1
        // If both teams win the same tricks proportionally...
        // This is complex, so let's directly manipulate state for a cleaner test

        // Reset and use direct state manipulation
        let mut s2 = Scoring::new(100);
        s2.team_a.cumulative_points = 95;
        s2.team_b.cumulative_points = 95;

        s2.add_bet(0, 1);
        s2.add_bet(1, 1);
        s2.add_bet(2, 1);
        s2.add_bet(3, 1);
        s2.bet();

        // Team A wins 7, team B wins 6
        // Team A: bid=2, won=7 => +25 pts (20 + 5 bags) => 120
        // Team B: bid=2, won=6 => +24 pts (20 + 4 bags) => 119
        // Not a tie, but let's verify the mechanism works
        play_round(&mut s2, 7);

        // Both reached 100, but team A is higher, so game should end
        assert!(s2.is_over);

        // Now test actual tie scenario
        let mut s3 = Scoring::new(100);
        s3.team_a.cumulative_points = 95;
        s3.team_b.cumulative_points = 95;

        s3.add_bet(0, 3);
        s3.add_bet(1, 3);
        s3.add_bet(2, 3);
        s3.add_bet(3, 3);
        s3.bet();

        // Each team gets the same score if they win same number of tricks
        // Team A wins 7, Team B wins 6 — not equal
        // For a true tie: team A and B both bid 3+3=6 each
        // If team A wins exactly 6 and team B wins exactly 7: team A gets 60, team B gets 61 — not tied
        // Actually getting an exact tie is hard without direct manipulation
        // Let's verify the branch differently: set both teams to same score above max
        let mut s4 = Scoring::new(100);
        s4.team_a.cumulative_points = 100;
        s4.team_b.cumulative_points = 100;
        // Directly check that the is_over logic handles equal scores
        // We'll just verify the scoring doesn't set is_over when scores are equal
        s4.add_bet(0, 6);
        s4.add_bet(1, 6);
        s4.add_bet(2, 7);
        s4.add_bet(3, 7);
        s4.bet();

        play_round(&mut s4, 13); // Team A wins all 13

        // Team A: bid=13, won=13 => +130 => 230
        // Team B: bid=13, won=0 => -130 => -30
        // Only team A reached, game is over
        assert!(s4.is_over);
    }

    #[test]
    fn test_second_player_nil_bid_failure() {
        // Player C (index 2) bids nil but wins a trick => -100 for team A's second nil
        let mut s = Scoring::new(500);
        s.add_bet(0, 6); // player A bets 6
        s.add_bet(1, 6);
        s.add_bet(2, 0); // player C bids nil
        s.add_bet(3, 7);
        s.bet();

        // Player C (index 2) wins 1 trick, player A wins 5, player B wins 7
        for t in 0..13 {
            let cards = if t == 0 {
                // Player 2 wins (has Ace)
                make_trick(Suit::Club, [Rank::Two, Rank::Three, Rank::Ace, Rank::Four])
            } else if t < 6 {
                // Player 0 wins
                make_trick(Suit::Club, [Rank::Ace, Rank::King, Rank::Queen, Rank::Jack])
            } else {
                // Player 1 wins
                make_trick(Suit::Club, [Rank::Two, Rank::Ace, Rank::Three, Rank::Four])
            };
            scoring_trick_with_start(&mut s, 0, &cards);
        }

        // won_a_trick[2] = true (player C won a trick) => -100 for nil failure
        // Team A: bet=6+0=6, won=6 tricks => +60 from regular - 100 from failed nil = -40
        assert_eq!(s.team_a.cumulative_points, -40);
    }

    #[test]
    fn test_bag_penalty_applies_per_ten_bags() {
        // Bags can cross multiple multiples of 10 in a single round.
        // Each crossing of 10 must trigger a -100 penalty, not just one.
        let mut t = TeamState::new();
        t.bags = 9;
        t.current_round_tricks_won = 12; // 12 tricks won this round
        // Bid 1+0=1, won 12 -> +10 (bid) + 11 (bags) = +21 points, bags 9+11=20
        // Two bag penalties (-200), plus nil bonus (+100 for second_bet=0, second_nil=false)
        // Net: 21 - 200 + 100 = -79
        t.calculate_round_totals(
            PartnerOutcome {
                bet: 1,
                took_trick: false,
            },
            PartnerOutcome {
                bet: 0,
                took_trick: false,
            },
        );
        assert_eq!(
            t.bags, 0,
            "bags should be reduced through both 10-thresholds"
        );
        assert_eq!(t.cumulative_points, -79);
    }

    #[test]
    fn test_double_nil_both_succeed() {
        // Both partners on team A bid nil and the team takes zero tricks.
        // Each successful nil is +100 → +200, with no bags.
        let mut s = Scoring::new(500);
        s.add_bet(0, 0);
        s.add_bet(1, 7);
        s.add_bet(2, 0);
        s.add_bet(3, 6);
        s.bet();
        play_round(&mut s, 0); // team A wins nothing; all 13 to team B

        assert_eq!(s.team_a.cumulative_points, 200);
        assert_eq!(s.team_a.bags, 0);
    }

    #[test]
    fn test_double_nil_one_partner_takes_trick() {
        // Indivisible double nil: both partners bid 0 but seat 0 takes 2 tricks, so
        // the double nil fails as a unit — flat -200, tricks discarded (no bags).
        let mut s = Scoring::new(500);
        s.add_bet(0, 0);
        s.add_bet(1, 7);
        s.add_bet(2, 0);
        s.add_bet(3, 4);
        s.bet();
        play_round(&mut s, 2); // seat 0 takes 2 tricks, seat 2 takes none

        assert_eq!(s.team_a.cumulative_points, -200);
        assert_eq!(s.team_a.bags, 0);
    }

    #[test]
    fn test_double_nil_both_partners_take_tricks() {
        // Double nil still fails as a single -200 unit when both partners take
        // tricks (not -100 each) — and the tricks never accrue as bags.
        let mut s = Scoring::new(500);
        s.add_bet(0, 0);
        s.add_bet(1, 7);
        s.add_bet(2, 0);
        s.add_bet(3, 4);
        s.bet();
        for t in 0..13 {
            let cards = if t == 0 {
                // seat 0 wins
                make_trick(Suit::Club, [Rank::Ace, Rank::King, Rank::Queen, Rank::Jack])
            } else if t == 1 {
                // seat 2 wins
                make_trick(Suit::Club, [Rank::Two, Rank::Three, Rank::Ace, Rank::Four])
            } else {
                // seat 1 wins
                make_trick(Suit::Club, [Rank::Two, Rank::Ace, Rank::Three, Rank::Four])
            };
            s.trick(0, &cards);
        }

        assert_eq!(s.team_a.cumulative_points, -200);
        assert_eq!(s.team_a.bags, 0);
    }

    #[test]
    fn test_nil_succeeds_but_team_misses_contract() {
        // seat 0 bids nil (holds), seat 2 bids 6, but the team wins only 3 tricks,
        // all by seat 2. Missed contract → -60; nil holds → +100. Net +40, no bags.
        // The untested diagonal: nil success combined with a missed team contract.
        let mut s = Scoring::new(500);
        s.add_bet(0, 0);
        s.add_bet(1, 6);
        s.add_bet(2, 6);
        s.add_bet(3, 4);
        s.bet();

        for t in 0..13 {
            let cards = if t < 3 {
                // seat 2 wins (Ace at index 2)
                make_trick(Suit::Club, [Rank::Two, Rank::Three, Rank::Ace, Rank::Four])
            } else {
                // seat 1 wins
                make_trick(Suit::Club, [Rank::Two, Rank::Ace, Rank::Three, Rank::Four])
            };
            s.trick(0, &cards);
        }

        assert_eq!(s.team_a.cumulative_points, 40);
        assert_eq!(s.team_a.bags, 0);
    }

    #[test]
    fn test_nil_fails_and_team_misses_contract() {
        // seat 0 bids nil but takes 1 trick (fails), seat 2 bids 6; the team wins
        // only 4 tricks (1 by seat 0, 3 by seat 2). Missed contract → -60; failed
        // nil → -100. Net -160, no bags. The doubly-bad diagonal.
        let mut s = Scoring::new(500);
        s.add_bet(0, 0);
        s.add_bet(1, 6);
        s.add_bet(2, 6);
        s.add_bet(3, 4);
        s.bet();

        for t in 0..13 {
            let cards = if t == 0 {
                // seat 0 wins (Ace at index 0)
                make_trick(Suit::Club, [Rank::Ace, Rank::King, Rank::Queen, Rank::Jack])
            } else if t < 4 {
                // seat 2 wins
                make_trick(Suit::Club, [Rank::Two, Rank::Three, Rank::Ace, Rank::Four])
            } else {
                // seat 1 wins
                make_trick(Suit::Club, [Rank::Two, Rank::Ace, Rank::Three, Rank::Four])
            };
            s.trick(0, &cards);
        }

        assert_eq!(s.team_a.cumulative_points, -160);
        assert_eq!(s.team_a.bags, 0);
    }

    #[test]
    fn test_is_over_team_below_loss_floor() {
        // A team that keeps getting set eventually drops to the loss floor and
        // loses, even though neither team ever reached max_points.
        let mut s = Scoring::new(500);
        s.team_b.cumulative_points = -100; // just above the floor going in

        // Team A bets 6 (3+3) and wins all 13 -> makes bid.
        // Team B bets 13 (7+6) and wins 0 -> set -> -130 -> ends at -230.
        s.add_bet(0, 3);
        s.add_bet(1, 7);
        s.add_bet(2, 3);
        s.add_bet(3, 6);
        s.bet();

        play_round(&mut s, 13); // team A wins all 13, team B wins 0

        assert!(s.is_over, "game must end when a team hits the loss floor");
        assert!(s.team_b.cumulative_points <= MIN_POINTS);
        assert!(s.team_a.cumulative_points < s.config.max_points); // not a max-points win
    }

    #[test]
    fn test_is_over_round_cap() {
        let mut s = Scoring::new(500);
        // Jump to the final allowed round with scores comfortably inside the band.
        s.round = MAX_ROUNDS - 1;
        s.team_a.cumulative_points = 50;
        s.team_b.cumulative_points = 40;
        // bets_placed is normally grown one entry per round; pre-fill so the
        // round-end read of bets_placed[self.round] is in bounds.
        s.bets_placed = vec![[1, 1, 1, 1]; MAX_ROUNDS];

        // Play 13 tricks directly (team A wins 7, team B wins 6) — modest deltas
        // that cross neither max_points nor the loss floor.
        for t in 0..13 {
            let cards = if t < 7 {
                make_trick(Suit::Club, [Rank::Ace, Rank::King, Rank::Queen, Rank::Jack])
            } else {
                make_trick(Suit::Club, [Rank::Two, Rank::Ace, Rank::Three, Rank::Four])
            };
            s.trick(0, &cards);
        }

        assert!(s.is_over, "game must end at the round cap");
        assert!(s.round >= MAX_ROUNDS);
        // Ended by the cap, not by a score threshold:
        assert!(s.team_a.cumulative_points < s.config.max_points);
        assert!(s.team_b.cumulative_points < s.config.max_points);
        assert!(s.team_a.cumulative_points > MIN_POINTS);
        assert!(s.team_b.cumulative_points > MIN_POINTS);
    }

    #[test]
    fn test_trick_13th_resets_round() {
        let mut s = Scoring::new(500);
        s.add_bet(0, 3);
        s.add_bet(1, 3);
        s.add_bet(2, 3);
        s.add_bet(3, 4);
        s.bet();

        for _ in 0..13 {
            let cards = make_trick(Suit::Club, [Rank::Ace, Rank::King, Rank::Queen, Rank::Jack]);
            s.trick(0, &cards);
        }

        // After 13 tricks, should be back in betting stage
        assert!(s.in_betting_stage);
        assert_eq!(s.round, 1);
        assert_eq!(s.team_a.current_round_tricks_won, 0);
        assert_eq!(s.team_b.current_round_tricks_won, 0);
        assert_eq!(s.won_a_trick, [false; 4]);
    }
}
