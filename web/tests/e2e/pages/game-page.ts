import { expect, type Page, type Locator } from '@playwright/test';

export class GamePage {
  constructor(private readonly page: Page) {}

  bets(): Locator {
    return this.page.locator('.spades-bets');
  }
  hand(): Locator {
    return this.page.locator('.hand-container .card');
  }
  clickableCards(): Locator {
    return this.page.locator('.cm-clickable');
  }

  /** Resolves once the game is in BETTING: either our bet buttons or non-empty center text. */
  async waitForBetting(): Promise<void> {
    await this.page.waitForFunction(
      () =>
        document.querySelector('.spades-bets') !== null ||
        (document.querySelector('.spades-center-text')?.textContent?.trim() ?? '') !== '',
      { timeout: 15_000 },
    );
  }

  /** Waits for our bet turn (bet buttons render only on our turn) and bets `n`. */
  async bet(n: number): Promise<void> {
    await expect(this.bets()).toBeVisible({ timeout: 15_000 });
    await this.bets()
      .getByRole('button', { name: String(n), exact: true })
      .click();
  }

  /** Resolves when at least one legal card is clickable (i.e., it is our turn to play). */
  async waitForPlayable(): Promise<void> {
    await expect(this.clickableCards().first()).toBeVisible({ timeout: 15_000 });
  }

  async playFirstLegalCard(): Promise<void> {
    await this.waitForPlayable();
    await this.clickableCards().first().click();
  }

  /** Plays the first legal card using only the keyboard (Tab-focus + Enter). */
  async playFirstLegalCardByKeyboard(): Promise<void> {
    await this.waitForPlayable();
    const card = this.clickableCards().first();
    await card.focus();
    await card.press('Enter');
  }

  /**
   * Plays the local player's card in all 13 tricks of the current hand. Uses the
   * hand-count decrement as the per-trick synchronization signal: bots resolve the
   * rest of each trick server-side, so our hand drops by exactly one per play.
   */
  async playOutHand(): Promise<void> {
    for (let remaining = 13; remaining > 0; remaining--) {
      await expect(this.hand()).toHaveCount(remaining, { timeout: 20_000 });
      await this.playFirstLegalCard();
      await expect(this.hand()).toHaveCount(remaining - 1, { timeout: 20_000 });
    }
  }
}
