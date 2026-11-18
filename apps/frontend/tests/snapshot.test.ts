import { describe, expect, it } from 'vitest';
import { parseSnapshot } from '../src/lib/snapshot';
import { fixtures } from '../src/lib/fixtures';

describe('snapshot fixtures (the type-sync contract)', () => {
  it('parses the workstation fixture', () => {
    const snapshot = fixtures.workstation.snapshot;
    expect(snapshot.schemaVersion).toBe(3);
    // The design check depends on this card existing.
    expect(snapshot.interfaces.some((i) => i.name === 'wlp3s0')).toBe(true);
    // Multiple default routes across families are part of the scenario.
    const defaults = snapshot.routes.filter(
      (r) => r.destination !== null && r.destination.prefixLen === 0,
    );
    expect(defaults.length).toBeGreaterThanOrEqual(2);
    // Both providers report their state.
    expect(snapshot.providers.map((p) => p.provider).sort()).toEqual([
      'netlink',
      'sysfs',
    ]);
    // MAC observation (v2): a present address and an observed absence.
    const eth0 = snapshot.interfaces.find((i) => i.name === 'eth0');
    expect(eth0?.macAddress).toBe('52:54:00:12:34:56');
    const wg0 = snapshot.interfaces.find((i) => i.name === 'wg0');
    expect(wg0?.macAddress).toBe('absent');
  });

  it('parses the incomplete-observations fixture with nulls preserved', () => {
    const snapshot = fixtures['incomplete-observations'].snapshot;
    const netlink = snapshot.providers.find((p) => p.provider === 'netlink');
    expect(netlink?.state).toBe('unavailable');
    const sysfs = snapshot.providers.find((p) => p.provider === 'sysfs');
    expect(sysfs?.state).toBe('degraded');
    // Fields the procfs source cannot observe stay null — never fabricated.
    const defaultRoute = snapshot.routes.find(
      (r) => r.destination !== null && r.destination.prefixLen === 0,
    );
    expect(defaultRoute).toBeDefined();
    expect(defaultRoute?.protocol).toBeNull();
    expect(defaultRoute?.scope).toBeNull();
    expect(defaultRoute?.routeType).toBeNull();
    expect(defaultRoute?.table).toBe(254);
  });
});

describe('parseSnapshot rejects invalid input loudly', () => {
  it('rejects non-objects', () => {
    expect(() => parseSnapshot(null)).toThrow(/snapshot/);
    expect(() => parseSnapshot([])).toThrow();
    expect(() => parseSnapshot('nope')).toThrow();
  });

  it('rejects unknown schema versions', () => {
    expect(() => parseSnapshot({ schemaVersion: 4 })).toThrow(/schema version/);
    expect(() => parseSnapshot({ schemaVersion: '1' })).toThrow();
  });

  it('rejects wrong shapes with a path in the message', () => {
    expect(() =>
      parseSnapshot({
        schemaVersion: 2,
        timestampMs: 0,
        interfaces: {},
        addresses: [],
        routes: [],
        providers: [],
      }),
    ).toThrow(/\$.interfaces/);
    expect(() =>
      parseSnapshot({
        schemaVersion: 2,
        timestampMs: 0,
        interfaces: [{ name: 'x', ifindex: 1, kind: 'spaceship' }],
        addresses: [],
        routes: [],
        providers: [],
      }),
    ).toThrow(/kind/);
  });

  it('rejects unknown provider ids and states', () => {
    const base = {
      schemaVersion: 2,
      timestampMs: 0,
      interfaces: [],
      addresses: [],
      routes: [],
    };
    expect(() =>
      parseSnapshot({
        ...base,
        providers: [{ provider: 'avahi', state: 'available', sections: [], notes: [] }],
      }),
    ).toThrow(/provider/);
    expect(() =>
      parseSnapshot({
        ...base,
        providers: [{ provider: 'netlink', state: 'fine', sections: [], notes: [] }],
      }),
    ).toThrow(/state/);
  });
});

describe('MAC observation validation (schema v2)', () => {
  const base = (macAddress: unknown) => ({
    schemaVersion: 2,
    timestampMs: 0,
    interfaces: [
      { name: 'eth0', ifindex: 2, kind: 'ethernet', macAddress, sources: ['netlink'], notes: [] },
    ],
    addresses: [],
    routes: [],
    providers: [],
  });

  it('accepts hardware addresses and both sentinels', () => {
    expect(() => parseSnapshot(base('52:54:00:12:34:56'))).not.toThrow();
    expect(() => parseSnapshot(base('absent'))).not.toThrow();
    expect(() => parseSnapshot(base('notObserved'))).not.toThrow();
    // 20-byte IPoIB-style hardware addresses are valid.
    expect(() =>
      parseSnapshot(
        base('00:11:22:33:44:55:66:77:88:99:aa:bb:cc:dd:ee:ff:00:11:22:33'),
      ),
    ).not.toThrow();
  });

  it('rejects null with the ambiguity explanation', () => {
    expect(() => parseSnapshot(base(null))).toThrow(/ambiguous/);
  });

  it('rejects malformed hardware addresses', () => {
    expect(() => parseSnapshot(base('zzz'))).toThrow(/hardware address/);
    expect(() => parseSnapshot(base('aa-bb-cc-dd-ee-ff'))).toThrow(/hardware address/);
    expect(() => parseSnapshot(base(123))).toThrow();
  });

  it('accepts unambiguous v1 documents and rejects ambiguous ones', () => {
    // v1 with a string macAddress everywhere: unambiguous, accepted.
    const v1 = base('52:54:00:12:34:56') as Record<string, unknown>;
    v1.schemaVersion = 1;
    expect(() => parseSnapshot(v1)).not.toThrow();
    // v1 with a null macAddress: ambiguous, rejected with the reason.
    const v1null = base(null) as Record<string, unknown>;
    v1null.schemaVersion = 1;
    expect(() => parseSnapshot(v1null)).toThrow(/ambiguous/);
  });
});

