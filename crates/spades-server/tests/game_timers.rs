//! Deterministic timer / idle-behavior tests for the game actor.
//!
//! These drive `GameManager` directly (no HTTP server, no real socket) so the
//! whole test runs on a single paused-time runtime. `tokio::time::advance`
//! steps the per-turn Fischer clock past its deadline on demand, which makes
//! timeout-driven auto-play, aborts, and clock accounting fully reproducible —
//! no `sleep`, no wall-clock flakiness.
//!
//! The HTTP/WebSocket-facing side of timeouts (an abort reaching a live socket)
//! is covered separately in the `main.rs` in-crate test module, which needs the
//! real network transport and therefore can't use paused time.

use spades::ai::RandomStrategy;
use spades::{GameTransition, State, TimerConfig};
use spades_server::game_manager::{GameEvent, GameManager};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use uuid::Uuid;

/// Drain broadcast events until one satisfies `pred`, returning it. Panics if
/// the channel closes or a generous budget is exhausted first. Under paused
/// time `recv().await` only resolves once an already-scheduled actor task
/// produces the event, so a hang here means the expected event never fired.
async fn recv_until<F>(rx: &mut broadcast::Receiver<GameEvent>, pred: F) -> GameEvent
where
    F: Fn(&GameEvent) -> bool,
{
    for _ in 0..64 {
        match rx.recv().await {
            Ok(ev) => {
                if pred(&ev) {
                    return ev;
                }
            }
            Err(e) => panic!("broadcast closed before a matching event: {e:?}"),
        }
    }
    panic!("did not observe a matching event within budget");
}

/// Complete round-0 betting: four `Bet(_)` transitions issued back-to-back with
/// no time advanced, so the betting clocks never run down. Leaves the game in
/// `State::Trick(_)` with seat 0 on lead.
async fn finish_first_round_betting(manager: &GameManager, game_id: Uuid) {
    for _ in 0..4 {
        manager
            .make_transition(game_id, GameTransition::Bet(3))
            .await
            .expect("bet should be accepted during round-0 betting");
    }
}

/// A timed-out player in the very first betting round forfeits the whole game:
/// the actor plays `Abort` and fans out a `GameAborted` event. (The HTTP replay
/// test checks the transcript side-effect; this pins the broadcast contract.)
#[tokio::test(start_paused = true)]
async fn first_round_betting_timeout_broadcasts_game_aborted() {
    let manager = GameManager::new();
    let created = manager
        .create_game(
            500,
            Some(TimerConfig {
                initial_time_secs: 5,
                increment_secs: 0,
            }),
        )
        .unwrap();
    let mut sub = manager.subscribe(created.game_id, None).await.unwrap();

    manager
        .make_transition(created.game_id, GameTransition::Start)
        .await
        .unwrap();

    // Nobody bets. Push the lead player's clock past zero.
    tokio::time::advance(Duration::from_millis(5_001)).await;

    let ev = recv_until(&mut sub.rx, |e| matches!(e, GameEvent::GameAborted { .. })).await;
    match ev {
        GameEvent::GameAborted {
            reason, game_id, ..
        } => {
            assert_eq!(game_id, created.game_id);
            assert!(
                reason.to_lowercase().contains("first round"),
                "unexpected abort reason: {reason}"
            );
        }
        other => panic!("expected GameAborted, got {other:?}"),
    }
}

/// Once past first-round betting, a trick-phase timeout does NOT abort — the
/// actor auto-plays a card for the idle player and the turn advances.
#[tokio::test(start_paused = true)]
async fn trick_timeout_autoplays_a_card_for_idle_player() {
    let manager = GameManager::new();
    let created = manager
        .create_game(
            500,
            Some(TimerConfig {
                initial_time_secs: 60,
                increment_secs: 0,
            }),
        )
        .unwrap();
    let mut sub = manager.subscribe(created.game_id, None).await.unwrap();

    manager
        .make_transition(created.game_id, GameTransition::Start)
        .await
        .unwrap();
    finish_first_round_betting(&manager, created.game_id).await;

    // Drain events up to and including entry into the trick phase. Seat 0 is on
    // lead and idle.
    recv_until(&mut sub.rx, |e| {
        matches!(
            e,
            GameEvent::StateChanged {
                state, ..
            } if matches!(state.state, State::Trick(_)) && state.current_player_id == Some(created.player_ids[0])
        )
    })
    .await;

    // The card the actor is about to auto-play comes from this hand.
    let hand_before = manager
        .get_hand(created.game_id, created.player_ids[0])
        .await
        .unwrap()
        .cards;

    // Run the lead player's clock out.
    tokio::time::advance(Duration::from_millis(60_001)).await;

    // The next state change must show a card on seat 0's slot and the turn
    // moved on — proof the server played *for* the idle seat rather than
    // aborting.
    let ev = recv_until(&mut sub.rx, |e| match e {
        GameEvent::StateChanged { state, .. } => {
            state.table_cards.map(|t| t[0].is_some()).unwrap_or(false)
        }
        _ => false,
    })
    .await;

    let GameEvent::StateChanged { state, .. } = ev else {
        unreachable!()
    };
    let played = state.table_cards.unwrap()[0].expect("seat 0 played a card");
    assert_ne!(
        state.current_player_id,
        Some(created.player_ids[0]),
        "turn should have advanced off the idle seat"
    );

    // We assert the auto-played card came from the idle seat's own hand. We
    // deliberately stop at hand-membership rather than full legal-lead checking:
    // the point of this test is that a timeout *plays for* the idle seat (vs.
    // aborting), and "a card it actually held" is the robust invariant for that.
    assert!(
        hand_before.contains(&played),
        "auto-played card {played:?} was not in the idle seat's hand"
    );
}

