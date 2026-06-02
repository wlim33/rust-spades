import { describe, it, expect, beforeEach, vi, type Mock } from 'vitest';
import { attachDrag } from '../../src/cards/drag';

function pointer(el: HTMLElement, type: string, opts: PointerEventInit = {}): void {
  el.dispatchEvent(new PointerEvent(type, { bubbles: true, pointerId: 1, ...opts }));
}

describe('attachDrag', () => {
  let el: HTMLElement;
  // Vitest 4's bare `vi.fn()` is `Mock<Procedure>`, no longer assignable to a
  // specific callback type, so spell out the signature attachDrag expects.
  let onPlay: Mock<(srcRect: DOMRect) => void>;

  beforeEach(() => {
    document.body.innerHTML =
      '<div id="parent"><div id="card" style="width:50px;height:70px"></div></div>';
    el = document.getElementById('card')!;
    onPlay = vi.fn();
    // happy-dom doesn't implement setPointerCapture on Elements consistently — stub it.
    (el as HTMLElement & { setPointerCapture?: (n: number) => void }).setPointerCapture = () => {};
  });

  it('plays on simple click (small move)', () => {
    attachDrag(el, { threshold: 60, onPlay });
    pointer(el, 'pointerdown', { clientX: 0, clientY: 0 });
    pointer(el, 'pointerup', { clientX: 2, clientY: 1 });
    expect(onPlay).toHaveBeenCalledTimes(1);
  });

  it('plays when dragged up past threshold', () => {
    attachDrag(el, { threshold: 60, onPlay });
    pointer(el, 'pointerdown', { clientX: 0, clientY: 0 });
    pointer(el, 'pointermove', { clientX: 0, clientY: -70 });
    pointer(el, 'pointerup', { clientX: 0, clientY: -70 });
    expect(onPlay).toHaveBeenCalledTimes(1);
  });

  it('does not play when drag is below threshold and not a click', () => {
    attachDrag(el, { threshold: 60, onPlay });
    pointer(el, 'pointerdown', { clientX: 0, clientY: 0 });
    pointer(el, 'pointermove', { clientX: 30, clientY: 30 });
    pointer(el, 'pointerup', { clientX: 30, clientY: 30 });
    expect(onPlay).not.toHaveBeenCalled();
  });

  it('cleanup detaches listeners', () => {
    const cleanup = attachDrag(el, { threshold: 60, onPlay });
    cleanup();
    pointer(el, 'pointerdown', { clientX: 0, clientY: 0 });
    pointer(el, 'pointerup', { clientX: 2, clientY: 2 });
    expect(onPlay).not.toHaveBeenCalled();
  });

  it('reports the source rect for fly animations', () => {
    attachDrag(el, { threshold: 60, onPlay });
    pointer(el, 'pointerdown', { clientX: 0, clientY: 0 });
    pointer(el, 'pointermove', { clientX: 0, clientY: -80 });
    pointer(el, 'pointerup', { clientX: 0, clientY: -80 });
    expect(onPlay).toHaveBeenCalledWith(
      expect.objectContaining({ width: expect.any(Number), height: expect.any(Number) }),
    );
  });
});
