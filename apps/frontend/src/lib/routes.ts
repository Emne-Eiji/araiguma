// Route presentation logic: the main-table default view with an explicit,
// visible filter for other tables. Pure and unit-tested.

import type { NetworkSnapshot, Route } from '../types';
import { formatRouteVia } from './format';

export const MAIN_TABLE = 254;

// WO-03 M3: route next-hop representation predicates. 'single' routes put
// their next hop in the route-level fields; 'multi' routes carry every
// observed hop in nextHops.multi (route-level gateway/outputInterface are
// null and must NOT be presented as on-link — the hops carry the gateways);
// 'undecoded' marks an RTA_MULTIPATH the kernel sent but could not be
// decoded (reason kept verbatim).
export function isMultipath(route: Route): boolean {
  return typeof route.nextHops === 'object' && 'multi' in route.nextHops;
}

export function undecodedHopsReason(route: Route): string | null {
  return typeof route.nextHops === 'object' && 'undecoded' in route.nextHops
    ? route.nextHops.undecoded
    : null;
}

export function hopCount(route: Route): number {
  return isMultipath(route) ? (route.nextHops as { multi: unknown[] }).multi.length : 1;
}

export interface TableOption {
  table: number;
  label: string;
  count: number;
}

// All routing tables present in the snapshot, with counts. The main table
// is always first (and always present in the selector, even when empty, so
// the default view is explicit).
export function tableOptions(snapshot: NetworkSnapshot): TableOption[] {
  const counts = new Map<number, number>();
  for (const route of snapshot.routes) {
    const table = route.table ?? MAIN_TABLE;
    counts.set(table, (counts.get(table) ?? 0) + 1);
  }
  const options: TableOption[] = [
    {
      table: MAIN_TABLE,
      label: 'main (254)',
      count: counts.get(MAIN_TABLE) ?? 0,
    },
  ];
  for (const [table, count] of [...counts.entries()].sort((a, b) => a[0] - b[0])) {
    if (table !== MAIN_TABLE) {
      options.push({ table, label: tableLabel(table), count });
    }
  }
  return options;
}

export function tableLabel(table: number): string {
  switch (table) {
    case 255:
      return 'local (255)';
    case 253:
      return 'default (253)';
    case 0:
      return 'unspecified (0)';
    default:
      return `table ${table}`;
  }
}

// The routes shown for the selected tables. The default selection is the
// main table only — other tables are never deleted, they are simply not
// part of the default presentation.
export function visibleRoutes(
  snapshot: NetworkSnapshot,
  selectedTables: number[],
): Route[] {
  const selected = new Set(selectedTables.length > 0 ? selectedTables : [MAIN_TABLE]);
  return snapshot.routes.filter((route) => selected.has(route.table ?? MAIN_TABLE));
}

export function routeKey(route: Route, index: number): string {
  return [
    route.family,
    route.destination ? `${route.destination.address}/${route.destination.prefixLen}` : 'none',
    route.gateway ?? (isMultipath(route) ? `multipath-${hopCount(route)}` : 'onlink'),
    route.outputInterfaceName ?? String(route.outputInterface ?? '?'),
    route.metric ?? 'none',
    route.table ?? '?',
    route.tos ?? 'none',
    undecodedHopsReason(route) ?? '',
    index,
  ].join('|');
}

// Route identity for re-resolving a selection against a NEW document
// (WO-03-F / AUD-09): every field that distinguishes two observed routes
// participates — the same rule the Rust diff uses. Provenance (sources,
// notes) deliberately does NOT: selecting "the default route via X" keeps
// pointing at that route when the document is re-imported, but a route
// that is not present in the new document resolves to nothing and the
// selection clears.
export function sameRouteIdentity(a: Route, b: Route): boolean {
  const destinationEquals =
    a.destination === null && b.destination === null
      ? true
      : a.destination !== null &&
        b.destination !== null &&
        a.destination.address === b.destination.address &&
        a.destination.prefixLen === b.destination.prefixLen;
  return (
    a.family === b.family &&
    destinationEquals &&
    a.gateway === b.gateway &&
    JSON.stringify(a.via) === JSON.stringify(b.via) &&
    a.outputInterface === b.outputInterface &&
    a.outputInterfaceName === b.outputInterfaceName &&
    JSON.stringify(a.protocol) === JSON.stringify(b.protocol) &&
    JSON.stringify(a.scope) === JSON.stringify(b.scope) &&
    JSON.stringify(a.routeType) === JSON.stringify(b.routeType) &&
    a.metric === b.metric &&
    a.table === b.table &&
    a.tos === b.tos &&
    JSON.stringify(a.flags) === JSON.stringify(b.flags) &&
    JSON.stringify(a.nextHops) === JSON.stringify(b.nextHops)
  );
}

// Accessible summary of a route row for screen readers and keyboard
// navigation (WO-02 correction §9.4): the same facts the visual row shows,
// plus a hint that observation notes exist when they do. Multipath rows
// name the hop count (WO-03 M3) — the gateways live in the hop list, and
// "on-link" would be wrong for them.
// WO-03-F / AUD-13: the effective next-hop label of a route. A route
// carrying an RTA_VIA encoding is NEVER "on-link" — the via (with its
// own address family) is the next hop.
export function nextHopLabel(route: Route): string {
  if (route.gateway !== null) return `via ${route.gateway}`;
  // `!= null` also covers `undefined` from pre-WO-03-F documents.
  if (route.via != null) return `via ${formatRouteVia(route.via)}`;
  return 'on-link';
}

export function routeAriaLabel(route: Route): string {
  const destination =
    route.destination === null
      ? 'destination not observed'
      : route.destination.prefixLen === 0
        ? 'default route'
        : `route ${route.destination.address}/${route.destination.prefixLen}`;
  const nextHops = undecodedHopsReason(route)
    ? 'next hops not decodable'
    : isMultipath(route)
      ? `multipath with ${hopCount(route)} next hops`
      : nextHopLabel(route);
  const output =
    route.outputInterfaceName !== null
      ? `on ${route.outputInterfaceName}`
      : route.outputInterface !== null
        ? `on interface ${route.outputInterface}`
        : 'output interface not observed';
  const table = route.table ?? MAIN_TABLE;
  const notes =
    route.notes.length > 0
      ? `, carries ${route.notes.length} observation note${route.notes.length === 1 ? '' : 's'} — open details for limitations`
      : '';
  return `${destination} ${nextHops} ${output}, ${route.family}, table ${table}${notes}`;
}

// Whether a route's observation carries explicit caveats (provider notes
// such as implied-table procfs provenance or undecodable attributes).
// Surfaced as a row marker and a details-panel warning so a partial
// representation can never be mistaken for a complete one (WO-02
// correction §4; the multipath first-hop-only limitation it was built for
// is resolved by WO-03 M3, and the marker stays for any future caveat).
export function routeHasLimitationNotes(route: Route): boolean {
  return route.notes.length > 0;
}
