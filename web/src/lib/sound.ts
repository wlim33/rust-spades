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
 * Soft two-note turn chime (E5 -> A5). Best-effort: any failure — including
 * autoplay policy before the first user gesture — is silently ignored.
 */
export function chime(): void {
  if (!getSoundPref()) return;
  const ac = getCtx();
  if (!ac) return;
  // A suspended context (autoplay policy) would queue these notes and replay
  // them garbled after the first user gesture — skip instead. getCtx() has
  // already requested resume(), so the next chime after a gesture plays.
  if (ac.state !== 'running') return;
  try {
    const t0 = ac.currentTime;
    for (const [freq, at] of [
      [659.25, 0],
      [880, 0.12],
    ] as const) {
      const osc = ac.createOscillator();
      const gain = ac.createGain();
      osc.type = 'sine';
      osc.frequency.value = freq;
      gain.gain.setValueAtTime(0, t0 + at);
      gain.gain.linearRampToValueAtTime(0.08, t0 + at + 0.02);
      gain.gain.exponentialRampToValueAtTime(0.0001, t0 + at + 0.3);
      osc.connect(gain).connect(ac.destination);
      osc.start(t0 + at);
      osc.stop(t0 + at + 0.32);
    }
  } catch {
    // audio is an enhancement, never an error
  }
}
