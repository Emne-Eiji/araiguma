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
