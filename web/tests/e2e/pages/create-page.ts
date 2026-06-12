import type { Page } from '@playwright/test';

export class CreatePage {
  constructor(private readonly page: Page) {}

  /** Submits the challenge form. The creator sits on the default Team A. */
  async create(): Promise<void> {
    await this.page.getByRole('button', { name: 'Create', exact: true }).click();
  }
}
