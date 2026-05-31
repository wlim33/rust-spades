import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { home, quickplay } from '../../src/routes/home';

describe('home route', () => {
  beforeEach(() => {
    document.body.innerHTML = '<main id="root"></main>';
    quickplay.value = null;
  });

  afterEach(() => {
    quickplay.value = null;
  });

  it('renders the menu with five action buttons', () => {
    const cleanup = home.render({}, { path: '/', search: new URLSearchParams() });
    const menu = document.querySelector('[data-testid="home-menu"]');
    expect(menu).not.toBeNull();
    const buttons = menu!.querySelectorAll('button');
    expect(buttons.length).toBe(5);

    const labels = Array.from(buttons).map(
      (b) => b.querySelector('.menu__row-title')?.textContent?.trim() ?? b.textContent?.trim(),
    );
    expect(labels).toEqual(['5+3', '10+5', '15+10', 'Play with friends', 'Play with computers']);

    // Icons now come from the vendored Remix pipeline (icon() → span.icon > svg), not inline <svg>
    expect(menu!.querySelector('.quickplay-tile .icon svg')).not.toBeNull();
    expect(menu!.querySelector('.menu__row .menu__row-icon .icon svg')).not.toBeNull();
    expect(menu!.querySelector('.menu__row-go .icon svg')).not.toBeNull();
    cleanup();
  });

  it('clicking a quickplay button shows the waiting view', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => {
        const stream = new ReadableStream<Uint8Array>({ start() {} });
        return new Response(stream, {
          status: 200,
          headers: { 'content-type': 'text/event-stream' },
        });
      }),
    );

    const cleanup = home.render({}, { path: '/', search: new URLSearchParams() });

    // Click the first quickplay button (5+3)
    document.querySelector<HTMLButtonElement>('[data-testid="home-menu"] button')!.click();

    // Allow microtasks for signal/effect to propagate
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();

    expect(document.body.textContent).toContain('Finding players');
    expect(document.body.textContent).toContain('of 4 seated');

    const cancelBtn = document.querySelector<HTMLButtonElement>('.home-searching button');
    expect(cancelBtn).not.toBeNull();
    cancelBtn!.click();

    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();

    expect(document.querySelector('[data-testid="home-menu"]')).not.toBeNull();

    cleanup();
    vi.unstubAllGlobals();
  });

  it('cleanup empties the root', () => {
    const cleanup = home.render({}, { path: '/', search: new URLSearchParams() });
    expect(document.getElementById('root')!.childNodes.length).toBeGreaterThan(0);
    cleanup();
    expect(document.getElementById('root')!.textContent?.trim()).toBe('');
  });
});
