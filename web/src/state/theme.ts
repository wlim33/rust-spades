import { signal } from '@preact/signals-core';
import { getThemePref, setThemePref } from '../lib/storage';

export type Theme = 'light' | 'dark';

function systemTheme(): Theme {
  return globalThis.matchMedia?.('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
}

export function initialTheme(): Theme {
  return getThemePref() ?? systemTheme();
}

const theme = signal<Theme>(initialTheme());

function apply(t: Theme): void {
  document.documentElement.setAttribute('data-theme', t);
}

function set(t: Theme): void {
  theme.value = t;
  setThemePref(t);
  apply(t);
}

function toggle(): void {
  set(theme.value === 'dark' ? 'light' : 'dark');
}

/** Apply current theme and follow the OS while the user hasn't chosen explicitly. */
function initTheme(): void {
  apply(theme.value);
  const mq = globalThis.matchMedia?.('(prefers-color-scheme: dark)');
  mq?.addEventListener?.('change', (e: MediaQueryListEvent) => {
    if (getThemePref() === null) {
      theme.value = e.matches ? 'dark' : 'light';
      apply(theme.value);
    }
  });
}

export const themeState = { theme, set, toggle, initTheme };
