import { describe, expect, it } from 'vitest';
import { parseSnapshot } from '../src/lib/snapshot';
import { parseTopology } from '../src/lib/topology';
import type { NetworkSnapshot } from '../src/types';
import workstation from '../../../fixtures/snapshots/workstation.json';
import { isIpv6Address } from '../src/lib/validate';

// WO-02 correction §8: the document boundary is validated completely.
// These tests hammer both loaders with malformed documents of every
// required category; each must be rejected with a path-qualified reason.

const validSnapshot = workstation as unknown as NetworkSnapshot;

function snapshotWith(mutate: (doc: Record<string, unknown>) => void): unknown {
  const doc = structuredClone(validSnapshot) as unknown as Record<string, unknown>;
  mutate(doc);
  return doc;
}

describe('snapshot validation: required fields and shapes', () => {
  it('accepts the committed fixture as-is (positive control)', () => {
    expect(() => parseSnapshot(validSnapshot)).not.toThrow();
  });

  it('rejects missing required document fields', () => {
    expect(() => parseSnapshot({ timestampMs: 0, interfaces: [] })).toThrow(/schemaVersion/);
    expect(() =>
      parseSnapshot({
        schemaVersion: 2,
        interfaces: [],
        addresses: [],
        routes: [],
        providers: [],
      }),
    ).toThrow(/timestampMs/);
    expect(() => parseSnapshot(snapshotWith((d) => delete d.routes))).toThrow(/\$.routes/);
    expect(() => parseSnapshot(snapshotWith((d) => delete d.providers))).toThrow(/\$.providers/);
  });

  it('rejects non-integer or negative numerics', () => {
    expect(() => parseSnapshot(snapshotWith((d) => (d.timestampMs = -1)))).toThrow(/timestampMs/);
    expect(() => parseSnapshot(snapshotWith((d) => (d.timestampMs = 1.5)))).toThrow(/timestampMs/);
    expect(() =>
      parseSnapshot(
        snapshotWith((d) => ((d.interfaces as Record<string, unknown>[])[0].ifindex = 2.5)),
      ),
    ).toThrow(/ifindex/);
    expect(() =>
      parseSnapshot(
        snapshotWith((d) => ((d.interfaces as Record<string, unknown>[])[0].ifindex = 0)),
      ),
    ).toThrow(/ifindex/);
    expect(() =>
      parseSnapshot(
        snapshotWith((d) => ((d.routes as Record<string, unknown>[])[0].metric = -5)),
      ),
    ).toThrow(/metric/);
  });

  it('rejects unknown discriminants across the model', () => {
    expect(() =>
      parseSnapshot(
        snapshotWith((d) => ((d.interfaces as Record<string, unknown>[])[0].kind = 'spaceship')),
      ),
    ).toThrow(/kind/);
    expect(() =>
      parseSnapshot(
        snapshotWith((d) => ((d.interfaces as Record<string, unknown>[])[0].operState = 'sleepy')),
      ),
    ).toThrow(/operState/);
    expect(() =>
      parseSnapshot(
        snapshotWith((d) => ((d.addresses as Record<string, unknown>[])[0].scope = 'galactic')),
      ),
    ).toThrow(/scope/);
    expect(() =>
      parseSnapshot(
        snapshotWith((d) => ((d.routes as Record<string, unknown>[])[0].protocol = 'bgp')),
      ),
    ).toThrow(/protocol/);
    expect(() =>
      parseSnapshot(
        snapshotWith((d) => ((d.providers as Record<string, unknown>[])[0].state = 'fine')),
      ),
    ).toThrow(/state/);
  });

  it('rejects invalid prefixes and addresses', () => {
    expect(() =>
      parseSnapshot(snapshotWith((d) => ((d.addresses as Record<string, unknown>[])[0].prefixLen = 33))),
    ).toThrow(/prefixLen/);
    // Deleting an optional destination is fine.
    expect(() =>
      parseSnapshot(
        snapshotWith((d) => delete (d.routes as Record<string, unknown>[])[0].destination),
      ),
    ).not.toThrow();
    expect(() =>
      parseSnapshot(
        snapshotWith(
          (d) =>
            ((d.routes as Record<string, unknown>[])[0].destination = {
              address: '::1',
              prefixLen: 129,
            }),
        ),
      ),
    ).toThrow(/destination/);
    expect(() =>
      parseSnapshot(
        snapshotWith((d) => ((d.addresses as Record<string, unknown>[])[0].address = '999.1.1.1')),
      ),
    ).toThrow(/address/);
    expect(() =>
      parseSnapshot(
        snapshotWith((d) => ((d.addresses as Record<string, unknown>[])[0].address = 'fe80:::1')),
      ),
    ).toThrow(/address/);
    // Cross-family: an ipv4 route cannot have an ipv6 gateway.
    expect(() =>
      parseSnapshot(
        snapshotWith((d) => ((d.routes as Record<string, unknown>[])[0].gateway = 'fe80::1')),
      ),
    ).toThrow(/ipv4/);
  });

  it('rejects malformed nested objects', () => {
    expect(() =>
      parseSnapshot(
        snapshotWith(
          (d) => ((d.routes as Record<string, unknown>[])[0].destination = '0.0.0.0/0'),
        ),
      ),
    ).toThrow(/destination/);
    expect(() =>
      parseSnapshot(
        snapshotWith((d) => ((d.interfaces as Record<string, unknown>[])[0].sources = 'netlink')),
      ),
    ).toThrow(/sources/);
    expect(() =>
      parseSnapshot(
        snapshotWith((d) => ((d.interfaces as Record<string, unknown>[])[0].notes = [42])),
      ),
    ).toThrow(/notes/);
    expect(() => parseSnapshot(snapshotWith((d) => (d.addresses = {})))).toThrow(/\$.addresses/);
  });

  it('rejects invalid provider sections and outcomes', () => {
    const provider = (doc: Record<string, unknown>, i: number) =>
      (doc.providers as Record<string, unknown>[])[i];
    expect(() =>
      parseSnapshot(
        snapshotWith(
          (d) =>
            (provider(d, 0).sections = [
              { section: 'telepathy', outcome: { kind: 'observed', count: 0 } },
            ]),
        ),
      ),
    ).toThrow(/section/);
    expect(() =>
      parseSnapshot(
        snapshotWith(
          (d) => (provider(d, 0).sections = [{ section: 'interfaces', outcome: { kind: 'observed' } }]),
        ),
      ),
    ).toThrow(/count/);
    expect(() =>
      parseSnapshot(
        snapshotWith(
          (d) =>
            (provider(d, 0).sections = [
              { section: 'interfaces', outcome: { kind: 'observed', count: -1 } },
            ]),
        ),
      ),
    ).toThrow(/count/);
    expect(() =>
      parseSnapshot(
        snapshotWith(
          (d) => (provider(d, 0).sections = [{ section: 'interfaces', outcome: { kind: 'failed' } }]),
        ),
      ),
    ).toThrow(/error/);
    expect(() =>
      parseSnapshot(
        snapshotWith(
          (d) => (provider(d, 0).sections = [{ section: 'interfaces', outcome: { kind: 'meh' } }]),
        ),
      ),
    ).toThrow(/outcome/);
  });

  it('accepts a minimal empty observation and rejects incomplete ones', () => {
    // Zero items is a valid observation.
    expect(() =>
      parseSnapshot({
        schemaVersion: 2,
        timestampMs: 0,
        interfaces: [],
        addresses: [],
        routes: [],
        providers: [],
      }),
    ).not.toThrow();
    // But the arrays themselves are required.
    expect(() =>
      parseSnapshot({ schemaVersion: 2, timestampMs: 0, interfaces: [] }),
    ).toThrow(/\$.addresses/);
  });
});