describe('next-hop representation (WO-03 M3, schema v3)', () => {
  // A minimal single-path route; nextHops is applied per test.
  const route = (nextHops: unknown) => ({
    family: 'ipv4',
    destination: { address: '10.9.0.0', prefixLen: 24 },
    gateway: null,
    outputInterface: null,
    outputInterfaceName: null,
    protocol: 'kernel',
    scope: 'link',
    routeType: 'unicast',
    metric: 0,
    table: 254,
    flags: null,
    nextHops,
    sources: ['netlink'],
    notes: [],
  });
  const doc = (nextHops: unknown) => ({
    schemaVersion: 3,
    timestampMs: 0,
    interfaces: [],
    addresses: [],
    routes: [route(nextHops)],
    providers: [],
  });

  it('parses the explicit multipath representation with every hop', () => {
    const snapshot = parseSnapshot(
      doc({
        multi: [
          { gateway: '10.0.0.1', outputInterface: 2, outputInterfaceName: 'eth0', weight: 2, flags: ['onlink'] },
          { gateway: '10.1.0.1', outputInterface: 3, outputInterfaceName: 'ar-mp1', weight: 3, flags: [] },
        ],
      }),
    );
    const hops = snapshot.routes[0].nextHops;
    expect(typeof hops).toBe('object');
    expect(hops).toEqual({
      multi: [
        { gateway: '10.0.0.1', outputInterface: 2, outputInterfaceName: 'eth0', weight: 2, flags: ['onlink'] },
        { gateway: '10.1.0.1', outputInterface: 3, outputInterfaceName: 'ar-mp1', weight: 3, flags: [] },
      ],
    });
  });

  it('normalizes pre-M3 documents (absent nextHops) to single', () => {
    // A v2 document and an M2-era v3 document both lack nextHops — the
    // loader must fill 'single' exactly like the Rust serde default, so
    // consumers can branch on the field without a null check.
    for (const schemaVersion of [2, 3]) {
      const legacy = doc(undefined) as Record<string, unknown>;
      // Remove the field entirely (undefined would be dropped by JSON anyway).
      delete (legacy.routes as Record<string, unknown>[])[0].nextHops;
      legacy.schemaVersion = schemaVersion;
      const snapshot = parseSnapshot(legacy);
      expect(snapshot.routes[0].nextHops).toBe('single');
    }
  });

  it('accepts explicit single and undecodable markers', () => {
    expect(() => parseSnapshot(doc('single'))).not.toThrow();
    const undecoded = parseSnapshot(
      doc({ undecoded: 'RTA_MULTIPATH attribute carried no next hops' }),
    );
    expect(undecoded.routes[0].nextHops).toEqual({
      undecoded: 'RTA_MULTIPATH attribute carried no next hops',
    });
  });

  it('rejects malformed next-hop representations with the path', () => {
    // null must not silently become 'single' (parity with serde's
    // absent-only default).
    expect(() => parseSnapshot(doc(null))).toThrow(
      /\$\.routes\[0\]\.nextHops: must not be null/,
    );
    // Unknown plain-string variant.
    expect(() => parseSnapshot(doc('multi'))).toThrow(/nextHops/);
    // Empty hop list: not producible by the writer.
    expect(() => parseSnapshot(doc({ multi: [] }))).toThrow(
      /nextHops\.multi: an empty hop list is not producible/,
    );
    // Hop gateway of the wrong family.
    expect(() =>
      parseSnapshot(doc({ multi: [{ gateway: 'fe80::1', outputInterface: 2, weight: 1, flags: [] }] })),
    ).toThrow(/expected an ipv4 address/);
    // Weight below the kernel range (rtnh_hops + 1 >= 1) and above (u8).
    expect(() =>
      parseSnapshot(doc({ multi: [{ gateway: '10.0.0.1', outputInterface: 2, weight: 0, flags: [] }] })),
    ).toThrow(/weight/);
    expect(() =>
      parseSnapshot(doc({ multi: [{ gateway: '10.0.0.1', outputInterface: 2, weight: 257, flags: [] }] })),
    ).toThrow(/weight/);
    // Unknown variant object.
    expect(() => parseSnapshot(doc({ flurb: 1 }))).toThrow(
      /expected 'single', \{ multi/,
    );
    // A hop that is not an object.
    expect(() => parseSnapshot(doc({ multi: ['eth0'] }))).toThrow(
      /nextHops\.multi\[0\]: expected an object/,
    );
  });
});
