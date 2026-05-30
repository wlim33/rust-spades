//! Glicko-2 rating algorithm for player skill estimation.
//!
//! Pure-math module — no I/O, no allocation beyond the per-call temporaries
//! Glickman's algorithm requires. Implements the standard formulation from
//! Glickman, "Example of the Glicko-2 system" (2013), including the
//! iterative volatility update (Step 5).
//!
//! For 4-player partnership Spades the natural application is: each player
//! "played against" the two opponents on the other team in a single
//! virtual match per game. Caller is responsible for assembling the
//! `(opponent_rating, outcome)` list for each player; outcomes are 1.0
//! for win, 0.0 for loss (draws are possible in principle — 0.5 — but
//! Spades has no draws).

use std::f64::consts::PI;

/// One player's Glicko-2 state. Stored against `users.rating` /
/// `users.rd` / `users.volatility` in the DB.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rating {
    /// Glicko rating in the visible 1500-scale.
    pub rating: f64,
    /// Rating deviation; lower = more confident.
    pub rd: f64,
    /// Volatility; how much the rating is expected to fluctuate.
    pub volatility: f64,
}

/// Glicko-2 system constant τ. Smaller τ = ratings change less in response
/// to surprising outcomes. The Glicko-2 paper recommends 0.3–1.2; 0.5 is
/// a common middle ground.
pub const TAU: f64 = 0.5;

/// Convergence tolerance for the volatility-update bisection.
const EPS: f64 = 1e-6;

/// Default starting Rating for a fresh user.
pub const DEFAULT_RATING: Rating = Rating {
    rating: 1500.0,
    rd: 350.0,
    volatility: 0.06,
};

/// Apply one rating-period update to `player` given a slate of opponents
/// and outcomes. Each `(opp, score)` pair contributes to the update;
/// `score` is 1.0 on win, 0.5 on draw, 0.0 on loss.
///
/// If `opponents` is empty the result is just the "no games played" RD
/// expansion (φ' = √(φ² + σ²)) — the rating drifts toward less confident
/// over time without games.
pub fn update(player: Rating, opponents: &[(Rating, f64)]) -> Rating {
    // Step 2: convert to Glicko-2 internal scale.
    let mu = (player.rating - 1500.0) / 173.7178;
    let phi = player.rd / 173.7178;
    let sigma = player.volatility;

    if opponents.is_empty() {
        let phi_new = (phi * phi + sigma * sigma).sqrt();
        return Rating {
            rating: mu * 173.7178 + 1500.0,
            rd: phi_new * 173.7178,
            volatility: sigma,
        };
    }

    // Steps 3 & 4: variance v and improvement Δ.
    let mut v_inv = 0.0;
    let mut delta_sum = 0.0;
    for (opp, score) in opponents {
        let mu_j = (opp.rating - 1500.0) / 173.7178;
        let phi_j = opp.rd / 173.7178;
        let g_j = 1.0 / (1.0 + 3.0 * phi_j * phi_j / (PI * PI)).sqrt();
        let e_j = 1.0 / (1.0 + (-g_j * (mu - mu_j)).exp());
        v_inv += g_j * g_j * e_j * (1.0 - e_j);
        delta_sum += g_j * (score - e_j);
    }
    let v = 1.0 / v_inv;
    let delta = v * delta_sum;

    // Step 5: iteratively determine new volatility σ' via Algorithm 2
    // (Illinois-method bisection on f).
    let a = (sigma * sigma).ln();
    let f = |x: f64| -> f64 {
        let ex = x.exp();
        let num = ex * (delta * delta - phi * phi - v - ex);
        let den = 2.0 * (phi * phi + v + ex).powi(2);
        num / den - (x - a) / (TAU * TAU)
    };

    let mut x_a = a;
    let mut x_b;
    if delta * delta > phi * phi + v {
        x_b = (delta * delta - phi * phi - v).ln();
    } else {
        let mut k = 1.0;
        while f(a - k * TAU) < 0.0 {
            k += 1.0;
        }
        x_b = a - k * TAU;
    }
    let mut f_a = f(x_a);
    let mut f_b = f(x_b);
    while (x_b - x_a).abs() > EPS {
        let x_c = x_a + (x_a - x_b) * f_a / (f_b - f_a);
        let f_c = f(x_c);
        if f_c * f_b <= 0.0 {
            x_a = x_b;
            f_a = f_b;
        } else {
            f_a /= 2.0;
        }
        x_b = x_c;
        f_b = f_c;
    }
    let sigma_new = (x_a / 2.0).exp();

    // Step 6: pre-rating-period value φ*.
    let phi_star = (phi * phi + sigma_new * sigma_new).sqrt();

    // Step 7: new RD and rating.
    let phi_new = 1.0 / (1.0 / (phi_star * phi_star) + 1.0 / v).sqrt();
    let mu_new = mu + phi_new * phi_new * delta_sum;

    Rating {
        rating: mu_new * 173.7178 + 1500.0,
        rd: phi_new * 173.7178,
        volatility: sigma_new,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The worked example in Glickman's "Example of the Glicko-2 system"
    /// paper: a 1500/200/0.06 player faces three opponents with given
    /// ratings and outcomes. Expected output ≈ 1464.06 / 151.52 / 0.05999.
    /// (Approximate equality — Glicko-2 step 5 is iterative, exact decimals
    /// depend on the bisection tolerance.)
    #[test]
    fn glicko2_paper_example() {
        let player = Rating {
            rating: 1500.0,
            rd: 200.0,
            volatility: 0.06,
        };
        let opps = [
            (
                Rating {
                    rating: 1400.0,
                    rd: 30.0,
                    volatility: 0.06,
                },
                1.0,
            ),
            (
                Rating {
                    rating: 1550.0,
                    rd: 100.0,
                    volatility: 0.06,
                },
                0.0,
            ),
            (
                Rating {
                    rating: 1700.0,
                    rd: 300.0,
                    volatility: 0.06,
                },
                0.0,
            ),
        ];
        let out = update(player, &opps);
        assert!(
            (out.rating - 1464.06).abs() < 0.5,
            "rating: got {}, want ~1464.06",
            out.rating
        );
        assert!(
            (out.rd - 151.52).abs() < 0.5,
            "rd: got {}, want ~151.52",
            out.rd
        );
        assert!(
            (out.volatility - 0.05999).abs() < 1e-4,
            "volatility: got {}, want ~0.05999",
            out.volatility
        );
    }

    #[test]
    fn no_opponents_only_widens_rd() {
        let player = Rating {
            rating: 1500.0,
            rd: 100.0,
            volatility: 0.06,
        };
        let out = update(player, &[]);
        // No games → rating unchanged, RD grows by √(φ² + σ²) on the
        // internal scale.
        assert_eq!(out.rating, 1500.0);
        assert!(out.rd > 100.0);
        assert_eq!(out.volatility, 0.06);
    }

    #[test]
    fn winning_against_higher_rated_increases_rating() {
        let player = DEFAULT_RATING;
        let higher = Rating {
            rating: 1800.0,
            rd: 30.0,
            volatility: 0.06,
        };
        let out = update(player, &[(higher, 1.0)]);
        assert!(out.rating > player.rating);
    }

    #[test]
    fn losing_against_lower_rated_decreases_rating() {
        let player = DEFAULT_RATING;
        let lower = Rating {
            rating: 1200.0,
            rd: 30.0,
            volatility: 0.06,
        };
        let out = update(player, &[(lower, 0.0)]);
        assert!(out.rating < player.rating);
    }
}
