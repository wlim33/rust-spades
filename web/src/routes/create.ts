import { html, nothing, render, type TemplateResult } from 'lit-html';
import { effect, signal } from '@preact/signals-core';
import { appShell } from '../ui/templates';
import { button } from '../ui/components/button';
import { openSse, type SseHandle } from '../api/sse';
import { navigateTo } from '../lib/util';
import { saveSession, markChallengeCreator } from '../lib/storage';
import type { RouteModule } from '../router';

type TimerCfg = { initial_time_secs: number; increment_secs: number } | null;
const TIMER_PRESETS: { label: string; value: TimerCfg }[] = [
  { label: 'None', value: null },
  { label: '5+3', value: { initial_time_secs: 300, increment_secs: 3 } },
  { label: '10+5', value: { initial_time_secs: 600, increment_secs: 5 } },
  { label: '15+10', value: { initial_time_secs: 900, increment_secs: 10 } },
];

type Seat = 'A' | 'B' | 'C' | 'D';

export const create: RouteModule = {
  render: () => {
    const root = document.getElementById('root');
    if (!root) return () => {};

    const name = signal('');
    const seat = signal<Seat | null>(null);
    const points = signal<200 | 300 | 500>(500);
    const timerIdx = signal(0);
    const errorMsg = signal<string | null>(null);
    const submitting = signal(false);
    let sse: SseHandle | null = null;

    const onSubmit = (): void => {
      if (submitting.value) return;
      submitting.value = true;
      errorMsg.value = null;

      sse = openSse(
        '/challenges',
        {
          max_points: points.value,
          creator_name: name.value || undefined,
          creator_seat: seat.value ?? undefined,
          timer_config: TIMER_PRESETS[timerIdx.value]!.value ?? undefined,
        },
        {
          onEvent: (eventType, data) => {
            try {
              const parsed = JSON.parse(data) as {
                challenge_id: string;
                short_id: string;
                creator_player_id?: string;
              };
              if (eventType === 'challenge_created') {
                if (parsed.creator_player_id) {
                  saveSession(parsed.short_id, parsed.challenge_id, parsed.creator_player_id);
                  markChallengeCreator(parsed.short_id);
                }
                sse?.close();
                sse = null;
                navigateTo(`/play/${parsed.short_id}`);
              } else if (eventType === 'cancelled') {
                errorMsg.value = 'Challenge cancelled.';
                submitting.value = false;
                sse?.close();
                sse = null;
              }
            } catch {
              // ignore parse errors
            }
          },
          onError: () => {
            errorMsg.value = 'Failed to create challenge.';
            submitting.value = false;
            sse?.close();
            sse = null;
          },
        },
      );
    };

    const template = (): TemplateResult =>
      appShell(html`
        <section class="form-page">
          <h2>Create Challenge</h2>
          ${errorMsg.value ? html`<p class="field-error">${errorMsg.value}</p>` : null}
          <label
            >Your name
            <input
              type="text"
              maxlength="20"
              .value=${name.value}
              @input=${(e: Event) => {
                name.value = (e.target as HTMLInputElement).value;
              }}
            />
          </label>
          <fieldset>
            <legend>Pick seat</legend>
            <div class="seg" role="group" aria-label="Pick seat">
              ${(['A', 'B', 'C', 'D'] as const).map(
                (s) =>
                  html`<button
                    type="button"
                    aria-pressed=${seat.value === s}
                    @click=${() => {
                      seat.value = seat.value === s ? null : s;
                    }}
                  >
                    ${s}
                  </button>`,
              )}
            </div>
          </fieldset>
          <fieldset>
            <legend>Points</legend>
            <div class="seg" role="group" aria-label="Points">
              ${([200, 300, 500] as const).map(
                (p) =>
                  html`<button
                    type="button"
                    aria-pressed=${points.value === p}
                    @click=${() => {
                      points.value = p;
                    }}
                  >
                    ${p}
                  </button>`,
              )}
            </div>
          </fieldset>
          <fieldset>
            <legend>Timer</legend>
            <div class="seg" role="group" aria-label="Timer">
              ${TIMER_PRESETS.map(
                (t, i) =>
                  html`<button
                    type="button"
                    aria-pressed=${timerIdx.value === i}
                    @click=${() => {
                      timerIdx.value = i;
                    }}
                  >
                    ${t.label}
                  </button>`,
              )}
            </div>
          </fieldset>
          ${button({
            label: submitting.value ? 'Creating…' : 'Create',
            onClick: onSubmit,
            variant: 'primary',
            disabled: submitting.value,
          })}
          ${button({
            label: 'Back',
            onClick: () => navigateTo('/'),
            variant: 'secondary',
          })}
        </section>
      `);

    const dispose = effect(() => {
      render(template(), root);
    });
    return () => {
      sse?.close();
      dispose();
      render(nothing, root);
    };
  },
};
