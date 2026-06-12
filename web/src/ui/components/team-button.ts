import { html, type TemplateResult } from 'lit-html';
import { icon } from '../icon';

export type TeamMember = { name: string; mine: boolean };

/**
 * One button per team: the background gauge (CSS ::before keyed off
 * data-fill) carries occupancy, icon+name rows carry identity. Disabled when
 * the viewer can't join (already seated, or team full) but keeps rendering —
 * the gauge still rises as others join.
 */
export function teamButton(opts: {
  teamNo: '1' | '2';
  label: string;
  members: TeamMember[];
  capacity: number;
  joinable: boolean;
  onJoin: () => void;
}): TemplateResult {
  const filled = opts.members.length;
  const seats = `${filled} of ${opts.capacity} seats filled`;
  const aria = opts.joinable ? `Join ${opts.label}, ${seats}` : `${opts.label}, ${seats}`;
  const openSlots = Math.max(0, opts.capacity - filled);
  return html`<button
    type="button"
    class="team-btn"
    data-team=${opts.teamNo}
    data-fill=${filled}
    ?disabled=${!opts.joinable}
    aria-label=${aria}
    @click=${opts.onJoin}
  >
    <span class="team-btn__label">${opts.label}</span>
    ${opts.members.map(
      (m) =>
        html`<span class="team-btn__slot${m.mine ? ' team-btn__slot--mine' : ''}">
          ${icon('user-fill')} ${m.name}
        </span>`,
    )}
    ${Array.from(
      { length: openSlots },
      () =>
        html`<span class="team-btn__slot team-btn__slot--open"> ${icon('user-line')} Open </span>`,
    )}
  </button>`;
}
