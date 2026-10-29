import { resolve } from 'node:path';

export const root = resolve(import.meta.dirname, '../..');

export async function until(check, message, timeout = 20000) {
  const end = Date.now() + timeout;
  while (Date.now() < end) {
    const result = await check();
    if (result) return result;
    await new Promise((resolve) => setTimeout(resolve, 30));
  }
  throw new Error(message);
}
