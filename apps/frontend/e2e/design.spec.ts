import { expect, test } from '@playwright/test';

// Design-verification suite (ported from the WO-01 design check script):
// verifies the visual foundation objectively — dark palette, rail geometry,
// workspace dominance, badge visibility — via computed styles in the real
// browser at a desktop viewport.

test.use({ viewport: { width: 1440, height: 900 } });

test('design foundation: dark theme, 56px rail, workspace dominance', async ({ page }) => {
  await page.goto('/');

  const checks = await page.evaluate(() => {
    const results: Record<string, unknown> = {};

    const body = getComputedStyle(document.body);
    results.bodyBackground = body.backgroundColor;
    results.bodyColor = body.color;

    const rail = document.querySelector('.rail');
    results.navRailWidth = rail ? rail.getBoundingClientRect().width : null;
    results.navItemCount = rail ? rail.querySelectorAll('.item').length : 0;

    const workspace = document.querySelector('.workspace');
    results.workspaceWidth = workspace
      ? Math.round(workspace.getBoundingClientRect().width)
      : null;
    results.viewportWidth = window.innerWidth;
    results.workspaceShare = workspace
      ? Math.round((workspace.getBoundingClientRect().width / window.innerWidth) * 100)
      : null;

    const badge = document.querySelector('[data-testid="mock-badge"]');
    results.badgeVisible = badge ? badge.getBoundingClientRect().height > 0 : false;
    results.badgeText = badge ? badge.textContent : null;

    const mono = document.querySelector('.mono');
    results.monoFont = mono ? getComputedStyle(mono).fontFamily : null;

    return results;
  });

  // Dark theme: body background parses to a dark color.
  const rgb = String(checks.bodyBackground)
    .match(/\d+/g)
    ?.map(Number) ?? [255, 255, 255];
  const brightness = (rgb[0] * 299 + rgb[1] * 587 + rgb[2] * 114) / 1000;
  expect(brightness, `body background must be dark (got ${checks.bodyBackground})`).toBeLessThan(60);

  // Narrow icon rail.
  expect(checks.navRailWidth).toBe(56);
  expect(checks.navItemCount as number).toBeGreaterThanOrEqual(1);

  // Central workspace dominates: at least 96% of the viewport width.
  expect(checks.workspaceShare as number).toBeGreaterThanOrEqual(96);

  // Mock data is explicitly visible.
  expect(checks.badgeVisible).toBe(true);
  expect(checks.badgeText).toBe('MOCK DATA');

  // Monospace values are used.
  expect(String(checks.monoFont).toLowerCase()).toContain('mono');
});

test('details panel overlays the workspace instead of shrinking it', async ({ page }) => {
  await page.goto('/');
  await page.getByTestId('nav-interfaces').click();
  const widthBefore = await page
    .locator('.workspace')
    .evaluate((el) => el.getBoundingClientRect().width);

  await page.getByTestId('iface-card-eth0').click();
  await expect(page.getByTestId('details-panel')).toBeVisible();

  const widthAfter = await page
    .locator('.workspace')
    .evaluate((el) => el.getBoundingClientRect().width);

  expect(Math.round(widthAfter)).toBe(Math.round(widthBefore));
});
