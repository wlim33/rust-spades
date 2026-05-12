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
