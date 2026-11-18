// The application's data source state (WO-03 M1): which documents the
// views render — the labeled synthetic fixtures (mock mode) or documents
// imported from real Araiguma CLI output (imported mode). A Svelte 5
// runes module so every view and badge can read the mode without prop
// threading; all import PARSING lives in lib/import.ts (pure, unit-tested)
// and the validators remain the document boundary.

import type { NetworkSnapshot, TopologyDocument } from '../types';
import { fixtures } from './fixtures';
import { defaultTopology } from './topology';
import { importDocumentText } from './import';

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

export interface DataSourceState {
  mode: 'mock' | 'imported';
  /** Each kind keeps its own provenance; only the same-kind slot is
   *  replaced by an import. */
  snapshot: ImportedSnapshot | null;
  topology: ImportedTopology | null;
}

export const dataSource = $state<DataSourceState>({
  mode: 'mock',
  snapshot: null,
  topology: null,
});

/// The snapshot the snapshot-driven views render.
export function currentSnapshot(): NetworkSnapshot {
  return dataSource.snapshot?.document ?? fixtures.workstation.snapshot;
}

/// The topology document the topology view renders.
export function currentTopology(): TopologyDocument {
  return dataSource.topology?.document ?? defaultTopology();
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
  const documents = importDocumentText(text);
  const importedAtMs = Date.now();
  if (documents.snapshot !== null) {
    dataSource.snapshot = { document: documents.snapshot, fileName, importedAtMs };
  }
  if (documents.topology !== null) {
    dataSource.topology = { document: documents.topology, fileName, importedAtMs };
  }
  dataSource.mode = 'imported';
}

/// Return to the synthetic demonstration data.
export function returnToMock(): void {
  dataSource.mode = 'mock';
  dataSource.snapshot = null;
  dataSource.topology = null;
}
