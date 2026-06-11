import { html, render } from 'lit-html';
import { request } from '../api/client';
import { openGameWs } from '../api/ws';
import type { GameStore, GameStateResponse, HandResponse, PresencePlayer } from '../state/game';
import { saveSession, clearSession } from '../lib/storage';
import { navigateTo } from '../lib/util';
import { toast } from '../state/toast';
import { appShell } from '../ui/templates';
import { makeRefs } from '../ui/components/game-table';
import type { RouteModule } from '../router';
import { type Resources, disposeResources } from './play-resources';
import { startAIGame, bootFromUrl } from './boot';
import { renderInGame } from './game-view';
import { renderLobby } from './lobby';

const POLL_INTERVAL = 2000;

async function pollOnce(store: GameStore, gameId: string, playerId: string): Promise<void> {
  try {
    const state = await request<GameStateResponse>(`/games/${gameId}`, { method: 'GET' });
    const hand = await request<HandResponse>(`/games/${gameId}/players/${playerId}/hand`, {
      method: 'GET',
    });
    store.applyState(state, hand);
    try {
      const presence = await request<{ players: PresencePlayer[] }>(`/games/${gameId}/presence`, {
        method: 'GET',
      });
      store.applyPresence(presence.players);
    } catch {
      // optional
    }
  } catch (e) {
    console.error('poll failed', e);
    toast.error('Failed to fetch game state.');
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
    render(
      appShell(html`
        <div class="skeleton-game" aria-busy="true" aria-label="Loading game">
          <div class="skeleton" style="grid-area: north; height: 24px; width: 120px;"></div>
          <div class="skeleton skeleton-card" style="grid-area: west;"></div>
          <div class="skeleton-row" style="grid-area: center; justify-content: center;">
            <div class="skeleton skeleton-card"></div>
            <div class="skeleton skeleton-card"></div>
            <div class="skeleton skeleton-card"></div>
            <div class="skeleton skeleton-card"></div>
          </div>
          <div class="skeleton skeleton-card" style="grid-area: east; justify-self: end;"></div>
          <div class="skeleton-row" style="grid-area: south; justify-content: center;">
            ${Array.from({ length: 13 }, () => html`<div class="skeleton skeleton-card"></div>`)}
          </div>
        </div>
      `),
      root,
    );

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
          toast.error('Failed to start AI game.');
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

          // Otherwise: state snapshot. Fetch hand and apply. The returned
          // promise is awaited by the WS event queue — events must apply in
          // order, or a slow hand fetch lets a stale snapshot win (the
          // "frozen game" bug: real-network jitter reorders concurrent
          // fetches; the last write was sometimes an old CPU-turn state).
          return (async () => {
            try {
              const hand = await request<HandResponse>(
                `/games/${gameId}/players/${playerId}/hand`,
                {
                  method: 'GET',
                },
              );
              store.applyState(data as GameStateResponse, hand);
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
