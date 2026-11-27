// Observation semantics for the WO-08 observability views.
//
// Everything here is a PURE derivation from the current documents. Nothing
// probes, polls, or invents a fact: a missing collection stays missing, an
// observed empty collection stays a valid observation, and provider reports
// are the only source for *why* an optional section is absent.
//
// The vocabulary is deliberately small and shared by every view:
//   observed / observed empty / not observed / unsupported / unavailable /
//   permission denied / failed
// so the UI cannot quietly collapse distinct outcomes into "unavailable".

import type {
  AddressFamily,
  ConnectionObservation,
  ConnectionState,
  DnsObservation,
  DnsScope,
  DnsServer,
  NetworkSnapshot,
  ProviderReport,
  ProviderState,
  Route,
  SectionKind,
  SectionOutcome,
} from '../types';
import { interfaceStateChip, providerStateLabel } from './format';

export type AvailabilityState =
  | 'observed'
  | 'observed-empty'
  | 'not-observed'
  | 'unsupported'
  | 'unavailable'
  | 'permission-denied'
  | 'failed';

export type EvidenceTone = 'ok' | 'warn' | 'bad' | 'muted';

export interface SectionEvidence {
  provider: string;
  outcome: SectionOutcome;
}

export interface Availability {
  state: AvailabilityState;
  /** Short label for a tile or chip. */
  label: string;
  /** One-line explanation that distinguishes the outcome. */
  detail: string;
  tone: EvidenceTone;
  /** Every provider report entry for this section, in document order. */
  reports: SectionEvidence[];
}

export interface InterfaceStateCounts {
  total: number;
  up: number;
  down: number;
  unknown: number;
}

export interface GatewayObservation {
  address: string;
  /** Routes (single-path or multipath) that name this gateway. */
  routes: number;
  defaultRoutes: number;
  families: AddressFamily[];
  sources: string[];
}

export interface OutcomeSummary {
  /** Very short cell text (the full meaning is in the accessible label). */
  short: string;
  label: string;
  tone: EvidenceTone;
  title: string;
}

const PROVIDER_NAMES: Record<string, string> = {
  netlink: 'rtnetlink (primary)',
  sysfs: 'sysfs / procfs (fallback)',
  networkManager: 'NetworkManager',
  systemdResolved: 'systemd-resolved',
};

const SECTION_LABELS: Record<string, string> = {
  interfaces: 'Interfaces',
  addressesV4: 'IPv4 addresses',
  addressesV6: 'IPv6 addresses',
  routesV4: 'IPv4 routes',
  routesV6: 'IPv6 routes',
  dns: 'DNS',
  connections: 'Connections',
};

/** The seven v4 sections, in the contract's wire order. */
export const SECTION_ORDER: readonly SectionKind[] = [
  'interfaces',
  'addressesV4',
  'addressesV6',
  'routesV4',
  'routesV6',
  'dns',
  'connections',
];

export function providerDisplayName(provider: string): string {
  return PROVIDER_NAMES[provider] ?? provider;
}

export function sectionLabel(section: SectionKind): string {
  return SECTION_LABELS[section] ?? section;
}

/** The provider that exclusively owns an optional v4 section, if any. */
export function sectionOwner(section: SectionKind): string | null {
  if (section === 'dns') return 'systemdResolved';
  if (section === 'connections') return 'networkManager';
  return null;
}

export function providerOwnerLabel(provider: string): string | null {
  if (provider === 'systemdResolved') return 'owns DNS';
  if (provider === 'networkManager') return 'owns connections';
  return null;
}

export function interfaceStateCounts(snapshot: NetworkSnapshot): InterfaceStateCounts {
  const counts: InterfaceStateCounts = {
    total: snapshot.interfaces.length,
    up: 0,
    down: 0,
    unknown: 0,
  };
  for (const iface of snapshot.interfaces) {
    const tone = interfaceStateChip(iface).tone;
    if (tone === 'up') counts.up += 1;
    else if (tone === 'down') counts.down += 1;
    else counts.unknown += 1;
  }
  return counts;
}

export function isDefaultRoute(route: Route): boolean {
  return route.destination !== null && route.destination.prefixLen === 0;
}

export function defaultRoutes(snapshot: NetworkSnapshot): Route[] {
  return snapshot.routes.filter(isDefaultRoute);
}

/** Every next-hop gateway a route names, without inventing one. */
export function routeGateways(route: Route): string[] {
  const gateways = new Set<string>();
  if (route.gateway !== null) gateways.add(route.gateway);
  if (typeof route.nextHops === 'object' && 'multi' in route.nextHops) {
    for (const hop of route.nextHops.multi) {
      if (hop.gateway !== null) gateways.add(hop.gateway);
    }
  }
  return [...gateways];
}

