import { html, render, type TemplateResult } from 'lit-html';
import { effect, signal } from '@preact/signals-core';
import { request } from '../api/client';
import { openSse, type SseHandle } from '../api/sse';
import { openGameWs, type WsHandle } from '../api/ws';
import { createGameStore, type GameStore } from '../state/game';
import { sortCards, isCardValid, oppCardCount, type Card } from '../state/helpers';
import { saveSession, loadSession, clearSession } from '../lib/storage';
import { navigateTo } from '../lib/util';
import { CardOrchestrator } from '../cards/orchestrator';
import { appShell } from '../ui/templates';
import { button } from '../ui/components/button';
import { scores } from '../ui/components/scores';
import { gameTable, makeRefs, type GameTableRefs } from '../ui/components/game-table';
import type { RouteModule } from '../router';

const POLL_INTERVAL = 2000;

type Resources = {
  cleanups: Array<() => void>;
  ws: WsHandle | null;
  pollTimer: ReturnType<typeof setInterval> | null;
  orchestrator: CardOrchestrator | null;
};

type ChallengeSeat = { seat: 'A' | 'B' | 'C' | 'D'; player_id: string; name: string | null } | null;

type ChallengeStatus = {
  challenge_id: string;
  max_points: number;
  seats: ChallengeSeat[];
  status: 'open' | 'started' | 'cancelled' | 'expired';
  expires_at_epoch_secs: number;
};

type BootResult =
  | { kind: 'game'; store: GameStore; gameId: string; playerId: string }
  | { kind: 'lobby'; challengeId: string; shortId: string; status: ChallengeStatus }
  | { kind: 'error'; message: string };

function disposeResources(r: Resources): void {
  r.ws?.close();
  r.ws = null;
  if (r.pollTimer) clearInterval(r.pollTimer);
  r.pollTimer = null;
  r.orchestrator?.destroy();
  r.orchestrator = null;
  for (const c of r.cleanups) c();
  r.cleanups = [];
}

async function startAIGame(): Promise<{ gameId: string; playerId: string; shortId: string }> {
  const created = await request<{ game_id: string; player_ids: string[] }>('/games', {
    method: 'POST',
    body: JSON.stringify({ max_points: 500, num_humans: 1 }),
  });
  const state = await request<{ short_id?: string | null }>(`/games/${created.game_id}`, {
    method: 'GET',
  });
  const shortId = state.short_id ?? created.game_id;
  return { gameId: created.game_id, playerId: created.player_ids[0]!, shortId };
}

async function bootFromUrl(shortId: string): Promise<BootResult> {
  // 1. localStorage
  const saved = loadSession(shortId);
  if (saved) {
    try {
      const state = await request<never>(`/games/${saved.gid}`, { method: 'GET' });
      const hand = await request<never>(`/games/${saved.gid}/players/${saved.pid}/hand`, {
        method: 'GET',
      });
      const store = createGameStore(saved.pid);
      store.applyState(state, hand);
      try {
        const presence = await request<{
          players: { player_id: string; connected: boolean }[];
        }>(`/games/${saved.gid}/presence`, { method: 'GET' });
        store.applyPresence(presence.players);
      } catch {
        // optional
      }
      return { kind: 'game', store, gameId: saved.gid, playerId: saved.pid };
    } catch {
      clearSession(shortId);
    }
  }

  // 2. by-player-url
  try {
    const resp = await request<{
      game_id: string;
      player_short_id?: string;
      player_id: string;
      game: never;
      hand: never;
    }>(`/games/by-player-url/${shortId}`, { method: 'GET' });
    const playerId = resp.player_short_id ?? resp.player_id;
    const store = createGameStore(playerId);
    store.applyState(resp.game, resp.hand);
    saveSession(shortId, resp.game_id, playerId);
    return { kind: 'game', store, gameId: resp.game_id, playerId };
  } catch {
    // fall through
  }

  // 3. by-short-id (challenge)
  try {
    const status = await request<ChallengeStatus>(`/challenges/by-short-id/${shortId}`, {
      method: 'GET',
    });
    if (status.status === 'open') {
      return { kind: 'lobby', challengeId: status.challenge_id, shortId, status };
    }
    if (status.status === 'started')
      return { kind: 'error', message: 'This game has already started.' };
    return { kind: 'error', message: 'This challenge is no longer available.' };
  } catch {
    return { kind: 'error', message: 'Game or challenge not found.' };
  }
}

