// Observation document import (WO-03 M1): the safe, explicit path from
// real Araiguma CLI documents to the frontend. Pure logic — the UI shell
// in App.svelte and the data-mode store handle presentation.
//
// The strengthened Part 1 validators (lib/snapshot.ts, lib/topology.ts)
// are the document boundary: a document is either fully valid or rejected
// with a path-qualified reason; it is never partially displayed.

import type { NetworkSnapshot, TopologyDocument } from '../types';
import { parseSnapshot } from './snapshot';
import { parseTopology } from './topology';

export type ImportedDocumentKind = 'snapshot' | 'topology';

export interface ImportedDocuments {
  /** The imported snapshot, when the last successful import was one. */
  snapshot: NetworkSnapshot | null;
  /** The imported topology document, when the last successful import was one. */
  topology: TopologyDocument | null;
}

export function detectDocumentKind(json: unknown): ImportedDocumentKind {
  if (typeof json !== 'object' || json === null || Array.isArray(json)) {
    throw new Error(
      'unrecognized document: expected an Araiguma snapshot (interfaces, addresses, routes, providers) or topology document (nodes, edges)',
    );
  }
  const doc = json as Record<string, unknown>;
  const looksLikeTopology = Array.isArray(doc.nodes) && Array.isArray(doc.edges);
  const looksLikeSnapshot =
    Array.isArray(doc.interfaces) &&
    Array.isArray(doc.addresses) &&
    Array.isArray(doc.routes) &&
    Array.isArray(doc.providers);
  if (looksLikeTopology && !looksLikeSnapshot) {
    return 'topology';
  }
  if (looksLikeSnapshot && !looksLikeTopology) {
    return 'snapshot';
  }
  throw new Error(
    looksLikeTopology && looksLikeSnapshot
      ? 'ambiguous document: contains both snapshot and topology fields'
      : 'unrecognized document: expected an Araiguma snapshot (interfaces, addresses, routes, providers) or topology document (nodes, edges)',
  );
}

/// Parse imported file text into validated documents. Throws Error with a
/// user-facing reason on any failure; never returns a partially valid
/// document.
export function importDocumentText(text: string): ImportedDocuments {
  let json: unknown;
  try {
    json = JSON.parse(text);
  } catch (error) {
    throw new Error(`not valid JSON: ${(error as Error).message}`);
  }
  const kind = detectDocumentKind(json);
  if (kind === 'topology') {
    return { snapshot: null, topology: parseTopology(json) };
  }
  return { snapshot: parseSnapshot(json), topology: null };
}

const TIMESTAMP_STYLE = {
  year: 'numeric',
  month: 'short',
  day: 'numeric',
  hour: '2-digit',
  minute: '2-digit',
  second: '2-digit',
  timeZone: 'UTC',
} as const;

/// Format an observation timestamp for display (UTC, explicit).
export function formatObservationTime(timestampMs: number): string {
  const date = new Date(timestampMs);
  if (Number.isNaN(date.getTime())) {
    return 'unknown time';
  }
  return `${date.toLocaleString('en-GB', TIMESTAMP_STYLE)} UTC`;
}
