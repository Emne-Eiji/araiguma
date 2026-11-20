import { expect, test } from '@playwright/test';

// WO-04 Stage A shell tests: the six-section navigation rail (collapsed +
// pinned modes, hover labels, keyboard operation, active state), the shared
// right details panel (open/close, Escape, empty state), the bottom status
// area, the Documents & Import view states, the scenario registry
// presentation, and the tray-compatible destination API (through the gated
// test hook — the same functions a future tray would call).

const SECTIONS = [
  { testid: 'nav-overview', view: 'overview-view' },
  { testid: 'nav-topology', view: 'topology-canvas' },
  { testid: 'nav-interfaces', view: 'interfaces-view' },
  { testid: 'nav-routes', view: 'routes-table' },
  { testid: 'nav-providers', view: 'provider-netlink' },
  { testid: 'nav-documents', view: 'documents-view' },
] as const;

test('all six sections are present, navigable, and marked active', async ({ page }) => {
  await page.goto('/');

  for (const section of SECTIONS) {
    const item = page.getByTestId(section.testid);
    await expect(item).toBeVisible();
    // Accessible name (aria-label) and tooltip exist on every item.
    expect(await item.getAttribute('aria-label')).toBeTruthy();
    expect(await item.getAttribute('title')).toBeTruthy();
    await item.click();
    await expect(page.getByTestId(section.view)).toBeVisible();
    await expect(item).toHaveAttribute('aria-current', 'page');
    await expect(item).toHaveClass(/active/);
  }
});

test('collapsed rail is 56px; hovering an item previews its label', async ({ page }) => {
  await page.goto('/');

  const rail = page.locator('.rail');
  expect(Math.round(await rail.evaluate((el) => el.getBoundingClientRect().width))).toBe(56);

  const item = page.getByTestId('nav-interfaces');
  const flyout = item.locator('.flyout');
  // The flyout is opacity-gated (kept in the layout, invisible until
  // hover/focus) — assert the opacity, not visibility.
  await expect(flyout).toHaveCSS('opacity', '0');
  await item.hover();
  await expect(flyout).toHaveCSS('opacity', '1');
  await expect(flyout).toHaveText('Interfaces');
});

test('pin toggle expands the rail with persistent labels and collapses back', async ({ page }) => {
  await page.goto('/');

  const rail = page.locator('.rail');
  const toggle = page.getByTestId('nav-pin-toggle');
  await expect(toggle).toHaveAttribute('aria-pressed', 'false');

  await toggle.click();
  await expect(toggle).toHaveAttribute('aria-pressed', 'true');
  // The width transition is animated; toHaveCSS waits it out.
  await expect(rail).toHaveCSS('width', '232px');
  // Labels are persistently visible next to the icons in pinned mode —
  // full names, wrapped rather than truncated.
  await expect(rail.getByText('Routes & Gateways')).toBeVisible();
  await expect(rail.getByText('Providers & Observation Health')).toBeVisible();
  await expect(rail.getByText('Documents & Import')).toBeVisible();
  // No hover flyouts in pinned mode.
  await expect(page.getByTestId('nav-overview').locator('.flyout')).toHaveCount(0);

  await toggle.click();
  await expect(toggle).toHaveAttribute('aria-pressed', 'false');
  await expect(rail).toHaveCSS('width', '56px');
});

test('rail items are keyboard-operable with visible focus', async ({ page }) => {
  await page.goto('/');

  // Keyboard-focus a rail item and activate it with Enter.
  const item = page.getByTestId('nav-routes');
  await item.focus();
  await expect(item).toBeFocused();
  await item.press('Enter');
  await expect(page.getByTestId('routes-table')).toBeVisible();
  await expect(item).toHaveAttribute('aria-current', 'page');

  // The pin toggle is also keyboard-operable.
  const toggle = page.getByTestId('nav-pin-toggle');
  await toggle.focus();
  await toggle.press('Enter');
  await expect(toggle).toHaveAttribute('aria-pressed', 'true');
});

test('the details panel closes with Escape and returns focus to the trigger', async ({ page }) => {
  await page.goto('/');
  await page.getByTestId('nav-interfaces').click();

  const card = page.getByTestId('iface-card-eth0');
  await card.click();
  const panel = page.getByTestId('details-panel');
  await expect(panel).toBeVisible();
  // Focus moved into the panel on open.
  await expect(panel).toBeFocused();

  await page.keyboard.press('Escape');
  await expect(page.getByTestId('details-panel')).toHaveCount(0);
  // Focus returned to the trigger.
  await expect(card).toBeFocused();
});

