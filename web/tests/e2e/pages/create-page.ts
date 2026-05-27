import type { Page } from '@playwright/test';

export class CreatePage {
  constructor(private readonly page: Page) {}

  /** Submits the challenge form. With no seat picked, all four seats stay open. */
  async create(): Promise<void> {
    await this.page.getByRole('button', { name: 'Create', exact: true }).click();
  }
}
