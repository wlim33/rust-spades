import type { WsHandle } from '../api/ws';
import type { CardOrchestrator } from '../cards/orchestrator';

export type Resources = {
  cleanups: Array<() => void>;
  ws: WsHandle | null;
  pollTimer: ReturnType<typeof setInterval> | null;
  orchestrator: CardOrchestrator | null;
};

export function disposeResources(r: Resources): void {
  r.ws?.close();
  r.ws = null;
  if (r.pollTimer) clearInterval(r.pollTimer);
  r.pollTimer = null;
  r.orchestrator?.destroy();
  r.orchestrator = null;
  for (const c of r.cleanups) c();
  r.cleanups = [];
}