test('destination API: section navigation and selected-object destinations', async ({ page }) => {
  await page.goto('/?araiguma-test-hooks');

  // Section destination.
  await page.evaluate(() => {
    (window as unknown as { __araigumaNavigation?: { navigateTo: (s: string) => void } })
      .__araigumaNavigation?.navigateTo('documents');
  });
  await expect(page.getByTestId('documents-view')).toBeVisible();

  // Selected-object destination: interface ifindex 2 (eth0 in the
  // workstation scenario) opens the shared panel with its details.
  await page.evaluate(() => {
    (window as unknown as { __araigumaNavigation?: { navigate: (d: unknown) => void } })
      .__araigumaNavigation?.navigate({
        section: 'interfaces',
        selection: { kind: 'interface', ifindex: 2 },
      });
  });
  const panel = page.getByTestId('details-panel');
  await expect(panel).toBeVisible();
  await expect(panel.locator('h2')).toHaveText('eth0');

  // An unresolvable selection opens the panel's explicit empty state —
  // never stale or invented details.
  await page.evaluate(() => {
    (window as unknown as { __araigumaNavigation?: { navigate: (d: unknown) => void } })
      .__araigumaNavigation?.navigate({
        section: 'interfaces',
        selection: { kind: 'interface', ifindex: 9999 },
      });
  });
  await expect(page.getByTestId('panel-empty')).toBeVisible();
  await expect(page.getByTestId('details-panel')).toHaveCount(0);

  // A route destination resolved through the same API.
  await page.evaluate(() => {
    (window as unknown as { __araigumaNavigation?: { navigate: (d: unknown) => void } })
      .__araigumaNavigation?.navigate({
        section: 'routes',
        selection: {
          kind: 'route',
          family: 'ipv4',
          destination: { address: '0.0.0.0', prefixLen: 0 },
          gateway: '192.168.1.1',
          metric: 100,
          tos: null,
        },
      });
  });
  await expect(page.getByTestId('routes-table')).toBeVisible();
  await expect(page.getByTestId('route-details')).toBeVisible();
});

test('route details offer the selected-object destination "Show in Topology"', async ({ page }) => {
  await page.goto('/');

  await page.getByTestId('nav-routes').click();
  const multipathRow = page.getByTestId('routes-table').locator('tbody tr').filter({ hasText: '2 next hops' });
  await multipathRow.click();
  const panel = page.getByTestId('route-details');
  await expect(panel).toBeVisible();

  // The destination keeps the route selected across the section switch:
  // topology is shown WITH the highlight and the route details still open.
  await panel.getByTestId('route-show-in-topology').click();
  await expect(page.getByTestId('topology-canvas')).toBeVisible();
  await expect(page.getByTestId('nav-topology')).toHaveAttribute('aria-current', 'page');
  await expect(page.getByTestId('route-details')).toBeVisible();
});

test('documents view: the five document states, honestly labeled', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', (error) => errors.push(String(error)));

  await page.goto('/');
  await page.getByTestId('nav-documents').click();

  // Mock mode: both kinds are "not observed"; the scenario is named.
  await expect(page.getByTestId('document-status-snapshot')).toHaveText('not observed');
  await expect(page.getByTestId('document-status-topology')).toHaveText('not observed');
  await expect(page.getByTestId('document-slot-snapshot')).toContainText('Synthetic workstation');

  // The scenario registry lists all five scenarios; workstation is active.
  for (const id of ['workstation', 'incomplete-observations', 'server-tunnel', 'dummy-virtual', 'empty']) {
    await expect(page.getByTestId(`scenario-${id}`)).toBeVisible();
  }
  await expect(page.getByTestId('scenario-workstation')).toContainText('shown now');
  await expect(page.getByTestId('scenario-server-tunnel')).toContainText('tunnel default');

  // Import a valid snapshot: imported OK, topology missing.
  await page.getByTestId('import-file-input').setInputFiles({
    name: 'server-room.json',
    mimeType: 'application/json',
    buffer: Buffer.from(JSON.stringify(serverTunnelSnapshot)),
  });
  await expect(page.getByTestId('document-status-snapshot')).toHaveText('imported OK');
  await expect(page.getByTestId('document-slot-snapshot')).toContainText('server-room.json');
  await expect(page.getByTestId('document-slot-snapshot')).toContainText('Observed');
  await expect(page.getByTestId('document-slot-snapshot')).toContainText('Imported');
  await expect(page.getByTestId('document-status-topology')).toHaveText('missing');
  await expect(page.getByTestId('last-accepted')).toContainText('server-room.json');

  // A rejected import is reported and classified as failed validation.
  await page.getByTestId('import-file-input').setInputFiles({
    name: 'broken.json',
    mimeType: 'application/json',
    buffer: Buffer.from(
      JSON.stringify({
        schemaVersion: 2,
        timestampMs: 1732000000000,
        interfaces: ['garbage'],
        addresses: [],
        routes: [],
        providers: [],
      }),
    ),
  });
  await expect(page.getByTestId('last-rejection')).toBeVisible();
  await expect(page.getByTestId('rejection-class')).toHaveText('failed validation');
  await expect(page.getByTestId('rejection-reason')).toContainText('interfaces');

  // An unsupported schema version is classified as unsupported.
  await page.getByTestId('import-file-input').setInputFiles({
    name: 'future.json',
    mimeType: 'application/json',
    buffer: Buffer.from(JSON.stringify({ ...(workstationSnapshot as object), schemaVersion: 99 })),
  });
  await expect(page.getByTestId('rejection-class')).toHaveText('unsupported');
  await expect(page.getByTestId('rejection-reason')).toContainText('unsupported schema');

  // In imported mode the scenario selector says what selecting means.
  await expect(page.getByTestId('scenario-workstation')).not.toContainText('shown now');
  await expect(
    page.getByTestId('documents-view').getByText('selecting returns to mock data'),
  ).toBeVisible();

  expect(errors).toEqual([]);
});

