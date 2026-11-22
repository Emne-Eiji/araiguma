// Document-state presentation logic for the Documents & Import view
// (WO-04 Stage A). Pure functions over a plain view of the data-source
// state — unit-tested without any Svelte runtime.
//
// The states a document kind can be in, and what each honestly means:
//   imported-ok   — a valid document of this kind is imported and rendered
//   missing       — imported mode, but this kind was never imported: the
//                   views that need it say so explicitly (never mock data)
//   not-observed  — mock mode: no real document exists for this kind; the
//                   labeled synthetic scenario is shown instead
//   live-ok / live-stale / live-unavailable — live mode's truthful engine
//                   document state; no imported or synthetic provenance is
//                   substituted for it
// plus the outcome of the most recent import attempt (accepted, or
// rejected as either 'unsupported' (e.g. an unknown schema version) or
// 'invalid' (failed validation)). No state is ever invented: everything is
// derived from the data-source state and the validators' own messages.

import type { DataMode, ImportAttempt } from './dataMode.svelte';

export type DocumentKind = 'snapshot' | 'topology';

/** Why a rejection happened, for presentation. */
export type RejectionClass = 'unsupported' | 'invalid';

/**
 * Classify a rejection reason produced by the real validators. The
 * validators' unsupported-schema message is the contract this keys on
 * ("unsupported schema version …" / "unsupported schemaVersion …"); any
 * other message is a validation failure. If the validator wording ever
 * changes, the unit test against the real validators fails loudly.
 */
export function classifyRejection(reason: string): RejectionClass {
  return /unsupported schema\s?version/i.test(reason) ? 'unsupported' : 'invalid';
}

export type DocumentSlotStatus =
  | 'imported-ok'
  | 'missing'
  | 'not-observed'
  | 'live-ok'
  | 'live-stale'
  | 'live-unavailable';

export interface DocumentSlotView {
  kind: DocumentKind;
  status: DocumentSlotStatus;
  /** File name, when a document of this kind is imported. */
  fileName: string | null;
  /** Observation time (from the DOCUMENT — when the machine was observed),
   *  formatted for display; null unless imported. */
  observedAt: string | null;
  /** Import time (when the file was read into the app) — a different fact,
   *  always labeled distinctly; null unless imported. */
  importedAt: string | null;
  /** The synthetic scenario shown instead, in mock mode. */
  scenarioName: string | null;
}

/** Minimal plain view of the data-source state (the store object satisfies
 *  this structurally; tests pass a plain object). */
export interface DocumentsStateView {
  mode: DataMode;
  snapshot: { fileName: string; importedAtMs: number; document: { timestampMs: number } } | null;
  topology: {
    fileName: string;
    importedAtMs: number;
    document: { sourceTimestampMs: number };
  } | null;
  /** Live documents are optional in this plain view so existing callers
   *  that exercise mock/imported modes need not provide them. */
  liveSnapshot?: { document: { timestampMs: number } } | null;
  liveTopology?: { document: { sourceTimestampMs: number } } | null;
  liveStale?: boolean;
  liveUnavailable?: string | null;
  mockScenarioName: string;
}

export interface RejectedImportView {
  fileName: string;
  kind: DocumentKind | null;
  reason: string;
  rejectionClass: RejectionClass;
}

export interface DocumentsView {
  snapshotSlot: DocumentSlotView;
  topologySlot: DocumentSlotView;
  /** The most recent rejected import, when the last attempt failed. */
  lastRejection: RejectedImportView | null;
  /** The most recent accepted import, when the last attempt succeeded. */
  lastAccepted: { fileName: string; kind: DocumentKind } | null;
}

export function documentViews(state: DocumentsStateView, lastImport: ImportAttempt | null): DocumentsView {
  return {
    snapshotSlot: slotView('snapshot', state),
    topologySlot: slotView('topology', state),
    lastRejection:
      lastImport && lastImport.outcome === 'rejected'
        ? {
            fileName: lastImport.fileName,
            kind: lastImport.kind,
            reason: lastImport.reason ?? 'rejected',
            rejectionClass: classifyRejection(lastImport.reason ?? ''),
          }
        : null,
    lastAccepted:
      lastImport && lastImport.outcome === 'accepted' && lastImport.kind !== null
        ? { fileName: lastImport.fileName, kind: lastImport.kind }
        : null,
  };
}

function slotView(kind: DocumentKind, state: DocumentsStateView): DocumentSlotView {
  if (state.mode === 'live') {
    const liveSlot = kind === 'snapshot' ? state.liveSnapshot : state.liveTopology;
    if (!liveSlot) {
      return {
        kind,
        status: 'live-unavailable',
        fileName: null,
        observedAt: null,
        importedAt: null,
        scenarioName: null,
      };
    }
    const observedAt = formatTime(
      kind === 'snapshot'
        ? (liveSlot.document as { timestampMs: number }).timestampMs
        : (liveSlot.document as { sourceTimestampMs: number }).sourceTimestampMs,
    );
    return {
      kind,
      status: state.liveStale ? 'live-stale' : 'live-ok',
      fileName: null,
      observedAt,
      importedAt: null,
      scenarioName: null,
    };
  }

  const slot = kind === 'snapshot' ? state.snapshot : state.topology;
  if (slot !== null) {
    return {
      kind,
      status: 'imported-ok',
      fileName: slot.fileName,
      observedAt:
        kind === 'snapshot'
          ? formatTime((slot as { document: { timestampMs: number } }).document.timestampMs)
          : formatTime((slot as { document: { sourceTimestampMs: number } }).document.sourceTimestampMs),
      importedAt: formatTime(slot.importedAtMs),
      scenarioName: null,
    };
  }
  if (state.mode === 'imported') {
    return {
      kind,
      status: 'missing',
      fileName: null,
      observedAt: null,
      importedAt: null,
      scenarioName: null,
    };
  }
  return {
    kind,
    status: 'not-observed',
    fileName: null,
    observedAt: null,
    importedAt: null,
    scenarioName: state.mockScenarioName,
  };
}

function formatTime(ms: number): string {
  const date = new Date(ms);
  if (Number.isNaN(date.getTime())) {
    return 'unknown time';
  }
  return `${date.toLocaleString('en-GB', {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    timeZone: 'UTC',
  })} UTC`;
}
