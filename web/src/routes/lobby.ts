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
  consumePendingJoin,
} from '../lib/storage';
import { navigateTo } from '../lib/util';
import { appShell } from '../ui/templates';
import { button } from '../ui/components/button';
import { icon } from '../ui/icon';
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
  const joiningTeam = signal<'A' | 'B' | null>(null);
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

  // Seats pair into teams: A/C vs B/D. The UI deals in teams; seats are an
  // implementation detail of the join call.
  const TEAMS = [
    { id: 'A' as const, no: '1' as const, seats: ['A', 'C'] as const },
    { id: 'B' as const, no: '2' as const, seats: ['B', 'D'] as const },
  ];

  const teamOccupants = (teamId: 'A' | 'B'): NonNullable<ChallengeSeat>[] => {
    const team = TEAMS.find((t) => t.id === teamId)!;
    return team.seats
      .map((s) => seats.value.find((seat) => seat !== null && seat.seat === s) ?? null)
      .filter((seat): seat is NonNullable<ChallengeSeat> => seat !== null);
  };

  const openTeamSeats = (teamId: 'A' | 'B'): ('A' | 'B' | 'C' | 'D')[] => {
    const team = TEAMS.find((t) => t.id === teamId)!;
    return team.seats.filter((s) => !seats.value.some((seat) => seat !== null && seat.seat === s));
  };

  const onJoinClick = (team: 'A' | 'B'): void => {
    joiningTeam.value = team;
    joinName.value = '';
  };

  const joinTeam = (team: 'A' | 'B', name: string): void => {
    // Seat resolves at join time: the team's first open seat may have changed
    // while the name modal was up.
    const seat = openTeamSeats(team)[0];
    if (!seat) {
      errorMsg.value = 'That team just filled up.';
      joiningTeam.value = null;
      return;
    }
    cleanupSse(); // close any prior join SSE
    const body = name.trim() ? { name: name.trim() } : {};

    joinSse = openSse(`/challenges/${args.challengeId}/join/${seat}`, body, {
      onEvent: (eventType, data) => {
        try {
          const parsed = JSON.parse(data) as Record<string, unknown>;
          if (eventType === 'joined') {
            myPlayerId.value = parsed.player_id as string;
            saveSession(args.shortId, args.challengeId, parsed.player_id as string);
            joiningTeam.value = null;
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

  const onJoinSubmit = (): void => {
    if (!joiningTeam.value) return;
    joinTeam(joiningTeam.value, joinName.value);
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

  // A pending intent from the create form: the creator's team choice can only
  // become a seat here, over this route's long-lived join SSE.
  const pending = consumePendingJoin(args.shortId);
  if (pending && !myPlayerId.value) {
    joinTeam(pending.team, pending.name);
  }

  const lobbyTemplate = (): TemplateResult => {
    // The first team with an opening is the default (Team A while it lasts).
    const defaultTeam = TEAMS.find((t) => openTeamSeats(t.id).length > 0)?.id ?? null;
    return appShell(html`
      <section class="lobby">
        <h2>Waiting for players</h2>
        ${errorMsg.value ? html`<p class="field-error">${errorMsg.value}</p>` : null}
        <div class="team-grid">
          ${TEAMS.map((t) => {
            const members = teamOccupants(t.id);
            const open = openTeamSeats(t.id);
            const joinable = !myPlayerId.value && open.length > 0;
            return html`<div class="team-card" data-team=${t.no}>
              <strong>Team ${t.id}</strong>
              <ul class="team-card__members">
                ${members.map(
                  (m) =>
                    html`<li class=${m.player_id === myPlayerId.value ? 'mine' : ''}>
                      ${m.name ?? 'Player'}
                    </li>`,
                )}
                ${open.map(() => html`<li class="team-card__open">Open</li>`)}
              </ul>
              ${joinable
                ? button({
                    label: `Join Team ${t.id}`,
                    onClick: () => onJoinClick(t.id),
                    variant: t.id === defaultTeam ? 'primary' : 'secondary',
                  })
                : null}
            </div>`;
          })}
        </div>

        ${joiningTeam.value
          ? html`<div class="join-modal">
              <label
                >Enter your name to join Team ${joiningTeam.value}:
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
                  joiningTeam.value = null;
                },
                variant: 'secondary',
              })}
            </div>`
          : null}

        <div class="share-link">
          <input
            type="text"
            readonly
            aria-label="Share link"
            .value=${`${location.origin}/play/${args.shortId}`}
          />
          <button
            type="button"
            class="btn btn--secondary btn--icon"
            aria-label=${copied.value ? 'Copied' : 'Copy link'}
            @click=${() => void copyShareLink()}
          >
            ${icon(copied.value ? 'checkbox-circle-fill' : 'file-copy-line')}
          </button>
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
  };

  const dispose = effect(() => {
    render(lobbyTemplate(), args.root);
  });
  args.resources.cleanups.push(dispose);
  args.resources.cleanups.push(cleanupSse);
}
