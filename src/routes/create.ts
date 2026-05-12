import { html, render, type TemplateResult } from 'lit-html';
import { effect, signal } from '@preact/signals-core';
import { appShell } from '../ui/templates';
import { button } from '../ui/components/button';
import { openSse, type SseHandle } from '../api/sse';
import { navigateTo } from '../lib/util';
import { saveSession } from '../lib/storage';
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
            ${(['A', 'B', 'C', 'D'] as const).map((s) =>
              button({
                label: `Seat ${s}`,
                onClick: () => {
                  seat.value = seat.value === s ? null : s;
                },
                variant: seat.value === s ? 'primary' : 'secondary',
              }),
            )}
          </fieldset>
          <fieldset>
            <legend>Points</legend>
            ${([200, 300, 500] as const).map((p) =>
              button({
                label: String(p),
                onClick: () => {
                  points.value = p;
                },
                variant: points.value === p ? 'primary' : 'secondary',
              }),
            )}
          </fieldset>
          <fieldset>
            <legend>Timer</legend>
            ${TIMER_PRESETS.map((t, i) =>
              button({
                label: t.label,
                onClick: () => {
                  timerIdx.value = i;
                },
                variant: timerIdx.value === i ? 'primary' : 'secondary',
              }),
            )}
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
      root.innerHTML = '';
    };
  },
};
