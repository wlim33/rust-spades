import type { Card } from '../state/helpers';
import { RANK_ORDER, seatRel } from '../state/helpers';
import type { Seat } from '../cards/hand-manager';
import { tnCardToApp } from './types';
import type { ReplayResponse } from './types';

// ----------------------------------------------------------------------------
// Types

export type Move =
  | { kind: 'bid'; round: number }
  | { kind: 'card'; round: number };

export type ViewState = {
  round: number;
  totalRounds: number;
  toAct: Seat | null;
  hands: Record<Seat, Card[]>;
  trick: { seat: Seat; card: Card }[];
  trickWinner: Seat | null;
  bids: Record<Seat, number | null>;
  tricksWon: Record<Seat, number>;
  score: [number, number];
  phase: 'bid' | 'play' | 'done';
  aborted: boolean;
};

// Absolute seat indices: N=0, E=1, S=2, W=3
const SEAT_NAME_TO_ABS: Record<string, number> = { N: 0, E: 1, S: 2, W: 3 };
const ABS_SEATS = [0, 1, 2, 3] as const;
const ALL_SEATS: Seat[] = ['south', 'north', 'east', 'west'];

// ----------------------------------------------------------------------------
// Internal parsed structures

type ParsedTrick = {
  // Parallel arrays: absIdx of player and the app Card they played
  plays: Array<{ absIdx: number; card: Card }>;
};

type ParsedRound = {
  roundNumber: number; // 1-based
  // hands[absIdx] = cards in that seat's hand
  hands: Record<number, Card[]>;
  // bids[absIdx] = bid value (nil → 0)
  bids: Record<number, number>;
  tricks: ParsedTrick[];
};

// A step in the linear step list
type Step =
  | { kind: 'bid'; round: number; absIdx: number }
  | { kind: 'card'; round: number; trickIdx: number; cardIdx: number };

// ----------------------------------------------------------------------------
// Trick winner logic (pure, local — no engine import)

function trickWinnerAbsIdx(plays: Array<{ absIdx: number; card: Card }>): number {
  if (plays.length === 0) return 0;
  const leadSuit = plays[0]!.card.suit;

  let bestIdx = plays[0]!.absIdx;
  let bestCard = plays[0]!.card;

  for (let i = 1; i < plays.length; i++) {
    const { absIdx, card } = plays[i]!;
    const isBetter = beats(card, bestCard, leadSuit);
    if (isBetter) {
      bestIdx = absIdx;
      bestCard = card;
    }
  }
  return bestIdx;
}

function beats(challenger: Card, current: Card, leadSuit: string): boolean {
  // Spade always beats non-spade
  if (challenger.suit === 'Spade' && current.suit !== 'Spade') return true;
  if (challenger.suit !== 'Spade' && current.suit === 'Spade') return false;
  // Both same suit category: compare by rank if same suit, off-suit can't beat
  if (challenger.suit === current.suit) {
    return RANK_ORDER.indexOf(challenger.rank) > RANK_ORDER.indexOf(current.rank);
  }
  // Off-suit non-spade vs on-suit non-spade: challenger loses
  if (challenger.suit !== leadSuit) return false;
  // Both are different suits from each other, challenger is lead suit, current is off
  return true;
}

// ----------------------------------------------------------------------------
// Parse the model into rounds

function parseRounds(res: ReplayResponse): ParsedRound[] {
  const events = res.model.events;
  const rounds: ParsedRound[] = [];
  let currentRound: ParsedRound | null = null;

  for (const evt of events) {
    if (evt.type === 'deal') {
      // Each deal starts a new round
      currentRound = {
        roundNumber: rounds.length + 1,
        hands: {},
        bids: {},
        tricks: [],
      };
      rounds.push(currentRound);
      for (const dh of (evt as { type: 'deal'; hands: { target: string; cards: { suit: string; rank: string }[] }[] }).hands) {
        const abs = SEAT_NAME_TO_ABS[dh.target];
        if (abs !== undefined) {
          currentRound.hands[abs] = dh.cards.map(tnCardToApp);
        }
      }
    } else if (evt.type === 'call' && currentRound) {
      // values are in N,E,S,W order (abs 0,1,2,3)
      const callEvt = evt as { type: 'call'; start: string; values: string[] };
      callEvt.values.forEach((v, i) => {
        currentRound!.bids[i] = v === 'nil' ? 0 : parseInt(v, 10);
      });
    } else if (evt.type === 'play' && currentRound) {
      const playEvt = evt as { type: 'play'; leader: string; cards: { suit: string; rank: string }[] };
      const leaderAbs = SEAT_NAME_TO_ABS[playEvt.leader] ?? 0;
      const plays: ParsedTrick['plays'] = playEvt.cards.map((c, i) => ({
        absIdx: (leaderAbs + i) % 4,
        card: tnCardToApp(c),
      }));
      currentRound.tricks.push({ plays });
    }
  }
  return rounds;
}

