//! `GET /leaderboard` — top players by conservative Glicko score.
//!
//! Mirrors the `handlers_users` pattern: `State(AuthState)`, hand-written
//! DTOs, `Result<Json<…>, AuthError>`. Ranking math lives in the store; the
//! board's domain types/constants live in `crate::leaderboard`.

use axum::extract::{Query, State};
use axum::response::Json;
use serde::{Deserialize, Serialize};

use crate::auth::{AuthError, AuthState};
use crate::leaderboard::{LEADERBOARD_SIZE, LeaderboardWindow, MIN_GAMES};

#[derive(Deserialize)]
pub struct LeaderboardQuery {
    /// `all-time` (default), `this-month`, or an explicit `YYYY-MM`.
    #[serde(default)]
    pub period: Option<String>,
}

#[derive(Serialize)]
pub struct LeaderboardEntry {
    pub rank: i64,
    pub username: String,
    pub rating: i32,
    pub rd: i32,
    pub games_played: i64,
    pub score: i32,
}

#[derive(Serialize)]
pub struct LeaderboardResponse {
    /// The resolved period label: `all-time` or `YYYY-MM`.
    pub period: String,
    pub entries: Vec<LeaderboardEntry>,
}

/// Resolve the `period` query param into a label + window. Returns
/// `AuthError::Validation` (422) for anything malformed.
fn parse_period(period: Option<&str>) -> Result<(String, LeaderboardWindow), AuthError> {
    use chrono::Datelike;
    match period {
        None | Some("all-time") => Ok(("all-time".to_string(), LeaderboardWindow::AllTime)),
        Some("this-month") => {
            let now = chrono::Utc::now().naive_utc();
            let (year, month) = (now.year(), now.month());
            Ok((
                format!("{year:04}-{month:02}"),
                LeaderboardWindow::Month { year, month },
            ))
        }
        Some(s) => {
            // Explicit YYYY-MM.
            let parts: Vec<&str> = s.split('-').collect();
            if parts.len() == 2 && parts[0].len() == 4 && parts[1].len() == 2 {
                if let (Ok(year), Ok(month)) = (parts[0].parse::<i32>(), parts[1].parse::<u32>()) {
                    if (1..=12).contains(&month) {
                        return Ok((
                            format!("{year:04}-{month:02}"),
                            LeaderboardWindow::Month { year, month },
                        ));
                    }
                }
            }
            Err(AuthError::Validation(format!("invalid period: {s}")))
        }
    }
}

pub async fn get_leaderboard(
    State(auth): State<AuthState>,
    Query(q): Query<LeaderboardQuery>,
) -> Result<Json<LeaderboardResponse>, AuthError> {
    let (period, window) = parse_period(q.period.as_deref())?;
    let rows = auth
        .store
        .leaderboard(window, MIN_GAMES, LEADERBOARD_SIZE)
        .map_err(AuthError::Storage)?;
    let entries = rows
        .into_iter()
        .enumerate()
        .map(|(i, r)| LeaderboardEntry {
            rank: i as i64 + 1,
            username: r.username,
            rating: r.rating.round() as i32,
            rd: r.rd.round() as i32,
            games_played: r.games_played,
            score: r.score.round() as i32,
        })
        .collect();
    Ok(Json(LeaderboardResponse { period, entries }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_period_defaults_and_validates() {
        assert!(matches!(
            parse_period(None).unwrap().1,
            LeaderboardWindow::AllTime
        ));
        assert!(matches!(
            parse_period(Some("all-time")).unwrap().1,
            LeaderboardWindow::AllTime
        ));
        let (label, win) = parse_period(Some("2020-01")).unwrap();
        assert_eq!(label, "2020-01");
        assert_eq!(
            win,
            LeaderboardWindow::Month {
                year: 2020,
                month: 1
            }
        );
        // this-month resolves to a YYYY-MM label.
        assert_eq!(parse_period(Some("this-month")).unwrap().0.len(), 7);
        // Malformed inputs are rejected.
        assert!(parse_period(Some("yesterday")).is_err());
        assert!(parse_period(Some("2020-13")).is_err());
        assert!(parse_period(Some("20-01")).is_err());
        // Non-canonical month widths are rejected (must be 2-digit MM).
        assert!(parse_period(Some("2020-1")).is_err());
        assert!(parse_period(Some("2020-001")).is_err());
    }
}
