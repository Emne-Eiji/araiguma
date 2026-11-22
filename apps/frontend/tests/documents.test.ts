import { beforeEach, describe, expect, it } from 'vitest';
import { classifyRejection, documentViews, type DocumentsStateView } from '../src/lib/documents';
import { importDocumentText } from '../src/lib/import';
import {
  dataSource,
  importFile,
  returnToMock,
  selectMockScenario,
} from '../src/lib/dataMode.svelte';
import workstation from '../../../fixtures/snapshots/workstation.json';
import workstationTopology from '../../../fixtures/topology/workstation.json';
import { scenarioById } from '../src/lib/scenarios';

// WO-04 Stage A: the Documents & Import view state derivation. The five
// document states are derived — never invented — and the rejection
// classification keys on the REAL validators' messages.

const baseState: DocumentsStateView = {
  mode: 'mock',
  snapshot: null,
  topology: null,
  mockScenarioName: scenarioById('workstation').name,
};

describe('rejection classification (against the real validators)', () => {
  it('an unsupported schema version classifies as unsupported', () => {
    const future = { ...(workstation as object), schemaVersion: 99 };
    let reason = '';
    try {
      importDocumentText(JSON.stringify(future));
    } catch (error) {
      reason = (error as Error).message;
    }
    expect(reason).toMatch(/unsupported schema ?version/i);
    expect(classifyRejection(reason)).toBe('unsupported');
  });

  it('a structural validation failure classifies as invalid', () => {
    // Snapshot-shaped (all four arrays present) so the document reaches
    // the snapshot validator and fails INSIDE it.
    const broken = JSON.stringify({
      schemaVersion: 2,
      timestampMs: 1732000000000,
      interfaces: ['garbage'],
      addresses: [],
      routes: [],
      providers: [],
    });
    let reason = '';
    try {
      importDocumentText(broken);
    } catch (error) {
      reason = (error as Error).message;
    }
    expect(reason).not.toMatch(/unsupported schema ?version/i);
    expect(reason).toMatch(/interfaces/);
    expect(classifyRejection(reason)).toBe('invalid');
  });

  it('an unsupported topology schema version also classifies as unsupported', () => {
    const futureTopology = { ...(workstationTopology as object), schemaVersion: 99 };
    let reason = '';
    try {
      importDocumentText(JSON.stringify(futureTopology));
    } catch (error) {
      reason = (error as Error).message;
    }
    expect(classifyRejection(reason)).toBe('unsupported');
  });
});

describe('document slot states', () => {
  it('mock mode: both kinds are not-observed, the scenario is named', () => {
    const views = documentViews(baseState, null);
    expect(views.snapshotSlot.status).toBe('not-observed');
    expect(views.topologySlot.status).toBe('not-observed');
    expect(views.snapshotSlot.scenarioName).toBe(scenarioById('workstation').name);
    expect(views.snapshotSlot.fileName).toBeNull();
    expect(views.lastRejection).toBeNull();
    expect(views.lastAccepted).toBeNull();
  });

  it('imported snapshot only: snapshot imported-ok, topology missing', () => {
    const views = documentViews(
      {
        ...baseState,
        mode: 'imported',
        snapshot: {
          fileName: 'obs.json',
          importedAtMs: 1732000000000,
          document: { timestampMs: 1731000000000 },
        },
      },
      null,
    );
    expect(views.snapshotSlot.status).toBe('imported-ok');
    expect(views.snapshotSlot.fileName).toBe('obs.json');
    expect(views.snapshotSlot.observedAt).toContain('UTC');
    expect(views.snapshotSlot.importedAt).toContain('UTC');
    // Observation time and import time are different facts, both present.
    expect(views.snapshotSlot.observedAt).not.toBe(views.snapshotSlot.importedAt);
    expect(views.topologySlot.status).toBe('missing');
    expect(views.topologySlot.scenarioName).toBeNull();
  });

  it('imported topology only: topology imported-ok, snapshot missing', () => {
    const views = documentViews(
      {
        ...baseState,
        mode: 'imported',
        topology: {
          fileName: 'topo.json',
          importedAtMs: 1732000000000,
          document: { sourceTimestampMs: 1731000000000 },
        },
      },
      null,
    );
    expect(views.topologySlot.status).toBe('imported-ok');
    expect(views.snapshotSlot.status).toBe('missing');
  });

  it('live mode reports live provenance instead of the mock scenario', () => {
    const views = documentViews(
      {
        ...baseState,
        mode: 'live',
        liveSnapshot: { document: { timestampMs: 1731000000000 } },
        liveTopology: { document: { sourceTimestampMs: 1731000000000 } },
        liveStale: false,
      },
      null,
    );
    expect(views.snapshotSlot.status).toBe('live-ok');
    expect(views.topologySlot.status).toBe('live-ok');
    expect(views.snapshotSlot.observedAt).toContain('UTC');
    expect(views.snapshotSlot.scenarioName).toBeNull();
    expect(views.topologySlot.scenarioName).toBeNull();
  });
});

