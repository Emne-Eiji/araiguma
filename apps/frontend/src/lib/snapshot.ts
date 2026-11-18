// Parse and validate an unknown JSON value as a NetworkSnapshot. This is
// the boundary where frontend data (fixture file, CLI output loaded by the
// user) enters the typed model — invalid input fails loudly instead of
// rendering fabricated state.

import type {
  AddressFamily,
  InterfaceKind,
  NetworkSnapshot,
  ProviderId,
  ProviderState,
} from '../types';
import { isMacObservation } from './format';

const INTERFACE_KINDS: readonly string[] = [
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
];

const PROVIDER_IDS: readonly string[] = ['netlink', 'sysfs'];

const PROVIDER_STATES: readonly string[] = [
  'notYetObserved',
  'available',
  'degraded',
  'partial',
  'unavailable',
  'notSupported',
];

const FAMILIES: readonly string[] = ['ipv4', 'ipv6'];

function fail(path: string, problem: string): never {
  throw new Error(`invalid snapshot at ${path}: ${problem}`);
}

function asRecord(value: unknown, path: string): Record<string, unknown> {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    fail(path, 'expected an object');
  }
  return value as Record<string, unknown>;
}

function asArray(value: unknown, path: string): unknown[] {
  if (!Array.isArray(value)) {
    fail(path, 'expected an array');
  }
  return value;
}

function asEnum<T extends string>(
  value: unknown,
  allowed: readonly T[],
  path: string,
): T {
  if (typeof value !== 'string' || !allowed.includes(value as T)) {
    fail(path, `expected one of ${allowed.join(', ')}`);
  }
  return value as T;
}

export function parseSnapshot(input: unknown): NetworkSnapshot {
  const root = asRecord(input, '$');
  const schema = root.schemaVersion;
  if (schema !== 1 && schema !== 2) {
    fail(
      '$.schemaVersion',
      `unsupported schema version ${String(schema)} (expected 1 or 2; this build writes v2)`,
    );
  }
  if (typeof root.timestampMs !== 'number') {
    fail('$.timestampMs', 'expected a number');
  }

  const interfaces = asArray(root.interfaces, '$.interfaces').map((raw, i) => {
    const iface = asRecord(raw, `$.interfaces[${i}]`);
    if (typeof iface.name !== 'string' || iface.name.length === 0) {
      fail(`$.interfaces[${i}].name`, 'expected a non-empty string');
    }
    if (typeof iface.ifindex !== 'number') {
      fail(`$.interfaces[${i}].ifindex`, 'expected a number');
    }
    asEnum<InterfaceKind>(iface.kind, INTERFACE_KINDS as never, `$.interfaces[${i}].kind`);
    // MAC observation (schema v2): a hardware-address string or the
    // sentinels 'absent' / 'notObserved'. JSON null is rejected with an
    // explanation: v1 null was ambiguous between "observed no hardware
    // address" and "not observed", and ambiguous data is rejected rather
    // than silently assigned a meaning.
    if (iface.macAddress === null || iface.macAddress === undefined) {
      fail(
        `$.interfaces[${i}].macAddress`,
        'must not be null/absent — v1 null was ambiguous between "observed no hardware address" and "not observed"; use the sentinels \'absent\' / \'notObserved\' (schema v2)',
      );
    } else if (!isMacObservation(iface.macAddress)) {
      fail(
        `$.interfaces[${i}].macAddress`,
        `expected a hardware address ("aa:bb:cc:dd:ee:ff") or the sentinels 'absent'/'notObserved', got ${JSON.stringify(iface.macAddress)}`,
      );
    }
    return iface;
  });

  const addresses = asArray(root.addresses, '$.addresses').map((raw, i) => {
    const address = asRecord(raw, `$.addresses[${i}]`);
    if (typeof address.address !== 'string') {
      fail(`$.addresses[${i}].address`, 'expected a string');
    }
    if (typeof address.ifindex !== 'number' || typeof address.prefixLen !== 'number') {
      fail(`$.addresses[${i}]`, 'expected ifindex and prefixLen numbers');
    }
    asEnum<AddressFamily>(address.family, FAMILIES as never, `$.addresses[${i}].family`);
    return address;
  });

  const routes = asArray(root.routes, '$.routes').map((raw, i) => {
    const route = asRecord(raw, `$.routes[${i}]`);
    asEnum<AddressFamily>(route.family, FAMILIES as never, `$.routes[${i}].family`);
    return route;
  });

  const providers = asArray(root.providers, '$.providers').map((raw, i) => {
    const provider = asRecord(raw, `$.providers[${i}]`);
    asEnum<ProviderId>(provider.provider, PROVIDER_IDS as never, `$.providers[${i}].provider`);
    asEnum<ProviderState>(provider.state, PROVIDER_STATES as never, `$.providers[${i}].state`);
    asArray(provider.sections, `$.providers[${i}].sections`);
    return provider;
  });

  // The per-item shape is validated above at its discriminant fields; the
  // final cast goes through unknown because the validated records are
  // structurally wider than the target types. (v1 documents that pass the
  // macAddress check above are unambiguous and accepted with v2
  // semantics.)
  return {
    schemaVersion: root.schemaVersion as number,
    timestampMs: root.timestampMs,
    interfaces,
    addresses,
    routes,
    providers,
  } as unknown as NetworkSnapshot;
}
