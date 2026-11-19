import { describe, expect, it } from 'vitest';
import {
  MAIN_TABLE,
  hopCount,
  isMultipath,
  routeAriaLabel,
  routeHasLimitationNotes,
  routeKey,
  tableOptions,
  undecodedHopsReason,
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
    // 3 procfs v4 + 3 procfs v6 (WO-03 M4): the file-spanning-tables
    // routes with table null group with main for display.
    expect(visibleRoutes(empty, [254]).length).toBe(6);
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
    via: null,
    destination: { address: '0.0.0.0', prefixLen: 0 },
    gateway: '192.168.1.1',
    outputInterface: 2,
    outputInterfaceName: 'eth0',
    protocol: 'dhcp',
    scope: 'universe',
    routeType: 'unicast',
    metric: 100,
    table: 254,
    tos: null,
    flags: ['onlink'],
    nextHops: 'single',
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

  it('marks routes whose observation carries caveats', () => {
    expect(routeHasLimitationNotes(base)).toBe(false);
    const procfs: Route = {
      ...base,
      notes: ['table implied: /proc/net/route exposes only the main table (254)'],
    };
    expect(routeHasLimitationNotes(procfs)).toBe(true);
  });

  it('the incomplete-observations fixture covers the note-marker path', () => {
    // The procfs-only scenario carries implied-table notes on v4 and
    // file-limitation notes on v6 (WO-03 M4) — the UI must be able to show
    // provider caveats honestly from the table itself.
    const marked = fixtures['incomplete-observations'].snapshot.routes.filter(routeHasLimitationNotes);
    expect(marked.length).toBe(6);
    expect(marked[0].notes[0]).toMatch(/table implied/);
    expect(
      marked.some((r) => r.notes.some((n) => n.includes('no routing-table id'))),
    ).toBe(true);
  });
});

describe('multipath representation (WO-03 M3)', () => {
  const single: Route = {
    family: 'ipv4',
    via: null,
    destination: { address: '10.0.0.0', prefixLen: 24 },
    gateway: '10.0.0.1',
    outputInterface: 2,
    outputInterfaceName: 'eth0',
    protocol: 'static',
    scope: 'link',
    routeType: 'unicast',
    metric: 0,
    table: 254,
    tos: null,
    flags: null,
    nextHops: 'single',
    sources: ['netlink'],
    notes: [],
  };

  const multi: Route = {
    ...single,
    gateway: null,
    outputInterface: null,
    outputInterfaceName: null,
    nextHops: {
      multi: [
        { gateway: '10.0.0.1', via: null, outputInterface: 2, outputInterfaceName: 'eth0', weight: 2, flags: ['onlink'] },
        { gateway: '10.0.1.1', via: null, outputInterface: 3, outputInterfaceName: 'wlp3s0', weight: 1, flags: [] },
      ],
    },
  };

  const undecoded: Route = {
    ...single,
    gateway: null,
    outputInterface: null,
    outputInterfaceName: null,
    nextHops: { undecoded: 'RTA_MULTIPATH attribute carried no next hops' },
  };

  it('classifies single, multi, and undecoded representations', () => {
    expect(isMultipath(single)).toBe(false);
    expect(isMultipath(multi)).toBe(true);
    expect(isMultipath(undecoded)).toBe(false);
    expect(hopCount(single)).toBe(1);
    expect(hopCount(multi)).toBe(2);
    expect(hopCount(undecoded)).toBe(1); // no hop list observed
    expect(undecodedHopsReason(single)).toBeNull();
    expect(undecodedHopsReason(multi)).toBeNull();
    expect(undecodedHopsReason(undecoded)).toBe(
      'RTA_MULTIPATH attribute carried no next hops',
    );
  });

  it('the aria label announces the hop count, never a false on-link', () => {
    // A multipath route has null route-level gateway BY DESIGN (the hops
    // carry the gateways) — the label must not read it as on-link.
    expect(routeAriaLabel(multi)).toBe(
      'route 10.0.0.0/24 multipath with 2 next hops output interface not observed, ipv4, table 254',
    );
    expect(routeAriaLabel(multi)).not.toContain('on-link');
    expect(routeAriaLabel(undecoded)).toContain('next hops not decodable');
    // Single-path on-link semantics are unchanged.
    const onlink: Route = { ...single, gateway: null };
    expect(routeAriaLabel(onlink)).toContain('on-link');
  });

  it('keys distinguish a multipath route from an on-link route to the same prefix', () => {
    const onlink: Route = { ...single, gateway: null, metric: 0 };
    const keys = new Set([routeKey(onlink, 0), routeKey(multi, 1), routeKey(undecoded, 2)]);
    expect(keys.size).toBe(3);
  });

  it('the workstation fixture carries a real multipath route with all hops', () => {
    const mp = fixtures.workstation.snapshot.routes.find(isMultipath);
    expect(mp).toBeDefined();
    expect(hopCount(mp!)).toBe(2);
    const hops = (mp!.nextHops as { multi: { gateway: string | null; outputInterfaceName: string | null; weight: number | null }[] }).multi;
    expect(hops.map((h) => h.gateway)).toEqual(['198.51.100.1', '198.51.100.1']);
    expect(hops.map((h) => h.outputInterfaceName)).toEqual(['eth0', 'wlp3s0']);
    expect(hops.map((h) => h.weight)).toEqual([1, 2]);
    // Route-level fields stay null — the hops carry them, never flattened.
    expect(mp!.gateway).toBeNull();
    expect(mp!.outputInterface).toBeNull();
  });
});