function renderInGame(args: {
  root: HTMLElement;
  store: GameStore;
  gameId: string;
  shortId: string;
  resources: Resources;
  refs: GameTableRefs;
}): void {
  const { store, refs, root } = args;

  const myIdx = (): number => store.playerIds.value.indexOf(store.playerId.value);
  const seatName = (idx: number): string => store.playerNames.value[idx] ?? `Seat ${idx + 1}`;

  const template = (): TemplateResult => {
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
        }
      };
      return html`<div class="spades-bets">
        ${Array.from({ length: 14 }, (_, n) =>
          button({
            label: String(n),
            onClick: () => void onBet(n),
            variant: 'primary',
          }),
        )}
      </div>`;
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
            ? `Trick ${
                typeof store.gameState.value === 'object' &&
                store.gameState.value &&
                'Trick' in store.gameState.value
                  ? (store.gameState.value as { Trick: number }).Trick
                  : 0
              }/13`
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
          clockText: null,
        },
        west: {
          name: seatName(west),
          active: store.playerIds.value[west] === store.currentPlayerId.value,
          connected: store.playerConnected.value[west] ?? true,
          betInfo:
            store.playerBets.value[west] != null
              ? `Bet ${store.playerBets.value[west]} / Won ${store.playerTricksWon.value[west]}`
              : '',
          clockText: null,
        },
        east: {
          name: seatName(east),
          active: store.playerIds.value[east] === store.currentPlayerId.value,
          connected: store.playerConnected.value[east] ?? true,
          betInfo:
            store.playerBets.value[east] != null
              ? `Bet ${store.playerBets.value[east]} / Won ${store.playerTricksWon.value[east]}`
              : '',
          clockText: null,
        },
        south: {
          name: seatName(i),
          active: store.playerIds.value[i] === store.currentPlayerId.value,
          connected: store.playerConnected.value[i] ?? true,
          betInfo:
            store.playerBets.value[i] != null
              ? `Bet ${store.playerBets.value[i]} / Won ${store.playerTricksWon.value[i]}`
              : '',
          clockText: null,
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

  // Side-effect effect: keep orchestrator in sync
  const disposeCards = effect(() => {
    const phase = store.phase.value;
    const hand = store.hand.value;
    const tableCards = store.tableCards.value;
    const currentPlayerId = store.currentPlayerId.value;
    const i = store.playerIds.value.indexOf(store.playerId.value);
    if (i < 0) return;
    if (phase !== 'BETTING' && phase !== 'PLAYING' && phase !== 'GAME_OVER') return;

    if (!orchestrator.isInitialized() && hand.length > 0) {
      orchestrator.setupImmediate({
        playerHand: sortCards(hand),
        oppCounts: {
          north: oppCardCount(phase, store.gameState.value, tableCards, (i + 2) % 4),
          west: oppCardCount(phase, store.gameState.value, tableCards, (i + 3) % 4),
          east: oppCardCount(phase, store.gameState.value, tableCards, (i + 1) % 4),
        },
        tableCards,
        myIdx: i,
        northIdx: (i + 2) % 4,
        westIdx: (i + 3) % 4,
        eastIdx: (i + 1) % 4,
        currentPlayerSeatIdx:
          store.playerIds.value.indexOf(currentPlayerId ?? '') >= 0
            ? store.playerIds.value.indexOf(currentPlayerId ?? '')
            : 0,
      });
    }

    if (orchestrator.isInitialized()) {
      orchestrator.updatePlayerHand(sortCards(hand));
      orchestrator.updateOpponentCount(
        'north',
        oppCardCount(phase, store.gameState.value, tableCards, (i + 2) % 4),
      );
      orchestrator.updateOpponentCount(
        'west',
        oppCardCount(phase, store.gameState.value, tableCards, (i + 3) % 4),
      );
      orchestrator.updateOpponentCount(
        'east',
        oppCardCount(phase, store.gameState.value, tableCards, (i + 1) % 4),
      );
    }

    const isMyTurn = currentPlayerId === store.playerId.value;
    if (phase === 'PLAYING' && isMyTurn) {
      const leadSuit = (() => {
        const currentSeat = store.playerIds.value.indexOf(currentPlayerId ?? '');
        let n = 0;
        for (const c of tableCards) if (c && (c as { suit?: string }).suit !== 'Blank') n++;
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
      orchestrator.enableInteraction(validCards, (card) => {
        void (async () => {
          orchestrator.disableInteraction();
          await orchestrator.playCardToCenter(card);
          try {
            await request(`/games/${args.gameId}/transition`, {
              method: 'POST',
              body: JSON.stringify({ type: 'card', card }),
            });
          } catch (e) {
            console.error('play failed', e);
          }
        })();
      });
    } else {
      orchestrator.disableInteraction();
    }
  });

  args.resources.cleanups.push(disposeRender);
  args.resources.cleanups.push(disposeCards);
}

type LobbyArgs = {
  root: HTMLElement;
  resources: Resources;
  shortId: string;
  challengeId: string;
  initialStatus: ChallengeStatus;
};

function renderLobby(args: LobbyArgs): void {
  const seats = signal<ChallengeSeat[]>(args.initialStatus.seats);
  const joiningSeat = signal<'A' | 'B' | 'C' | 'D' | null>(null);
  const joinName = signal('');
  const errorMsg = signal<string | null>(null);
  const saved = loadSession(args.shortId);
  const myPlayerId = signal<string | null>(saved?.pid ?? null);

  let joinSse: SseHandle | null = null;

  const cleanupSse = (): void => {
    joinSse?.close();
    joinSse = null;
  };

  const onJoinClick = (seat: 'A' | 'B' | 'C' | 'D'): void => {
    joiningSeat.value = seat;
    joinName.value = '';
  };

  const onJoinSubmit = (): void => {
    if (!joiningSeat.value) return;
    cleanupSse(); // close any prior join SSE
    const seat = joiningSeat.value;
    const body = joinName.value.trim() ? { name: joinName.value.trim() } : {};

    joinSse = openSse(`/challenges/${args.challengeId}/join/${seat}`, body, {
      onEvent: (eventType, data) => {
        try {
          const parsed = JSON.parse(data) as Record<string, unknown>;
          if (eventType === 'joined') {
            myPlayerId.value = parsed.player_id as string;
            saveSession(args.shortId, args.challengeId, parsed.player_id as string);
            joiningSeat.value = null;
          } else if (eventType === 'seat_update') {
            seats.value = parsed.seats as ChallengeSeat[];
          } else if (eventType === 'game_start') {
            const playerId =
              (parsed.player_short_id as string | undefined) ??
              (parsed.player_id as string | undefined) ??
              '';
            saveSession(args.shortId, parsed.game_id as string, playerId);
            cleanupSse();
            navigateTo(`/play/${args.shortId}`);
          } else if (eventType === 'cancelled') {
            errorMsg.value = 'Challenge was cancelled.';
            cleanupSse();
            setTimeout(() => navigateTo('/'), 1500);
          }
        } catch {
          // ignore
        }
      },
      onError: () => {
        errorMsg.value = 'Connection lost.';
        cleanupSse();
      },
    });
  };

  const onCancel = async (): Promise<void> => {
    if (!myPlayerId.value) return;
    try {
      await request(`/challenges/${args.challengeId}`, {
        method: 'DELETE',
        body: JSON.stringify({ creator_id: myPlayerId.value }),
      });
      cleanupSse();
      clearSession(args.shortId);
      navigateTo('/');
    } catch {
      errorMsg.value = 'Failed to cancel.';
    }
  };

  const copyShareLink = async (): Promise<void> => {
    const url = `${location.origin}/play/${args.shortId}`;
    try {
      await navigator.clipboard.writeText(url);
    } catch {
      // ignore
    }
  };

  const isCreator = (): boolean => {
    if (!myPlayerId.value) return false;
    return seats.value.some((s) => s !== null && s.player_id === myPlayerId.value);
  };

  const SEAT_TEAMS: Record<'A' | 'B' | 'C' | 'D', '1' | '2'> = {
    A: '1',
    B: '2',
    C: '1',
    D: '2',
  };

  const lobbyTemplate = (): TemplateResult =>
    appShell(html`
      <section class="lobby">
        <h2>Waiting for players</h2>
        ${errorMsg.value ? html`<p class="field-error">${errorMsg.value}</p>` : null}
        <div class="seat-grid">
          ${(['A', 'B', 'C', 'D'] as const).map((s) => {
            const occupant = seats.value.find((seat) => seat !== null && seat.seat === s) ?? null;
            if (occupant) {
              return html`<div
                class="seat-taken ${occupant.player_id === myPlayerId.value ? 'mine' : ''}"
              >
                <strong>Seat ${s}</strong>
                <span>Team ${SEAT_TEAMS[s]}</span>
                <span>${occupant.name ?? 'Player'}</span>
                ${occupant.player_id === myPlayerId.value ? html`<small>(You)</small>` : null}
              </div>`;
            }
            if (myPlayerId.value) {
              return html`<div class="seat-open">
                <strong>Seat ${s}</strong>
                <span>Team ${SEAT_TEAMS[s]}</span>
                <span>Open</span>
              </div>`;
            }
            return html`<button class="seat-open btn btn--primary" @click=${() => onJoinClick(s)}>
              <strong>Seat ${s}</strong>
              <span>Team ${SEAT_TEAMS[s]}</span>
            </button>`;
          })}
        </div>

        ${joiningSeat.value
          ? html`<div class="join-modal">
              <label
                >Enter your name to join Seat ${joiningSeat.value}:
                <input
                  type="text"
                  maxlength="20"
                  .value=${joinName.value}
                  @input=${(e: Event) => {
                    joinName.value = (e.target as HTMLInputElement).value;
                  }}
                />
              </label>
              ${button({ label: 'Join', onClick: onJoinSubmit, variant: 'primary' })}
              ${button({
                label: 'Cancel',
                onClick: () => {
                  joiningSeat.value = null;
                },
                variant: 'secondary',
              })}
            </div>`
          : null}

        <div class="share-link">
          <label
            >Share this link:
            <input type="text" readonly .value=${`${location.origin}/play/${args.shortId}`} />
          </label>
          ${button({ label: 'Copy', onClick: () => void copyShareLink(), variant: 'secondary' })}
        </div>

        ${isCreator()
          ? button({
              label: 'Cancel Challenge',
              onClick: () => void onCancel(),
              variant: 'danger',
            })
          : null}
      </section>
    `);

  const dispose = effect(() => {
    render(lobbyTemplate(), args.root);
  });
  args.resources.cleanups.push(dispose);
  args.resources.cleanups.push(cleanupSse);
}

async function pollOnce(store: GameStore, gameId: string, playerId: string): Promise<void> {
  try {
    const state = await request<never>(`/games/${gameId}`, { method: 'GET' });
    const hand = await request<never>(`/games/${gameId}/players/${playerId}/hand`, {
      method: 'GET',
    });
    store.applyState(state, hand);
    try {
      const presence = await request<{ players: never[] }>(`/games/${gameId}/presence`, {
        method: 'GET',
      });
      store.applyPresence(presence.players);
    } catch {
      // optional
    }
  } catch (e) {
    console.error('poll failed', e);
  }
}

export const play: RouteModule = {
  render: (params) => {
    const shortId = params['shortId'] ?? '';
    const root = document.getElementById('root');
    if (!root) return () => {};
    const resources: Resources = {
      cleanups: [],
      ws: null,
      pollTimer: null,
      orchestrator: null,
    };

    // Pre-render a loading shell
    render(appShell(html`<p>Loading game…</p>`), root);

    void (async () => {
      // Special case: "/play/new-ai" boots an AI game synthetically.
      if (shortId === 'new-ai') {
        try {
          const ai = await startAIGame();
          saveSession(ai.shortId, ai.gameId, ai.playerId);
          navigateTo(`/play/${ai.shortId}`);
          return;
        } catch (e) {
          render(appShell(html`<p>Failed to start AI game.</p>`), root);
          console.error('startAIGame failed', e);
          return;
        }
      }

      const result = await bootFromUrl(shortId);
      if (result.kind === 'error') {
        render(
          appShell(
            html`<p>${result.message}</p>
              <p><a href="/" data-link>Back home</a></p>`,
          ),
          root,
        );
        return;
      }
      if (result.kind === 'lobby') {
        renderLobby({
          root,
          resources,
          shortId: result.shortId,
          challengeId: result.challengeId,
          initialStatus: result.status,
        });
        return;
      }
      const { store, gameId, playerId } = result;

      const refs = makeRefs();
      renderInGame({ root, store, gameId, shortId, resources, refs });

      // Open WS; on close, fall back to polling
      resources.ws = openGameWs(gameId, playerId, {
        onEvent: (data) => {
          const obj = data as {
            event?: string;
            reason?: string;
            players?: { player_id: string; connected: boolean }[];
          } & Record<string, unknown>;

          if (obj.event === 'presence_changed' && obj.players) {
            store.applyPresence(obj.players);
            return;
          }

          if (obj.event === 'game_aborted') {
            console.warn('game aborted:', obj.reason);
            resources.orchestrator?.clearAll();
            clearSession(shortId);
            navigateTo('/');
            return;
          }

          // Otherwise: state snapshot. Fetch hand and apply.
          void (async () => {
            try {
              const hand = await request<never>(`/games/${gameId}/players/${playerId}/hand`, {
                method: 'GET',
              });
              store.applyState(data as never, hand);
            } catch {
              // ignore
            }
          })();
        },
        onClose: () => {
          if (store.phase.value !== 'GAME_OVER') {
            const tick = async (): Promise<void> => {
              await pollOnce(store, gameId, playerId);
              if (store.phase.value === 'GAME_OVER' && resources.pollTimer) {
                clearInterval(resources.pollTimer);
                resources.pollTimer = null;
              }
            };
            resources.pollTimer = setInterval(() => void tick(), POLL_INTERVAL);
          }
        },
      });
    })();

    return () => disposeResources(resources);
  },
};