/// A timely move bills the elapsed time against the mover's clock and credits
/// the Fischer increment. Guards the `cancel_timer` accounting in game_actor.rs.
#[tokio::test(start_paused = true)]
async fn timely_move_bills_elapsed_and_credits_increment() {
    let manager = GameManager::new();
    let created = manager
        .create_game(
            500,
            Some(TimerConfig {
                initial_time_secs: 60,
                increment_secs: 5,
            }),
        )
        .unwrap();
    let mut sub = manager.subscribe(created.game_id, None).await.unwrap();

    manager
        .make_transition(created.game_id, GameTransition::Start)
        .await
        .unwrap();
    // Start broadcasts a StateChanged; consume it so the next recv is the bet.
    let _ = sub.rx.recv().await.unwrap();

    // Seat 0 thinks for exactly 10s, then bets in time.
    tokio::time::advance(Duration::from_millis(10_000)).await;
    manager
        .make_transition(created.game_id, GameTransition::Bet(3))
        .await
        .unwrap();

    let ev = sub.rx.recv().await.unwrap();
    let GameEvent::StateChanged { state, .. } = ev else {
        panic!("expected StateChanged after bet, got {ev:?}");
    };
    let clocks = state.player_clocks_ms.expect("timed game reports clocks");

    // Under paused time the elapsed bill is exact, so we assert the whole clock
    // array exactly. Seat 0: 60_000 initial - 10_000 elapsed + 5_000 increment =
    // 55_000. Seats 1-3 never moved, so they remain at the full 60_000 (seat 1 is
    // now the active player, but with 0 ms elapsed its overlaid clock is still
    // 60_000). Exact equality catches off-by-one billing, a missing or
    // double-applied increment, and accidental cross-seat clock writes.
    assert_eq!(
        clocks,
        [55_000, 60_000, 60_000, 60_000],
        "only seat 0's clock should change: initial - elapsed + increment"
    );
}

/// Acting before the deadline must cancel the pending timeout: advancing past
/// the *original* deadline afterwards fires nothing. Exercises the
/// cancel-on-transition path (and, by extension, the stale-timer guard).
#[tokio::test(start_paused = true)]
async fn timely_move_cancels_the_pending_timeout() {
    let manager = GameManager::new();
    let created = manager
        .create_game(
            500,
            Some(TimerConfig {
                initial_time_secs: 60,
                increment_secs: 0,
            }),
        )
        .unwrap();
    let mut sub = manager.subscribe(created.game_id, None).await.unwrap();

    manager
        .make_transition(created.game_id, GameTransition::Start)
        .await
        .unwrap();
    let _ = sub.rx.recv().await.unwrap(); // Start

    // Seat 0 uses 30s, then bets. This cancels its 60s timer and starts seat 1's.
    tokio::time::advance(Duration::from_millis(30_000)).await;
    manager
        .make_transition(created.game_id, GameTransition::Bet(3))
        .await
        .unwrap();
    let _ = sub.rx.recv().await.unwrap(); // bet result

    // Cross what *would* have been seat 0's original deadline (30s + 31s = 61s).
    // Seat 1's fresh 60s timer is only 31s in, so still nothing should fire.
    tokio::time::advance(Duration::from_millis(31_000)).await;
    for _ in 0..8 {
        tokio::task::yield_now().await;
    }

    match sub.rx.try_recv() {
        Err(broadcast::error::TryRecvError::Empty) => {} // expected: no stale fire
        Ok(ev) => panic!("a cancelled timer produced a spurious event: {ev:?}"),
        Err(e) => panic!("unexpected receiver error: {e:?}"),
    }
}

/// Idle human in an AI game: once past first-round betting, the human's trick
/// timeout auto-plays for them and the AI seats then play out the rest of the
/// trick in the same `handle_timer_fired` loop.
#[tokio::test(start_paused = true)]
async fn idle_human_timeout_then_ai_finishes_the_trick() {
    let manager = GameManager::new();
    let human_seats: HashSet<usize> = [0].into_iter().collect();
    let created = manager
        .create_ai_game(
            human_seats,
            500,
            Some(TimerConfig {
                initial_time_secs: 60,
                increment_secs: 0,
            }),
            Arc::new(RandomStrategy),
        )
        .unwrap();
    let mut sub = manager.subscribe(created.game_id, None).await.unwrap();

    // Human (seat 0) starts and bets; the AI seats auto-bet to close round-0
    // betting, leaving seat 0 on lead in the trick phase.
    manager
        .make_transition(created.game_id, GameTransition::Start)
        .await
        .unwrap();
    manager
        .make_transition(created.game_id, GameTransition::Bet(3))
        .await
        .unwrap();

    recv_until(&mut sub.rx, |e| match e {
        GameEvent::StateChanged { state, .. } => {
            matches!(state.state, State::Trick(_))
                && state.current_player_id == Some(created.player_ids[0])
        }
        _ => false,
    })
    .await;

    // Human goes idle; their clock runs out.
    tokio::time::advance(Duration::from_millis(60_001)).await;

    // A completed trick proves the human was auto-played *and* the AI seats
    // followed without any further human input.
    recv_until(&mut sub.rx, |e| match e {
        GameEvent::StateChanged { state, .. } => state.last_completed_trick.is_some(),
        _ => false,
    })
    .await;
}
