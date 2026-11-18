// Pure display helpers. Unit-tested; the UI renders only through these so
// wording and state semantics stay consistent (and honest: unknown stays
// unknown, missing stays "not observed").

import type {
  Address,
  Interface,
  InterfaceKind,
  IpNet,
  MacObservation,
  MacObservationKind,
  ProviderState,
  RouteProtocol,
} from '../types';

export type StateTone = 'up' | 'down' | 'unknown' | 'unavailable';

export interface StateChip {
  label: string;
  tone: StateTone;
}

// Derive the interface state chip from observed admin/oper state.
// Semantics (mirrors docs/discovery-notes.md and the topology projection's
// node_state — the two views must never disagree):
// - admin down => down (regardless of operstate)
// - operstate up => up
// - operstate down / lowerLayerDown / notPresent => down: none of these is
//   operational, and lowerLayerDown/notPresent are labeled explicitly
//   (never silently folded into "unknown state" or presented as up)
// - operstate unknown + admin up => unknown (e.g. loopback and dummy
//   devices report UNKNOWN even when functional — never shown as "up")
export function interfaceStateChip(iface: Interface): StateChip {
  if (iface.adminState === 'down') {
    return { label: 'down', tone: 'down' };
  }
  switch (iface.operState) {
    case 'up':
      return { label: 'up', tone: 'up' };
    case 'down':
      return { label: 'down', tone: 'down' };
    case 'lowerLayerDown':
      // The interface is down because a layer below it is down (e.g. a
      // bridge port whose member link is down).
      return { label: 'lower layer down', tone: 'down' };
    case 'notPresent':
      // RFC 2863: the component is physically absent.
      return { label: 'not present', tone: 'down' };
    case 'dormant':
      return { label: 'dormant', tone: 'unknown' };
    case 'testing':
      return { label: 'testing', tone: 'unknown' };
    default:
      // unknown, or not observed
      return { label: 'unknown state', tone: 'unknown' };
  }
}

// The three MAC observation states are distinct facts and must stay
// distinguishable everywhere the value is displayed.
const MAC_PATTERN = /^(?:[0-9a-f]{2}:)+[0-9a-f]{2}$/i;

export function macObservationKind(value: MacObservation): MacObservationKind {
  if (value === 'absent') return 'absent';
  if (value === 'notObserved') return 'notObserved';
  return 'present';
}

export function isMacObservation(value: unknown): value is MacObservation {
  return (
    typeof value === 'string' &&
    (value === 'absent' || value === 'notObserved' || MAC_PATTERN.test(value))
  );
}

export function formatMacObservation(value: MacObservation): string {
  switch (macObservationKind(value)) {
    case 'present':
      return value;
    case 'absent':
      return 'no hardware address (observed)';
    case 'notObserved':
      return 'not observed';
  }
}

export function kindLabel(kind: InterfaceKind): string {
  switch (kind) {
    case 'loopback':
      return 'Loopback';
    case 'ethernet':
      return 'Ethernet';
    case 'wireless':
      return 'Wi-Fi';
    case 'bridge':
      return 'Bridge';
    case 'bond':
      return 'Bond';
    case 'dummy':
      return 'Dummy';
    case 'wireguard':
      return 'WireGuard';
    case 'tunnel':
      return 'Tunnel';
    case 'veth':
      return 'veth endpoint';
    case 'vlan':
      return 'VLAN';
    case 'virtual':
      return 'Virtual (unclassified)';
    default:
      return 'Other';
  }
}

export function formatIpNet(net: IpNet | null): string {
  if (net === null) {
    return '—';
  }
  return net.prefixLen === 0 ? 'default' : `${net.address}/${net.prefixLen}`;
}

export function formatGateway(gateway: string | null): string {
  // The kernel reports "no gateway" for on-link routes; that is an
  // observation, not a gap.
  return gateway ?? 'on-link';
}

export function formatProtocol(protocol: RouteProtocol | null): string {
  if (protocol === null) {
    return '—';
  }
  if (typeof protocol === 'string') {
    return protocol;
  }
  return `protocol-${protocol.other}`;
}

export function providerStateLabel(state: ProviderState): { label: string; tone: StateTone } {
  switch (state) {
    case 'available':
      return { label: 'available', tone: 'up' };
    case 'degraded':
      return { label: 'degraded (limited fidelity)', tone: 'unknown' };
    case 'partial':
      // Covers section failures AND supported sections that were never
      // attempted — both leave the run incomplete; health is never claimed
      // from missing observations.
      return { label: 'partial / incomplete run', tone: 'down' };
    case 'unavailable':
      return { label: 'unavailable', tone: 'down' };
    case 'notSupported':
      return { label: 'not supported here', tone: 'unavailable' };
    default:
      return { label: 'not yet observed', tone: 'unknown' };
  }
}

// Pick the addresses worth showing on a compact card: global scope first,
// then link-local; at most `limit`.
export function summarizeAddresses(
  addresses: Address[],
  limit = 2,
): { shown: Address[]; more: number } {
  const sorted = [...addresses].sort((a, b) => scopeRank(a) - scopeRank(b));
  return { shown: sorted.slice(0, limit), more: Math.max(0, sorted.length - limit) };
}

function scopeRank(address: Address): number {
  switch (address.scope) {
    case 'global':
      return 0;
    case 'site':
      return 1;
    case 'link':
      return 2;
    case 'host':
      return 3;
    default:
      return 4; // not observed
  }
}

export const NOT_OBSERVED = '—';

export function formatMissing(value: string | null | undefined): string {
  return value && value.length > 0 ? value : NOT_OBSERVED;
}
