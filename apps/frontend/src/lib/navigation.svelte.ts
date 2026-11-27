// Application navigation (WO-04 Stage A; extended by WO-08): the stable,
// identifiable sections and a destination API. WO-08 adds the two optional
// observation surfaces (DNS and connections) that the v4 snapshot contract
// already carries.
//
// TRAY-COMPATIBILITY FOUNDATION (WO-TRAY-01 future, context only): a future
// tray/notification layer needs (a) stable navigation destinations, (b) the
// ability to navigate to a specific section, and (c) selected-object
// destinations where practical. This module provides exactly that as a
// plain store plus functions — deliberately WITHOUT any event bus,
// notification framework, indicator resolution, or tray logic: the resolver
// concept stays separate from the GUI. A future tray calls navigate(); it
// never needs to know how the shell renders.
//
// Selected-object destinations resolve through the EXISTING selection
// mechanisms in App.svelte (identity-based resolution against the current
// documents; a target that does not resolve clears the selection — the same
// discipline as WO-03-F / AUD-09 re-resolution).

import type { AddressFamily } from '../types';

export type SectionId =
  | 'overview'
  | 'topology'
  | 'interfaces'
  | 'routes'
  | 'dns'
  | 'connections'
  | 'providers'
  | 'documents';

export const SECTIONS: readonly SectionId[] = [
  'overview',
  'topology',
  'interfaces',
  'routes',
  'dns',
  'connections',
  'providers',
  'documents',
];

export interface SectionMeta {
  id: SectionId;
  /** Full label (pinned rail, overview links). */
  label: string;
  /** One-line explanation (rail tooltips, overview). */
  blurb: string;
  /** Stable test id of the rail item. */
  testid: string;
}

export const SECTION_META: Record<SectionId, SectionMeta> = {
  overview: {
    id: 'overview',
    label: 'Overview',
    blurb: 'Summary of the current observation documents',
    testid: 'nav-overview',
  },
  topology: {
    id: 'topology',
    label: 'Topology',
    blurb: 'Observed interfaces and evidenced relations only',
    testid: 'nav-topology',
  },
  interfaces: {
    id: 'interfaces',
    label: 'Interfaces',
    blurb: 'Every observed interface, address, and state',
    testid: 'nav-interfaces',
  },
  routes: {
    id: 'routes',
    label: 'Routes & Gateways',
    blurb: 'The route tables, gateways, and next hops exactly as observed',
    testid: 'nav-routes',
  },
  dns: {
    id: 'dns',
    label: 'DNS Configuration',
    blurb: 'Resolver configuration evidence by scope — never a resolution claim',
    testid: 'nav-dns',
  },
  connections: {
    id: 'connections',
    label: 'Connections',
    blurb: 'NetworkManager connection objects — not socket or flow telemetry',
    testid: 'nav-connections',
  },
  providers: {
    id: 'providers',
    label: 'Providers & Observation Health',
    blurb: 'What each observation source provided or failed to provide',
    testid: 'nav-providers',
  },
  documents: {
    id: 'documents',
    label: 'Documents & Import',
    blurb: 'Import observation documents; inspect their state and scenarios',
    testid: 'nav-documents',
  },
};

export function isSectionId(value: string): value is SectionId {
  return (SECTIONS as readonly string[]).includes(value);
}

// ── Selected-object destinations ──────────────────────────────────────────
// A reference names an entity by its stable identity facts. App.svelte
// resolves it against the CURRENT documents; an unresolvable reference
// clears the selection (never renders stale details).

/** An interface, by its kernel ifindex. */
export interface InterfaceRef {
  kind: 'interface';
  ifindex: number;
}

/** A topology node, by its stable key (lib/topology nodeIdKey). */
export interface NodeRef {
  kind: 'node';
  nodeKey: string;
}

/**
 * A route, by the identity facts a caller naturally knows ("the default
 * route via 198.51.100.1"). Resolution picks the first route whose family,
 * destination, gateway, metric, and TOS all match — fields beyond these
 * (protocol, scope, hops) are not part of the navigation identity; full
 * document-identity comparison remains sameRouteIdentity (lib/routes).
 */
export interface RouteRef {
  kind: 'route';
  family: AddressFamily;
  destination: { address: string; prefixLen: number } | null;
  gateway: string | null;
  metric: number | null;
  tos: number | null;
}

export type SelectionRef = InterfaceRef | NodeRef | RouteRef;

/** Where a navigation should land: a section, optionally with a selected
 *  object whose details panel opens (where the section has one). */
export interface NavigationDestination {
  section: SectionId;
  selection?: SelectionRef;
}

export interface NavigationState {
  section: SectionId;
  /** The last destination received (App.svelte resolves any selection ref
   *  against the current documents; null after plain section navigation). */
  destination: NavigationDestination | null;
}

export const navigation = $state<NavigationState>({
  section: 'overview',
  destination: null,
});

/** Navigate to a section (no selection). */
export function navigateTo(section: SectionId): void {
  navigation.section = section;
  navigation.destination = { section };
}

/** Navigate to a destination — a section, optionally with a selected
 *  object. This is the entry point a future tray uses; it never throws and
 *  never invents entities: an unresolvable selection clears. */
export function navigate(destination: NavigationDestination): void {
  navigation.section = destination.section;
  navigation.destination = destination;
}
