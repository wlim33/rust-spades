import { html, render } from 'lit-html';
import { request } from '../api/client';
import { openGameWs } from '../api/ws';
import type { GameStore, GameStateResponse, HandResponse, PresencePlayer } from '../state/game';
import { applyStateWithHand, createPollLoop } from '../state/game-sync';
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
/** ~20s of consecutive dead polls before the fallback gives up. */
const MAX_POLL_FAILURES = 10;

/**
 * Idle-watchdog window for the live socket. Comfortably above bot think time and
 * normal network gaps, so it only fires on a genuine silent stall (a half-open
 * socket while we await a peer move). A dev-only `?idlems=` override lets the
 * chaos e2e drive it fast.
 */
const IDLE_RECONNECT_MS =
  (import.meta.env.DEV ? Number(new URLSearchParams(location.search).get('idlems')) : 0) || 20_000;

/** One polling round. Throws on failure so the poll loop can count it. */
async function pollOnce(store: GameStore, gameId: string, playerId: string): Promise<void> {
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
}

export const play: RouteModule = {
  render: (params) => {
    const shortId = params['shortId'] ?? '';
    const root = document.getElementById('root');
    if (!root) return () => {};
    const resources: Resources = {
      cleanups: [],
      ws: null,
      poller: null,
      orchestrator: null,
    };

    // Pre-render a loading shell
    render(
      appShell(
        html`
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
        `,
        { fit: true },
      ),
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
        idleReconnectMs: IDLE_RECONNECT_MS,
        // Silence is only a stall while we're waiting on someone else to move.
        // On our own turn (or at game over) the server legitimately sends
        // nothing, so the watchdog must not churn the socket.
        expectingActivity: () =>
          (store.phase.value === 'BETTING' || store.phase.value === 'PLAYING') &&
          store.currentPlayerId.value !== store.playerId.value,
        onEvent: (data, isSnapshot) => {
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

          if (obj.event === 'resync') {
            // Server dropped us past its broadcast buffer; it follows this with
            // a Close that drives a clean reconnect + fresh snapshot. The
            // payload carries no game state, so just log and wait for that.
            console.warn('ws resync:', obj.reason);
            return;
          }

          // Only `state_changed` carries game state. chat_message (and any
          // future event types) must NOT reach applyState: they'd blank the
          // store, and chat even carries a seq that would advance the cursor
          // and drop the next real event.
          if (obj.event && obj.event !== 'state_changed') return;

          // State snapshot. Fetch hand and apply. The returned promise is
          // awaited by the WS event queue — events must apply in order, or a
          // slow hand fetch lets a stale snapshot win (the "frozen game" bug:
          // real-network jitter reorders concurrent fetches; the last write was
          // sometimes an old CPU-turn state).
          return applyStateWithHand(store, gameId, playerId, data as GameStateResponse, isSnapshot);
        },
        onClose: () => {
          // WS reconnects are exhausted — fall back to bounded polling.
          if (store.phase.value === 'GAME_OVER') return;
          resources.poller ??= createPollLoop({
            poll: () => pollOnce(store, gameId, playerId),
            isDone: () => store.phase.value === 'GAME_OVER',
            intervalMs: POLL_INTERVAL,
            maxConsecutiveFailures: MAX_POLL_FAILURES,
            onGiveUp: () => toast.error('Connection to the game lost. Reload the page to resume.'),
          });
          resources.poller.start();
        },
      });
    })();

    return () => disposeResources(resources);
  },
};