/** Distinct gateways named by routing-table entries (a routing fact). */
export function gatewayObservations(snapshot: NetworkSnapshot): GatewayObservation[] {
  const byAddress = new Map<string, GatewayObservation>();
  for (const route of snapshot.routes) {
    const gateways = routeGateways(route);
    for (const address of gateways) {
      const existing = byAddress.get(address);
      const observation: GatewayObservation = existing ?? {
        address,
        routes: 0,
        defaultRoutes: 0,
        families: [],
        sources: [],
      };
      observation.routes += 1;
      if (isDefaultRoute(route)) observation.defaultRoutes += 1;
      if (!observation.families.includes(route.family)) {
        observation.families.push(route.family);
      }
      for (const source of route.sources) {
        if (!observation.sources.includes(source)) observation.sources.push(source);
      }
      byAddress.set(address, observation);
    }
  }
  return [...byAddress.values()].sort((a, b) => a.address.localeCompare(b.address));
}

/** Routes that use an interface, including any multipath hop's interface. */
export function routesForInterface(snapshot: NetworkSnapshot, ifindex: number): Route[] {
  return snapshot.routes.filter((route) => {
    if (route.outputInterface === ifindex) return true;
    return (
      typeof route.nextHops === 'object' &&
      'multi' in route.nextHops &&
      route.nextHops.multi.some((hop) => hop.outputInterface === ifindex)
    );
  });
}

export function gatewaysForInterface(snapshot: NetworkSnapshot, ifindex: number): string[] {
  const gateways = new Set<string>();
  for (const route of routesForInterface(snapshot, ifindex)) {
    for (const address of routeGateways(route)) gateways.add(address);
  }
  return [...gateways].sort((a, b) => a.localeCompare(b));
}

export function dnsForInterface(
  snapshot: NetworkSnapshot,
  ifindex: number,
): DnsObservation[] {
  return (snapshot.dns ?? []).filter(
    (observation) =>
      observation.scope.kind === 'link' && observation.scope.ifindex === ifindex,
  );
}

export function connectionsForInterface(
  snapshot: NetworkSnapshot,
  ifindex: number,
): ConnectionObservation[] {
  return (snapshot.connections ?? []).filter((connection) =>
    connection.devices.includes(ifindex),
  );
}

/**
 * Derive the availability of an optional v4 collection.
 *
 * The collection field is authoritative: an array (including an empty one)
 * is an observation. When it is absent, provider reports explain the gap;
 * they are never used to manufacture data.
 */
export function optionalSectionAvailability(
  snapshot: NetworkSnapshot,
  section: 'dns' | 'connections',
): Availability {
  const collection = section === 'dns' ? snapshot.dns : snapshot.connections;
  const reports = snapshot.providers.flatMap((provider) =>
    provider.sections
      .filter((entry) => entry.section === section)
      .map((entry) => ({ provider: provider.provider, outcome: entry.outcome })),
  );

  if (collection !== undefined) {
    const count = collection.length;
    return {
      state: count === 0 ? 'observed-empty' : 'observed',
      label: count === 0 ? 'observed empty' : `${count} observed`,
      detail:
        count === 0
          ? 'A provider looked and found none — this is a valid observation, not missing data.'
          : reports.length > 0
            ? 'Supplied by a provider report in this document.'
            : 'Carried by this document; no provider report names this section.',
      tone: 'ok',
      reports,
    };
  }

  const failed = reports.find((entry) => entry.outcome.kind === 'failed');
  if (failed && failed.outcome.kind === 'failed') {
    return {
      state: 'failed',
      label: 'observation failed',
      detail: `${providerDisplayName(failed.provider)} reported a failure: ${failed.outcome.error}`,
      tone: 'bad',
      reports,
    };
  }
  const denied = reports.find((entry) => entry.outcome.kind === 'permissionDenied');
  if (denied) {
    return {
      state: 'permission-denied',
      label: 'permission denied',
      detail: `${providerDisplayName(denied.provider)} was denied access by policy.`,
      tone: 'bad',
      reports,
    };
  }
  const unavailable = reports.find((entry) => entry.outcome.kind === 'unavailable');
  if (unavailable) {
    return {
      state: 'unavailable',
      label: 'unavailable',
      detail: `${providerDisplayName(unavailable.provider)} could not reach the service or resource for this run.`,
      tone: 'bad',
      reports,
    };
  }
  const observedReport = reports.find((entry) => entry.outcome.kind === 'observed');
  if (observedReport) {
    return {
      state: 'not-observed',
      label: 'not observed',
      detail: `${providerDisplayName(observedReport.provider)} reports observations, but this document carries no ${section} collection.`,
      tone: 'bad',
      reports,
    };
  }
  const notAttempted = reports.find((entry) => entry.outcome.kind === 'notObserved');
  if (notAttempted) {
    return {
      state: 'not-observed',
      label: 'not observed',
      detail: `${providerDisplayName(notAttempted.provider)} did not attempt this section in this run.`,
      tone: 'warn',
      reports,
    };
  }
  if (reports.length > 0 && reports.every((entry) => entry.outcome.kind === 'unsupported')) {
    return {
      state: 'unsupported',
      label: 'not supported',
      detail: 'No provider in this document can observe this section in principle.',
      tone: 'muted',
      reports,
    };
  }
  return {
    state: 'not-observed',
    label: 'not observed',
    detail:
      reports.length === 0
        ? 'No provider report mentions this section, and the document carries no collection.'
        : 'No provider supplied this section in this run.',
    tone: 'warn',
    reports,
  };
}

