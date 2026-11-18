// Parse and validate an unknown JSON value as a NetworkSnapshot. This is
// the boundary where frontend data (fixture file, CLI output loaded by the
// user) enters the typed model — invalid input fails loudly, with a
// path-qualified reason, instead of rendering fabricated state.
//
// WO-02 correction §8: the whole document structure is validated — not
// just top-level discriminants — covering interfaces, addresses, routes,
// provider sections and outcomes, provenance, and numeric ranges. Schema
// v1 documents are accepted only where unambiguous (every macAddress a
// non-null string); v1 null macAddress is rejected with the reason.

import type { NetworkSnapshot, ObservationSource } from '../types';
import { isMacObservation } from './format';
import {
  asArray,
  asEnum,
  asInt,
  asIpAddress,
  asNonEmptyString,
  asPrefixLen,
  asRecord,
  asString,
  asStringArray,
  fail,
  optional,
} from './validate';

const INTERFACE_KINDS = [
  'loopback',
  'ethernet',
  'wireless',
  'bridge',
  'bond',
  'dummy',
  'wireguard',
  'tunnel',
  'veth',
  'vlan',
  'virtual',
  'other',
] as const;

const PROVIDER_IDS = ['netlink', 'sysfs'] as const;
const PROVIDER_STATES = [
  'notYetObserved',
  'available',
  'degraded',
  'partial',
  'unavailable',
  'notSupported',
] as const;
const FAMILIES = ['ipv4', 'ipv6'] as const;
const OPER_STATES = [
  'up',
  'down',
  'unknown',
  'testing',
  'dormant',
  'notPresent',
  'lowerLayerDown',
] as const;
const ADMIN_STATES = ['up', 'down'] as const;
const ADDRESS_SCOPES = ['global', 'site', 'link', 'host'] as const;
const ROUTE_PROTOCOLS = [
  'unspec',
  'redirect',
  'kernel',
  'boot',
  'static',
  'ra',
  'dhcp',
] as const;
const ROUTE_SCOPES = ['universe', 'site', 'link', 'host', 'nowhere'] as const;
const ROUTE_TYPES = [
  'unspec',
  'unicast',
  'local',
  'broadcast',
  'anycast',
  'multicast',
  'blackhole',
  'unreachable',
  'prohibit',
  'throw',
  'nat',
  'xresolve',
] as const;
const SECTION_KINDS = [
  'interfaces',
  'addressesV4',
  'addressesV6',
  'routesV4',
  'routesV6',
] as const;
const OBSERVATION_SOURCES = ['netlink', 'sysfs', 'procfs'] as const;

function isMacObservationField(value: unknown, path: string): string {
  if (value === null || value === undefined) {
    fail(
      path,
      'must not be null/absent — v1 null was ambiguous between "observed no hardware address" and "not observed"; use the sentinels \'absent\' / \'notObserved\' (schema v2)',
    );
  }
  if (!isMacObservation(value)) {
    fail(
      path,
      `expected a hardware address ("aa:bb:cc:dd:ee:ff") or the sentinels 'absent'/'notObserved', got ${JSON.stringify(value)}`,
    );
  }
  return value;
}

function asSource(value: unknown, path: string): ObservationSource {
  return asEnum(value, OBSERVATION_SOURCES, path);
}

function asSources(value: unknown, path: string): ObservationSource[] {
  return asArray(value, path).map((item, i) => asSource(item, `${path}[${i}]`));
}

// A route protocol/scope/type: a known name, or { other: <raw value> }
// where the Rust model keeps unmapped kernel ids verbatim.
function asTaggedOrOther(
  value: unknown,
  allowed: readonly string[],
  path: string,
): string | { other: number } {
  if (typeof value === 'string') {
    return asEnum(value, allowed, path);
  }
  const record = asRecord(value, path);
  if (typeof record.other === 'number') {
    if (!Number.isInteger(record.other) || record.other < 0 || record.other > 255) {
      fail(`${path}.other`, 'expected an integer between 0 and 255');
    }
    return { other: record.other };
  }
  return asEnum(value, allowed, path);
}

function validateInterface(raw: unknown, path: string): void {
  const iface = asRecord(raw, path);
  asNonEmptyString(iface.name, `${path}.name`);
  asInt(iface.ifindex, `${path}.ifindex`, { min: 1 });
  asEnum(iface.kind, INTERFACE_KINDS, `${path}.kind`);
  optional(asString, iface.kindDetail, `${path}.kindDetail`);
  optional((v, p) => asEnum(v, OPER_STATES, p), iface.operState, `${path}.operState`);
  optional((v, p) => asEnum(v, ADMIN_STATES, p), iface.adminState, `${path}.adminState`);
  isMacObservationField(iface.macAddress, `${path}.macAddress`);
  optional((v, p) => asInt(v, p, { min: 0 }), iface.mtu, `${path}.mtu`);
  optional((v, p) => asInt(v, p, { min: 0 }), iface.txQueueLen, `${path}.txQueueLen`);
  optional((v, p) => asInt(v, p, { min: 1 }), iface.masterIfindex, `${path}.masterIfindex`);
  optional((v, p) => asInt(v, p, { min: 1 }), iface.parentIfindex, `${path}.parentIfindex`);
  optional((v, p) => asInt(v, p, { min: 1 }), iface.peerIfindex, `${path}.peerIfindex`);
  asSources(iface.sources, `${path}.sources`);
  asStringArray(iface.notes, `${path}.notes`);
}

