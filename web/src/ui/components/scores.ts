import { html, type TemplateResult } from 'lit-html';

export type ScoresProps = {
  teamAScore: number;
  teamBScore: number;
  teamABags: number;
  teamBBags: number;
  myTeam: 'A' | 'B';
  centerText: string;
};

export function scores(p: ScoresProps): TemplateResult {
  return html`<section class="spades-scores">
    <div class="spades-score-team">
      <strong>Team A${p.myTeam === 'A' ? ' (You)' : ''}</strong>
      <span>Score: ${p.teamAScore} | Bags: ${p.teamABags}</span>
    </div>
    <div class="spades-scores-center">${p.centerText}</div>
    <div class="spades-score-team">
      <strong>Team B${p.myTeam === 'B' ? ' (You)' : ''}</strong>
      <span>Score: ${p.teamBScore} | Bags: ${p.teamBBags}</span>
    </div>
  </section>`;
}