export function dnsAvailability(snapshot: NetworkSnapshot): Availability {
  return optionalSectionAvailability(snapshot, 'dns');
}

export function connectionsAvailability(snapshot: NetworkSnapshot): Availability {
  return optionalSectionAvailability(snapshot, 'connections');
}

/** Short uppercase evidence marker for tiles and chips. */
export function availabilityEvidenceLabel(state: AvailabilityState): string {
  switch (state) {
    case 'observed':
      return 'OBSERVED';
    case 'observed-empty':
      return 'OBSERVED EMPTY';
    case 'not-observed':
      return 'NOT OBSERVED';
    case 'unsupported':
      return 'UNSUPPORTED';
    case 'unavailable':
      return 'UNAVAILABLE';
    case 'permission-denied':
      return 'PERMISSION DENIED';
    case 'failed':
      return 'FAILED';
  }
}

export function dnsServerText(server: DnsServer): string {
  return server.port === undefined ? server.address : `${server.address}:${server.port}`;
}

export function dnsScopeLabel(scope: DnsScope, snapshot: NetworkSnapshot): string {
  if (scope.kind === 'global') return 'Global scope';
  const iface = snapshot.interfaces.find((candidate) => candidate.ifindex === scope.ifindex);
  return iface
    ? `Link scope · ${iface.name}`
    : `Link scope · ifindex ${scope.ifindex} (not in this observation)`;
}

export function connectionStateLabel(state: ConnectionState): {
  label: string;
  tone: EvidenceTone;
} {
  if (typeof state === 'string') {
    switch (state) {
      case 'activated':
        return { label: 'activated', tone: 'ok' };
      case 'activating':
        return { label: 'activating', tone: 'warn' };
      case 'deactivating':
        return { label: 'deactivating', tone: 'warn' };
      case 'deactivated':
        return { label: 'deactivated', tone: 'muted' };
      case 'unknown':
        return { label: 'state unknown', tone: 'warn' };
    }
  }
  return { label: `other state (${state.other})`, tone: 'warn' };
}

export function deviceLabel(snapshot: NetworkSnapshot, ifindex: number): string {
  const iface = snapshot.interfaces.find((candidate) => candidate.ifindex === ifindex);
  return iface ? iface.name : `ifindex ${ifindex} (not in this observation)`;
}

/** The distinct outcome vocabulary used by the provider coverage matrix. */
export function outcomeSummary(outcome: SectionOutcome): OutcomeSummary {
  switch (outcome.kind) {
    case 'observed':
      return {
        short: String(outcome.count),
        label: `${outcome.count} observed`,
        tone: 'ok',
        title: 'observed (zero is a valid observation)',
      };
    case 'failed':
      return {
        short: 'failed',
        label: `failed — ${outcome.error}`,
        tone: 'bad',
        title: outcome.error,
      };
    case 'unavailable':
      return {
        short: 'unavailable',
        label: 'unavailable',
        tone: 'bad',
        title: 'the service or resource was unavailable for this run',
      };
    case 'permissionDenied':
      return {
        short: 'denied',
        label: 'permission denied',
        tone: 'bad',
        title: 'access was denied by policy',
      };
    case 'unsupported':
      return {
        short: 'n/a',
        label: 'not supported by this source',
        tone: 'muted',
        title: 'this provider has no way to observe the section in principle',
      };
    case 'notObserved':
      return {
        short: '—',
        label: 'not observed — not attempted in this run',
        tone: 'muted',
        title: 'not attempted in this run',
      };
  }
}

export function providerStateSummary(report: ProviderReport): {
  label: string;
  tone: string;
} {
  return providerStateLabel(report.state);
}

export function providerStateCounts(
  reports: readonly ProviderReport[],
): Array<{ state: ProviderState; label: string; tone: string; count: number }> {
  const states: ProviderState[] = [
    'available',
    'degraded',
    'partial',
    'unavailable',
    'notSupported',
    'notYetObserved',
  ];
  return states
    .map((state) => {
      const label = providerStateLabel(state);
      return {
        state,
        label: label.label,
        tone: label.tone,
        count: reports.filter((report) => report.state === state).length,
      };
    })
    .filter((entry) => entry.count > 0);
}

export interface ProblemSection {
  provider: string;
  section: SectionKind;
  outcome: SectionOutcome;
  summary: OutcomeSummary;
}

/** Failed / unavailable / denied sections, the provider-health attention list. */
export function problemSections(reports: readonly ProviderReport[]): ProblemSection[] {
  const problems: ProblemSection[] = [];
  for (const report of reports) {
    for (const entry of report.sections) {
      if (
        entry.outcome.kind === 'failed' ||
        entry.outcome.kind === 'unavailable' ||
        entry.outcome.kind === 'permissionDenied'
      ) {
        problems.push({
          provider: report.provider,
          section: entry.section,
          outcome: entry.outcome,
          summary: outcomeSummary(entry.outcome),
        });
      }
    }
  }
  return problems;
}

export function formatCount(count: number, singular: string, plural = `${singular}s`): string {
  return `${count} ${count === 1 ? singular : plural}`;
}
