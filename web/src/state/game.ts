import { signal, type Signal } from '@preact/signals-core';
import type { Card, Phase, Suit } from './helpers';
import { getLeadSuit } from './helpers';

export type GameStateValue = string | { Bidding?: number; Trick?: number; Completed?: unknown };

export type PlayerNameEntry = { player_id: string; name: string | null };
export type TimerConfig = { initial_time_secs: number; increment_secs: number } | null;
export type PresencePlayer = { player_id: string; connected: boolean };

export type GameStateResponse = {
  game_id: string;
  state: GameStateValue;
  team_a_score: number;
  team_b_score: number;
  team_a_bags: number;
  team_b_bags: number;
  current_player_id: string | null;
  player_names: PlayerNameEntry[];
  table_cards?: (Card | null)[];
  player_bets?: (number | null)[];
  player_tricks_won?: number[];
  last_trick_winner_id?: string | null;
  timer_config?: TimerConfig;
  player_clocks_ms?: (number | null)[] | null;
  active_player_clock_ms?: number | null;
  last_completed_trick?: (Card | null)[] | null;
  short_id?: string | null;
  /** Per-game monotonic event cursor; present on WS events, absent on REST snapshots. */
  seq?: number;
};

export type HandResponse = {
  player_id: string;
  cards: Card[];
};

export type GameStore = {
  playerId: Signal<string>;
  phase: Signal<Phase>;
  gameState: Signal<GameStateValue | null>;
  playerIds: Signal<string[]>;
  playerNames: Signal<(string | null)[]>;
  playerConnected: Signal<boolean[]>;
  currentPlayerId: Signal<string | null>;
  hand: Signal<Card[]>;
  tableCards: Signal<(Card | null)[]>;
  playerBets: Signal<(number | null)[]>;
  playerTricksWon: Signal<number[]>;
  lastTrickWinnerId: Signal<string | null>;
  lastCompletedTrick: Signal<(Card | null)[] | null>;
  teamAScore: Signal<number>;
  teamBScore: Signal<number>;
  teamABags: Signal<number>;
  teamBBags: Signal<number>;
  timerConfig: Signal<TimerConfig>;
  playerClocksMs: Signal<(number | null)[] | null>;
  activePlayerClockMs: Signal<number | null>;
  spadesBroken: Signal<boolean>;
  applyState: (state: GameStateResponse, hand: HandResponse, isSnapshot?: boolean) => void;
  applyPresence: (players: PresencePlayer[]) => void;
};

function phaseFromState(g: GameStateValue): Phase {
  if (typeof g === 'object' && g !== null && 'Bidding' in g) return 'BETTING';
  if (typeof g === 'object' && g !== null && 'Trick' in g) return 'PLAYING';
  if (g === 'Completed') return 'GAME_OVER';
  if (typeof g === 'string') {
    if (g.startsWith('Bidding')) return 'BETTING';
    if (g.startsWith('Trick')) return 'PLAYING';
  }
  return 'LOBBY';
}

/**
 * Assign to an array signal only when its contents actually changed. Signals
 * compare by Object.is, so a freshly built array always notifies even when it
 * holds identical values; this shallow compare (over our ≤13-element game
 * arrays of primitives) keeps unchanged signals quiet, so one WS event doesn't
 * re-render everything subscribed to seats/bets/tricks that didn't move.
 */
function setIfChanged<T>(sig: Signal<T[]>, next: T[]): void {
  const cur = sig.value;
  if (cur.length === next.length && cur.every((v, i) => v === next[i])) return;
  sig.value = next;
}

