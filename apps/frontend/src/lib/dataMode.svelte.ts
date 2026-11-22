// The application's data source state (WO-03 M1): which documents the
// views render — the labeled synthetic fixtures of the active scenario
// (mock mode, WO-04 Stage A scenario registry), documents imported from
// real Araiguma CLI output (imported mode), or the validated live engine
// pair (live mode). A Svelte 5 runes module so every view and badge can
// read the mode without prop threading; all import PARSING lives in
// lib/import.ts (pure, unit-tested) and the validators remain the
// document boundary.

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

export type DataMode = 'mock' | 'imported' | 'live';

export type LiveStatus = 'live' | 'stale' | 'unavailable';

/** State of the backend watch used by the live document store. */
export type LiveWatchState = 'starting' | 'active' | 'degraded' | 'failed';

export interface DataSourceState {
  mode: DataMode;
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
  /** Live mode (doc 04): the validated live document when the app runs in
   *  the desktop and the engine delivered one. Live documents enter the
   *  same validating loaders as file import; the frontend never invents
   *  facts — a live document is timestampMs-stamped and provenance-complete
   *  (AUD-06). */
  live: NetworkSnapshot | null;
  /** Doc 04 truthful-state flags: last live refresh failed while showing
   *  the previous live document (stale); last live refresh failed with
   *  nothing to show yet (unavailable). */
  liveStale: boolean;
  liveUnavailable: string | null;
  /** The watch is not LIVE until the backend subscription is active. */
  liveWatchState: LiveWatchState;
  /** The event monitor's most recent degradation reason. Kept separate
   *  from refresh errors so recovery clears only the health failure it
   *  owns. */
  liveMonitorReason: string | null;
  /** A `recovered` monitor-health fact has been received. It becomes
   *  effective only after a fresh authoritative live pair is rendered. */
  liveRecoveryConfirmed: boolean;
  /** A fresh authoritative live pair has been rendered since the latest
   *  monitor degradation. Recovery alone is never enough to claim LIVE. */
  liveFreshObservation: boolean;
  /** Live topology projection (from the engine, doc 04). */
  liveTopology: TopologyDocument | null;
  /** Monotonic counter, bumped on EVERY change of the rendered documents
   *  (import, return to mock, scenario switch, live pair replacement).
   *  Views re-resolve dependent selection state against the new documents
   *  when it changes (WO-03-F / AUD-09: selections from a previous
   *  document must never keep rendering as if they belonged to the current
   *  one). */
  generation: number;
}

export const dataSource = $state<DataSourceState>({
  mode: 'mock',
  snapshot: null,
  topology: null,
  mockScenario: DEFAULT_SCENARIO_ID,
  lastImport: null,
  live: null,
  liveStale: false,
  liveUnavailable: null,
  liveWatchState: 'starting',
  liveMonitorReason: null,
  liveRecoveryConfirmed: false,
  liveFreshObservation: false,
  liveTopology: null,
  generation: 0,
});

/** Enter live mode without pretending that live data has arrived yet. */
export function enterLiveMode(): void {
  dataSource.mode = 'live';
  dataSource.live = null;
  dataSource.liveTopology = null;
  dataSource.liveStale = false;
  dataSource.liveUnavailable = null;
  dataSource.liveWatchState = 'starting';
  dataSource.liveMonitorReason = null;
  dataSource.liveRecoveryConfirmed = false;
  dataSource.liveFreshObservation = false;
  dataSource.generation += 1;
}

/**
 * Replace the live snapshot and topology as one coherent pair. The pair is
 * installed only after both documents have crossed their validation
 * boundaries, so the views never combine observations from different
 * engine states.
 */
export function setLiveDocuments(
  snapshot: NetworkSnapshot,
  topology: TopologyDocument,
): void {
  dataSource.live = snapshot;
  dataSource.liveTopology = topology;
  dataSource.liveStale = false;
  // A failed or still-starting watch has its own reason. A successful
  // document refresh must not erase that reason (and therefore must not
  // make an inactive watch look healthy).
  if (dataSource.liveWatchState !== 'failed') {
    dataSource.liveUnavailable = null;
  }
  dataSource.generation += 1;
}

/** Mark the backend watch active after `subscribe_watch` succeeds. */
export function setLiveWatchActive(): void {
  // A degradation event can arrive before `subscribe_watch` resolves. The
  // subscription becoming active must not erase that newer health fact.
  if (
    dataSource.liveWatchState !== 'degraded' &&
    dataSource.liveWatchState !== 'failed'
  ) {
    dataSource.liveWatchState = 'active';
  }
}

