import { expect, test } from '@playwright/test';

// WO-02 feature smoke tests against the production build.

test('topology view renders the projected graph with legend', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', (error) => errors.push(String(error)));

  // The inspection hook requires the explicit test-hooks parameter (see
  // TopologyView.svelte) — the production build exposes nothing without it.
  await page.goto('/?araiguma-test-hooks');
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
  expect(counts!.nodeCount).toBe(9); // 6 interfaces + 3 gateways (incl. the multipath route's)
  expect(counts!.edgeCount).toBe(5); // three single-path gateway edges + both multipath hops

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
  await expect(table.locator('[title*="no gateway"]').first()).toBeVisible();
});

test('route details expose flags, notes, sources, and the full multipath hop list', async ({
  page,
}) => {
  await page.goto('/');
  await page.getByTestId('nav-routes').click();

  const rows = page.getByTestId('routes-table').locator('tbody tr');

  // The multipath route is identifiable from the table itself: the
  // gateway cell announces the hop count and the interface cell lists the
  // hop interfaces (WO-03 M3) — never "on-link", which the null
  // route-level gateway would wrongly suggest.
  const multipathRow = rows.filter({ hasText: '2 next hops' });
  await expect(multipathRow).toHaveCount(1);
  await expect(multipathRow).toContainText('eth0, wlp3s0');

  // Rows are keyboard-operable: focus a row and press Enter.
  await multipathRow.focus();
  await multipathRow.press('Enter');
  const panel = page.getByTestId('route-details');
  await expect(panel).toBeVisible();

  // The panel lists EVERY observed next hop: gateways, output
  // interfaces, weights, per-hop flags — nothing flattened or dropped.
  const hops = panel.getByTestId('route-next-hop');
  await expect(hops).toHaveCount(2);
  await expect(hops.nth(0)).toContainText('via 198.51.100.1');
  await expect(hops.nth(0)).toContainText('eth0');
  await expect(hops.nth(0)).toContainText('weight 1');
  await expect(hops.nth(1)).toContainText('wlp3s0');
  await expect(hops.nth(1)).toContainText('weight 2');
  // Route-level fields point at the hop list instead of masquerading as
  // a single gateway / on-link.
  await expect(panel.getByText('per next hop (2 hops)')).toBeVisible();

  // The rest of the metadata the table cannot fit stays fully visible.
  await expect(panel.getByTestId('route-flags')).toBeVisible();
  await expect(panel.getByTestId('route-flags')).toContainText('none set');
  await expect(panel.getByTestId('route-sources')).toContainText('netlink');

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
  // Accessible labels exist on every operable row, and the multipath row
  // announces its hop count.
  const labels = await page.getByTestId('routes-table').locator('tbody tr').evaluateAll(
    (trs) => trs.map((tr) => (tr as HTMLElement).getAttribute('aria-label') ?? ''),
  );
  expect(labels.every((l) => l.length > 0)).toBe(true);
  expect(labels.some((l) => l.includes('multipath with 2 next hops'))).toBe(true);
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
  // Since WO-03 M4 the sysfs provider parses /proc/net/ipv6_route, so its
  // only remaining unsupported section is IPv4 addresses.
  await expect(sysfs.getByText('not supported by this source')).toHaveCount(1);
  await expect(netlink.getByText('9 observed')).toBeVisible();

  // Notes are shown verbatim.
  await expect(
    sysfs.getByText(/sysfs cannot distinguish dummy, veth, vlan/),
  ).toBeVisible();
});

test('overview and interfaces still work after navigation', async ({ page }) => {
  await page.goto('/');
  await page.getByTestId('nav-routes').click();
  await page.getByTestId('nav-topology').click();
  await page.getByTestId('nav-overview').click();
  // The Overview summarizes the current documents (mock scenario counts).
  await expect(page.getByTestId('overview-view')).toBeVisible();
  await expect(page.getByTestId('mock-badge')).toBeVisible();
  // The Interfaces section still renders every card from the fixture.
  await page.getByTestId('nav-interfaces').click();
  await expect(page.getByTestId('iface-card-wlp3s0')).toBeVisible();
  await expect(page.getByTestId('mock-badge')).toBeVisible();
});

