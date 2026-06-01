import { html, render, type TemplateResult } from 'lit-html';
import { signal, effect } from '@preact/signals-core';
import { request } from '../api/client';
import { openSse, type SseHandle } from '../api/sse';
import {
  saveSession,
  loadSession,
  clearSession,
  isChallengeCreator,
  clearChallengeCreator,
} from '../lib/storage';
import { navigateTo } from '../lib/util';
import { appShell } from '../ui/templates';
import { button } from '../ui/components/button';
import type { Resources } from './play-resources';
import type { ChallengeSeat, ChallengeStatus } from './boot';

export type LobbyArgs = {
  root: HTMLElement;
  resources: Resources;
  shortId: string;
  challengeId: string;
  initialStatus: ChallengeStatus;
};

export function renderLobby(args: LobbyArgs): void {
  const seats = signal<ChallengeSeat[]>(args.initialStatus.seats);
  const joiningSeat = signal<'A' | 'B' | 'C' | 'D' | null>(null);
  const joinName = signal('');
  const errorMsg = signal<string | null>(null);
  const copied = signal(false);
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
            clearChallengeCreator(args.shortId);
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
      await request(`/challenges/${args.challengeId}`, { method: 'DELETE' });
      cleanupSse();
      clearSession(args.shortId);
      clearChallengeCreator(args.shortId);
      navigateTo('/');
    } catch {
      errorMsg.value = 'Failed to cancel.';
    }
  };

  const copyShareLink = async (): Promise<void> => {
    const url = `${location.origin}/play/${args.shortId}`;
    try {
      await navigator.clipboard.writeText(url);
      copied.value = true;
      setTimeout(() => {
        copied.value = false;
      }, 1500);
    } catch {
      // ignore
    }
  };

  const isCreator = (): boolean => isChallengeCreator(args.shortId);

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
                data-team=${SEAT_TEAMS[s]}
              >
                <strong>Seat ${s}</strong>
                <span>Team ${SEAT_TEAMS[s]}</span>
                <span>${occupant.name ?? 'Player'}</span>
                ${occupant.player_id === myPlayerId.value ? html`<small>(You)</small>` : null}
              </div>`;
            }
            if (myPlayerId.value) {
              return html`<div class="seat-open" data-team=${SEAT_TEAMS[s]}>
                <strong>Seat ${s}</strong>
                <span>Team ${SEAT_TEAMS[s]}</span>
                <span>Open</span>
              </div>`;
            }
            return html`<button
              class="seat-open btn btn--primary"
              data-team=${SEAT_TEAMS[s]}
              @click=${() => onJoinClick(s)}
            >
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
          ${button({
            label: copied.value ? 'Copied!' : 'Copy',
            onClick: () => void copyShareLink(),
            variant: 'secondary',
          })}
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