test('scenario selection switches the labeled mock documents', async ({ page }) => {
  await page.goto('/');
  await page.getByTestId('nav-documents').click();

  await page.getByTestId('scenario-server-tunnel').click();
  await expect(page.getByTestId('scenario-server-tunnel')).toContainText('shown now');
  // The status bar names the scenario explicitly.
  await expect(page.getByTestId('data-provenance')).toContainText('Server with tunnel');

  // The views render the newly selected scenario's documents.
  await page.getByTestId('nav-interfaces').click();
  await expect(page.getByTestId('iface-card-tun0')).toBeVisible();
  await expect(page.getByTestId('iface-card-wlp3s0')).toHaveCount(0);
  await expect(page.getByTestId('mock-badge')).toBeVisible();

  // Returning to mock keeps the selected scenario.
  await page.getByTestId('nav-documents').click();
  await expect(page.getByTestId('documents-import-button')).toBeVisible();
  await page.getByTestId('import-file-input').setInputFiles({
    name: 'obs.json',
    mimeType: 'application/json',
    buffer: Buffer.from(JSON.stringify(workstationSnapshot)),
  });
  await expect(page.getByTestId('imported-badge')).toBeVisible();
  await page.getByTestId('nav-return-mock').click();
  await expect(page.getByTestId('status-mode')).toHaveText('MOCK DATA');
  await expect(page.getByTestId('data-provenance')).toContainText('Server with tunnel');
});

test('the bottom status area states the mode and provenance without inventing times', async ({
  page,
}) => {
  await page.goto('/');

  // Mock mode: chip + explicit synthetic scenario provenance, no
  // observation timestamp (a synthetic timestamp is never shown as one).
  await expect(page.getByTestId('status-bar')).toBeVisible();
  await expect(page.getByTestId('status-mode')).toHaveText('MOCK DATA');
  await expect(page.getByTestId('data-provenance')).toContainText('Synthetic demonstration data');
  await expect(page.getByTestId('data-provenance')).toContainText('not an observation');

  // Imported mode: the file names and their OBSERVATION times appear.
  await page.getByTestId('import-file-input').setInputFiles({
    name: 'real-obs.json',
    mimeType: 'application/json',
    buffer: Buffer.from(JSON.stringify(serverTunnelSnapshot)),
  });
  await expect(page.getByTestId('status-mode')).toHaveText('IMPORTED OBSERVATION');
  await expect(page.getByTestId('data-provenance')).toContainText('real-obs.json');
  await expect(page.getByTestId('data-provenance')).toContainText('observed');
  await expect(page.getByTestId('data-provenance')).toContainText('UTC');
  await expect(page.getByTestId('data-provenance')).toContainText('not a live connection');
});

test('the overview summarizes the current documents and links the sections', async ({ page }) => {
  await page.goto('/');

  await expect(page.getByTestId('overview-view')).toBeVisible();
  // Counts are the scenario's real counts (workstation: 6 interfaces, 12
  // routes; topology: 9 nodes, 5 evidenced relations).
  await expect(page.getByTestId('overview-scenario')).toContainText('Synthetic workstation');
  const counts = page.getByTestId('overview-view').locator('.counts');
  await expect(counts).toContainText('Observed interfaces');
  await expect(counts.getByText('6', { exact: true })).toBeVisible();
  await expect(counts.getByText('12', { exact: true })).toBeVisible();
  await expect(counts.getByText('9', { exact: true })).toBeVisible();
  await expect(counts.getByText('5', { exact: true })).toBeVisible();

  // Section links navigate through the same destination API as the rail.
  await page.getByTestId('overview-link-topology').click();
  await expect(page.getByTestId('topology-canvas')).toBeVisible();
  await page.getByTestId('nav-overview').click();
  await page.getByTestId('overview-link-documents').click();
  await expect(page.getByTestId('documents-view')).toBeVisible();
});

import serverTunnelSnapshot from '../../../fixtures/snapshots/server-tunnel.json' with { type: 'json' };
import workstationSnapshot from '../../../fixtures/snapshots/workstation.json' with { type: 'json' };