const baseNode = {
  id: { interface: { ifindex: 2 } },
  label: 'eth0',
  nodeKind: 'interface',
  interfaceKind: 'ethernet',
  kindDetail: null,
  state: 'active',
  annotations: [],
  detail: {
    ifindex: 2,
    addresses: [
      {
        ifindex: 2,
        family: 'ipv4',
        address: '192.168.1.42',
        prefixLen: 24,
        scope: 'global',
        flags: null,
        label: null,
        sources: ['netlink'],
      },
    ],
    macAddress: '52:54:00:12:34:56',
    mtu: 1500,
    operState: 'up',
    adminState: 'up',
    masterIfindex: null,
    parentIfindex: null,
    peerIfindex: null,
    linkNetnsid: null,
    routesUsing: 1,
    defaultRoutes: 1,
    sources: ['netlink'],
    notes: [],
  },
} as const;

const gatewayNode = {
  id: { gateway: { address: '192.168.1.1' } },
  label: '192.168.1.1',
  nodeKind: 'gateway',
  interfaceKind: null,
  kindDetail: null,
  state: 'unknown',
  annotations: [],
  detail: {
    ifindex: null,
    addresses: [],
    macAddress: 'notObserved',
    mtu: null,
    operState: null,
    adminState: null,
    masterIfindex: null,
    parentIfindex: null,
    routesUsing: 0,
    defaultRoutes: 0,
    sources: [],
    notes: [],
  },
} as const;

