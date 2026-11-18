import { describe, expect, it } from 'vitest';
import { parseSnapshot } from '../src/lib/snapshot';
import { fixtures } from '../src/lib/fixtures';

describe('snapshot fixtures (the type-sync contract)', () => {
  it('parses the workstation fixture', () => {
    const snapshot = fixtures.workstation.snapshot;
    expect(snapshot.schemaVersion).toBe(2);
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
    expect(() => parseSnapshot({ schemaVersion: 3 })).toThrow(/schema version/);
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
    interfaces: [{ name: 'eth0', ifindex: 2, kind: 'ethernet', macAddress }],
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
