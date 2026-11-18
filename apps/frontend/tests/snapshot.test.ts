import { describe, expect, it } from 'vitest';
import { parseSnapshot } from '../src/lib/snapshot';
import { fixtures } from '../src/lib/fixtures';

describe('snapshot fixtures (the type-sync contract)', () => {
  it('parses the workstation fixture', () => {
    const snapshot = fixtures.workstation.snapshot;
    expect(snapshot.schemaVersion).toBe(1);
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
    expect(() => parseSnapshot({ schemaVersion: 2 })).toThrow(/schema version/);
    expect(() => parseSnapshot({ schemaVersion: '1' })).toThrow();
  });

  it('rejects wrong shapes with a path in the message', () => {
    expect(() =>
      parseSnapshot({ schemaVersion: 1, timestampMs: 0, interfaces: {}, addresses: [], routes: [], providers: [] }),
    ).toThrow(/\$.interfaces/);
    expect(() =>
      parseSnapshot({
        schemaVersion: 1,
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
      schemaVersion: 1,
      timestampMs: 0,
      interfaces: [],
      addresses: [],
      routes: [],
    };
    expect(() =>
      parseSnapshot({ ...base, providers: [{ provider: 'avahi', state: 'available', sections: [], notes: [] }] }),
    ).toThrow(/provider/);
    expect(() =>
      parseSnapshot({ ...base, providers: [{ provider: 'netlink', state: 'fine', sections: [], notes: [] }] }),
    ).toThrow(/state/);
  });
});