function topologyWith(nodes: unknown[], edges: unknown[]): unknown {
  return { schemaVersion: 2, sourceTimestampMs: 0, nodes, edges };
}

describe('topology validation: nodes, edges, references', () => {
  it('accepts a well-formed document (positive control)', () => {
    expect(() =>
      parseTopology(
        topologyWith(
          [baseNode, gatewayNode],
          [
            {
              from: { interface: { ifindex: 2 } },
              to: { gateway: { address: '192.168.1.1' } },
              edgeKind: 'gatewayRoute',
              evidence: { observed: { source: 'netlink' } },
              detail: 'default via this gateway',
            },
          ],
        ),
      ),
    ).not.toThrow();
  });

  it('rejects missing or malformed node ids and structures', () => {
    expect(() =>
      parseTopology(topologyWith([{ ...baseNode, id: { ifindex: 2 } }], [])),
    ).toThrow(/id/);
    expect(() =>
      parseTopology(topologyWith([{ ...baseNode, id: { interface: { ifindex: 0 } } }], [])),
    ).toThrow(/ifindex/);
    expect(() =>
      parseTopology(topologyWith([{ ...baseNode, id: { gateway: { address: 'nope' } } }], [])),
    ).toThrow(/address/);
    expect(() => parseTopology(topologyWith([{ ...baseNode, nodeKind: 'cloud' }], []))).toThrow(
      /nodeKind/,
    );
    expect(() => parseTopology(topologyWith([{ ...baseNode, state: 'cloudy' }], []))).toThrow(
      /state/,
    );
    expect(() =>
      parseTopology(
        topologyWith(
          [
            {
              ...baseNode,
              detail: { ...(baseNode.detail as object), addresses: [{ ifindex: 2, family: 'ipv4', address: '10.0.0.1', prefixLen: 40 }] },
            },
          ],
          [],
        ),
      ),
    ).toThrow(/prefixLen/);
  });

  it('rejects duplicate node identifiers', () => {
    expect(() => parseTopology(topologyWith([baseNode, { ...baseNode }], []))).toThrow(
      /duplicate node id/,
    );
  });

  it('rejects dangling edge references', () => {
    expect(() =>
      parseTopology(
        topologyWith([baseNode], [
          {
            from: { interface: { ifindex: 2 } },
            to: { gateway: { address: '192.168.1.1' } }, // node absent
            edgeKind: 'gatewayRoute',
            evidence: { observed: { source: 'netlink' } },
            detail: null,
          },
        ]),
      ),
    ).toThrow(/does not exist/);
  });

  it('rejects unknown edge kinds and malformed evidence', () => {
    expect(() =>
      parseTopology(
        topologyWith([baseNode, gatewayNode], [
          {
            from: { interface: { ifindex: 2 } },
            to: { gateway: { address: '192.168.1.1' } },
            edgeKind: 'wormhole',
            evidence: { observed: { source: 'netlink' } },
            detail: null,
          },
        ]),
      ),
    ).toThrow(/edgeKind/);
    expect(() =>
      parseTopology(
        topologyWith([baseNode, gatewayNode], [
          {
            from: { interface: { ifindex: 2 } },
            to: { gateway: { address: '192.168.1.1' } },
            edgeKind: 'gatewayRoute',
            evidence: { observed: { source: 'telepathy' } },
            detail: null,
          },
        ]),
      ),
    ).toThrow(/source/);
    expect(() =>
      parseTopology(
        topologyWith([baseNode, gatewayNode], [
          {
            from: { interface: { ifindex: 2 } },
            to: { gateway: { address: '192.168.1.1' } },
            edgeKind: 'gatewayRoute',
            evidence: { guessed: true },
            detail: null,
          },
        ]),
      ),
    ).toThrow(/evidence/);
  });

  it('rejects inconsistent detail counts and gateway interface kinds', () => {
    expect(() =>
      parseTopology(
        topologyWith([
          { ...baseNode, detail: { ...(baseNode.detail as object), defaultRoutes: 5, routesUsing: 2 } },
        ], []),
      ),
    ).toThrow(/defaultRoutes/);
    expect(() =>
      parseTopology(topologyWith([{ ...gatewayNode, interfaceKind: 'ethernet' }], [])),
    ).toThrow(/interfaceKind/);
  });

  it('accepts the empty document and rejects incomplete ones', () => {
    expect(() => parseTopology(topologyWith([], []))).not.toThrow();
    expect(() =>
      parseTopology({ schemaVersion: 2, sourceTimestampMs: 0, nodes: [] }),
    ).toThrow(/\$.edges/);
    expect(() => parseTopology({ schemaVersion: 2, nodes: [], edges: [] })).toThrow(
      /sourceTimestampMs/,
    );
  });
});