test('test instrumentation is inert in the production build', async ({ page }) => {
  // WO-02 correction §9.3: the topology test hook is gated behind
  // development mode or the explicit ?araiguma-test-hooks parameter. A
  // plain visit of the production build must not expose it, and navigating
  // away must never leave a stale global behind.
  await page.goto('/?araiguma-test-hooks');
  await page.getByTestId('nav-topology').click();
  const exposed = await page.evaluate(
    () => (window as unknown as Record<string, unknown>).__araigumaTopology !== undefined,
  );
  expect(exposed).toBe(true);
  // Leaving the view removes the global entirely.
  await page.getByTestId('nav-overview').click();
  const stale = await page.evaluate(
    () => (window as unknown as Record<string, unknown>).__araigumaTopology !== undefined,
  );
  expect(stale).toBe(false);

  // A normal visit (no parameter) never gets the hook at all.
  await page.goto('/');
  await page.getByTestId('nav-topology').click();
  const plain = await page.evaluate(
    () => (window as unknown as Record<string, unknown>).__araigumaTopology !== undefined,
  );
  expect(plain).toBe(false);
});

// WO-03 M1: importing actual observation documents into the frontend.

import workstationSnapshot from '../../../fixtures/snapshots/workstation.json' with { type: 'json' };
import serverTunnelSnapshot from '../../../fixtures/snapshots/server-tunnel.json' with { type: 'json' };
import incompleteSnapshot from '../../../fixtures/snapshots/incomplete-observations.json' with { type: 'json' };
import workstationTopology from '../../../fixtures/topology/workstation.json' with { type: 'json' };

test('import workflow: snapshot and topology documents, provenance, and return to mock', async ({
  page,
}) => {
  await page.goto('/');
  // Mock mode: badge and provenance line.
  await expect(page.getByTestId('mock-badge')).toBeVisible();
  await expect(page.getByTestId('data-provenance')).toContainText('Synthetic demonstration data');

  // Import a snapshot through the file input.
  await page.getByTestId('import-file-input').setInputFiles({
    name: 'real-snapshot.json',
    mimeType: 'application/json',
    buffer: Buffer.from(JSON.stringify(serverTunnelSnapshot)),
  });
  await expect(page.getByTestId('imported-badge')).toBeVisible();
  await expect(page.getByTestId('data-provenance')).toContainText('real-snapshot.json');
  await expect(page.getByTestId('data-provenance')).toContainText('not a live connection');
  // The snapshot drives the views: server-tunnel has ens3+tun0, no wlp3s0.
  await page.getByTestId('nav-interfaces').click();
  await expect(page.getByTestId('iface-card-tun0')).toBeVisible();
  await expect(page.getByTestId('iface-card-wlp3s0')).toHaveCount(0);

  // The topology view has no imported document yet: an explicit notice,
  // never blank and never silently-substituted mock data.
  await page.getByTestId('nav-topology').click();
  await expect(page.getByTestId('missing-topology-notice')).toBeVisible();
  await expect(page.getByTestId('data-provenance')).toContainText('topology');

  // Import the topology document too; the view renders it.
  await page.getByTestId('import-file-input').setInputFiles({
    name: 'real-topology.json',
    mimeType: 'application/json',
    buffer: Buffer.from(JSON.stringify(workstationTopology)),
  });
  await expect(page.getByTestId('missing-topology-notice')).toHaveCount(0);
  const canvas = page.getByTestId('topology-canvas');
  await expect(canvas).toBeVisible();

  // An invalid import is rejected with a visible reason and leaves the
  // valid imported state intact.
  await page.getByTestId('import-file-input').setInputFiles({
    name: 'broken.json',
    mimeType: 'application/json',
    buffer: Buffer.from('{"schemaVersion": 2, "interfaces": "not-an-array"}'),
  });
  const error = page.getByTestId('import-error');
  await expect(error).toBeVisible();
  await expect(error).toContainText('broken.json');
  await expect(page.getByTestId('topology-canvas')).toBeVisible();

  // Returning to the demonstration data is explicit and works.
  await page.getByTestId('nav-return-mock').click();
  await expect(page.getByTestId('mock-badge')).toBeVisible();
  await page.getByTestId('nav-interfaces').click();
  await expect(page.getByTestId('iface-card-wlp3s0')).toBeVisible();
});

