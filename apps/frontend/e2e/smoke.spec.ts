import { expect, test } from '@playwright/test';

// Browser smoke test against the production build (preview server).
// Establishes: the app starts, renders the shell and mock data, and does
// not throw runtime errors.

test('application renders the shell with mock interface cards', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', (error) => errors.push(String(error)));

  await page.goto('/');
  await expect(page).toHaveTitle(/Araiguma/);

  // Navigation rail with the overview item.
  await expect(page.getByTestId('nav-overview')).toBeVisible();

  // Mock interface cards render from the fixture.
  const card = page.getByTestId('iface-card-wlp3s0');
  await expect(card).toBeVisible();
  await expect(card.getByText('Wi-Fi')).toBeVisible();

  // Every interface from the fixture gets a card.
  for (const name of ['lo', 'eth0', 'wlp3s0', 'br0', 'wg0', 'dummy0']) {
    await expect(page.getByTestId(`iface-card-${name}`)).toBeVisible();
  }

  // The MOCK DATA badge is visible while synthetic data is shown.
  await expect(page.getByTestId('mock-badge')).toBeVisible();
  await expect(page.getByTestId('mock-badge')).toHaveText('MOCK DATA');

  // Selecting a card opens the contextual details panel.
  await card.click();
  const panel = page.getByTestId('details-panel');
  await expect(panel).toBeVisible();
  await expect(panel.getByText('Wi-Fi')).toBeVisible();

  // Closing the panel works.
  await panel.getByRole('button', { name: 'Close details' }).click();
  await expect(page.getByTestId('details-panel')).toHaveCount(0);

  expect(errors, 'no uncaught runtime errors').toEqual([]);
});
