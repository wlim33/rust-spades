import { describe, it, expect, beforeEach } from 'vitest';
import { render } from 'lit-html';
import { formField } from '../../src/ui/components/form-field';

describe('formField', () => {
  beforeEach(() => {
    document.body.innerHTML = '<main id="root"></main>';
  });

  it('renders a labeled input with id and value', () => {
    render(
      formField({
        id: 'email',
        label: 'Email',
        type: 'email',
        value: 'a@x',
        onInput: () => {},
      }),
      document.getElementById('root')!,
    );
    const input = document.querySelector<HTMLInputElement>('#email')!;
    expect(input).not.toBeNull();
    expect(input.type).toBe('email');
    expect(input.value).toBe('a@x');
    expect(document.querySelector('label[for=email]')?.textContent?.trim()).toBe('Email');
  });

  it('shows error message when provided', () => {
    render(
      formField({
        id: 'email',
        label: 'Email',
        value: '',
        onInput: () => {},
        error: 'Required',
      }),
      document.getElementById('root')!,
    );
    expect(document.querySelector('[data-testid=field-error]')?.textContent?.trim()).toBe(
      'Required',
    );
  });
});

describe('formField invalid state', () => {
  beforeEach(() => {
    document.body.innerHTML = '<main id="root"></main>';
  });
  it('marks the field invalid + wires aria when error is present', () => {
    render(
      formField({ id: 'email', label: 'Email', value: '', onInput: () => {}, error: 'Required.' }),
      document.getElementById('root')!,
    );
    const wrap = document.querySelector('.form-field')!;
    expect(wrap.classList.contains('invalid')).toBe(true);
    const input = wrap.querySelector('input')!;
    expect(input.getAttribute('aria-invalid')).toBe('true');
    expect(input.getAttribute('aria-describedby')).toBe('email-error');
    expect(document.querySelector('#email-error.field-error')?.textContent).toBe('Required.');
  });
  it('is not invalid without an error', () => {
    render(
      formField({ id: 'x', label: 'X', value: '', onInput: () => {} }),
      document.getElementById('root')!,
    );
    expect(document.querySelector('.form-field')!.classList.contains('invalid')).toBe(false);
    expect(document.querySelector('input')!.hasAttribute('aria-invalid')).toBe(false);
  });
});
