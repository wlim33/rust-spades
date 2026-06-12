import type { WsHandle } from '../api/ws';
import type { CardOrchestrator } from '../cards/orchestrator';
import type { PollLoop } from '../state/game-sync';

export type Resources = {
  cleanups: Array<() => void>;
  ws: WsHandle | null;
  poller: PollLoop | null;
  orchestrator: CardOrchestrator | null;
};

export function disposeResources(r: Resources): void {
  r.ws?.close();
  r.ws = null;
  r.poller?.stop();
  r.poller = null;
  r.orchestrator?.destroy();
  r.orchestrator = null;
  for (const c of r.cleanups) c();
  r.cleanups = [];
}