function validateAddress(raw: unknown, path: string): void {
  const address = asRecord(raw, path);
  asInt(address.ifindex, `${path}.ifindex`, { min: 1 });
  const family = asEnum(address.family, FAMILIES, `${path}.family`);
  asIpAddress(address.address, `${path}.address`, family);
  asPrefixLen(address.prefixLen, family, `${path}.prefixLen`);
  optional((v, p) => asEnum(v, ADDRESS_SCOPES, p), address.scope, `${path}.scope`);
  optional(asStringArray, address.flags, `${path}.flags`);
  optional(asString, address.label, `${path}.label`);
  asSources(address.sources, `${path}.sources`);
}

function validateRoute(raw: unknown, path: string): void {
  const route = asRecord(raw, path);
  const family = asEnum(route.family, FAMILIES, `${path}.family`);
  if (route.destination !== null && route.destination !== undefined) {
    const destination = asRecord(route.destination, `${path}.destination`);
    asIpAddress(destination.address, `${path}.destination.address`, family);
    asPrefixLen(destination.prefixLen, family, `${path}.destination.prefixLen`);
  }
  optional((v, p) => asIpAddress(v, p, family), route.gateway, `${path}.gateway`);
  optional((v, p) => asInt(v, p, { min: 1 }), route.outputInterface, `${path}.outputInterface`);
  optional(asString, route.outputInterfaceName, `${path}.outputInterfaceName`);
  optional((v, p) => asTaggedOrOther(v, ROUTE_PROTOCOLS, p), route.protocol, `${path}.protocol`);
  optional((v, p) => asTaggedOrOther(v, ROUTE_SCOPES, p), route.scope, `${path}.scope`);
  optional((v, p) => asTaggedOrOther(v, ROUTE_TYPES, p), route.routeType, `${path}.routeType`);
  optional((v, p) => asInt(v, p, { min: 0 }), route.metric, `${path}.metric`);
  optional((v, p) => asInt(v, p, { min: 0 }), route.table, `${path}.table`);
  optional(asStringArray, route.flags, `${path}.flags`);
  asSources(route.sources, `${path}.sources`);
  asStringArray(route.notes, `${path}.notes`);
}

function validateSectionOutcome(raw: unknown, path: string): void {
  const outcome = asRecord(raw, path);
  switch (outcome.kind) {
    case 'observed':
      asInt(outcome.count, `${path}.count`, { min: 0 });
      return;
    case 'failed':
      asNonEmptyString(outcome.error, `${path}.error`);
      return;
    case 'unsupported':
    case 'notObserved':
      return;
    default:
      fail(path, `expected an outcome kind of observed, failed, unsupported, or notObserved`);
  }
}

function validateProvider(raw: unknown, path: string): void {
  const provider = asRecord(raw, path);
  asEnum(provider.provider, PROVIDER_IDS, `${path}.provider`);
  asEnum(provider.state, PROVIDER_STATES, `${path}.state`);
  for (const [i, section] of asArray(provider.sections, `${path}.sections`).entries()) {
    const sectionPath = `${path}.sections[${i}]`;
    const record = asRecord(section, sectionPath);
    asEnum(record.section, SECTION_KINDS, `${sectionPath}.section`);
    validateSectionOutcome(record.outcome, `${sectionPath}.outcome`);
  }
  asStringArray(provider.notes, `${path}.notes`);
}

export function parseSnapshot(input: unknown): NetworkSnapshot {
  try {
    return parseSnapshotInner(input);
  } catch (error) {
    // Every validation failure below already carries a path-qualified
    // reason; prefix the document kind so a bare message is still
    // self-identifying.
    if (error instanceof Error) {
      throw new Error(`invalid snapshot ${error.message}`);
    }
    throw error;
  }
}

function parseSnapshotInner(input: unknown): NetworkSnapshot {
  const root = asRecord(input, '$');
  const schema = root.schemaVersion;
  if (schema !== 1 && schema !== 2 && schema !== 3) {
    fail(
      '$.schemaVersion',
      `unsupported schema version ${String(schema)} (expected 1, 2, or 3; this build writes v3)`,
    );
  }
  asInt(root.timestampMs, '$.timestampMs', { min: 0 });

  asArray(root.interfaces, '$.interfaces').forEach((raw, i) => {
    validateInterface(raw, `$.interfaces[${i}]`);
  });
  asArray(root.addresses, '$.addresses').forEach((raw, i) => {
    validateAddress(raw, `$.addresses[${i}]`);
  });
  asArray(root.routes, '$.routes').forEach((raw, i) => {
    validateRoute(raw, `$.routes[${i}]`);
  });
  asArray(root.providers, '$.providers').forEach((raw, i) => {
    validateProvider(raw, `$.providers[${i}]`);
  });

  return {
    schemaVersion: schema,
    timestampMs: root.timestampMs as number,
    interfaces: root.interfaces as never,
    addresses: root.addresses as never,
    routes: root.routes as never,
    providers: root.providers as never,
  };
}
