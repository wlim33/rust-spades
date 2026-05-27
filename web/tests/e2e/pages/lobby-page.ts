import type { Page } from '@playwright/test';

export class LobbyPage {
  constructor(private readonly page: Page) {}

  /** Claims the first open seat, names the player, joins, and waits for the modal to close. */
  async joinFirstOpenSeat(name: string): Promise<void> {
    await this.page.locator('button.seat-open').first().click({ timeout: 10_000 });
    await this.page.locator('.join-modal input').fill(name);
    await this.page.getByRole('button', { name: 'Join', exact: true }).click();
    await this.page.waitForFunction(() => document.querySelector('.join-modal') === null, {
      timeout: 10_000,
    });
  }
}
