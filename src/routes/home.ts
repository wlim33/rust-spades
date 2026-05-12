import { html, render } from 'lit-html';
import { appShell } from '../ui/templates';
import { button } from '../ui/components/button';
import type { RouteModule } from '../router';

type TimerCfg = { initial_time_secs: number; increment_secs: number } | null;

const QUICKPLAY_TIMERS: { label: string; value: TimerCfg }[] = [
  { label: '5+3', value: { initial_time_secs: 300, increment_secs: 3 } },
  { label: '10+5', value: { initial_time_secs: 600, increment_secs: 5 } },
  { label: '15+10', value: { initial_time_secs: 900, increment_secs: 10 } },
];

function onSeek(timer: TimerCfg): void {
  // Plan 2 wires this to the matchmaking SSE call.
  console.log('seek quickplay', timer);
}

function onFriends(): void {
  // Plan 2 navigates to a challenge-create view.
  console.log('play with friends');
}

function onComputers(): void {
  // Plan 2 wires this to POST /games with num_humans=1.
  console.log('play with computers');
}

function template() {
  return appShell(html`
    <h1>Spades</h1>
    <div class="menu" data-testid="home-menu">
      <p class="menu__label">Quick Play</p>
      <div class="menu__quickplay">
        ${QUICKPLAY_TIMERS.map((t) =>
          button({
            label: t.label,
            onClick: () => onSeek(t.value),
            variant: 'primary',
          }),
        )}
      </div>
      ${button({ label: 'Play with Friends', onClick: onFriends, variant: 'secondary' })}
      ${button({ label: 'Play with Computers', onClick: onComputers, variant: 'secondary' })}
    </div>
  `);
}

export const home: RouteModule = {
  render: () => {
    const root = document.getElementById('root');
    if (!root) return () => {};
    render(template(), root);
    return () => {
      // No subscriptions to dispose yet — render nothing on cleanup.
      render(html``, root);
    };
  },
};
