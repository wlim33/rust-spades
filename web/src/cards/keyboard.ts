/**
 * Make `el` keyboard-operable as a button: focusable via Tab, with Enter or
 * Space triggering `onActivate`. Returns a cleanup that removes the handler and
 * restores the element to non-focusable. Pairs with `attachDrag` so a card can
 * be played by pointer or keyboard.
 */
export function attachKeyboard(el: HTMLElement, onActivate: () => void): () => void {
  el.tabIndex = 0;
  const onKeyDown = (e: KeyboardEvent): void => {
    if (e.key === 'Enter' || e.key === ' ' || e.key === 'Spacebar') {
      e.preventDefault();
      onActivate();
    }
  };
  el.addEventListener('keydown', onKeyDown);
  return () => {
    el.removeEventListener('keydown', onKeyDown);
    el.removeAttribute('tabindex');
  };
}
