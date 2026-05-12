import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { home } from '../../src/routes/home';

describe('home route', () => {
  let logSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    document.body.innerHTML = '<main id="root"></main>';
    logSpy = vi.spyOn(console, 'log').mockImplementation(() => {});
  });

  afterEach(() => {
    logSpy.mockRestore();
  });

  it('renders the menu with five action buttons', () => {
    const cleanup = home.render({}, { path: '/', search: new URLSearchParams() });
    const menu = document.querySelector('[data-testid="home-menu"]');
    expect(menu).not.toBeNull();
    const buttons = menu!.querySelectorAll('button');
    expect(buttons.length).toBe(5);
    expect(Array.from(buttons).map((b) => b.textContent?.trim())).toEqual([
      '5+3',
      '10+5',
      '15+10',
      'Play with Friends',
      'Play with Computers',
    ]);
    cleanup();
  });

  it('logs seek payload for quickplay 10+5', () => {
    const cleanup = home.render({}, { path: '/', search: new URLSearchParams() });
    const buttons = document.querySelectorAll('[data-testid="home-menu"] button');
    (buttons[1] as HTMLButtonElement).click();
    expect(logSpy).toHaveBeenCalledWith('seek quickplay', {
      initial_time_secs: 600,
      increment_secs: 5,
    });
    cleanup();
  });

  it('cleanup empties the root', () => {
    const cleanup = home.render({}, { path: '/', search: new URLSearchParams() });
    expect(document.getElementById('root')!.childNodes.length).toBeGreaterThan(0);
    cleanup();
    expect(document.getElementById('root')!.textContent?.trim()).toBe('');
  });
});
