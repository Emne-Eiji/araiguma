import { describe, expect, it } from 'vitest';
import {
  defaultTopology,
  edgeKindLabel,
  evidenceLabel,
  nodeIdKey,
  parseTopology,
  routeHighlightTargets,
  topologyGoldens,
} from '../src/lib/topology';
import { fixtures } from '../src/lib/fixtures';

describe('topology goldens (type-sync contract)', () => {
  it('includes the fixture scenarios', () => {
    expect(topologyGoldens.map((g) => g.name)).toEqual([
      'workstation',
      'incomplete-observations',
      'server-tunnel',
      'dummy-virtual',
      'empty',
    ]);
  });

  it('workstation golden matches the design invariants', () => {
    const doc = defaultTopology();
    expect(doc.schemaVersion).toBe(1);
    expect(doc.sourceTimestampMs).toBe(fixtures.workstation.snapshot.timestampMs);

    const gateways = doc.nodes.filter((n) => n.nodeKind === 'gateway');
    expect(gateways.map((g) => g.label).sort()).toEqual(['192.168.1.1', 'fe80::1']);

    // Only evidence-backed edges exist; all gateway edges are observed.
    const gatewayEdges = doc.edges.filter((e) => e.edgeKind.startsWith('gateway'));
    expect(gatewayEdges.length).toBe(3);
    expect(gatewayEdges.every((e) => 'observed' in e.evidence)).toBe(true);

    // No invented internet/upstream nodes.
    expect(doc.nodes.every((n) => n.nodeKind === 'interface' || n.nodeKind === 'gateway')).toBe(true);
  });

  it('empty golden is a valid empty document', () => {
    const empty = topologyGoldens.find((g) => g.name === 'empty')!;
    expect(empty.document.nodes).toEqual([]);
    expect(empty.document.edges).toEqual([]);
  });

  it('server-tunnel golden keeps the on-link default honest', () => {
    const doc = topologyGoldens.find((g) => g.name === 'server-tunnel')!.document;
    const tun0 = doc.nodes.find((n) => n.label === 'tun0')!;
    expect(tun0.annotations.some((a) => a.includes('default route without a gateway'))).toBe(true);
    // No edges from tun0: its default has no gateway.
    expect(doc.edges.some((e) => nodeIdKey(e.from) === nodeIdKey(tun0.id))).toBe(false);
  });

  it('dummy-virtual golden carries structural edges', () => {
    const doc = topologyGoldens.find((g) => g.name === 'dummy-virtual')!.document;
    expect(doc.edges.filter((e) => e.edgeKind === 'bridgeMember').length).toBe(1);
    expect(doc.edges.filter((e) => e.edgeKind === 'linkParent').length).toBe(1);
    // veth nodes exist but no fabricated pairing edge.
    const veth0 = doc.nodes.find((n) => n.label === 'veth0')!;
    const veth1 = doc.nodes.find((n) => n.label === 'veth1')!;
    expect(veth0 && veth1).toBeTruthy();
    expect(
      doc.edges.some(
        (e) =>
          (nodeIdKey(e.from) === nodeIdKey(veth0.id) && nodeIdKey(e.to) === nodeIdKey(veth1.id)) ||
          (nodeIdKey(e.from) === nodeIdKey(veth1.id) && nodeIdKey(e.to) === nodeIdKey(veth0.id)),
      ),
    ).toBe(false);
  });
});

describe('parseTopology rejects invalid input', () => {
  it('rejects non-objects and wrong schema versions', () => {
    expect(() => parseTopology(null)).toThrow();
    expect(() => parseTopology([])).toThrow();
    expect(() => parseTopology({ schemaVersion: 2, nodes: [], edges: [] })).toThrow(/schemaVersion/);
  });

  it('rejects nodes without ids or labels', () => {
    expect(() =>
      parseTopology({ schemaVersion: 1, nodes: [{ label: 'x', nodeKind: 'interface' }], edges: [] }),
    ).toThrow(/id/);
    expect(() =>
      parseTopology({ schemaVersion: 1, nodes: [{ id: { interface: { ifindex: 1 } }, label: '', nodeKind: 'interface' }], edges: [] }),
    ).toThrow(/label/);
  });
});

describe('topology display helpers', () => {
  it('labels edges and evidence', () => {
    expect(edgeKindLabel('bridgeMember')).toBe('bridge port');
    expect(evidenceLabel({ observed: { source: 'procfs' } })).toBe('observed (procfs)');
    expect(evidenceLabel({ inferred: { reason: 'because' } })).toContain('because');
  });

  it('computes stable node keys', () => {
    expect(nodeIdKey({ interface: { ifindex: 2 } })).toBe('if:2');
    expect(nodeIdKey({ gateway: { address: '10.0.0.1' } })).toBe('gw:10.0.0.1');
  });

  it('finds the highlight targets for a selected route', () => {
    const doc = defaultTopology();
    const route = fixtures.workstation.snapshot.routes.find(
      (r) => r.destination !== null && r.destination.prefixLen === 0 && r.gateway === '192.168.1.1' && r.outputInterface === 2,
    )!;
    const targets = routeHighlightTargets(route, doc);
    expect(targets).toEqual([
      { interface: { ifindex: 2 } },
      { gateway: { address: '192.168.1.1' } },
    ]);
  });
});
