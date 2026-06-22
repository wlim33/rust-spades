/**
 * /replay/:id route
 *
 * Fetches a finished game's replay data, renders the table scaffold, wires
 * ReplayController ↔ ReplayBoard, and exposes playback controls + a side panel
 * with per-seat bids, tricks-won, running score, and round progress.
 *
 * Error states:
 *   403 → game still in progress (link to live game)
 *   404 → not found
 *   other → generic error with retry
 *
 * Cleanup: clears autoplay interval + board on route teardown.
 */

import '../replay/replay.css';

import { html, render, nothing } from 'lit-html';
import { effect, signal } from '@preact/signals-core';
import { appShell } from '../ui/templates';
import { fetchReplay } from '../api/hand-written';
import { ApiError } from '../api/client';
import { ReplayController } from '../replay/controller';
import { ReplayBoard } from '../replay/board';
import { makeRefs, gameTable } from '../ui/components/game-table';
import type { SeatProps } from '../ui/components/game-table';
import type { Containers } from '../cards/hand-manager';
import type { ViewState } from '../replay/controller';
import type { RouteModule } from '../router';

// ---------------------------------------------------------------------------
// Helpers

/** Format a bid value for display. nil bids (0 in the model) show as "nil". */
function fmtBid(v: number | null): string {
  if (v === null) return '—';
  if (v === 0) return 'nil';
  return String(v);
}

// Seat display order: north partner is top, south is bottom (viewer perspective)
const SEAT_LABELS: Record<string, string> = {
  south: 'South',
  north: 'North',
  east: 'East',
  west: 'West',
};

// ---------------------------------------------------------------------------
// Route module

