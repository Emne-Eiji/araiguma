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

import type { NetworkSnapshot, ObservationSource, RouteViaAddr } from '../types';
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
  isIpv4Address,
  isIpv6Address,
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
  const sources = asArray(value, path).map((item, i) => asSource(item, `${path}[${i}]`));
  // Provenance contract (WO-03-F / AUD-06, parity with the Rust
  // projector): an observation with NO source is invalid — nothing
  // actually supplied it, and no attribution may be invented for it.
  if (sources.length === 0) {
    fail(
      path,
      'must name at least one provider — an observation without provenance is not a valid observation',
    );
  }
  return sources;
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

// The next-hop representation (WO-03 M3): 'single', { multi: [...] }, or
// { undecoded: reason }. Absence is valid (pre-M3 documents could not
// represent multipath hop lists; the Rust model defaults the field to
// 'single' and the loader normalizes to match). Explicit null is INVALID —
// same discipline as macAddress: the Rust model never writes null here,
// and a null must not silently become 'single' (parity with serde, whose
// default applies only to an absent field).

// An RTA_VIA next-hop encoding (WO-03-F / AUD-13): a gateway with its own
// address family — which may DIFFER from the route's family, so no
// family check applies — or a non-IP encoding preserved as raw bytes.
// (An EMPTY multi array is rejected inside validateNextHops: the writer
// never produces one.)
function asRouteVia(value: unknown, path: string): RouteViaAddr {
  const record = asRecord(value, path);
  if (record.inet !== undefined) {
    const raw = asString(record.inet, `${path}.inet`);
    if (!isIpv4Address(raw)) {
      fail(`${path}.inet`, `expected an IPv4 address, got ${JSON.stringify(raw)}`);
    }
    return { inet: raw };
  }
  if (record.inet6 !== undefined) {
    const raw = asString(record.inet6, `${path}.inet6`);
    if (!isIpv6Address(raw)) {
      fail(`${path}.inet6`, `expected an IPv6 address, got ${JSON.stringify(raw)}`);
    }
    return { inet6: raw };
  }
  if (record.packet !== undefined) {
    const bytes = asArray(record.packet, `${path}.packet`);
    bytes.forEach((byte, i) => asInt(byte, `${path}.packet[${i}]`, { min: 0, max: 255 }));
    return { packet: bytes as number[] };
  }
  if (record.other !== undefined) {
    const other = asRecord(record.other, `${path}.other`);
    const family = asInt(other.family, `${path}.other.family`, {
      min: 0,
      max: 65535,
    });
    const bytes = asArray(other.bytes, `${path}.other.bytes`);
    bytes.forEach((byte, i) => asInt(byte, `${path}.other.bytes[${i}]`, { min: 0, max: 255 }));
    return { other: { family, bytes: bytes as number[] } };
  }
  fail(path, 'expected {inet}, {inet6}, {packet}, or {other}');
}

function validateNextHops(value: unknown, family: 'ipv4' | 'ipv6', path: string): void {
  if (value === undefined) return;
  if (value === null) {
    fail(
      path,
      'must not be null — omit the field (pre-M3 documents) or write \'single\' / { multi } / { undecoded }',
    );
  }
  if (value === 'single') return;
  if (typeof value === 'string') {
    fail(path, `expected 'single', { multi: [...] }, or { undecoded: reason }, got ${JSON.stringify(value)}`);
  }
  const record = asRecord(value, path);
  if (record.multi !== undefined) {
    const hops = asArray(record.multi, `${path}.multi`);
    // An EMPTY multi array is rejected: the writer never produces one (an
    // undecodable RTA_MULTIPATH is recorded as { undecoded } instead), so
    // an empty list can only be a hand-written or corrupted document
    // claiming a multipath observation that carries no hops.
    if (hops.length === 0) {
      fail(
        `${path}.multi`,
        'an empty hop list is not producible by the writer — an undecodable RTA_MULTIPATH is recorded as { undecoded } instead',
      );
    }
    hops.forEach((hop, i) => {
      const hopPath = `${path}.multi[${i}]`;
      const hopRecord = asRecord(hop, hopPath);
      optional((v, p) => asIpAddress(v, p, family), hopRecord.gateway, `${hopPath}.gateway`);
      optional(asRouteVia, hopRecord.via, `${hopPath}.via`);
      optional((v, p) => asInt(v, p, { min: 1 }), hopRecord.outputInterface, `${hopPath}.outputInterface`);
      optional(asString, hopRecord.outputInterfaceName, `${hopPath}.outputInterfaceName`);
      // The kernel stores rtnh_hops (weight − 1) in a u8, so the effective
      // weight is an integer between 1 and 256 as iproute2 displays it.
      optional((v, p) => asInt(v, p, { min: 1, max: 256 }), hopRecord.weight, `${hopPath}.weight`);
      optional(asStringArray, hopRecord.flags, `${hopPath}.flags`);
    });
    return;
  }
  if (record.undecoded !== undefined) {
    asNonEmptyString(record.undecoded, `${path}.undecoded`);
    return;
  }
  fail(path, `expected 'single', { multi: [...] }, or { undecoded: reason }`);
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
  // IFLA_LINK_NETNSID: a signed 32-bit netnsid (the kernel can report an
  // unassigned -1); null/absent = not reported. A netnsid means the link
  // target (veth peer / parent) lives in another network namespace.
  optional(
    (v, p) => asInt(v, p, { min: -2147483648, max: 2147483647 }),
    iface.linkNetnsid,
    `${path}.linkNetnsid`,
  );
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
  // An RTA_VIA next hop may carry an address family DIFFERENT from the
  // route's family (that is the point of the encoding), so the via
  // validator deliberately does not check against `family`.
  optional(asRouteVia, route.via, `${path}.via`);
  optional((v, p) => asInt(v, p, { min: 1 }), route.outputInterface, `${path}.outputInterface`);
  optional(asString, route.outputInterfaceName, `${path}.outputInterfaceName`);
  optional((v, p) => asTaggedOrOther(v, ROUTE_PROTOCOLS, p), route.protocol, `${path}.protocol`);
  optional((v, p) => asTaggedOrOther(v, ROUTE_SCOPES, p), route.scope, `${path}.scope`);
  optional((v, p) => asTaggedOrOther(v, ROUTE_TYPES, p), route.routeType, `${path}.routeType`);
  optional((v, p) => asInt(v, p, { min: 0 }), route.metric, `${path}.metric`);
  optional((v, p) => asInt(v, p, { min: 0 }), route.table, `${path}.table`);
  optional((v, p) => asInt(v, p, { min: 0, max: 255 }), route.tos, `${path}.tos`);
  optional(asStringArray, route.flags, `${path}.flags`);
  validateNextHops(route.nextHops, family, `${path}.nextHops`);
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
    // Pre-M3 documents carry no nextHops; normalize to 'single' so every
    // consumer can branch on the field without a null check (same default
    // the Rust serde model applies).
    routes: (root.routes as { nextHops?: unknown }[]).map((route) => ({
      ...route,
      nextHops: route.nextHops ?? 'single',
    })) as never,
    providers: root.providers as never,
  };
}
