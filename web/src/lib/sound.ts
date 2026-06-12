import { getSoundPref } from './storage';

let ctx: AudioContext | null = null;

function getCtx(): AudioContext | null {
  try {
    ctx ??= new AudioContext();
    if (ctx.state === 'suspended') void ctx.resume();
    return ctx;
  } catch {
    return null;
  }
}

/**
 * Pref + autoplay gate shared by every cue. A suspended context (autoplay
 * policy) would queue notes and replay them garbled after the first user
 * gesture — skip instead. getCtx() has already requested resume(), so the
 * next cue after a gesture plays.
 */
function readyCtx(): AudioContext | null {
  if (!getSoundPref()) return null;
  const ac = getCtx();
  if (!ac || ac.state !== 'running') return null;
  return ac;
}

/** One soft sine note `at` seconds from now: fast attack, ~0.3 s decay. */
function playNote(ac: AudioContext, freq: number, at: number): void {
  const t0 = ac.currentTime + at;
  const osc = ac.createOscillator();
  const gain = ac.createGain();
  osc.type = 'sine';
  osc.frequency.value = freq;
  gain.gain.setValueAtTime(0, t0);
  gain.gain.linearRampToValueAtTime(0.08, t0 + 0.02);
  gain.gain.exponentialRampToValueAtTime(0.0001, t0 + 0.3);
  osc.connect(gain).connect(ac.destination);
  osc.start(t0);
  osc.stop(t0 + 0.32);
}

/**
 * Soft two-note turn chime (E5 -> A5). Best-effort: any failure — including
 * autoplay policy before the first user gesture — is silently ignored.
 */
export function chime(): void {
  const ac = readyCtx();
  if (!ac) return;
  try {
    playNote(ac, 659.25, 0);
    playNote(ac, 880, 0.12);
  } catch {
    // audio is an enhancement, never an error
  }
}

/* Lobby fill ladder: A-major arpeggio, one step per filled seat. The 4th
   seat lands on the octave, so a full lobby resolves just as the game
   becomes startable. */
const SEAT_PITCH = [440, 554.37, 659.25, 880] as const;

/** One rising tick per seat filled in the lobby (1-indexed total count). */
export function seatTick(filledSeats: 1 | 2 | 3 | 4): void {
  const ac = readyCtx();
  if (!ac) return;
  try {
    playNote(ac, SEAT_PITCH[filledSeats - 1], 0);
  } catch {
    // audio is an enhancement, never an error
  }
}

/** Three-note rising flourish (E5 -> A5 -> C#6): the game-launch cue. */
export function gameStart(): void {
  const ac = readyCtx();
  if (!ac) return;
  try {
    playNote(ac, 659.25, 0);
    playNote(ac, 880, 0.12);
    playNote(ac, 1108.73, 0.24);
  } catch {
    // audio is an enhancement, never an error
  }
}
