import { randomUUID } from 'node:crypto';

export type TestUser = { username: string; email: string; password: string };

/** Unique, validation-safe credentials. username: "e2e_" + 10 lowercase hex = 14 chars (<=20, [0-9a-f]). */
export function uniqueUser(): TestUser {
  const id = randomUUID().replace(/-/g, '').slice(0, 10);
  const username = `e2e_${id}`;
  return {
    username,
    email: `${username}@example.test`,
    password: 'e2e-strong-passphrase-9',
  };
}