/** Mark the event monitor degraded. A confirmed pair remains visible but
 *  stale; without one, the live state is unavailable. Recovery requires
 *  both a later recovery fact and a fresh authoritative observation. */
export function setLiveMonitorDegraded(message: string): void {
  if (dataSource.liveWatchState === 'failed') return;
  dataSource.liveWatchState = 'degraded';
  dataSource.liveMonitorReason = message;
  dataSource.liveRecoveryConfirmed = false;
  dataSource.liveFreshObservation = false;
  dataSource.liveStale = dataSource.live !== null;
}

/** Mark the event monitor healthy again. This does not erase an unrelated
 *  document-refresh failure; the live pair returns to LIVE only when it is
 *  coherent and no other failure remains. Recovery alone cannot promote
 *  an old pair to LIVE: a fresh authoritative observation must have been
 *  rendered since the latest degradation as well. */
export function setLiveMonitorRecovered(): void {
  if (dataSource.liveWatchState === 'failed') return;
  dataSource.liveRecoveryConfirmed = true;

  if (!dataSource.liveFreshObservation) {
    dataSource.liveWatchState = 'degraded';
    dataSource.liveMonitorReason =
      'The live event monitor recovered, but no fresh live observation has been confirmed yet.';
    dataSource.liveStale = dataSource.live !== null;
    return;
  }

  dataSource.liveWatchState = 'active';
  dataSource.liveMonitorReason = null;
  if (dataSource.liveUnavailable === null) {
    dataSource.liveStale = false;
  }
}

/** Record that a fresh authoritative live pair has been rendered since the
 *  latest degradation. Recovery remains pending until the matching
 *  `recovered` health fact is also received. */
export function setLiveMonitorFreshObservation(): void {
  if (dataSource.liveWatchState === 'failed') return;
  dataSource.liveFreshObservation = true;
  if (!dataSource.liveRecoveryConfirmed) return;

  dataSource.liveWatchState = 'active';
  dataSource.liveMonitorReason = null;
  if (dataSource.liveUnavailable === null) {
    dataSource.liveStale = false;
  }
}

/** Mark the backend watch failed. A confirmed pair remains stale; without
 * one, the live state is unavailable. */
export function setLiveWatchFailed(message: string): void {
  dataSource.liveWatchState = 'failed';
  dataSource.liveUnavailable = message;
  dataSource.liveMonitorReason = null;
  dataSource.liveRecoveryConfirmed = false;
  dataSource.liveFreshObservation = false;
  dataSource.liveStale = dataSource.live !== null;
}

/** Record a live-mode failure: a previous live document (if any) stays. */
export function setLiveUnavailable(message: string): void {
  dataSource.liveUnavailable = message;
  dataSource.liveStale = dataSource.live !== null;
}

/** Live staleness (doc 04): a previous live document with a failed refresh. */
export function liveStale(): boolean {
  return dataSource.liveStale;
}

/** Last live-mode failure reason, if any. */
export function liveUnavailable(): string | null {
  return dataSource.liveUnavailable ?? dataSource.liveMonitorReason;
}

/**
 * The truthful live state. No live snapshot means unavailable; a live
 * snapshot accompanied by a failed or pending refresh is stale; otherwise
 * the live pair is current.
 */
export function liveStatus(): LiveStatus {
  if (dataSource.live === null) return 'unavailable';
  if (
    dataSource.liveTopology === null ||
    dataSource.liveTopology.sourceTimestampMs !== dataSource.live.timestampMs
  ) {
    return 'stale';
  }
  if (dataSource.liveWatchState !== 'active') return 'stale';
  if (dataSource.liveMonitorReason !== null) return 'stale';
  if (dataSource.liveStale || dataSource.liveUnavailable !== null) return 'stale';
  return 'live';
}

/// The snapshot the snapshot-driven views render. The selected mode is
/// authoritative: live documents are never presented as imports, and
/// imported documents are never silently replaced by a later live event.
export function currentSnapshot(): NetworkSnapshot | null {
  if (dataSource.mode === 'live') return dataSource.live;
  if (dataSource.mode === 'imported') return dataSource.snapshot?.document ?? null;
  return scenarioSnapshot(dataSource.mockScenario);
}

/// The topology document the topology view renders. As with snapshots, the
/// selected mode is authoritative and live/imported documents never mix.
export function currentTopology(): TopologyDocument | null {
  if (dataSource.mode === 'live') return dataSource.liveTopology;
  if (dataSource.mode === 'imported') return dataSource.topology?.document ?? null;
  return scenarioTopology(dataSource.mockScenario);
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
