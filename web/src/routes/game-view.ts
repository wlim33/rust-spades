import { html, render, type TemplateResult } from 'lit-html';
import { effect } from '@preact/signals-core';
import { request } from '../api/client';
import {
  sortCards,
  isCardValid,
  oppCardCount,
  seatRel,
  formatClock,
  trickNumber,
  type Card,
  type RelativeSeat,
} from '../state/helpers';
import {
  clockTick,
  captureActiveClock,
  liveActiveMs,
  startClockTicker,
  stopClockTicker,
  LOW_CLOCK_MS,
} from '../state/clocks';
import { clearSession } from '../lib/storage';
import { navigateTo } from '../lib/util';
import { toast } from '../state/toast';
import { CardOrchestrator } from '../cards/orchestrator';
import { appShell } from '../ui/templates';
import { button } from '../ui/components/button';
import { bidBar } from '../ui/components/bid-bar';
import { scores } from '../ui/components/scores';
import { gameTable, type GameTableRefs } from '../ui/components/game-table';
import type { GameStore } from '../state/game';
import type { Resources } from './play-resources';

export function renderInGame(args: {
  root: HTMLElement;
  store: GameStore;
  gameId: string;
  shortId: string;
  resources: Resources;
  refs: GameTableRefs;
}): void {
  const { store, refs, root } = args;

  const myIdx = (): number => store.playerIds.value.indexOf(store.playerId.value);
  const seatName = (idx: number): string => {
    return store.playerNames.value[idx] ?? `Seat ${idx + 1}`;
  };

  const timed = (): boolean => store.timerConfig.value != null;
  const clockFor = (absIdx: number): string | null => {
    if (!timed()) return null;
    if (store.playerIds.value[absIdx] === store.currentPlayerId.value)
      return formatClock(liveActiveMs());
    return formatClock(store.playerClocksMs.value?.[absIdx] ?? null);
  };
  const lowFor = (absIdx: number): boolean =>
    timed() &&
    store.playerIds.value[absIdx] === store.currentPlayerId.value &&
    (liveActiveMs() ?? Infinity) <= LOW_CLOCK_MS;
  const fracFor = (absIdx: number): number | null => {
    if (!timed() || store.playerIds.value[absIdx] !== store.currentPlayerId.value) return null;
    const initialMs = (store.timerConfig.value?.initial_time_secs ?? 0) * 1000;
    if (initialMs <= 0) return null;
    const live = liveActiveMs();
    if (live == null) return null;
    return Math.max(0, Math.min(1, live / initialMs));
  };

  const template = (): TemplateResult => {
    void clockTick.value;
    const i = myIdx();
    const north = (i + 2) % 4;
    const west = (i + 3) % 4;
    const east = (i + 1) % 4;
    const teamA = i === 0 || i === 2 ? 'A' : 'B';
    const isMyTurn = store.currentPlayerId.value === store.playerId.value;

    const betButtons = (): TemplateResult => {
      if (store.phase.value !== 'BETTING' || !isMyTurn) return html``;
      const onBet = async (amount: number): Promise<void> => {
        try {
          await request(`/games/${args.gameId}/transition`, {
            method: 'POST',
            body: JSON.stringify({ type: 'bet', amount }),
          });
        } catch (e) {
          console.error('bet failed', e);
          toast.error('Bet failed.');
        }
      };
      return bidBar({ onBet: (amount) => void onBet(amount) });
    };

    const centerText =
      store.phase.value === 'GAME_OVER'
        ? store.teamAScore.value === store.teamBScore.value
          ? "It's a tie!"
          : store.teamAScore.value > store.teamBScore.value
            ? 'Team A wins!'
            : 'Team B wins!'
        : store.phase.value === 'BETTING'
          ? isMyTurn
            ? 'Place your bet!'
            : `Waiting for ${seatName(store.playerIds.value.indexOf(store.currentPlayerId.value ?? ''))}…`
          : store.phase.value === 'PLAYING' && isMyTurn
            ? 'Your turn'
            : '';

    const playAgain =
      store.phase.value === 'GAME_OVER'
        ? button({
            label: 'Play Again',
            onClick: () => {
              clearSession(args.shortId);
              navigateTo('/');
            },
            variant: 'primary',
          })
        : html``;

    return appShell(html`
      ${scores({
        teamAScore: store.teamAScore.value,
        teamBScore: store.teamBScore.value,
        teamABags: store.teamABags.value,
        teamBBags: store.teamBBags.value,
        myTeam: teamA,
        centerText:
          store.phase.value === 'PLAYING'
            ? `Trick ${trickNumber(store.playerTricksWon.value)}/13`
            : '',
      })}
      ${gameTable({
        north: {
          name: seatName(north),
          active: store.playerIds.value[north] === store.currentPlayerId.value,
          connected: store.playerConnected.value[north] ?? true,
          betInfo:
            store.playerBets.value[north] != null
              ? `Bet ${store.playerBets.value[north]} / Won ${store.playerTricksWon.value[north]}`
              : '',
          clockText: clockFor(north),
          low: lowFor(north),
          clockFrac: fracFor(north),
        },
        west: {
          name: seatName(west),
          active: store.playerIds.value[west] === store.currentPlayerId.value,
          connected: store.playerConnected.value[west] ?? true,
          betInfo:
            store.playerBets.value[west] != null
              ? `Bet ${store.playerBets.value[west]} / Won ${store.playerTricksWon.value[west]}`
              : '',
          clockText: clockFor(west),
          low: lowFor(west),
          clockFrac: fracFor(west),
        },
        east: {
          name: seatName(east),
          active: store.playerIds.value[east] === store.currentPlayerId.value,
          connected: store.playerConnected.value[east] ?? true,
          betInfo:
            store.playerBets.value[east] != null
              ? `Bet ${store.playerBets.value[east]} / Won ${store.playerTricksWon.value[east]}`
              : '',
          clockText: clockFor(east),
          low: lowFor(east),
          clockFrac: fracFor(east),
        },
        south: {
          name: seatName(i),
          active: store.playerIds.value[i] === store.currentPlayerId.value,
          connected: store.playerConnected.value[i] ?? true,
          betInfo:
            store.playerBets.value[i] != null
              ? `Bet ${store.playerBets.value[i]} / Won ${store.playerTricksWon.value[i]}`
              : '',
          clockText: clockFor(i),
          low: lowFor(i),
          clockFrac: fracFor(i),
        },
        centerText,
        refs,
      })}
      ${betButtons()} ${playAgain}
    `);
  };

  // Top-level effect: render the template
  const disposeRender = effect(() => {
    render(template(), root);
  });

  // After first render, set up the orchestrator with the refs
  const containers = {
    south: refs.hand.value!,
    north: refs.north.value!,
    west: refs.west.value!,
    east: refs.east.value!,
    trick: refs.trick.value!,
  };
  const orchestrator = new CardOrchestrator({ containers });
  args.resources.orchestrator = orchestrator;

  // Track previous tableCards to diff opponent card plays and trick collection.
  let lastTableCards: readonly (Card | null)[] = [null, null, null, null];

  // Side-effect effect: keep orchestrator in sync
  const disposeCards = effect(() => {
    const phase = store.phase.value;
    const hand = store.hand.value;
    const tableCards = store.tableCards.value;
    const currentPlayerId = store.currentPlayerId.value;
    const i = store.playerIds.value.indexOf(store.playerId.value);
    if (i < 0) return;
    if (phase !== 'BETTING' && phase !== 'PLAYING' && phase !== 'GAME_OVER') return;

    const tricksDone = store.playerTricksWon.value.reduce((a, b) => a + b, 0);

    if (!orchestrator.isInitialized() && hand.length > 0) {
      const curSeat = store.playerIds.value.indexOf(currentPlayerId ?? '');
      orchestrator.setupImmediate({
        playerHand: sortCards(hand),
        oppCounts: {
          north: oppCardCount(phase, tricksDone, tableCards, (i + 2) % 4),
          west: oppCardCount(phase, tricksDone, tableCards, (i + 3) % 4),
          east: oppCardCount(phase, tricksDone, tableCards, (i + 1) % 4),
        },
        tableCards,
        myIdx: i,
        northIdx: (i + 2) % 4,
        westIdx: (i + 3) % 4,
        eastIdx: (i + 1) % 4,
        currentPlayerSeatIdx: curSeat >= 0 ? curSeat : 0,
      });
      // Snapshot current table after init to avoid replaying pre-existing cards.
      lastTableCards = [...tableCards];
    }

    if (orchestrator.isInitialized()) {
      orchestrator.updatePlayerHand(sortCards(hand));
      orchestrator.updateOpponentCount(
        'north',
        oppCardCount(phase, tricksDone, tableCards, (i + 2) % 4),
      );
      orchestrator.updateOpponentCount(
        'west',
        oppCardCount(phase, tricksDone, tableCards, (i + 3) % 4),
      );
      orchestrator.updateOpponentCount(
        'east',
        oppCardCount(phase, tricksDone, tableCards, (i + 1) % 4),
      );

      // Trick-animation diff: detect opponent card plays and trick completion.
      const isEmpty = (tc: Card | null | undefined): boolean => !tc;

      const allEmptyNow = tableCards.every(isEmpty);
      const hadCardsBefore = lastTableCards.some((tc) => !isEmpty(tc));

      if (allEmptyNow && hadCardsBefore) {
        // Trick just completed. The server scores and clears the table within
        // the same event as the trick's last card, so a "4 cards on table"
        // snapshot never arrives. Hand the full trick to the orchestrator: it
        // backfills whatever its queue hasn't rendered yet, then collects.
        const completed = store.lastCompletedTrick.value ?? [];
        const plays: { card: Card; seat: RelativeSeat }[] = [];
        for (let slot = 0; slot < 4; slot++) {
          const card = completed[slot];
          if (card) plays.push({ card, seat: seatRel(slot, i) });
        }
        const winnerId = store.lastTrickWinnerId.value;
        let winnerSeat: RelativeSeat = 'south';
        if (winnerId) {
          const winnerAbs = store.playerIds.value.indexOf(winnerId);
          if (winnerAbs >= 0) winnerSeat = seatRel(winnerAbs, i);
        }
        orchestrator.completeTrick(plays, winnerSeat);
      } else {
        // Look for newly-added opponent cards.
        for (let slot = 0; slot < 4; slot++) {
          if (slot === i) continue; // south plays via playCardToCenter; skip
          const before = lastTableCards[slot];
          const now = tableCards[slot];
          if (isEmpty(before) && !isEmpty(now)) {
            const seat = seatRel(slot, i);
            if (seat !== 'south' && now) {
              orchestrator.playOpponentCardToCenter(now, seat);
            }
          }
        }
      }

      lastTableCards = [...tableCards];
    }

    const isMyTurn = currentPlayerId === store.playerId.value;
    if (phase === 'PLAYING' && isMyTurn) {
      const leadSuit = (() => {
        const currentSeat = store.playerIds.value.indexOf(currentPlayerId ?? '');
        let n = 0;
        for (const c of tableCards) if (c) n++;
        if (n === 0) return null;
        const leaderSeat = (((currentSeat - n) % 4) + 4) % 4;
        return tableCards[leaderSeat]?.suit ?? null;
      })();
      const validCards = sortCards(hand).filter((card: Card) =>
        isCardValid({
          hand,
          leadSuit,
          spadesBroken: store.spadesBroken.value,
          card,
          isMyTurn: true,
          phase: 'PLAYING',
        }),
      );
      orchestrator.enableInteraction(
        validCards,
        (card) => {
          // playCardToCenter first (it captures the pointer rect that
          // disableInteraction clears), then revoke input synchronously so
          // no second click can land. The server request runs inside the
          // queued play step, keeping every later step ordered after it.
          void orchestrator.playCardToCenter(card, async () => {
            try {
              await request(`/games/${args.gameId}/transition`, {
                method: 'POST',
                body: JSON.stringify({ type: 'card', card }),
              });
            } catch (e) {
              console.error('play failed', e);
              toast.error('Play failed.');
            }
          });
          orchestrator.disableInteraction();
        },
        // Re-evaluated when the queued enable actually runs: the freshest
        // store state decides whether input opens.
        () =>
          store.phase.value === 'PLAYING' && store.currentPlayerId.value === store.playerId.value,
      );
    } else {
      orchestrator.disableInteraction();
    }
  });

  const disposeClock = effect(() => {
    captureActiveClock(store.activePlayerClockMs.value);
  });
  startClockTicker();
  args.resources.cleanups.push(disposeRender);
  args.resources.cleanups.push(disposeCards);
  args.resources.cleanups.push(disposeClock);
  args.resources.cleanups.push(stopClockTicker);
}