describe('linkNetnsid validation (WO-03-F / AUD-01)', () => {
  it('accepts a veth interface carrying a peer and its netnsid', () => {
    const doc = snapshotWith((d) => {
      (d.interfaces as Array<Record<string, unknown>>)[0] = {
        ...(d.interfaces as Array<Record<string, unknown>>)[0],
        kind: 'veth',
        peerIfindex: 7,
        linkNetnsid: 0,
      };
    });
    expect(() => parseSnapshot(doc)).not.toThrow();
  });

  it('accepts an unassigned netnsid (-1) and rejects out-of-range values', () => {
    const withUnassigned = snapshotWith((d) => {
      (d.interfaces as Array<Record<string, unknown>>)[0] = {
        ...(d.interfaces as Array<Record<string, unknown>>)[0],
        linkNetnsid: -1,
      };
    });
    expect(() => parseSnapshot(withUnassigned)).not.toThrow();

    const tooLarge = snapshotWith((d) => {
      (d.interfaces as Array<Record<string, unknown>>)[0] = {
        ...(d.interfaces as Array<Record<string, unknown>>)[0],
        linkNetnsid: 2147483648,
      };
    });
    expect(() => parseSnapshot(tooLarge)).toThrow(
      /interfaces\[0\]\.linkNetnsid: expected an integer <= 2147483647/,
    );

    const fractional = snapshotWith((d) => {
      (d.interfaces as Array<Record<string, unknown>>)[0] = {
        ...(d.interfaces as Array<Record<string, unknown>>)[0],
        linkNetnsid: 0.5,
      };
    });
    expect(() => parseSnapshot(fractional)).toThrow(
      /interfaces\[0\]\.linkNetnsid: expected an integer/,
    );
  });

  it('topology node details carry linkNetnsid with the same rules', () => {
    const withNetnsid = {
      ...baseNode,
      detail: { ...baseNode.detail, peerIfindex: 9, linkNetnsid: 2 },
    };
    expect(() => parseTopology(topologyWith([withNetnsid], []))).not.toThrow();

    const unassigned = {
      ...baseNode,
      detail: { ...baseNode.detail, linkNetnsid: -1 },
    };
    expect(() => parseTopology(topologyWith([unassigned], []))).not.toThrow();

    const bad = {
      ...baseNode,
      detail: { ...baseNode.detail, linkNetnsid: 'zero' },
    };
    expect(() => parseTopology(topologyWith([bad], []))).toThrow(
      /nodes\[0\]\.detail\.linkNetnsid: expected an integer/,
    );
  });
});

