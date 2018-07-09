use deck::{Card, get_trick_winner};

#[derive(Debug)]
pub struct GameConfig {
    max_points: i32
}

#[derive(Debug)]
pub struct TeamState {
    pub bets: Vec<i32>,
    pub current_round_tricks_won: [i32 ; 13],
    bags: i32,
    cumulative_points: i32
}

impl TeamState {
    fn new() -> TeamState {
        TeamState {
            bets: vec![],
            current_round_tricks_won: [0; 13],
            bags: 0,
            cumulative_points: 0,
        }
    }

    fn calculate_round_totals(&mut self, round: usize) {
        let team_tricks : i32 = self.current_round_tricks_won.iter().sum();

        if team_tricks >= self.bets[round] {
            let round_bags = team_tricks - self.bets[round];
            self.bags += round_bags;
            self.cumulative_points += round_bags + (self.bets[round] * 10) as i32;
        }

        if self.bags >= 10 {
            self.bags -= 10;
            self.cumulative_points -= 100;
        }
    }
}

#[derive(Debug)]
pub struct ScoringState {
    pub config: GameConfig,
    pub team_a: TeamState,
    pub team_b: TeamState,
    pub in_betting_stage: bool,
    pub dealer: usize,
    pub is_over: bool,
    pub round: usize,
    pub trick: usize,
}

impl ScoringState {
    pub fn new(max_points: i32) -> ScoringState {
        ScoringState {
            team_a: TeamState::new(),
            team_b: TeamState::new(),
            in_betting_stage: true,
            dealer: 0,
            is_over: false,
            round: 0,
            trick: 0,
            config: GameConfig {max_points: max_points}
        }
    }

    pub fn bet(&mut self, bets: [i32; 4]) {
        self.trick = 0;
        
        self.team_a.bets.push(bets[0] + bets[2]);
        self.team_b.bets.push(bets[1] + bets[3]);

        self.in_betting_stage = false;
    }

    pub fn trick(&mut self, starting_player_index: usize, cards: &[Card; 4]) -> usize {
        let winner = get_trick_winner(starting_player_index, &cards);

        if winner % 2 == 0 {
            self.team_a.current_round_tricks_won[self.trick] += 1;
        } else {
            self.team_b.current_round_tricks_won[self.trick] += 1;
        }

        if self.trick == 12 {
            self.team_a.calculate_round_totals(self.round);
            self.team_b.calculate_round_totals(self.round);
            self.in_betting_stage = true;
            self.team_a.current_round_tricks_won = [0; 13];
            self.team_b.current_round_tricks_won = [0; 13];

            if self.team_a.cumulative_points >= self.config.max_points {
                self.is_over = true;
            }
            self.round += 1;
        } else {
            self.trick += 1;
        }

        return winner;
    }
}
