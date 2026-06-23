use axum::http::StatusCode;
use serde_json::json;
mod common;

#[tokio::test]
async fn profile_after_register() {
    let server = common::test_server();
    server
        .post("/auth/register")
        .json(&json!({
            "username": "Alice", "email": "alice@example.com", "password": "hunter2-strong",
        }))
        .await
        .assert_status(StatusCode::CREATED);

    let resp = server.get("/users/Alice").await;
    resp.assert_status(StatusCode::OK);
    let body: serde_json::Value = resp.json();
    assert_eq!(body["username"], "Alice");
    assert_eq!(body["games_played"], 0);
}

#[tokio::test]
async fn profile_404_for_unknown() {
    let server = common::test_server();
    let resp = server.get("/users/nobody").await;
    resp.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn profile_lookup_case_insensitive() {
    let server = common::test_server();
    server
        .post("/auth/register")
        .json(&json!({
            "username": "Alice", "email": "alice@example.com", "password": "hunter2-strong",
        }))
        .await
        .assert_status(StatusCode::CREATED);

    let resp = server.get("/users/ALICE").await;
    resp.assert_status(StatusCode::OK);
}

#[tokio::test]
async fn profile_games_list_empty() {
    let server = common::test_server();
    server
        .post("/auth/register")
        .json(&json!({
            "username": "Alice", "email": "alice@example.com", "password": "hunter2-strong",
        }))
        .await
        .assert_status(StatusCode::CREATED);

    let resp = server.get("/users/Alice/games").await;
    resp.assert_status(StatusCode::OK);
    let body: serde_json::Value = resp.json();
    assert_eq!(body["total"], 0);
    assert_eq!(body["games"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn profile_games_list_all_four_players_by_seat() {
    use spades_server::auth::game_seats::SeatOwner;
    use uuid::Uuid;

    let env = common::test_env();
    let server = &env.server;
    for (name, email) in [("Alice", "alice@example.com"), ("Bob", "bob@example.com")] {
        server
            .post("/auth/register")
            .json(&json!({ "username": name, "email": email, "password": "hunter2-strong" }))
            .await
            .assert_status(StatusCode::CREATED);
    }
    let alice = env.store.find_user_by_username("Alice").unwrap().unwrap();
    let bob = env.store.find_user_by_username("Bob").unwrap().unwrap();

    // One game: seats 0/2 = Team A, seats 1/3 = Team B.
    // seat0 Alice (registered), seat1 Bob (registered), seat2 bot, seat3 guest.
    let game = Uuid::new_v4();
    let human = |id| SeatOwner {
        user_id: Some(id),
        anon_user_id: None,
        is_bot: false,
    };
    env.store
        .insert_game_seat(game, 0, Uuid::new_v4(), human(alice.id))
        .unwrap();
    env.store
        .insert_game_seat(game, 1, Uuid::new_v4(), human(bob.id))
        .unwrap();
    env.store
        .insert_game_seat(
            game,
            2,
            Uuid::new_v4(),
            SeatOwner {
                user_id: None,
                anon_user_id: None,
                is_bot: true,
            },
        )
        .unwrap();
    env.store
        .insert_game_seat(
            game,
            3,
            Uuid::new_v4(),
            SeatOwner {
                user_id: None,
                anon_user_id: Some(Uuid::new_v4()),
                is_bot: false,
            },
        )
        .unwrap();

    let resp = server.get("/users/Alice/games").await;
    resp.assert_status(StatusCode::OK);
    let body: serde_json::Value = resp.json();
    assert_eq!(body["total"], 1);
    let games = body["games"].as_array().unwrap();
    assert_eq!(games.len(), 1);
    // The profile owner's own seat is still reported, for emphasis client-side.
    assert_eq!(games[0]["seat_index"], 0);

    let players = games[0]["players"].as_array().unwrap();
    assert_eq!(players.len(), 4, "all four seats are returned");
    // Ordered by seat_index, with name fallbacks for non-registered seats.
    assert_eq!(players[0]["seat_index"], 0);
    assert_eq!(players[0]["name"], "Alice");
    assert_eq!(players[0]["is_bot"], false);
    assert_eq!(players[1]["name"], "Bob");
    assert_eq!(players[2]["name"], "Bot");
    assert_eq!(players[2]["is_bot"], true);
    assert_eq!(players[3]["name"], "Guest");
    // No stamped result and no surviving game row → unknown.
    assert_eq!(games[0]["state"], "unknown");
    assert!(games[0]["team_score"].is_null());
}

#[tokio::test]
async fn profile_games_reflect_stamped_results() {
    use spades_server::auth::game_seats::SeatOwner;
    use uuid::Uuid;

    let env = common::test_env();
    let server = &env.server;
    server
        .post("/auth/register")
        .json(&json!({ "username": "Alice", "email": "alice@example.com", "password": "hunter2-strong" }))
        .await
        .assert_status(StatusCode::CREATED);
    let alice = env.store.find_user_by_username("Alice").unwrap().unwrap();

    let human = |id| SeatOwner {
        user_id: Some(id),
        anon_user_id: None,
        is_bot: false,
    };
    let bot = || SeatOwner {
        user_id: None,
        anon_user_id: None,
        is_bot: true,
    };

    // Alice at seat 0 (team A) in both games.
    let won = Uuid::new_v4();
    let lost = Uuid::new_v4();
    for g in [won, lost] {
        env.store
            .insert_game_seat(g, 0, Uuid::new_v4(), human(alice.id))
            .unwrap();
        for s in 1..4 {
            env.store
                .insert_game_seat(g, s, Uuid::new_v4(), bot())
                .unwrap();
        }
    }
    // Team A 312, Team B 245 → Alice (team A) won.
    env.store.record_game_results(won, 312, 245, false).unwrap();
    // Team A 180, Team B 320 → Alice lost.
    env.store
        .record_game_results(lost, 180, 320, false)
        .unwrap();

    let body: serde_json::Value = server.get("/users/Alice/games").await.json();
    let games = body["games"].as_array().unwrap();
    assert_eq!(games.len(), 2);
    // Find each by game_id (order is by created_at, not asserted here).
    let by_id = |id: Uuid| {
        games
            .iter()
            .find(|g| g["game_id"] == id.to_string())
            .unwrap()
            .clone()
    };
    let w = by_id(won);
    assert_eq!(w["state"], "won");
    assert_eq!(w["team_score"], 312);
    assert_eq!(w["opp_score"], 245);
    let l = by_id(lost);
    assert_eq!(l["state"], "lost");
    assert_eq!(l["team_score"], 180);
    assert_eq!(l["opp_score"], 320);
}

#[tokio::test]
async fn patch_me_email_change_triggers_reverify() {
    let server = common::test_server();
    server
        .post("/auth/register")
        .json(&json!({
            "username": "Alice", "email": "alice@example.com", "password": "hunter2-strong",
        }))
        .await
        .assert_status(StatusCode::CREATED);

    let resp = server
        .patch("/users/me")
        .json(&json!({
            "email": "alice2@example.com"
        }))
        .await;
    resp.assert_status(StatusCode::OK);

    // /auth/me should reflect the new email and email_verified=false.
    let me: serde_json::Value = server.get("/auth/me").await.json();
    assert_eq!(me["email"], "alice2@example.com");
    assert_eq!(me["email_verified"], false);
}

#[tokio::test]
async fn patch_me_password_change_requires_current() {
    let server = common::test_server();
    server
        .post("/auth/register")
        .json(&json!({
            "username": "Alice", "email": "alice@example.com", "password": "hunter2-strong",
        }))
        .await
        .assert_status(StatusCode::CREATED);

    // Missing current_password → 422.
    let resp = server
        .patch("/users/me")
        .json(&json!({
            "new_password": "even-stronger-pw",
        }))
        .await;
    resp.assert_status(StatusCode::UNPROCESSABLE_ENTITY);

    // Wrong current_password → 401.
    let resp = server
        .patch("/users/me")
        .json(&json!({
            "current_password": "wrong-pw",
            "new_password": "even-stronger-pw",
        }))
        .await;
    resp.assert_status(StatusCode::UNAUTHORIZED);

    // Correct → 200.
    let resp = server
        .patch("/users/me")
        .json(&json!({
            "current_password": "hunter2-strong",
            "new_password": "even-stronger-pw",
        }))
        .await;
    resp.assert_status(StatusCode::OK);
}

#[tokio::test]
async fn email_change_invalidates_other_sessions_via_token_version() {
    let env = common::test_env();

    let reg: serde_json::Value = env
        .server
        .post("/auth/register")
        .json(&json!({
            "username": "Alice", "email": "alice@example.com", "password": "hunter2-strong",
        }))
        .await
        .json();
    let user_id_s = reg["id"].as_str().unwrap();
    let user_id = uuid::Uuid::parse_str(user_id_s).unwrap();

    let before = env
        .store
        .find_user_by_id(user_id)
        .unwrap()
        .unwrap()
        .token_version;

    env.server
        .patch("/users/me")
        .json(&json!({
            "email": "alice2@example.com"
        }))
        .await
        .assert_status(axum::http::StatusCode::OK);

    let after = env
        .store
        .find_user_by_id(user_id)
        .unwrap()
        .unwrap()
        .token_version;
    assert!(after > before, "token_version should bump on email change");

    // The requester's session SHOULD still work (it was re-stamped).
    env.server
        .get("/auth/me")
        .await
        .assert_status(axum::http::StatusCode::OK);
}

#[tokio::test]
async fn patch_me_invalid_password_does_not_change_email() {
    let env = common::test_env();
    env.server
        .post("/auth/register")
        .json(&json!({
            "username": "Alice", "email": "alice@example.com", "password": "hunter2-strong",
        }))
        .await
        .assert_status(StatusCode::CREATED);

    // Try a combined change where new_password is invalid (too short).
    let resp = env
        .server
        .patch("/users/me")
        .json(&json!({
            "email": "alice2@example.com",
            "current_password": "hunter2-strong",
            "new_password": "tiny",  // <8 chars
        }))
        .await;
    resp.assert_status(StatusCode::UNPROCESSABLE_ENTITY);

    // Email should NOT have been changed (validation failed up front).
    let me: serde_json::Value = env.server.get("/auth/me").await.json();
    assert_eq!(me["email"], "alice@example.com");
}