// Build linear step list from all rounds
function buildSteps(rounds: ParsedRound[]): Step[] {
  const steps: Step[] = [];
  for (const round of rounds) {
    // 4 bid steps
    for (const abs of ABS_SEATS) {
      steps.push({ kind: 'bid', round: round.roundNumber, absIdx: abs });
    }
    // per-trick, 4 card steps
    for (let tIdx = 0; tIdx < round.tricks.length; tIdx++) {
      const trick = round.tricks[tIdx]!;
      for (let cIdx = 0; cIdx < trick.plays.length; cIdx++) {
        steps.push({ kind: 'card', round: round.roundNumber, trickIdx: tIdx, cardIdx: cIdx });
      }
    }
  }
  return steps;
}

// ----------------------------------------------------------------------------
// ReplayController

export class ReplayController {
  readonly viewerSeatIdx: number;

  private readonly rounds: ParsedRound[];
  private readonly steps: Step[];
  private readonly cumulativeScores: [number, number][];
  private readonly abortedFlag: boolean;

  // cursor: -1 = before any step (atStart), steps.length-1 = atEnd
  private cursor = -1;

  constructor(res: ReplayResponse) {
    this.viewerSeatIdx = res.viewer_seat ?? 0;
    this.rounds = parseRounds(res);
    this.steps = buildSteps(this.rounds);
    this.cumulativeScores = (res.cumulative_by_round ?? []).map(
      (pair) => [pair[0]!, pair[1]!] as [number, number],
    );
    // Check meta.extra for Termination/Aborted marker
    const extra: unknown[] = (res.model.meta as unknown as { extra?: unknown[] }).extra ?? [];
    this.abortedFlag = extra.some((e) => {
      if (typeof e === 'string') return e === 'Aborted';
      if (typeof e === 'object' && e !== null) return 'Aborted' in e;
      return false;
    });
  }

  // Navigation
  next(): void {
    if (this.cursor < this.steps.length - 1) this.cursor++;
  }

  prev(): void {
    if (this.cursor > -1) this.cursor--;
  }

  seekStart(): void {
    this.cursor = -1;
  }

  seekEnd(): void {
    this.cursor = this.steps.length - 1;
  }

  jumpRound(r: number): void {
    if (this.rounds.length === 0 || r < 1) {
      this.cursor = -1;
      return;
    }
    if (r > this.rounds.length) {
      this.cursor = this.steps.length - 1;
      return;
    }
    // Find the last step belonging to round `r`
    let lastStepForRound = -1;
    for (let i = 0; i < this.steps.length; i++) {
      if (this.steps[i]!.round === r) lastStepForRound = i;
    }
    if (lastStepForRound === -1) {
      this.cursor = -1;
    } else {
      this.cursor = lastStepForRound;
    }
  }

  atStart(): boolean {
    return this.cursor === -1;
  }

  atEnd(): boolean {
    return this.cursor === this.steps.length - 1;
  }

  stepIndex(): number {
    return this.cursor;
  }

  totalSteps(): number {
    return this.steps.length;
  }