describe('route TOS validation (WO-03-F / AUD-02)', () => {
  it('accepts a TOS value in 0..255 and rejects out-of-range values', () => {
    const withTos = snapshotWith((d) => {
      (d.routes as Array<Record<string, unknown>>)[0].tos = 4;
    });
    expect(() => parseSnapshot(withTos)).not.toThrow();

    const tooHigh = snapshotWith((d) => {
      (d.routes as Array<Record<string, unknown>>)[0].tos = 256;
    });
    expect(() => parseSnapshot(tooHigh)).toThrow(
      /routes\[0\]\.tos: expected an integer <= 255/,
    );
  });
});

describe('route via (RTA_VIA) validation and display (WO-03-F / AUD-13)', () => {
  it('accepts cross-family via encodings and rejects malformed ones', () => {
    // An IPv4 route whose next hop is an IPv6 address via RTA_VIA: the
    // family deliberately does NOT need to match the route's family.
    const crossFamily = snapshotWith((d) => {
      (d.routes as Array<Record<string, unknown>>)[0].via = { inet6: '2001:db8::9' };
    });
    expect(() => parseSnapshot(crossFamily)).not.toThrow();

    const packet = snapshotWith((d) => {
      (d.routes as Array<Record<string, unknown>>)[0].via = { packet: [0xaa, 0xbb] };
    });
    expect(() => parseSnapshot(packet)).not.toThrow();

    const other = snapshotWith((d) => {
      (d.routes as Array<Record<string, unknown>>)[0].via = {
        other: { family: 247, bytes: [0, 1, 2] },
      };
    });
    expect(() => parseSnapshot(other)).not.toThrow();

    const badFamily = snapshotWith((d) => {
      (d.routes as Array<Record<string, unknown>>)[0].via = { inet: 'not-an-ip' };
    });
    expect(() => parseSnapshot(badFamily)).toThrow(/routes\[0\]\.via\.inet/);

    const unknownShape = snapshotWith((d) => {
      (d.routes as Array<Record<string, unknown>>)[0].via = { mpls: 100 };
    });
    expect(() => parseSnapshot(unknownShape)).toThrow(
      /routes\[0\]\.via: expected \{inet\}, \{inet6\}, \{packet\}, or \{other\}/,
    );

    const byteOutOfRange = snapshotWith((d) => {
      (d.routes as Array<Record<string, unknown>>)[0].via = { packet: [256] };
    });
    expect(() => parseSnapshot(byteOutOfRange)).toThrow(
      /routes\[0\]\.via\.packet\[0\]: expected an integer <= 255/,
    );
  });

  it('a via-carrying route is never labeled on-link', async () => {
    const { nextHopLabel } = await import('../src/lib/routes');
    const viaRoute = { ...baseRouteFixture(), gateway: null, via: { inet6: '2001:db8::9' } };
    expect(nextHopLabel(viaRoute)).toContain('2001:db8::9');
    expect(nextHopLabel(viaRoute)).not.toContain('on-link');
    const onLinkRoute = { ...baseRouteFixture(), gateway: null, via: null };
    expect(nextHopLabel(onLinkRoute)).toBe('on-link');
  });
});

function baseRouteFixture() {
  return {
    family: 'ipv4' as const,
    destination: { address: '10.5.0.0', prefixLen: 24 },
    gateway: null,
    via: null,
    outputInterface: 2,
    outputInterfaceName: 'eth0',
    protocol: 'static' as const,
    scope: 'universe' as const,
    routeType: 'unicast' as const,
    metric: 0,
    table: 254,
    tos: null,
    flags: [],
    nextHops: 'single' as const,
    sources: ['netlink' as const],
    notes: [],
  };
}

