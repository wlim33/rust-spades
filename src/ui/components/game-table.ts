import { html, type TemplateResult } from 'lit-html';
import { createRef, ref, type Ref } from 'lit-html/directives/ref.js';

export type SeatProps = {
  name: string;
  active: boolean;
  connected: boolean;
  betInfo: string;
  clockText: string | null;
};

export type GameTableRefs = {
  hand: Ref<HTMLDivElement>;
  north: Ref<HTMLDivElement>;
  west: Ref<HTMLDivElement>;
  east: Ref<HTMLDivElement>;
  trick: Ref<HTMLDivElement>;
};

export function makeRefs(): GameTableRefs {
  return {
    hand: createRef<HTMLDivElement>(),
    north: createRef<HTMLDivElement>(),
    west: createRef<HTMLDivElement>(),
    east: createRef<HTMLDivElement>(),
    trick: createRef<HTMLDivElement>(),
  };
}

export function gameTable(args: {
  north: SeatProps;
  west: SeatProps;
  east: SeatProps;
  south: SeatProps;
  centerText: string;
  refs: GameTableRefs;
}): TemplateResult {
  const seat = (cls: string, p: SeatProps, refEl: Ref<HTMLDivElement>): TemplateResult =>
    html`<div class=${`spades-seat ${cls}${p.active ? ' active' : ''}`}>
      <span class="spades-seat-label">${p.connected ? '● ' : '○ '}${p.name}</span>
      ${p.clockText ? html`<span class="spades-clock">${p.clockText}</span>` : null}
      <span class="spades-seat-info">${p.betInfo}</span>
      <div class="card-container opp-container" ${ref(refEl)}></div>
    </div>`;

  return html`<div class="spades-table">
    ${seat('seat-north', args.north, args.refs.north)}
    ${seat('seat-west', args.west, args.refs.west)}
    <div class="spades-table-center">
      <div class="spades-trick-area">
        <div class="card-container trick-container" ${ref(args.refs.trick)}></div>
      </div>
      <span class="spades-center-text">${args.centerText}</span>
    </div>
    ${seat('seat-east', args.east, args.refs.east)}
    <div class="spades-seat seat-south${args.south.active ? ' active' : ''}">
      <span class="spades-seat-label">${args.south.connected ? '● ' : '○ '}${args.south.name}</span>
      ${args.south.clockText
        ? html`<span class="spades-clock">${args.south.clockText}</span>`
        : null}
      <span class="spades-seat-info">${args.south.betInfo}</span>
      <div class="card-container hand-container" ${ref(args.refs.hand)}></div>
    </div>
  </div>`;
}
