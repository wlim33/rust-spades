import { describe, it, expect, beforeEach } from 'vitest';
import { render } from 'lit-html';
import { icon } from '../../src/ui/icon';

describe('icon', () => {
  beforeEach(() => {
    document.body.innerHTML = '<main id="root"></main>';
  });

  it('renders an inline svg for a known icon', () => {
    render(icon('sun-line'), document.getElementById('root')!);
    expect(document.querySelector('.icon svg')).not.toBeNull();
  });

  it('a labeled icon exposes role=img + aria-label', () => {
    render(icon('group-line', { label: 'Friends' }), document.getElementById('root')!);
    const el = document.querySelector('.icon')!;
    expect(el.getAttribute('role')).toBe('img');
    expect(el.getAttribute('aria-label')).toBe('Friends');
  });

  it('an unlabeled icon is aria-hidden', () => {
    render(icon('moon-line'), document.getElementById('root')!);
    expect(document.querySelector('.icon')!.getAttribute('aria-hidden')).toBe('true');
  });

  it('returns empty for an unknown icon name', () => {
    render(icon('does-not-exist'), document.getElementById('root')!);
    expect(document.querySelector('.icon')).toBeNull();
  });

  it('renders vendored Lucide icons without clobbering their stroke style', () => {
    render(icon('spade'), document.getElementById('root')!);
    const svg = document.querySelector('.icon svg');
    expect(svg).not.toBeNull();
    expect(svg!.getAttribute('fill')).toBe('none');
    expect(svg!.getAttribute('stroke')).toBe('currentColor');
  });
});