export function createGameStore(playerIdInit: string): GameStore {
  const playerId = signal(playerIdInit);
  const phase = signal<Phase>('LOBBY');
  const gameState = signal<GameStateValue | null>(null);
  const playerIds = signal<string[]>([]);
  const playerNames = signal<(string | null)[]>([null, null, null, null]);
  const playerConnected = signal<boolean[]>([true, true, true, true]);
  const currentPlayerId = signal<string | null>(null);
  const hand = signal<Card[]>([]);
  const tableCards = signal<(Card | null)[]>([null, null, null, null]);
  const playerBets = signal<(number | null)[]>([null, null, null, null]);
  const playerTricksWon = signal<number[]>([0, 0, 0, 0]);
  const lastTrickWinnerId = signal<string | null>(null);
  const lastCompletedTrick = signal<(Card | null)[] | null>(null);
  const teamAScore = signal(0);
  const teamBScore = signal(0);
  const teamABags = signal(0);
  const teamBBags = signal(0);
  const timerConfig = signal<TimerConfig>(null);
  const playerClocksMs = signal<(number | null)[] | null>(null);
  const activePlayerClockMs = signal<number | null>(null);
  const spadesBroken = signal(false);

  const updateSpadesBroken = (): void => {
    if (phase.value === 'BETTING') {
      spadesBroken.value = false;
      return;
    }
    if (spadesBroken.value) return;
    const myIdx = playerIds.value.indexOf(playerId.value);
    const leadSuit: Suit | null = getLeadSuit(tableCards.value, myIdx < 0 ? 0 : myIdx);
    if (leadSuit && leadSuit !== 'Spade') {
      for (const c of tableCards.value) {
        if (c && c.suit === 'Spade') {
          spadesBroken.value = true;
          return;
        }
      }
    }
  };

  // WS events can resolve out of order (each triggers an async hand fetch);
  // the per-game `seq` cursor lets us drop anything older than what we hold.
  let lastSeq = -1;

  const applyState: GameStore['applyState'] = (state, handData, isSnapshot = false) => {
    // A non-state WS event (resync / chat_message) mis-routed here carries no
    // `state`. Ignore it outright: applying it would blank the board, and for
    // chat (which carries a seq) it would also advance the cursor and drop the
    // next real event.
    if (state == null || state.state === undefined) return;
    if (typeof state.seq === 'number') {
      if (isSnapshot) {
        // The connect snapshot's seq is the cursor the NEXT streamed event will
        // carry (server contract) — not an event we've applied. Seed one below
        // it so that first event passes the `<=` guard instead of being dropped.
        // (max() guards against a snapshot somehow trailing applied events.)
        lastSeq = Math.max(lastSeq, state.seq - 1);
      } else {
        if (state.seq <= lastSeq) return;
        lastSeq = state.seq;
      }
    }
    gameState.value = state.state;
    currentPlayerId.value = state.current_player_id;
    teamAScore.value = state.team_a_score;
    teamBScore.value = state.team_b_score;
    teamABags.value = state.team_a_bags;
    teamBBags.value = state.team_b_bags;
    timerConfig.value = state.timer_config ?? null;
    playerClocksMs.value = state.player_clocks_ms ?? null;
    activePlayerClockMs.value = state.active_player_clock_ms ?? null;
    // Identity-stable arrays that rarely change mid-game: assign only on real
    // change so unchanged signals stay quiet. The object arrays (table/hand/
    // last-trick) change most events anyway, so they're direct assignments.
    setIfChanged(
      playerNames,
      (state.player_names ?? []).map((e) => e.name),
    );
    setIfChanged(
      playerIds,
      (state.player_names ?? []).map((e) => e.player_id),
    );
    tableCards.value = state.table_cards ?? [null, null, null, null];
    setIfChanged(playerBets, state.player_bets ?? [null, null, null, null]);
    setIfChanged(playerTricksWon, state.player_tricks_won ?? [0, 0, 0, 0]);
    lastTrickWinnerId.value = state.last_trick_winner_id ?? null;
    lastCompletedTrick.value = state.last_completed_trick ?? null;
    hand.value = handData.cards ?? [];
    phase.value = phaseFromState(state.state);
    updateSpadesBroken();
  };

  const applyPresence: GameStore['applyPresence'] = (players) => {
    // Seed from the current flags, not all-connected: a partial frame (or one
    // that arrives before playerIds is populated) must not resurrect a player
    // we already know is disconnected.
    const next = [...playerConnected.value];
    for (const p of players) {
      const idx = playerIds.value.indexOf(p.player_id);
      if (idx !== -1) next[idx] = p.connected;
    }
    setIfChanged(playerConnected, next);
  };

  return {
    playerId,
    phase,
    gameState,
    playerIds,
    playerNames,
    playerConnected,
    currentPlayerId,
    hand,
    tableCards,
    playerBets,
    playerTricksWon,
    lastTrickWinnerId,
    lastCompletedTrick,
    teamAScore,
    teamBScore,
    teamABags,
    teamBBags,
    timerConfig,
    playerClocksMs,
    activePlayerClockMs,
    spadesBroken,
    applyState,
    applyPresence,
  };
}
