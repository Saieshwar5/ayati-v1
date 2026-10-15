import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { test, expect, projectRoot } from './fixtures.ts';

test('live model computes expenses through tools and returns a correct downloadable summary', async ({ page, ayati }) => {
  await ayati.attach('expenses.csv');
  await ayati.completed(await ayati.send(
    'Calculate my expenses by category. Create expense-summary.csv with category,amount columns, one row per category and a Total row. Verify the numbers, and preserve my original file.',
  ));
  const csv = await ayati.download('expense-summary.csv');
  const rows = Object.fromEntries(csv.trim().split(/\r?\n/).slice(1).map((line) => {
    const [category, amount] = line.split(',');
    return [category.trim().replace(/^"|"$/g, '').toLowerCase(), Number(amount.replace(/"/g, ''))];
  }));
  ayati.check('Live model produced correct expense totals', { travel: 320, food: 80, supplies: 150, total: 550 }, rows);
  ayati.check('Live model preserved original expenses',
    await readFile(join(projectRoot, 'tests/fixtures/live/expenses.csv'), 'utf8'), await ayati.download('expenses.csv'));
  await page.getByRole('button', { name: 'Close files' }).click();
  await ayati.completed(await ayati.send('What was my total, and which category was the largest?'));
  await expect(page.locator('.message.assistant').last()).toContainText(/550/);
  await expect(page.locator('.message.assistant').last()).toContainText(/travel/i);
  await ayati.screenshot('live-expense-result');
});

test('live model revises notes into a new file while preserving the original', async ({ ayati }) => {
  await ayati.attach('notes.txt');
  await ayati.completed(await ayati.send(
    'Rewrite my notes clearly, keeping every factual detail. Save the revision as revised-notes.txt. Keep my original unchanged, read your revision to verify it, and explain your changes.',
  ));
  const original = await readFile(join(projectRoot, 'tests/fixtures/live/notes.txt'), 'utf8');
  const revised = await ayati.download('revised-notes.txt');
  ayati.check('Live revision is nonempty', true, revised.trim().length > 0);
  for (const fact of ['supplier', 'Friday', 'invoice', 'next week', '500']) {
    ayati.check(`Revision retains ${fact}`, true, revised.toLowerCase().includes(fact.toLowerCase()));
  }
  ayati.check('Live original is preserved', original, await ayati.download('notes.txt'));
  ayati.evidence.record('human_review_required', {
    reason: 'Clarity and preservation of every factual detail require reviewing the downloaded revision.', original, revised,
  });
  await ayati.screenshot('live-notes-result');
});