export const replay: RouteModule = {
  render: (params) => {
    const id = params['id'] ?? '';
    const root = document.getElementById('root');
    if (!root) return () => {};

    // -----------------------------------------------------------------------
    // State signals
    const loading = signal(true);
    const errorKind = signal<'403' | '404' | 'generic' | null>(null);
    const errorMsg = signal<string>('');
    const viewState = signal<ViewState | null>(null);
    const isPlaying = signal(false);

    // Autoplay handle
    let autoInterval: ReturnType<typeof setInterval> | null = null;

    // Controller + Board live outside signals (mutable, not reactive)
    let controller: ReplayController | null = null;
    let board: ReplayBoard | null = null;

    // Track the previous ViewState for animated transitions
    let prevState: ViewState | null = null;

    // -----------------------------------------------------------------------
    // Control actions

    function seekStart(): void {
      if (!controller || !board) return;
      pauseAutoplay();
      prevState = null;
      controller.seekStart();
      void board.render(null, controller.state(), { animate: false });
      viewState.value = controller.state();
    }

    function seekEnd(): void {
      if (!controller || !board) return;
      pauseAutoplay();
      prevState = null;
      controller.seekEnd();
      void board.render(null, controller.state(), { animate: false });
      viewState.value = controller.state();
    }

    function stepPrev(): void {
      if (!controller || !board) return;
      pauseAutoplay();
      const before = controller.state();
      prevState = null;
      controller.prev();
      void board.render(null, controller.state(), { animate: false });
      viewState.value = controller.state();
      void before; // suppress unused warning — kept for symmetry
    }

    function stepNext(animate: boolean): void {
      if (!controller || !board) return;
      const before = controller.state();
      controller.next();
      const after = controller.state();
      void board.render(before, after, { animate });
      prevState = after;
      viewState.value = after;
    }

    function jumpRound(delta: number): void {
      if (!controller || !board) return;
      pauseAutoplay();
      const vs = viewState.value;
      const currentRound = vs?.round ?? 1;
      const totalRounds = vs?.totalRounds ?? 1;
      const target = Math.min(totalRounds, Math.max(1, currentRound + delta));
      prevState = null;
      controller.jumpRound(target);
      void board.render(null, controller.state(), { animate: false });
      viewState.value = controller.state();
    }

    function startAutoplay(): void {
      if (!controller || !board) return;
      if (controller.atEnd()) return;
      isPlaying.value = true;
      autoInterval = setInterval(() => {
        if (!controller || !board) {
          pauseAutoplay();
          return;
        }
        stepNext(/* animate */ true);
        if (controller.atEnd()) {
          pauseAutoplay();
        }
      }, 600);
    }

    function pauseAutoplay(): void {
      if (autoInterval !== null) {
        clearInterval(autoInterval);
        autoInterval = null;
      }
      isPlaying.value = false;
    }

    function toggleAutoplay(): void {
      if (isPlaying.value) {
        pauseAutoplay();
      } else {
        startAutoplay();
      }
    }

    // -----------------------------------------------------------------------
    // Seat props for gameTable (all static for replay — no clock, no active turn)
    const NOOP_SEAT: SeatProps = {
      name: '',
      active: false,
      connected: true,
      betInfo: '',
      clockText: null,
      low: false,
      clockFrac: null,
    };

    function seatProps(seat: 'north' | 'west' | 'east' | 'south'): SeatProps {
      const vs = viewState.value;
      const bid = vs?.bids[seat] ?? null;
      const tricks = vs?.tricksWon[seat] ?? 0;
      const label = SEAT_LABELS[seat] ?? seat;
      const betInfo = bid !== null
        ? `Bid ${fmtBid(bid)} / Won ${tricks}`
        : tricks > 0 ? `Won ${tricks}` : '';
      return {
        ...NOOP_SEAT,
        name: label,
        betInfo,
      };
    }

    // -----------------------------------------------------------------------
    // Render loop (reactive on viewState + isPlaying + loading)

    const dispose = effect(() => {
      const ldg = loading.value;
      const errKind = errorKind.value;
      const errMsg = errorMsg.value;
      const vs = viewState.value;
      const playing = isPlaying.value;

      if (ldg) {
        render(
          appShell(
            html`<div class="replay-loading" aria-busy="true" aria-label="Loading replay">
              <p>Loading replay…</p>
            </div>`,
          ),
          root,
        );
        return;
      }

      if (errKind !== null) {
        let body;
        if (errKind === '403') {
          body = html`
            <div class="replay-error">
              <p>This game is still in progress.</p>
              <p><a href="/play/${id}" data-link>Watch it live →</a></p>
              <p><a href="/" data-link>Back home</a></p>
            </div>`;
        } else if (errKind === '404') {
          body = html`
            <div class="replay-error">
              <h1>Not found</h1>
              <p>No replay found for this game.</p>
              <p><a href="/" data-link>Back home</a></p>
            </div>`;
        } else {
          body = html`
            <div class="replay-error">
              <p>Could not load replay: ${errMsg}</p>
              <p>
                <button
                  class="replay-btn"
                  @click=${() => { location.reload(); }}
                >Retry</button>
              </p>
              <p><a href="/" data-link>Back home</a></p>
            </div>`;
        }
        render(appShell(body), root);
        return;
      }

      if (vs === null || controller === null) return;

      const round = vs.round;
      const totalRounds = vs.totalRounds;
      const atStart = controller.atStart();
      const atEnd = controller.atEnd();
      const stepIdx = controller.stepIndex();
      const totalSteps = controller.totalSteps();

      // Side panel content
      const panel = html`
        <div class="replay-panel">
          ${vs.aborted ? html`<span class="replay-aborted">Aborted</span>` : nothing}

          <div class="replay-panel__section">
            <div class="replay-panel__heading">Round</div>
            <div class="replay-panel__row">
              <span class="replay-panel__value">${round} / ${totalRounds}</span>
            </div>
          </div>

          <div class="replay-panel__section">
            <div class="replay-panel__heading">Bids</div>
            ${(['north', 'south', 'east', 'west'] as const).map(
              (s) => html`
                <div class="replay-panel__row">
                  <span class="replay-panel__label">${SEAT_LABELS[s]}</span>
                  <span class="replay-panel__value">${fmtBid(vs.bids[s])}</span>
                </div>`,
            )}
          </div>

          <div class="replay-panel__section">
            <div class="replay-panel__heading">Tricks</div>
            ${(['north', 'south', 'east', 'west'] as const).map(
              (s) => html`
                <div class="replay-panel__row">
                  <span class="replay-panel__label">${SEAT_LABELS[s]}</span>
                  <span class="replay-panel__value">${vs.tricksWon[s]}</span>
                </div>`,
            )}
          </div>

          <div class="replay-panel__section">
            <div class="replay-panel__heading">Score</div>
            <div class="replay-panel__row">
              <span class="replay-panel__label">N/S</span>
              <span class="replay-score">${vs.score[0]}</span>
            </div>
            <div class="replay-panel__row">
              <span class="replay-panel__label">E/W</span>
              <span class="replay-score">${vs.score[1]}</span>
            </div>
          </div>
        </div>`;

      // Controls bar
      const controls = html`
        <div class="replay-controls" role="toolbar" aria-label="Replay controls">
          <button
            class="replay-btn"
            title="Seek to start"
            aria-label="Seek to start"
            ?disabled=${atStart}
            @click=${seekStart}
          >|&lt;</button>
          <button
            class="replay-btn"
            title="Previous step"
            aria-label="Previous step"
            ?disabled=${atStart}
            @click=${stepPrev}
          >&lt;</button>
          <button
            class="replay-btn replay-btn--play"
            title=${playing ? 'Pause' : 'Play'}
            aria-label=${playing ? 'Pause' : 'Play'}
            ?disabled=${atEnd && !playing}
            @click=${toggleAutoplay}
          >${playing ? '⏸' : '▶'}</button>
          <button
            class="replay-btn"
            title="Next step"
            aria-label="Next step"
            ?disabled=${atEnd}
            @click=${() => { pauseAutoplay(); stepNext(false); }}
          >&gt;</button>
          <button
            class="replay-btn"
            title="Seek to end"
            aria-label="Seek to end"
            ?disabled=${atEnd}
            @click=${seekEnd}
          >&gt;|</button>

          <div class="replay-round-nav" aria-label="Round navigation">
            <button
              class="replay-btn"
              title="Previous round"
              aria-label="Previous round"
              ?disabled=${round <= 1}
              @click=${() => jumpRound(-1)}
            >&#8249;</button>
            <span>Round ${round}/${totalRounds}</span>
            <button
              class="replay-btn"
              title="Next round"
              aria-label="Next round"
              ?disabled=${round >= totalRounds}
              @click=${() => jumpRound(1)}
            >&#8250;</button>
          </div>

          <span class="replay-progress" aria-label="Step ${stepIdx + 1} of ${totalSteps}">
            ${stepIdx + 1}/${totalSteps}
          </span>
        </div>`;

      // Full page: table + panel + controls
      // We use appShell with fit:true to get the 100dvh treatment since the
      // game table is a fixed-layout component.
      render(
        appShell(
          html`
            <div class="replay-page">
              <div class="replay-layout">
                <div class="replay-table-wrap">
                  ${gameTable({
                    refs: tableRefs,
                    north: seatProps('north'),
                    west: seatProps('west'),
                    east: seatProps('east'),
                    south: seatProps('south'),
                    centerText: vs.phase === 'bid'
                      ? `Bidding — Round ${round}`
                      : vs.phase === 'done'
                        ? 'Round over'
                        : '',
                  })}
                </div>
                ${panel}
              </div>
              ${controls}
            </div>`,
          { fit: true },
        ),
        root,
      );
    });

    // -----------------------------------------------------------------------
    // Bootstrap: fetch replay data then initialise controller + board

    // Create refs before the effect runs so `tableRefs` is stable across renders.
    const tableRefs = makeRefs();
    let containers: Containers | null = null;

    void (async () => {
      try {
        const res = await fetchReplay(id);
        // Build controller from replay data
        controller = new ReplayController(res);

        // Force initial render of the shell by setting viewState (loading still true)
        // — actually we need the DOM mounted first. Flip loading off; the effect
        // renders the table DOM, then we resolve the containers from the refs.
        loading.value = false;
        // viewState will be null here so effect early-returns. Kick a microtask
        // so the DOM is flushed by lit-html before we read ref.value.
        await Promise.resolve();

        // At this point the effect should have rendered the game table shell
        // (vs === null path returns early). We need the table to be rendered
        // once with the game table so refs populate. Render the game table shell
        // now by setting viewState to the initial state.
        const initialState = controller.state();
        prevState = null;

        // Resolve containers from refs
        containers = {
          south: tableRefs.hand.value!,
          north: tableRefs.north.value!,
          west: tableRefs.west.value!,
          east: tableRefs.east.value!,
          trick: tableRefs.trick.value!,
        };
        board = new ReplayBoard(containers);

        // Set state to trigger effect re-render, then draw initial board snap
        viewState.value = initialState;
        await Promise.resolve();
        void board.render(null, initialState, { animate: false });
      } catch (e) {
        if (e instanceof ApiError) {
          if (e.status === 403) {
            errorKind.value = '403';
          } else if (e.status === 404) {
            errorKind.value = '404';
          } else {
            errorKind.value = 'generic';
            errorMsg.value = e.message;
          }
        } else {
          errorKind.value = 'generic';
          errorMsg.value = e instanceof Error ? e.message : 'Unknown error';
        }
        loading.value = false;
      }
    })();

    // -----------------------------------------------------------------------
    // Cleanup

    return () => {
      pauseAutoplay();
      board?.clear();
      dispose();
      render(nothing, root);
    };
  },
};
