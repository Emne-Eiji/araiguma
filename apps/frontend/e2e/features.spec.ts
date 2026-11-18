import { expect, test } from '@playwright/test';

// WO-02 feature smoke tests against the production build.

test('topology view renders the projected graph with legend', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', (error) => errors.push(String(error)));

  await page.goto('/');
  await page.getByTestId('nav-topology').click();

  const canvas = page.getByTestId('topology-canvas');
  await expect(canvas).toBeVisible();

  // The graph matches the golden document's counts (exposed read-only for
  // verification; canvas pixels are not assertable).
  const counts = await page.evaluate(() => {
    const hook = (window as unknown as { __araigumaTopology?: { nodeCount: number; edgeCount: number } })
      .__araigumaTopology;
    return hook ?? null;
  });
  expect(counts).not.toBeNull();
  expect(counts!.nodeCount).toBe(8); // 6 interfaces + 2 gateways
  expect(counts!.edgeCount).toBe(3); // three evidenced gateway edges

  // Legend explains the semantics, including what is NOT drawn.
  await expect(page.getByText('observed relation')).toBeVisible();
  await expect(page.getByText('inferred relation')).toBeVisible();
  await expect(
    page.getByText('Anything beyond a gateway — upstream or internet — is not observed and not drawn.'),
  ).toBeVisible();

  // Mock badge still visible on this view.
  await expect(page.getByTestId('mock-badge')).toBeVisible();

  // Selecting a node opens the details overlay (workspace is not shrunk).
  const widthBefore = await page
    .locator('.workspace')
    .evaluate((el) => el.getBoundingClientRect().width);
  await page.evaluate(() => {
    const hook = (
      window as unknown as {
        __araigumaTopology?: { selectNode: (label: string) => void };
      }
    ).__araigumaTopology;
    hook?.selectNode('wg0');
  });
  const panel = page.getByTestId('topology-details');
  await expect(panel).toBeVisible();
  await expect(panel.getByText('WireGuard', { exact: true })).toBeVisible();
  // The honest unobservable note is shown.
  await expect(panel.getByText(/peers and endpoints are not observable/)).toBeVisible();
  const widthAfter = await page
    .locator('.workspace')
    .evaluate((el) => el.getBoundingClientRect().width);
  expect(Math.round(widthAfter)).toBe(Math.round(widthBefore));

  expect(errors).toEqual([]);
});

test('routes view defaults to the main table with an explicit filter', async ({ page }) => {
  await page.goto('/');
  await page.getByTestId('nav-routes').click();

  const table = page.getByTestId('routes-table');
  await expect(table).toBeVisible();

  // Main table only by default: 8 of 12 routes (workstation fixture).
  const rows = table.locator('tbody tr');
  await expect(rows).toHaveCount(8);

  // The filter states what is hidden and offers the local table.
  await expect(page.getByText('routes in other tables not shown')).toBeVisible();
  const localChip = page.getByRole('button', { name: /local \(255\)/ });
  await expect(localChip).toBeVisible();
  await localChip.click();

  // Both tables visible now — 12 rows, unmerged, with a table column.
  await expect(rows).toHaveCount(12);

  // Default routes are emphasized and multiple defaults coexist.
  const defaultRows = table.locator('tr.default-route');
  await expect(defaultRows).toHaveCount(3);

  // Selecting a route shows the linkage note.
  await defaultRows.first().click();
  await expect(page.getByTestId('route-selection-note')).toBeVisible();

  // Semantics are explicit: on-link rows carry the explanatory tooltip
  // (an observed absence of a gateway, not missing data).
  await expect(table.locator('td[title*="no gateway"]').first()).toBeVisible();
});

test('route details expose flags, notes, sources, and limitations', async ({ page }) => {
  await page.goto('/');
  await page.getByTestId('nav-routes').click();

  const rows = page.getByTestId('routes-table').locator('tbody tr');

  // The multipath-marked default carries the row marker (the limitation is
  // discoverable from the table itself).
  const markedRow = rows.filter({ has: page.locator('.note-badge') });
  await expect(markedRow).toHaveCount(1);

  // Rows are keyboard-operable: focus a row and press Enter.
  const target = markedRow.first();
  await target.focus();
  await target.press('Enter');
  const panel = page.getByTestId('route-details');
  await expect(panel).toBeVisible();

  // The panel shows the full metadata the table cannot fit: flags,
  // provenance, the observation note stating the incomplete multipath
  // representation, and an explicit warning that this observation is
  // limited.
  await expect(panel.getByTestId('route-flags')).toBeVisible();
  await expect(panel.getByTestId('route-flags')).toContainText('none set');
  await expect(panel.getByTestId('route-sources')).toContainText('netlink');
  await expect(panel.getByTestId('route-notes')).toContainText(
    /multipath route with 2 next hops; only the first is represented/,
  );
  await expect(panel.getByTestId('route-limitation-warning')).toBeVisible();

  // The workspace is not permanently narrowed (overlay panel).
  const widthDuring = await page
    .locator('.workspace')
    .evaluate((el) => el.getBoundingClientRect().width);
  expect(Math.round(widthDuring)).toBeGreaterThanOrEqual(900);

  // Close, then open a route that carries real flag values (the default
  // via the down wireless interface carries the kernel's linkdown flag).
  await panel.getByRole('button', { name: 'Close route details' }).click();
  await expect(panel).not.toBeVisible();
  const linkdownRow = rows.filter({ hasText: 'wlp3s0' }).first();
  await linkdownRow.click();
  await expect(panel).toBeVisible();
  await expect(panel.getByTestId('route-flags')).toContainText('linkdown');
  await panel.getByRole('button', { name: 'Close route details' }).click();
  await expect(rows).toHaveCount(8);

  // Space also selects (keyboard parity with pointer clicks).
  await rows.first().focus();
  await rows.first().press(' ');
  await expect(page.getByTestId('route-details')).toBeVisible();
  // Accessible labels exist on every operable row.
  const labels = await page.getByTestId('routes-table').locator('tbody tr').evaluateAll(
    (trs) => trs.map((tr) => (tr as HTMLElement).getAttribute('aria-label') ?? ''),
  );
  expect(labels.every((l) => l.length > 0)).toBe(true);
  expect(labels.some((l) => l.includes('multipath') === false && l.includes('route'))).toBe(true);
});

test('provider status view reports both providers honestly', async ({ page }) => {
  await page.goto('/');
  await page.getByTestId('nav-providers').click();

  const netlink = page.getByTestId('provider-netlink');
  const sysfs = page.getByTestId('provider-sysfs');
  await expect(netlink).toBeVisible();
  await expect(sysfs).toBeVisible();

  await expect(netlink.getByText('available', { exact: false })).toBeVisible();
  await expect(sysfs.getByText('degraded', { exact: false })).toBeVisible();

  // Section outcomes include counts and an unsupported statement.
  await expect(sysfs.getByText('not supported by this source')).toHaveCount(2);
  await expect(netlink.getByText('9 observed')).toBeVisible();

  // Notes are shown verbatim.
  await expect(
    sysfs.getByText(/sysfs cannot distinguish dummy, veth, vlan/),
  ).toBeVisible();
});

test('overview still works after navigation', async ({ page }) => {
  await page.goto('/');
  await page.getByTestId('nav-routes').click();
  await page.getByTestId('nav-topology').click();
  await page.getByTestId('nav-overview').click();
  await expect(page.getByTestId('iface-card-wlp3s0')).toBeVisible();
  await expect(page.getByTestId('mock-badge')).toBeVisible();
});