test('import workflow: an unsupported schema version is rejected with the reason', async ({
  page,
}) => {
  await page.goto('/');
  const bad = { ...(workstationSnapshot as object), schemaVersion: 99 };
  await page.getByTestId('import-file-input').setInputFiles({
    name: 'future.json',
    mimeType: 'application/json',
    buffer: Buffer.from(JSON.stringify(bad)),
  });
  await expect(page.getByTestId('import-error')).toContainText('schema version');
  // Still mock mode; nothing was partially applied.
  await expect(page.getByTestId('mock-badge')).toBeVisible();
});

// WO-02 §4/§9.6 (still current after WO-03 M3): a route whose observation
// carries caveats is marked in the table and explained in the details —
// exercised through the import workflow with the procfs-only fixture,
// whose routes carry implied-table notes.
test('import workflow: noted observations are marked and explained in the routes view', async ({
  page,
}) => {
  await page.goto('/');
  await page.getByTestId('import-file-input').setInputFiles({
    name: 'procfs-run.json',
    mimeType: 'application/json',
    buffer: Buffer.from(JSON.stringify(incompleteSnapshot)),
  });
  await expect(page.getByTestId('imported-badge')).toBeVisible();

  await page.getByTestId('nav-routes').click();
  const rows = page.getByTestId('routes-table').locator('tbody tr');
  // 3 procfs v4 routes + 3 procfs v6 routes (WO-03 M4).
  await expect(rows).toHaveCount(6);

  // Every route carries a provider caveat (v4: implied table; v6: file
  // limits) and the marker.
  const marked = rows.filter({ has: page.locator('.note-badge') });
  await expect(marked).toHaveCount(6);

  await marked.first().click();
  const panel = page.getByTestId('route-details');
  await expect(panel).toBeVisible();
  await expect(panel.getByTestId('route-limitation-warning')).toBeVisible();
  await expect(panel.getByTestId('route-notes')).toContainText(
    /table implied: \/proc\/net\/route exposes only the main table/,
  );

  // An IPv6 route from the same file shows its own caveat: the table is
  // not observed (the file has no table column) — displayed as — with the
  // explanation in the notes.
  const v6row = rows.filter({ hasText: 'fe80::/64' });
  await expect(v6row).toHaveCount(1);
  await v6row.click();
  await expect(panel.getByTestId('route-notes')).toContainText(
    /no routing-table id/,
  );
  const tableCell = v6row.locator('td').last();
  await expect(tableCell).toHaveText('—', { useInnerText: true });
});

test('import workflow: a selected route never survives as stale details (WO-03-F / AUD-09)', async ({
  page,
}) => {
  await page.goto('/');

  // Select a route from the mock workstation snapshot and open its
  // details panel.
  await page.getByTestId('nav-routes').click();
  const firstRow = page.locator('[data-testid="route-row"]').first();
  await firstRow.click();
  const details = page.getByTestId('route-details');
  await expect(details).toBeVisible();

  // Import a snapshot whose routes differ from the selected one (the
  // server-tunnel fixture observes different routes).
  await page.getByTestId('import-file-input').setInputFiles({
    name: 'other-observation.json',
    mimeType: 'application/json',
    buffer: Buffer.from(JSON.stringify(serverTunnelSnapshot)),
  });
  await expect(page.getByTestId('imported-badge')).toBeVisible();

  // The stale selection is gone: the details panel of a route that is not
  // part of the new observation must never keep rendering.
  await expect(details).toHaveCount(0);

  // Selecting again from the new document works normally.
  const newFirstRow = page.locator('[data-testid="route-row"]').first();
  if ((await newFirstRow.count()) > 0) {
    await newFirstRow.click();
    await expect(page.getByTestId('route-details')).toBeVisible();
  }

  // Import the ORIGINAL workstation snapshot again and re-select a route;
  // importing the server-tunnel snapshot once more must clear it again
  // (the re-resolution happens on EVERY document change, not just once).
  await page.getByTestId('import-file-input').setInputFiles({
    name: 'workstation-again.json',
    mimeType: 'application/json',
    buffer: Buffer.from(JSON.stringify(workstationSnapshot)),
  });
  await expect(page.getByTestId('imported-badge')).toBeVisible();
  await page.locator('[data-testid="route-row"]').first().click();
  await expect(page.getByTestId('route-details')).toBeVisible();
  await page.getByTestId('import-file-input').setInputFiles({
    name: 'other-observation-2.json',
    mimeType: 'application/json',
    buffer: Buffer.from(JSON.stringify(serverTunnelSnapshot)),
  });
  await expect(page.getByTestId('route-details')).toHaveCount(0);
});

