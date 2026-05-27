import type { Page } from '@playwright/test';

export class HomePage {
  constructor(private readonly page: Page) {}

  async goto(): Promise<void> {
    await this.page.goto('/');
  }

  async quickplay(label: '5+3' | '10+5' | '15+10'): Promise<void> {
    await this.page.getByRole('button', { name: label, exact: true }).click();
  }

  async playWithComputers(): Promise<void> {
    await this.page.getByTestId('play-computers').click();
  }

  async playWithFriends(): Promise<void> {
    await this.page.getByTestId('play-friends').click();
  }
}
