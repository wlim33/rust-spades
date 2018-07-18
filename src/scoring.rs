use cards::{Card, get_trick_winner};

#[derive(Debug)]
pub struct GameConfig {
    max_points: i32
}

#[derive(Debug)]
pub struct TeamState {
    pub current_round_tricks_won: [i32 ; 13],
    pub bags: i32,
    pub cumulative_points: i32,
}

impl TeamState {
    fn new() -> TeamState {
        TeamState {
            current_round_tricks_won: [0; 13],
            bags: 0,
            cumulative_points: 0,
        }
    }

    fn calculate_round_totals(&mut self, first_bet: i32, first_nil: bool, second_bet:i32, second_nil: bool) {
        let team_tricks : i32 = self.current_round_tricks_won.iter().sum();

        let team_bets = first_bet + second_bet;
        
        if team_tricks >= team_bets {
            let round_bags = team_tricks - team_bets;
            self.bags += round_bags;
            self.cumulative_points += round_bags + (team_bets * 10) as i32;
        }

        if self.bags >= 10 {
            self.bags -= 10;
            self.cumulative_points -= 100;
        }
        
        if first_bet == 0 {
            if !first_nil {
                self.cumulative_points += 100;
            } else {
                self.cumulative_points -= 100;
            }
        }
        if second_bet == 0 {
            if !second_nil {
                self.cumulative_points += 100;
            } else {
                self.cumulative_points -= 100;
            }
        }

    }
}

#[derive(Debug)]
pub struct Scoring {
    pub config: GameConfig,
    pub team_a: TeamState,
    pub team_b: TeamState,
    pub in_betting_stage: bool,
    pub bets_placed: Vec<[i32; 4]>,
    pub is_over: bool,
    pub round: usize,
    pub trick: usize,
    pub nil_check: [bool; 4]
}

impl Scoring {
    pub fn new(max_points: i32) -> Scoring {
        Scoring {
            team_a: TeamState::new(),
            team_b: TeamState::new(),
            in_betting_stage: true,
            bets_placed: vec![[0;4]],
            is_over: false,
            round: 0,
            trick: 0,
            config: GameConfig {max_points: max_points},
            nil_check: [false, false, false, false]

        }
    }
    
    pub fn add_bet(&mut self, current_player_index: usize, bet: i32) {
        self.bets_placed.last_mut().unwrap()[current_player_index] = bet;
    }

    pub fn bet(&mut self) {
        self.trick = 0;
        self.in_betting_stage = false;
        
        self.bets_placed.push([0;4]);
    }

    pub fn trick(&mut self, starting_player_index: usize, cards: &[Card; 4]) -> usize {
        let winner = get_trick_winner(starting_player_index, &cards);
        self.nil_check[winner] = true;

        if winner % 2 == 0 {
            self.team_a.current_round_tricks_won[self.trick] += 1;
        } else {
            self.team_b.current_round_tricks_won[self.trick] += 1;
        }

        if self.trick == 12 {
            self.team_a.calculate_round_totals(self.bets_placed[self.round][0], self.nil_check[0], self.bets_placed[self.round][2], self.nil_check[1]);
            self.team_b.calculate_round_totals(self.bets_placed[self.round][1], self.nil_check[1], self.bets_placed[self.round][3], self.nil_check[3]);
            self.nil_check = [false; 4];
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
