// The application's data source state (WO-03 M1): which documents the
// views render — the labeled synthetic fixtures of the active scenario
// (mock mode, WO-04 Stage A scenario registry) or documents imported from
// real Araiguma CLI output (imported mode). A Svelte 5 runes module so
// every view and badge can read the mode without prop threading; all
// import PARSING lives in lib/import.ts (pure, unit-tested) and the
// validators remain the document boundary.

import type { NetworkSnapshot, TopologyDocument } from '../types';
import {
  DEFAULT_SCENARIO_ID,
  scenarioSnapshot,
  scenarioTopology,
  type ScenarioId,
} from './scenarios';
import { detectDocumentKind, importDocumentText } from './import';

export interface ImportedSnapshot {
  document: NetworkSnapshot;
  fileName: string;
  importedAtMs: number;
}

export interface ImportedTopology {
  document: TopologyDocument;
  fileName: string;
  importedAtMs: number;
}

/** The most recent import attempt (WO-04 Stage A: the Documents & Import
 *  view reports it explicitly). `kind` is null when the file was not even
 *  recognizable as an Araiguma document. */
export interface ImportAttempt {
  fileName: string;
  kind: 'snapshot' | 'topology' | null;
  outcome: 'accepted' | 'rejected';
  /** Rejection reason verbatim (user-facing), when rejected. */
  reason?: string;
}

export interface DataSourceState {
  mode: 'mock' | 'imported';
  /** Each kind keeps its own provenance; only the same-kind slot is
   *  replaced by an import. */
  snapshot: ImportedSnapshot | null;
  topology: ImportedTopology | null;
  /** The active synthetic scenario in mock mode (WO-04 Stage A). Selecting
   *  a scenario is an explicit return to mock mode; it never mixes with
   *  imported documents. */
  mockScenario: ScenarioId;
  /** The most recent import attempt, for explicit reporting in the
   *  Documents & Import view. Historical fact — kept across mode changes
   * until the next attempt replaces it. */
  lastImport: ImportAttempt | null;
  /** Monotonic counter, bumped on EVERY change of the rendered documents
   *  (import, return to mock, scenario switch). Views re-resolve dependent
   *  selection state against the new documents when it changes (WO-03-F /
   *  AUD-09: selections from a previous document must never keep rendering
   *  as if they belonged to the current one). */
  generation: number;
}

export const dataSource = $state<DataSourceState>({
  mode: 'mock',
  snapshot: null,
  topology: null,
  mockScenario: DEFAULT_SCENARIO_ID,
  lastImport: null,
  generation: 0,
});

/// The snapshot the snapshot-driven views render.
export function currentSnapshot(): NetworkSnapshot {
  return dataSource.snapshot?.document ?? scenarioSnapshot(dataSource.mockScenario);
}

/// The topology document the topology view renders.
export function currentTopology(): TopologyDocument {
  return dataSource.topology?.document ?? scenarioTopology(dataSource.mockScenario);
}

/// Whether a view that needs a snapshot has an explicitly imported one.
export function snapshotImported(): boolean {
  return dataSource.snapshot !== null;
}

/// Whether the topology view has an explicitly imported document.
export function topologyImported(): boolean {
  return dataSource.topology !== null;
}

/// Import a document (its file name is kept as provenance). Throws with a
/// user-facing reason on invalid input; on failure the existing state is
/// untouched — valid imported observations are never silently replaced.
export function importFile(fileName: string, text: string): void {
  try {
    const documents = importDocumentText(text);
    const importedAtMs = Date.now();
    if (documents.snapshot !== null) {
      dataSource.snapshot = { document: documents.snapshot, fileName, importedAtMs };
    }
    if (documents.topology !== null) {
      dataSource.topology = { document: documents.topology, fileName, importedAtMs };
    }
    dataSource.mode = 'imported';
    dataSource.lastImport = {
      fileName,
      kind: documents.snapshot !== null ? 'snapshot' : 'topology',
      outcome: 'accepted',
    };
    // Atomically with the documents: a new generation invalidates every
    // selection derived from the previous documents (they re-resolve or
    // clear in the views; nothing stale keeps rendering).
    dataSource.generation += 1;
  } catch (error) {
    // Record the attempt for the Documents & Import view (the document
    // kind, when the file was at least recognizable), then rethrow: the
    // existing valid state is untouched.
    let kind: 'snapshot' | 'topology' | null = null;
    try {
      kind = detectDocumentKind(JSON.parse(text));
    } catch {
      kind = null;
    }
    dataSource.lastImport = {
      fileName,
      kind,
      outcome: 'rejected',
      reason: (error as Error).message,
    };
    throw error;
  }
}

/// Return to the synthetic demonstration data (the active scenario; the
/// scenario registry presentation always names which one is shown).
export function returnToMock(): void {
  dataSource.mode = 'mock';
  dataSource.snapshot = null;
  dataSource.topology = null;
  dataSource.generation += 1;
}

/// Select the active synthetic scenario. This is an explicit return to
/// mock mode with the chosen scenario — imported documents are set aside
/// (never mixed with scenario content), provenance always says which
/// scenario is shown, and the generation bump re-resolves selections.
export function selectMockScenario(id: ScenarioId): void {
  dataSource.mode = 'mock';
  dataSource.snapshot = null;
  dataSource.topology = null;
  dataSource.mockScenario = id;
  dataSource.generation += 1;
}
