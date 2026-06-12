import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render } from 'lit-html';
import { teamButton, type TeamMember } from '../../src/ui/components/team-button';

function mount(opts: {
  members: TeamMember[];
  joinable: boolean;
  onJoin?: () => void;
}): HTMLButtonElement {
  render(
    teamButton({
      teamNo: '1',
      label: 'Team A',
      capacity: 2,
      onJoin: opts.onJoin ?? (() => {}),
      members: opts.members,
      joinable: opts.joinable,
    }),
    document.getElementById('root')!,
  );
  return document.querySelector<HTMLButtonElement>('.team-btn')!;
}

describe('teamButton', () => {
  beforeEach(() => {
    document.body.innerHTML = '<main id="root"></main>';
  });

  it.each([
    // members, joinable, data-fill, disabled, aria-label
    [[], true, '0', false, 'Join Team A, 0 of 2 seats filled'],
    [[{ name: 'Ada', mine: false }], true, '1', false, 'Join Team A, 1 of 2 seats filled: Ada'],
    [
      [
        { name: 'Ada', mine: false },
        { name: 'Bo', mine: false },
      ],
      false,
      '2',
      true,
      'Team A, 2 of 2 seats filled: Ada, Bo',
    ],
    // viewer already seated elsewhere: open team renders but is not joinable
    [[], false, '0', true, 'Team A, 0 of 2 seats filled'],
  ] as [TeamMember[], boolean, string, boolean, string][])(
    'members=%j joinable=%s -> fill=%s disabled=%s',
    (members, joinable, fill, disabled, label) => {
      const btn = mount({ members, joinable });
      expect(btn.getAttribute('data-fill')).toBe(fill);
      expect(btn.disabled).toBe(disabled);
      expect(btn.getAttribute('aria-label')).toBe(label);
      expect(btn.getAttribute('data-team')).toBe('1');
    },
  );

  it('renders a filled-icon row per member and an open row per empty seat', () => {
    const btn = mount({ members: [{ name: 'Ada', mine: false }], joinable: true });
    const slots = btn.querySelectorAll('.team-btn__slot');
    expect(slots).toHaveLength(2);
    expect(slots[0]!.textContent).toContain('Ada');
    expect(slots[0]!.querySelector('.icon')).toBeTruthy();
    expect(slots[1]!.classList.contains('team-btn__slot--open')).toBe(true);
    expect(slots[1]!.textContent).toContain('Open');
  });

  it('uses a different icon for member rows than open rows', () => {
    const btn = mount({ members: [{ name: 'Ada', mine: false }], joinable: true });
    const slots = btn.querySelectorAll('.team-btn__slot');
    const memberIcon = slots[0]!.querySelector('.icon')!.innerHTML;
    const openIcon = slots[1]!.querySelector('.icon')!.innerHTML;
    expect(memberIcon).not.toBe(openIcon);
  });

  it('bolds my own row via the mine modifier', () => {
    const btn = mount({ members: [{ name: 'Me', mine: true }], joinable: false });
    expect(btn.querySelector('.team-btn__slot--mine')!.textContent).toContain('Me');
  });

  it('fires onJoin on click only while joinable', () => {
    const onJoin = vi.fn();
    mount({ members: [], joinable: true, onJoin }).click();
    expect(onJoin).toHaveBeenCalledTimes(1);
    const disabled = mount({ members: [], joinable: false, onJoin });
    disabled.click();
    expect(onJoin).toHaveBeenCalledTimes(1); // native disabled swallows the click
  });

  it('maps teamNo 2 onto data-team 2', () => {
    render(
      teamButton({
        teamNo: '2',
        label: 'Team B',
        capacity: 2,
        members: [],
        joinable: true,
        onJoin: () => {},
      }),
      document.getElementById('root')!,
    );
    const btn = document.querySelector<HTMLButtonElement>('.team-btn')!;
    expect(btn.getAttribute('data-team')).toBe('2');
  });

  it('clamps overflow to a full, disabled button', () => {
    const btn = mount({
      members: [
        { name: 'Ada', mine: false },
        { name: 'Bo', mine: false },
        { name: 'Cal', mine: false },
      ],
      joinable: true,
    });
    expect(btn.disabled).toBe(true);
    expect(btn.getAttribute('data-fill')).toBe('2');
  });
});
