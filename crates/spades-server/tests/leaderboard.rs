use axum::http::StatusCode;
use serde_json::json;
mod common;

#[tokio::test]
async fn leaderboard_empty_when_no_eligible_players() {
    let server = common::test_server();
    let resp = server.get("/leaderboard").await;
    resp.assert_status(StatusCode::OK);
    let body: serde_json::Value = resp.json();
    assert_eq!(body["period"], "all-time");
    assert_eq!(body["entries"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn leaderboard_invalid_period_is_rejected() {
    let server = common::test_server();
    let resp = server.get("/leaderboard?period=yesterday").await;
    resp.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn leaderboard_this_month_period_echoed() {
    let server = common::test_server();
    let resp = server.get("/leaderboard?period=this-month").await;
    resp.assert_status(StatusCode::OK);
    let body: serde_json::Value = resp.json();
    let period = body["period"].as_str().unwrap();
    assert_eq!(
        period.len(),
        7,
        "period should look like YYYY-MM, got {period}"
    );
    assert_eq!(&period[4..5], "-");
}

#[tokio::test]
async fn leaderboard_lists_eligible_player() {
    let env = common::test_env();
    let reg: serde_json::Value = env
        .server
        .post("/auth/register")
        .json(&json!({
            "username": "Alice", "email": "alice@example.com", "password": "hunter2-strong",
        }))
        .await
        .json();
    let uid = uuid::Uuid::parse_str(reg["id"].as_str().unwrap()).unwrap();
    for _ in 0..5 {
        env.store
            .insert_game_seat(
                uuid::Uuid::new_v4(),
                0,
                uuid::Uuid::new_v4(),
                spades_server::auth::game_seats::SeatOwner {
                    user_id: Some(uid),
                    anon_user_id: None,
                    is_bot: false,
                },
            )
            .unwrap();
    }
    let resp = env.server.get("/leaderboard").await;
    resp.assert_status(StatusCode::OK);
    let body: serde_json::Value = resp.json();
    let entries = body["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["username"], "Alice");
    assert_eq!(entries[0]["rank"], 1);
    // Default-rated player: 1500 - 2*350 = 800.
    assert_eq!(entries[0]["score"], 800);
}
