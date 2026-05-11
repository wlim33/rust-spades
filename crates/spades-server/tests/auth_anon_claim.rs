use serde_json::json;
mod common;

#[tokio::test]
async fn anon_claim_attaches_seats() {
    let env = common::test_env();

    // 1. Register Bob so we have a user_id in the store.
    let reg = env.server.post("/auth/register").json(&json!({
        "username": "Bob", "email": "bob@example.com", "password": "hunter2-strong",
    })).await;
    reg.assert_status(axum::http::StatusCode::CREATED);
    let bob_id_s = reg.json::<serde_json::Value>()["id"].as_str().unwrap().to_string();
    let bob_id = uuid::Uuid::parse_str(&bob_id_s).unwrap();

    // 2. Insert a game_seat row owned by a random anon_user_id.
    let anon_id = uuid::Uuid::new_v4();
    env.store.insert_game_seat(
        uuid::Uuid::new_v4(),
        0,
        uuid::Uuid::new_v4(),
        spades_server::auth::game_seats::SeatOwner {
            user_id: None,
            anon_user_id: Some(anon_id),
            is_bot: false,
        },
    ).unwrap();

    // 3. Call claim_anon_game_seats: the seat should transfer to Bob.
    let claimed = env.store.claim_anon_game_seats(anon_id, bob_id).unwrap();
    assert_eq!(claimed, 1, "exactly one seat should have been claimed");

    // 4. Bob's seat count should now reflect that seat.
    let count = env.store.count_game_seats_for_user(bob_id).unwrap();
    assert_eq!(count, 1, "Bob should show 1 game seat after claim");
}

#[tokio::test]
async fn claim_with_wrong_anon_id_is_noop() {
    let env = common::test_env();

    let reg = env.server.post("/auth/register").json(&json!({
        "username": "Alice", "email": "alice@example.com", "password": "hunter2-strong",
    })).await;
    reg.assert_status(axum::http::StatusCode::CREATED);
    let alice_id = uuid::Uuid::parse_str(
        reg.json::<serde_json::Value>()["id"].as_str().unwrap()
    ).unwrap();

    // Insert seat owned by a different anon_id.
    let other_anon = uuid::Uuid::new_v4();
    env.store.insert_game_seat(
        uuid::Uuid::new_v4(),
        0,
        uuid::Uuid::new_v4(),
        spades_server::auth::game_seats::SeatOwner {
            user_id: None,
            anon_user_id: Some(other_anon),
            is_bot: false,
        },
    ).unwrap();

    // Claim with a completely different anon_id → 0 rows updated.
    let claimed = env.store.claim_anon_game_seats(uuid::Uuid::new_v4(), alice_id).unwrap();
    assert_eq!(claimed, 0);

    // Alice still has 0 seats.
    assert_eq!(env.store.count_game_seats_for_user(alice_id).unwrap(), 0);
}
