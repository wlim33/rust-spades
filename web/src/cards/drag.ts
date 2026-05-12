export type DragOpts = {
  threshold: number;
  onPlay: (srcRect: DOMRect) => void;
};

export function attachDrag(el: HTMLElement, opts: DragOpts): () => void {
  let startX = 0;
  let startY = 0;
  let dragging = false;
  let placeholder: HTMLElement | null = null;

  const onDown = (e: PointerEvent): void => {
    e.preventDefault();
    try {
      el.setPointerCapture(e.pointerId);
    } catch {
      // pointer capture isn't supported (e.g. in test envs); proceed without it
    }
    startX = e.clientX;
    startY = e.clientY;
    dragging = true;

    if (el.parentNode) {
      placeholder = document.createElement('div');
      placeholder.className = 'card-placeholder';
      placeholder.style.width = el.offsetWidth + 'px';
      placeholder.style.height = el.offsetHeight + 'px';
      el.parentNode.insertBefore(placeholder, el);
    }

    const rect = el.getBoundingClientRect();
    el.classList.add('dragging');
    el.style.left = rect.left + 'px';
    el.style.top = rect.top + 'px';
    el.style.width = rect.width + 'px';
    el.style.height = rect.height + 'px';
    el.style.transform = '';
  };

  const onMove = (e: PointerEvent): void => {
    if (!dragging) return;
    e.preventDefault();
    const dx = e.clientX - startX;
    const dy = e.clientY - startY;
    el.style.transform = `translate(${dx}px, ${dy}px)`;
    if (dy < -opts.threshold) el.classList.add('card-will-play');
    else el.classList.remove('card-will-play');
  };

  const reset = (): void => {
    if (placeholder?.parentNode) placeholder.parentNode.removeChild(placeholder);
    placeholder = null;
    el.classList.remove('dragging', 'card-will-play');
    el.style.left = '';
    el.style.top = '';
    el.style.width = '';
    el.style.height = '';
    el.style.transform = '';
  };

  const onUp = (e: PointerEvent): void => {
    if (!dragging) return;
    dragging = false;
    const dx = e.clientX - startX;
    const dy = e.clientY - startY;
    const isClick = Math.abs(dx) < 10 && Math.abs(dy) < 10;
    const isPlay = dy < -opts.threshold;
    const srcRect = el.getBoundingClientRect();
    reset();
    if (isPlay || isClick) opts.onPlay(srcRect);
  };

  const onCancel = (): void => {
    if (!dragging) return;
    dragging = false;
    reset();
  };

  el.addEventListener('pointerdown', onDown);
  el.addEventListener('pointermove', onMove);
  el.addEventListener('pointerup', onUp);
  el.addEventListener('pointercancel', onCancel);

  return () => {
    el.removeEventListener('pointerdown', onDown);
    el.removeEventListener('pointermove', onMove);
    el.removeEventListener('pointerup', onUp);
    el.removeEventListener('pointercancel', onCancel);
  };
}
