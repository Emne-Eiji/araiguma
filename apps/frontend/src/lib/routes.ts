// Route presentation logic: the main-table default view with an explicit,
// visible filter for other tables. Pure and unit-tested.

import type { NetworkSnapshot, Route } from '../types';

export const MAIN_TABLE = 254;

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
    route.gateway ?? 'onlink',
    route.outputInterfaceName ?? String(route.outputInterface ?? '?'),
    route.metric ?? 'none',
    route.table ?? '?',
    index,
  ].join('|');
}

// Accessible summary of a route row for screen readers and keyboard
// navigation (WO-02 correction §9.4): the same facts the visual row shows,
// plus a hint that observation notes exist when they do.
export function routeAriaLabel(route: Route): string {
  const destination =
    route.destination === null
      ? 'destination not observed'
      : route.destination.prefixLen === 0
        ? 'default route'
        : `route ${route.destination.address}/${route.destination.prefixLen}`;
  const gateway =
    route.gateway === null ? 'on-link' : `via ${route.gateway}`;
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
  return `${destination} ${gateway} ${output}, ${route.family}, table ${table}${notes}`;
}

// Whether a route's observation is explicitly incomplete (provider notes
// such as the current multipath first-hop-only representation). Surfaced as
// a row marker and a details-panel warning so a partial representation can
// never be mistaken for a complete one (WO-02 correction §4/§9.6).
export function routeHasLimitationNotes(route: Route): boolean {
  return route.notes.length > 0;
}
