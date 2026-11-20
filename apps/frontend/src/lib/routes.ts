// Route presentation logic: the main-table default view with an explicit,
// visible filter for other tables. Pure and unit-tested.

import type { AddressFamily, NetworkSnapshot, Route } from '../types';
import { formatRouteVia } from './format';

export const MAIN_TABLE = 254;

// Navigation identity for a route (WO-04 Stage A destination foundation):
// the facts a caller naturally knows ("the default route via 198.51.100.1").
// Resolution picks the first route whose family, destination, gateway,
// metric, and TOS match — protocol/scope/hops are deliberately not part of
// the navigation identity (full document-identity comparison remains
// sameRouteIdentity below).
export interface RouteLookup {
  family: AddressFamily;
  destination: { address: string; prefixLen: number } | null;
  gateway: string | null;
  metric: number | null;
  tos: number | null;
}

/** Find the route a navigation reference points at, in the given routes —
 *  null when no route matches (the caller shows the panel's empty state;
 *  nothing stale is rendered). Fields the serializer omits (undefined) and
 *  explicit nulls are the same "not observed" state, so both sides are
 *  normalized before comparing. */
export function findRouteByRef(routes: Route[], ref: RouteLookup): Route | null {
  return (
    routes.find(
      (route) =>
        route.family === ref.family &&
        ((route.destination === null && ref.destination === null) ||
          (route.destination !== null &&
            ref.destination !== null &&
            route.destination.address === ref.destination.address &&
            route.destination.prefixLen === ref.destination.prefixLen)) &&
        (route.gateway ?? null) === (ref.gateway ?? null) &&
        (route.metric ?? null) === (ref.metric ?? null) &&
        (route.tos ?? null) === (ref.tos ?? null),
    ) ?? null
  );
}

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
  /** The routing-table id, or `null` for routes whose table was NOT
   * observed (e.g. table-less procfs rows) — never folded into main. */
  table: number | null;
  label: string;
  count: number;
}

// All routing tables present in the snapshot, with counts. The main table
// is always first (and always present in the selector, even when empty, so
// the default view is explicit). Routes whose table was NOT observed get
// their own explicit group at the end — presenting them under main would
// falsely imply a table the source never exposed (WO-03-F / AUD-03).
export function tableOptions(snapshot: NetworkSnapshot): TableOption[] {
  const counts = new Map<number | null, number>();
  for (const route of snapshot.routes) {
    const table = route.table;
    counts.set(table, (counts.get(table) ?? 0) + 1);
  }
  const options: TableOption[] = [
    {
      table: MAIN_TABLE,
      label: 'main (254)',
      count: counts.get(MAIN_TABLE) ?? 0,
    },
  ];
  const numbered = [...counts.entries()]
    .filter(([table]) => table !== null && table !== MAIN_TABLE)
    .sort((a, b) => (a[0] as number) - (b[0] as number));
  for (const [table, count] of numbered) {
    options.push({ table, label: tableLabel(table as number), count });
  }
  const notObserved = counts.get(null) ?? 0;
  if (notObserved > 0) {
    options.push({
      table: null,
      label: 'table not observed',
      count: notObserved,
    });
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
// main table plus the not-observed group when present (nothing observed is
// ever hidden by default); other tables are never deleted, they are simply
// not part of the default presentation.
export function visibleRoutes(
  snapshot: NetworkSnapshot,
  selectedTables: Array<number | null>,
): Route[] {
  const selected =
    selectedTables.length > 0 ? new Set(selectedTables) : new Set<number | null>([MAIN_TABLE]);
  return snapshot.routes.filter((route) => selected.has(route.table));
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
