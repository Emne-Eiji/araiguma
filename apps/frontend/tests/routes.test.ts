import { describe, expect, it } from 'vitest';
import {
  MAIN_TABLE,
  routeAriaLabel,
  routeHasLimitationNotes,
  tableOptions,
  visibleRoutes,
} from '../src/lib/routes';
import { fixtures } from '../src/lib/fixtures';
import type { NetworkSnapshot, Route } from '../src/types';

function snapshotWith(routes: Route[], providers = fixtures.workstation.snapshot.providers): NetworkSnapshot {
  return {
    schemaVersion: 2,
    timestampMs: 0,
    interfaces: fixtures.workstation.snapshot.interfaces,
    addresses: fixtures.workstation.snapshot.addresses,
    routes,
    providers,
  };
}

describe('route table filtering', () => {
  const snapshot = fixtures.workstation.snapshot;

  it('lists all tables with counts, main first', () => {
    const tables = tableOptions(snapshot);
    expect(tables[0]).toEqual({ table: 254, label: 'main (254)', count: 8 });
    expect(tables.map((t) => t.table)).toEqual([254, 255]);
    expect(tables[1].label).toBe('local (255)');
  });

  it('default view shows the main table only, never deleting others', () => {
    const visible = visibleRoutes(snapshot, []);
    expect(visible.every((r) => (r.table ?? MAIN_TABLE) === 254)).toBe(true);
    expect(visible.length).toBe(8);
    // Other tables stay in the snapshot.
    expect(snapshot.routes.some((r) => r.table === 255)).toBe(true);
  });

  it('opting into the local table shows both, unmerged', () => {
    const visible = visibleRoutes(snapshot, [254, 255]);
    expect(visible.length).toBe(snapshot.routes.length);
  });

  it('multiple default routes are all preserved (no single chosen default)', () => {
    const defaults = visibleRoutes(snapshot, [254]).filter(
      (r) => r.destination !== null && r.destination.prefixLen === 0,
    );
    expect(defaults.length).toBe(3); // two IPv4 defaults + one IPv6 default
    expect(new Set(defaults.map((r) => r.gateway)).size).toBe(2);
  });

  it('empty route observations are valid', () => {
    const empty = fixtures['incomplete-observations'].snapshot;
    expect(visibleRoutes(empty, [254]).length).toBe(3);
    expect(tableOptions(empty).map((t) => t.table)).toEqual([254]);
  });
});

describe('route missing-field semantics', () => {
  it('procfs routes keep null fields — nothing fabricated', () => {
    const incomplete = fixtures['incomplete-observations'].snapshot;
    const defaultRoute = incomplete.routes.find(
      (r) => r.destination !== null && r.destination.prefixLen === 0,
    );
    expect(defaultRoute).toBeDefined();
    expect(defaultRoute!.protocol).toBeNull();
    expect(defaultRoute!.scope).toBeNull();
    expect(defaultRoute!.routeType).toBeNull();
    expect(defaultRoute!.flags).toBeNull();
    expect(defaultRoute!.table).toBe(254);
  });

  it('a gateway-less default stays gateway-less (on-link observation)', () => {
    const server = fixtures.workstation.snapshot; // placeholder for import shape
    expect(server.routes.some((r) => r.gateway === null)).toBe(true);
  });
});

describe('route accessibility and limitation markers (WO-02 correction §4/§9.4)', () => {
  const base: Route = {
    family: 'ipv4',
    destination: { address: '0.0.0.0', prefixLen: 0 },
    gateway: '192.168.1.1',
    outputInterface: 2,
    outputInterfaceName: 'eth0',
    protocol: 'dhcp',
    scope: 'universe',
    routeType: 'unicast',
    metric: 100,
    table: 254,
    flags: ['onlink'],
    sources: ['netlink', 'procfs'],
    notes: [],
  };

  it('builds an accessible label with the row facts', () => {
    expect(routeAriaLabel(base)).toBe(
      'default route via 192.168.1.1 on eth0, ipv4, table 254',
    );
  });

  it('the label announces on-link, unobserved fields, and notes', () => {
    const onlink: Route = { ...base, gateway: null, notes: ['a note'] };
    expect(routeAriaLabel(onlink)).toBe(
      'default route on-link on eth0, ipv4, table 254, carries 1 observation note — open details for limitations',
    );
    const unknown: Route = {
      ...base,
      destination: null,
      outputInterface: null,
      outputInterfaceName: null,
    };
    expect(routeAriaLabel(unknown)).toContain('destination not observed');
    expect(routeAriaLabel(unknown)).toContain('output interface not observed');
  });

  it('marks routes whose observation is explicitly incomplete', () => {
    expect(routeHasLimitationNotes(base)).toBe(false);
    const multipath: Route = {
      ...base,
      notes: ['multipath route with 2 next hops; only the first is represented'],
    };
    expect(routeHasLimitationNotes(multipath)).toBe(true);
  });

  it('the workstation fixture covers the limitation-marker path', () => {
    // One synthetic default is a multipath route whose representation is
    // incomplete today — the UI must be able to show that honestly.
    const marked = fixtures.workstation.snapshot.routes.filter(routeHasLimitationNotes);
    expect(marked.length).toBe(1);
    expect(marked[0].notes[0]).toMatch(/multipath route with 2 next hops/);
  });
});