  state(): ViewState {
    const v = this.viewerSeatIdx;
    const rel = (abs: number): Seat => seatRel(abs, v);

    // Determine which round we're in and what has been revealed up to cursor
    // Start: cursor = -1, all deals visible but no bids/plays shown
    const totalRounds = this.rounds.length;

    // Determine current round number (1-based)
    let currentRoundNum = 1;
    if (this.cursor >= 0 && this.steps[this.cursor]) {
      currentRoundNum = this.steps[this.cursor]!.round;
    }
    const round = this.rounds.find((r) => r.roundNumber === currentRoundNum) ?? this.rounds[0];
    if (!round) {
      // Empty model fallback
      return this.emptyViewState(totalRounds);
    }

    // Build revealed bids and trick state by replaying up to cursor
    const revealedBids: Partial<Record<number, number>> = {};
    const completedTricks: ParsedTrick[] = [];
    let currentTrickPlays: Array<{ absIdx: number; card: Card }> = [];
    let phase: 'bid' | 'play' | 'done' = 'bid';

    // Count completed tricks before current round for toAct tracking
    for (let i = 0; i <= this.cursor; i++) {
      const step = this.steps[i]!;
      if (step.round !== currentRoundNum) continue;

      if (step.kind === 'bid') {
        revealedBids[step.absIdx] = round.bids[step.absIdx] ?? 0;
      } else if (step.kind === 'card') {
        const trick = round.tricks[step.trickIdx];
        if (!trick) continue;
        const play = trick.plays[step.cardIdx];
        if (!play) continue;

        if (step.trickIdx > completedTricks.length) {
          // skip gap (shouldn't happen)
        } else if (step.trickIdx === completedTricks.length) {
          // Adding to current trick
          currentTrickPlays.push(play);
          if (currentTrickPlays.length === 4) {
            completedTricks.push({ plays: [...currentTrickPlays] });
            currentTrickPlays = [];
          }
        }
      }
    }

    // Determine phase
    const allBidsRevealed = Object.keys(revealedBids).length === 4;
    if (!allBidsRevealed) {
      phase = 'bid';
    } else if (this.cursor === this.steps.length - 1) {
      phase = 'done';
    } else {
      phase = 'play';
    }

    // Build bids record (relative seats)
    const bids: Record<Seat, number | null> = { south: null, north: null, east: null, west: null };
    for (const [absStr, val] of Object.entries(revealedBids)) {
      const abs = parseInt(absStr, 10);
      if (val !== undefined) bids[rel(abs)] = val;
    }

    // Build trick record for display (current incomplete trick)
    const trick: { seat: Seat; card: Card }[] = currentTrickPlays.map((p) => ({
      seat: rel(p.absIdx),
      card: p.card,
    }));

    // Trick winner: only when current trick is complete (currentTrickPlays was just flushed)
    // The last completed trick is relevant when a trick just finished
    // trickWinner = winner of the most recently completed trick (if at end of a trick)
    let trickWinner: Seat | null = null;
    if (completedTricks.length > 0 && currentTrickPlays.length === 0) {
      // We just completed a trick — show winner
      const lastTrick = completedTricks[completedTricks.length - 1]!;
      trickWinner = rel(trickWinnerAbsIdx(lastTrick.plays));
    }

    // Tricks won per seat (relative) — count all completed tricks in this round
    const tricksWonAbs: Record<number, number> = { 0: 0, 1: 0, 2: 0, 3: 0 };
    for (const t of completedTricks) {
      const winnerAbs = trickWinnerAbsIdx(t.plays);
      tricksWonAbs[winnerAbs] = (tricksWonAbs[winnerAbs] ?? 0) + 1;
    }
    const tricksWon: Record<Seat, number> = { south: 0, north: 0, east: 0, west: 0 };
    for (const abs of ABS_SEATS) {
      tricksWon[rel(abs)] = tricksWonAbs[abs] ?? 0;
    }

    // Score: cumulative from completed rounds
    // At step in round R: score = cumulative_by_round[R-1] once the round is done
    // During round R: carry cumulative_by_round[R-2] (previous round), or [0,0] for round 1
    let score: [number, number] = [0, 0];
    const isLastStepOfRound = this.cursor >= 0 &&
      (this.cursor === this.steps.length - 1 ||
        this.steps[this.cursor + 1]!.round !== currentRoundNum);
    if (isLastStepOfRound && this.cumulativeScores[currentRoundNum - 1]) {
      score = this.cumulativeScores[currentRoundNum - 1]!;
    } else if (currentRoundNum > 1 && this.cumulativeScores[currentRoundNum - 2]) {
      score = this.cumulativeScores[currentRoundNum - 2]!;
    }

    // Hands (from current round's deal)
    const hands: Record<Seat, Card[]> = { south: [], north: [], east: [], west: [] };
    for (const abs of ABS_SEATS) {
      hands[rel(abs)] = round.hands[abs] ?? [];
    }

    // toAct: next seat to act
    let toAct: Seat | null = null;
    if (phase === 'bid') {
      const nextBidAbs = ABS_SEATS.find((abs) => !(abs in revealedBids));
      toAct = nextBidAbs !== undefined ? rel(nextBidAbs) : null;
    } else if (phase === 'play') {
      // Next card to play in current trick
      if (currentTrickPlays.length < 4) {
        const trickIdx = completedTricks.length;
        const trick = round.tricks[trickIdx];
        if (trick) {
          const nextPlay = trick.plays[currentTrickPlays.length];
          if (nextPlay) toAct = rel(nextPlay.absIdx);
        }
      }
    }

    return {
      round: currentRoundNum,
      totalRounds,
      toAct,
      hands,
      trick,
      trickWinner,
      bids,
      tricksWon,
      score,
      phase,
      aborted: this.abortedFlag,
    };
  }

  private emptyViewState(totalRounds: number): ViewState {
    return {
      round: 1,
      totalRounds,
      toAct: null,
      hands: { south: [], north: [], east: [], west: [] },
      trick: [],
      trickWinner: null,
      bids: { south: null, north: null, east: null, west: null },
      tricksWon: { south: 0, north: 0, east: 0, west: 0 },
      score: [0, 0],
      phase: 'done',
      aborted: this.abortedFlag,
    };
  }
}

// Export RANK_ORDER for test access (re-export from helpers to keep controller self-contained)
export { RANK_ORDER };
