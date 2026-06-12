import { html, type TemplateResult } from 'lit-html';
import { icon } from '../icon';

export type ScoresProps = {
  teamAScore: number;
  teamBScore: number;
  teamABags: number;
  teamBBags: number;
  myTeam: 'A' | 'B';
  centerText: string;
};

/** One scoreboard chip in the seat-chip language, overlaid on the felt rail. */
export function scores(p: ScoresProps): TemplateResult {
  const team = (
    label: string,
    you: boolean,
    teamNo: 1 | 2,
    score: number,
    bags: number,
  ): TemplateResult =>
    html`<span class="spades-scoreboard__team" data-team=${teamNo}>
      <span class="spades-scoreboard__label">${label}${you ? ' (You)' : ''}</span>
      <span class="spades-scoreboard__nums"
        >${score} · ${icon('shopping-bag', { label: 'Bags' })} ${bags}</span
      >
    </span>`;
  return html`<section class="spades-scoreboard" aria-label="Scores">
    ${team('Team A', p.myTeam === 'A', 1, p.teamAScore, p.teamABags)}
    ${p.centerText ? html`<span class="spades-scoreboard__center">${p.centerText}</span>` : null}
    ${team('Team B', p.myTeam === 'B', 2, p.teamBScore, p.teamBBags)}
  </section>`;
}