describe('import attempts recorded through the real store', () => {
  beforeEach(() => {
    selectMockScenario('workstation');
    returnToMock();
  });

  it('a successful import is recorded as accepted with its kind', () => {
    importFile('good.json', JSON.stringify(workstation));
    expect(dataSource.lastImport).toEqual({
      fileName: 'good.json',
      kind: 'snapshot',
      outcome: 'accepted',
    });
    const views = documentViews(
      {
        mode: dataSource.mode,
        snapshot: dataSource.snapshot,
        topology: dataSource.topology,
        mockScenarioName: 'Synthetic workstation',
      },
      dataSource.lastImport,
    );
    expect(views.lastAccepted).toEqual({ fileName: 'good.json', kind: 'snapshot' });
    expect(views.lastRejection).toBeNull();
    expect(views.snapshotSlot.status).toBe('imported-ok');
  });

  it('a rejected import is recorded with its reason and kind, and classifies', () => {
    const broken = JSON.stringify({
      schemaVersion: 2,
      timestampMs: 1732000000000,
      interfaces: ['garbage'],
      addresses: [],
      routes: [],
      providers: [],
    });
    expect(() => importFile('broken.json', broken)).toThrow();
    expect(dataSource.lastImport?.outcome).toBe('rejected');
    expect(dataSource.lastImport?.kind).toBe('snapshot');
    expect(dataSource.lastImport?.reason).toMatch(/interfaces/);

    const views = documentViews(baseState, dataSource.lastImport);
    expect(views.lastRejection?.fileName).toBe('broken.json');
    expect(views.lastRejection?.rejectionClass).toBe('invalid');
    expect(views.lastRejection?.kind).toBe('snapshot');
  });

  it('an unrecognized file records kind null', () => {
    expect(() => importFile('mystery.json', '{"hello": "world"}')).toThrow(/unrecognized/);
    expect(dataSource.lastImport?.kind).toBeNull();
    expect(dataSource.lastImport?.outcome).toBe('rejected');
  });

  it('an unsupported schema version surfaces as the unsupported class', () => {
    const future = { ...(workstation as object), schemaVersion: 99 };
    expect(() => importFile('future.json', JSON.stringify(future))).toThrow(
      /unsupported schema ?version/i,
    );
    const views = documentViews(baseState, dataSource.lastImport);
    expect(views.lastRejection?.rejectionClass).toBe('unsupported');
    expect(views.lastRejection?.fileName).toBe('future.json');
  });

  it('the attempt history survives returning to mock (it is a fact, clearly labeled)', () => {
    importFile('ok.json', JSON.stringify(workstation));
    returnToMock();
    expect(dataSource.lastImport?.outcome).toBe('accepted');
    expect(dataSource.mode).toBe('mock');
  });
});