// WO-03-FC / AUD-01 closure: an unassigned netnsid (NETNSA_NSID_NOT_ASSIGNED,
// -1 — the kernel emits the ATTRIBUTE with value -1 when no id could be
// allocated for the peer's namespace) is a cross-namespace statement. The
// details panel must render it as "not assigned", never as a raw -1 that
// looks like a namespace id, and never as a resolved local peer.
test('import workflow: an unassigned netnsid renders as not assigned (WO-03-FC / AUD-01)', async ({
  page,
}) => {
  await page.goto('/?araiguma-test-hooks');

  // A minimal valid topology document with a veth whose peer carries an
  // unassigned netnsid — imported through the real validators.
  const unassignedNetnsidTopology = {
    schemaVersion: 3,
    sourceTimestampMs: 1732000000000,
    nodes: [
      {
        id: { interface: { ifindex: 6 } },
        label: 'veth0',
        nodeKind: 'interface',
        interfaceKind: 'veth',
        kindDetail: null,
        state: 'inactive',
        annotations: [
          'veth peer ifindex 7 lives in another network namespace (the kernel allocated no id for the peer namespace (netnsid not assigned)); the peer is not part of this observation — no relationship drawn',
        ],
        detail: {
          ifindex: 6,
          addresses: [],
          macAddress: 'absent',
          mtu: 1500,
          operState: 'down',
          adminState: 'down',
          masterIfindex: null,
          parentIfindex: null,
          peerIfindex: 7,
          linkNetnsid: -1,
          routesUsing: 0,
          defaultRoutes: 0,
          sources: ['netlink'],
          notes: [],
        },
      },
    ],
    edges: [],
  };

  await page.getByTestId('import-file-input').setInputFiles({
    name: 'unassigned-netnsid-topology.json',
    mimeType: 'application/json',
    buffer: Buffer.from(JSON.stringify(unassignedNetnsidTopology)),
  });

  // A topology-only import drives the topology view directly (the overview
  // has no snapshot and says so explicitly — never substituted mock data).
  await page.getByTestId('nav-topology').click();
  await expect(page.getByTestId('topology-canvas')).toBeVisible();
  await expect(page.getByTestId('missing-topology-notice')).toHaveCount(0);
  await expect(page.getByTestId('imported-badge')).toBeVisible();

  // Select the veth node through the read-only test hook and open the
  // details panel.
  await page.evaluate(() => {
    const hook = (
      window as unknown as {
        __araigumaTopology?: { selectNode: (label: string) => void };
      }
    ).__araigumaTopology;
    hook?.selectNode('veth0');
  });
  const panel = page.getByTestId('topology-details');
  await expect(panel).toBeVisible();

  // The peer line renders the cross-namespace fact with the unassigned id
  // spelled out — not a bare -1 that could be mistaken for a namespace id
  // (both the identity line and the annotation carry the wording).
  await expect(panel.getByText(/in another netns/)).toBeVisible();
  await expect(panel.getByText(/netnsid not assigned/).first()).toBeVisible();
  await expect(panel.getByText(/netnsid -1/)).toHaveCount(0);
});
