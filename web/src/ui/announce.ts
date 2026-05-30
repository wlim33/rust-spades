let region: HTMLElement | null = null;

function ensureRegion(): HTMLElement {
  if (region && region.isConnected) return region;
  region = document.createElement('div');
  region.setAttribute('aria-live', 'polite');
  region.setAttribute('aria-atomic', 'true');
  region.setAttribute('role', 'status');
  // Visually hidden, but available to assistive technology.
  region.style.cssText =
    'position:absolute;width:1px;height:1px;padding:0;margin:-1px;overflow:hidden;clip:rect(0,0,0,0);white-space:nowrap;border:0;';
  document.body.appendChild(region);
  return region;
}

/**
 * Announce a message to assistive technology via a shared polite live region.
 * Used for game events a sighted player sees but a screen-reader user would
 * otherwise miss (opponent plays, trick results).
 */
export function announce(message: string): void {
  const el = ensureRegion();
  // Reset first so an identical consecutive message is still re-announced.
  el.textContent = '';
  el.textContent = message;
}
