//! Leaderboard domain types and helpers.
//!
//! Pure data + a date helper — no HTTP, no axum. The SQL ranking lives in
//! `sqlite_store::SqliteStore::leaderboard`; the HTTP surface lives in
//! `handlers_leaderboard`.

/// Minimum number of games (game-seat rows) a player must have to appear
/// on any board. Conservative scoring already sinks unproven players; this
/// gate keeps the tail tidy.
pub const MIN_GAMES: i64 = 5;

/// Maximum number of rows any board returns.
pub const LEADERBOARD_SIZE: i64 = 10;

/// Conservatism multiplier `k` in the ranking score `rating - k * rd`.
/// k = 2 is the standard Glicko lower-confidence-bound choice (≈2 standard
/// deviations below the point estimate); see Glickman's Glicko/Glicko-2
/// papers. Larger k penalizes rating uncertainty (high RD) more heavily.
pub const RD_CONSERVATISM: f64 = 2.0;

/// Which players a board covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeaderboardWindow {
    /// All players, ranked by current rating.
    AllTime,
    /// Players who have a game-seat in the given UTC calendar month,
    /// ranked by current rating. `month` is 1-based (1 = January).
    Month { year: i32, month: u32 },
}

/// One ranked player as returned by the store. Display rounding happens in
/// the HTTP layer.
#[derive(Debug, Clone, PartialEq)]
pub struct LeaderboardRow {
    pub username: String,
    pub rating: f64,
    pub rd: f64,
    /// All-time game-seat count (matches the profile's "games_played").
    pub games_played: i64,
    /// `rating - RD_CONSERVATISM * rd`.
    pub score: f64,
}

/// Inclusive-start / exclusive-end timestamp bounds for the given UTC
/// calendar month, formatted to match `game_seats.created_at`
/// (`YYYY-MM-DD HH:MM:SS`). Lexicographic string comparison on this fixed
/// format is equivalent to chronological comparison.
///
/// # Preconditions
///
/// `month` must be in `1..=12` and `year` a representable calendar year.
/// Callers are responsible for validating these — the HTTP layer
/// (`handlers_leaderboard::parse_period`) rejects out-of-range months and
/// non-4-digit years before constructing a `LeaderboardWindow::Month`, so
/// values reaching this function are already in range. Passing an invalid
/// `month` panics (via `.expect`); this is treated as a caller bug, not a
/// runtime condition.
pub fn month_bounds(year: i32, month: u32) -> (String, String) {
    use chrono::NaiveDate;
    let start = NaiveDate::from_ymd_opt(year, month, 1)
        .expect("valid year/month")
        .and_hms_opt(0, 0, 0)
        .expect("midnight is valid");
    let (next_year, next_month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    let end = NaiveDate::from_ymd_opt(next_year, next_month, 1)
        .expect("valid year/month")
        .and_hms_opt(0, 0, 0)
        .expect("midnight is valid");
    let fmt = "%Y-%m-%d %H:%M:%S";
    (start.format(fmt).to_string(), end.format(fmt).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn month_bounds_basic_and_year_rollover() {
        assert_eq!(
            month_bounds(2026, 6),
            (
                "2026-06-01 00:00:00".to_string(),
                "2026-07-01 00:00:00".to_string()
            )
        );
        // December rolls into the next January.
        assert_eq!(
            month_bounds(2026, 12),
            (
                "2026-12-01 00:00:00".to_string(),
                "2027-01-01 00:00:00".to_string()
            )
        );
        // January zero-pads the month and end-month.
        assert_eq!(
            month_bounds(2026, 1),
            (
                "2026-01-01 00:00:00".to_string(),
                "2026-02-01 00:00:00".to_string()
            )
        );
    }
}