describe('empty provenance is rejected (WO-03-F / AUD-06)', () => {
  it('rejects observations whose sources array is empty', () => {
    const noRouteSources = snapshotWith((d) => {
      (d.routes as Array<Record<string, unknown>>)[0].sources = [];
    });
    expect(() => parseSnapshot(noRouteSources)).toThrow(
      /routes\[0\]\.sources: must name at least one provider/,
    );

    const noInterfaceSources = snapshotWith((d) => {
      (d.interfaces as Array<Record<string, unknown>>)[0].sources = [];
    });
    expect(() => parseSnapshot(noInterfaceSources)).toThrow(
      /interfaces\[0\]\.sources: must name at least one provider/,
    );

    const noAddressSources = snapshotWith((d) => {
      (d.addresses as Array<Record<string, unknown>>)[0].sources = [];
    });
    expect(() => parseSnapshot(noAddressSources)).toThrow(
      /addresses\[0\]\.sources: must name at least one provider/,
    );
  });
});

describe('IPv6 validation covers the full grammar (WO-03-F / AUD-10, section 5.3)', () => {
  it.each([
    ['::1', true],
    ['fe80::', true],
    ['fe80::1', true],
    ['2001:db8:9::2', true],
    ['1:2:3:4:5:6:7:8', true],
    ['::ffff:192.168.1.1', true],
    ['1:2:3:4:5:6:1.2.3.4', true],
    ['::', true],
    ['1::', true],
    ['::2', true],
    // The audit's malformed examples.
    [':::', false],
    ['1:::2', false],
    // Repeated compression.
    ['::a::', false],
    ['1::a::2', false],
    // Group-count violations.
    ['1:2:3:4:5:6:7', false],
    ['1:2:3:4:5:6:7:8:9', false],
    ['1:2:3:4:5:6:7::8', false],
    ['a:b:c:d:e:f::1.2.3.4', false],
    // Invalid group lengths and characters.
    ['12345::', false],
    ['::g', false],
    ['2001:db8::zzzz', false],
    // Leading/trailing stray colons.
    [':1:2::', false],
    ['1:2:3:4:5:6:7:8:', false],
    [':', false],
    // Zone identifiers are outside the document contract.
    ['fe80::1%eth0', false],
    ['fe80::1%1', false],
  ])('isIpv6Address(%j) === %s', (value, expected) => {
    expect(isIpv6Address(value)).toBe(expected);
  });

  it('a malformed IPv6 in a document is rejected with the location', () => {
    const bad = snapshotWith((d) => {
      (d.addresses as Array<Record<string, unknown>>)[0] = {
        ...(d.addresses as Array<Record<string, unknown>>)[0],
        family: 'ipv6',
        address: ':::',
        prefixLen: 128,
      };
    });
    expect(() => parseSnapshot(bad)).toThrow(/addresses\[0\]\.address/);
  });
});

describe('interface and topology structural rules (WO-03-F / AUD-10)', () => {
  it('rejects duplicate interface ifindexes within one observation', () => {
    const doc = snapshotWith((d) => {
      const interfaces = d.interfaces as Array<Record<string, unknown>>;
      interfaces[1] = { ...interfaces[1], ifindex: interfaces[0].ifindex };
    });
    expect(() => parseSnapshot(doc)).toThrow(
      /interfaces\[1\]\.ifindex: duplicate ifindex/,
    );
  });

  it('rejects self-referential topology edges', () => {
    const selfEdge = {
      from: { interface: { ifindex: 2 } },
      to: { interface: { ifindex: 2 } },
      edgeKind: 'vethPeer',
      evidence: { observed: { source: 'netlink' } },
      detail: 'self edge',
    };
    expect(() => parseTopology(topologyWith([baseNode], [selfEdge]))).toThrow(
      /self-referential edge/,
    );
  });

  it('the schema version message names the current version (v3)', () => {
    const versioned = topologyWith([], []) as Record<string, unknown>;
    versioned.schemaVersion = 7;
    expect(() => parseTopology(versioned)).toThrow(
      /expected 1, 2, or 3; this build writes v3/,
    );
    expect(() =>
      parseSnapshot(snapshotWith((d) => { d.schemaVersion = 9; })),
    ).toThrow(/expected 1, 2, or 3; this build writes v3/);
  });
});
