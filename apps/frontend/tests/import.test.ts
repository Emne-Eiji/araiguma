import { beforeEach, describe, expect, it } from 'vitest';
import {
  detectDocumentKind,
  formatObservationTime,
  importDocumentText,
} from '../src/lib/import';
import {
  dataSource,
  importFile,
  returnToMock,
  currentSnapshot,
  currentTopology,
} from '../src/lib/dataMode.svelte';
import workstation from '../../../fixtures/snapshots/workstation.json';
import workstationTopology from '../../../fixtures/topology/workstation.json';
import incompleteObservations from '../../../fixtures/snapshots/incomplete-observations.json';

// WO-03 M1: importing actual observation documents. The Part 1 validators
// are the boundary — the tests below cover both directions (acceptance of
// valid documents, rejection with useful errors) and the state rules
// (provenance, per-kind replacement, failure safety, return to mock).

const validSnapshotText = JSON.stringify(workstation);
const validTopologyText = JSON.stringify(workstationTopology);

describe('document kind detection', () => {
  it('recognizes snapshots and topologies', () => {
    expect(detectDocumentKind(workstation)).toBe('snapshot');
    expect(detectDocumentKind(workstationTopology)).toBe('topology');
  });

  it('rejects unrecognized or ambiguous documents with guidance', () => {
    expect(() => detectDocumentKind({ hello: 'world' })).toThrow(/unrecognized document/);
    expect(() => detectDocumentKind([1, 2, 3])).toThrow(/unrecognized document/);
    expect(() =>
      detectDocumentKind({
        interfaces: [],
        addresses: [],
        routes: [],
        providers: [],
        nodes: [],
        edges: [],
      }),
    ).toThrow(/ambiguous/);
  });
});

describe('importDocumentText', () => {
  it('imports a valid snapshot', () => {
    const result = importDocumentText(validSnapshotText);
    expect(result.snapshot).not.toBeNull();
    expect(result.snapshot!.schemaVersion).toBe(3);
    expect(result.topology).toBeNull();
  });

  it('imports a valid topology document', () => {
    const result = importDocumentText(validTopologyText);
    expect(result.topology).not.toBeNull();
    expect(result.topology!.nodes.length).toBeGreaterThan(0);
    expect(result.snapshot).toBeNull();
  });

  it('imports an empty observation (zero items is valid)', () => {
    const result = importDocumentText(
      JSON.stringify({
        schemaVersion: 2,
        timestampMs: 0,
        interfaces: [],
        addresses: [],
        routes: [],
        providers: [],
      }),
    );
    expect(result.snapshot!.interfaces).toEqual([]);
  });

  it('imports an incomplete observation without inventing fields', () => {
    // The incomplete-observations fixture: netlink failed, sysfs degraded,
    // procfs-only routes with null protocol/scope/type.
    const result = importDocumentText(JSON.stringify(incompleteObservations));
    expect(result.snapshot!.providers.some((p) => p.state === 'unavailable')).toBe(true);
  });

  it('rejects malformed JSON with a clear reason', () => {
    expect(() => importDocumentText('{not json')).toThrow(/not valid JSON/);
  });

  it('rejects unsupported schema versions', () => {
    const doc = { ...workstation, schemaVersion: 99 };
    expect(() => importDocumentText(JSON.stringify(doc))).toThrow(/schema version/);
  });

  it('rejects missing required fields', () => {
    const doc = { ...workstation } as Record<string, unknown>;
    delete doc.providers;
    expect(() => importDocumentText(JSON.stringify(doc))).toThrow(/providers/);
  });

  it('rejects invalid nested structures with the path in the reason', () => {
    const doc = structuredClone(workstation) as Record<string, unknown>;
    (doc.routes as Record<string, unknown>[])[0].gateway = 'not-an-ip';
    expect(() => importDocumentText(JSON.stringify(doc))).toThrow(/gateway/);
  });

  it('rejects invalid topology edges', () => {
    const doc = structuredClone(workstationTopology) as Record<string, unknown>;
    (doc.edges as Record<string, unknown>[]).push({
      from: { interface: { ifindex: 9999 } },
      to: { gateway: { address: '10.0.0.1' } },
      edgeKind: 'gatewayRoute',
      evidence: { observed: { source: 'netlink' } },
      detail: null,
    });
    expect(() => importDocumentText(JSON.stringify(doc))).toThrow(/does not exist/);
  });
});

describe('data source state', () => {
  beforeEach(() => returnToMock());

  it('starts in mock mode rendering the labeled fixtures', () => {
    expect(dataSource.mode).toBe('mock');
    expect(currentSnapshot().interfaces.length).toBeGreaterThan(0);
    expect(currentTopology().nodes.length).toBeGreaterThan(0);
  });

  it('importing a snapshot switches modes and keeps per-kind provenance', () => {
    importFile('real-snapshot.json', validSnapshotText);
    expect(dataSource.mode).toBe('imported');
    expect(dataSource.snapshot?.fileName).toBe('real-snapshot.json');
    expect(currentSnapshot().timestampMs).toBe(workstation.timestampMs);
    // Topology still falls back to the demo document until one is imported.
    expect(dataSource.topology).toBeNull();
    expect(currentTopology().nodes.length).toBeGreaterThan(0);
  });

  it('importing a topology then a snapshot keeps both independently', () => {
    importFile('topology.json', validTopologyText);
    importFile('snapshot.json', validSnapshotText);
    expect(dataSource.snapshot?.fileName).toBe('snapshot.json');
    expect(dataSource.topology?.fileName).toBe('topology.json');
  });

  it('a failed import leaves existing valid state untouched', () => {
    importFile('snapshot.json', validSnapshotText);
    expect(() => importFile('broken.json', '{invalid')).toThrow();
    expect(dataSource.mode).toBe('imported');
    expect(dataSource.snapshot?.fileName).toBe('snapshot.json');
  });

  it('returning to mock clears the imported documents', () => {
    importFile('snapshot.json', validSnapshotText);
    returnToMock();
    expect(dataSource.mode).toBe('mock');
    expect(dataSource.snapshot).toBeNull();
    expect(dataSource.topology).toBeNull();
  });
});

describe('observation time formatting', () => {
  it('formats a UTC timestamp explicitly', () => {
    expect(formatObservationTime(1765824000000)).toMatch(/2025.*UTC$/);
  });

  it('reports unknown times honestly', () => {
    expect(formatObservationTime(Number.NaN)).toBe('unknown time');
  });
});
