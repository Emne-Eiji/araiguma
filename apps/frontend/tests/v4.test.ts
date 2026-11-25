import { describe, expect, it } from 'vitest';
import { parseSnapshot } from '../src/lib/snapshot';
import type { NetworkSnapshot } from '../src/types';
import workstation from '../../../fixtures/snapshots/workstation.json';

const v4Base = () => ({
  schemaVersion: 4,
  timestampMs: 0,
  interfaces: [],
  addresses: [],
  routes: [],
  providers: [],
});

describe('schema v4 snapshot parsing', () => {
  it('accepts a v3 document unchanged', () => {
    const doc = structuredClone(workstation) as unknown as Record<string, unknown>;
    doc.schemaVersion = 3;
    expect(() => parseSnapshot(doc)).not.toThrow();
  });

  it('parses DNS and connection observations', () => {
    const doc = {
      ...v4Base(),
      dns: [
        {
          scope: { kind: 'global' },
          mode: 'stub',
          servers: [{ address: '192.168.1.1', port: 53 }],
          searchDomains: ['example.com'],
          routingDomains: [],
          sources: ['systemdResolved'],
          notes: ['derivedFromManagerAggregate'],
        },
        {
          scope: { kind: 'link', ifindex: 2 },
          servers: [],
          searchDomains: [],
          routingDomains: ['corp.example'],
          sources: ['systemdResolved'],
          notes: [],
        },
      ],
      connections: [
        {
          connectionType: '802-3-ethernet',
          state: 'activated',
          devices: [2],
          vpn: false,
          sources: ['networkManager'],
          notes: [],
        },
      ],
    };

    const parsed = parseSnapshot(doc) as NetworkSnapshot;
    expect(parsed.schemaVersion).toBe(4);
    expect(parsed.dns).toHaveLength(2);
    expect(parsed.dns?.[1].scope).toEqual({ kind: 'link', ifindex: 2 });
    expect(parsed.connections?.[0].connectionType).toBe('802-3-ethernet');
  });

  it('distinguishes absent, empty, and rejects null', () => {
    const absent = parseSnapshot(v4Base()) as NetworkSnapshot;
    expect(absent.dns).toBeUndefined();
    expect(absent.connections).toBeUndefined();

    const empty = parseSnapshot({ ...v4Base(), dns: [], connections: [] }) as NetworkSnapshot;
    expect(empty.dns).toEqual([]);
    expect(empty.connections).toEqual([]);

    expect(() => parseSnapshot({ ...v4Base(), dns: null })).toThrow(
      /must be an array, not null/,
    );
    expect(() => parseSnapshot({ ...v4Base(), connections: null })).toThrow(
      /must be an array, not null/,
    );
  });

  it('preserves unknown non-empty identities', () => {
    const doc = {
      ...v4Base(),
      providers: [
        {
          provider: 'futureProvider',
          state: 'available',
          sections: [
            { section: 'futureSection', outcome: { kind: 'observed', count: 0 } },
          ],
          notes: [],
        },
      ],
      dns: [
        {
          scope: { kind: 'global' },
          servers: [],
          searchDomains: [],
          routingDomains: [],
          sources: ['futureSource'],
          notes: [],
        },
      ],
    };

    const parsed = parseSnapshot(doc) as NetworkSnapshot;
    expect(parsed.providers[0].provider).toBe('futureProvider');
    expect(parsed.providers[0].sections[0].section).toBe('futureSection');
    expect(parsed.dns?.[0].sources[0]).toBe('futureSource');
  });

  it('accepts the new section outcomes', () => {
    const doc = {
      ...v4Base(),
      providers: [
        {
          provider: 'systemdResolved',
          state: 'unavailable',
          sections: [{ section: 'dns', outcome: { kind: 'unavailable' } }],
          notes: [],
        },
        {
          provider: 'networkManager',
          state: 'degraded',
          sections: [{ section: 'connections', outcome: { kind: 'permissionDenied' } }],
          notes: [],
        },
      ],
    };
    expect(() => parseSnapshot(doc)).not.toThrow();
  });

  it('rejects malformed DNS and connection shapes', () => {
    expect(() =>
      parseSnapshot({
        ...v4Base(),
        dns: [{ scope: { kind: 'link' }, servers: [] }],
      }),
    ).toThrow(/ifindex/);
    expect(() =>
      parseSnapshot({
        ...v4Base(),
        dns: [
          {
            scope: { kind: 'global' },
            servers: [{ address: 'nope' }],
            searchDomains: [],
            routingDomains: [],
            sources: ['netlink'],
            notes: [],
          },
        ],
      }),
    ).toThrow(/address/);
    expect(() =>
      parseSnapshot({
        ...v4Base(),
        connections: [
          {
            connectionType: 'vpn',
            state: 'activated',
            devices: [2],
            vpn: false,
            sources: [],
            notes: [],
          },
        ],
      }),
    ).toThrow(/connections\[0\]\.sources/);
  });
});

describe('schema v4 DNS mode validation', () => {
  const withMode = (mode: unknown) => ({
    ...v4Base(),
    dns: [
      {
        scope: { kind: 'global' },
        mode,
        servers: [],
        searchDomains: [],
        routingDomains: [],
        sources: ['systemdResolved'],
        notes: [],
      },
    ],
  });

  it('accepts known, unknown, and slash-bearing mode tokens', () => {
    for (const mode of ['stub', 'futureMode', 'n/a']) {
      expect(() => parseSnapshot(withMode(mode))).not.toThrow();
    }
  });

  it('treats absent and null mode as not observed', () => {
    const absent = withMode(undefined);
    delete (absent.dns[0] as Record<string, unknown>).mode;
    expect(() => parseSnapshot(absent)).not.toThrow();
    expect(() => parseSnapshot(withMode(null))).not.toThrow();
  });

  it('rejects empty and whitespace/control-bearing mode values', () => {
    for (const mode of ['', '.', '///', 'bad mode', 'org.freedesktop.DBus.Error.AccessDenied: no such file', 'bad\nmode']) {
      expect(() => parseSnapshot(withMode(mode))).toThrow(/mode/);
    }
  });
});
