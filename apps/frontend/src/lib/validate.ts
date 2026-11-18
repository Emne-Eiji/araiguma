// Small, explicit, dependency-free validation primitives shared by the
// document loaders (parseSnapshot, parseTopology). No schema framework:
// every rule is a plain function that throws a path-qualified error naming
// the exact location and the expected shape, so a rejected document is
// self-explanatory (WO-02 correction §8).

const IPV4_OCTET = /^(25[0-5]|2[0-4][0-9]|1[0-9][0-9]|[1-9]?[0-9])$/;

export function isIpv4Address(value: string): boolean {
  const parts = value.split('.');
  return (
    parts.length === 4 &&
    parts.every((part) => IPV4_OCTET.test(part)) &&
    // Leading zeros are ambiguous (octal); the Rust model never emits them.
    parts.every((part) => part.length === 1 || part[0] !== '0')
  );
}

export function isIpv6Address(value: string): boolean {
  // Structural check mirroring what the Rust model emits and accepts:
  // hex groups of 1-4 digits, optional single "::" compression, optional
  // dotted-quad tail, no stray characters. (Full RFC 4291 canonicalization
  // is the Rust side's job; this boundary rejects malformed shapes.)
  if (!/^[0-9a-fA-F:.]+$/.test(value)) return false;
  const doubleColonCount = (value.match(/::/g) ?? []).length;
  if (doubleColonCount > 1) return false;
  let head = value;
  let tail: string | null = null;
  const lastColon = value.lastIndexOf(':');
  if (value.includes('.')) {
    if (lastColon === -1) return false;
    tail = value.slice(lastColon + 1);
    head = value.slice(0, lastColon + 1); // keep the trailing colon for splitting
    if (!isIpv4Address(tail)) return false;
  }
  const groups = head.split(':').filter((g) => g.length > 0);
  const maxGroups = tail !== null ? 6 : 8;
  if (groups.length > maxGroups) return false;
  if (!groups.every((g) => /^[0-9a-fA-F]{1,4}$/.test(g))) return false;
  if (doubleColonCount === 0 && groups.length !== maxGroups) return false;
  return true;
}

export function isIpAddress(value: string): boolean {
  return value.includes(':') ? isIpv6Address(value) : isIpv4Address(value);
}

export function ipFamily(value: string): 'ipv4' | 'ipv6' | null {
  if (isIpv4Address(value)) return 'ipv4';
  if (isIpv6Address(value)) return 'ipv6';
  return null;
}

export function fail(path: string, problem: string): never {
  throw new Error(`${path}: ${problem}`);
}

export function asRecord(value: unknown, path: string): Record<string, unknown> {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    fail(path, 'expected an object');
  }
  return value as Record<string, unknown>;
}

export function asArray(value: unknown, path: string): unknown[] {
  if (!Array.isArray(value)) {
    fail(path, 'expected an array');
  }
  return value;
}

export function asString(value: unknown, path: string): string {
  if (typeof value !== 'string') {
    fail(path, 'expected a string');
  }
  return value;
}

export function asNonEmptyString(value: unknown, path: string): string {
  const result = asString(value, path);
  if (result.length === 0) {
    fail(path, 'expected a non-empty string');
  }
  return result;
}

export function asEnum<T extends string>(value: unknown, allowed: readonly T[], path: string): T {
  if (typeof value !== 'string' || !allowed.includes(value as T)) {
    fail(path, `expected one of ${allowed.join(', ')}`);
  }
  return value as T;
}

export function asInt(
  value: unknown,
  path: string,
  options: { min?: number; max?: number } = {},
): number {
  if (typeof value !== 'number' || !Number.isInteger(value)) {
    fail(path, 'expected an integer');
  }
  if (options.min !== undefined && value < options.min) {
    fail(path, `expected an integer >= ${options.min}`);
  }
  if (options.max !== undefined && value > options.max) {
    fail(path, `expected an integer <= ${options.max}`);
  }
  return value;
}

// Validate `value` as `T | null` where a missing key also means null
// (the Rust model omits nothing today, but absence is never an error for
// an optional field). Note the argument order: the validator function
// comes first, mirroring the call sites' reading ("optional string").
export function optional<T>(
  validate: (v: unknown, path: string) => T,
  value: unknown,
  path: string,
): T | null {
  if (value === undefined || value === null) {
    return null;
  }
  return validate(value, path);
}

export function asStringArray(value: unknown, path: string): string[] {
  return asArray(value, path).map((item, i) => asString(item, `${path}[${i}]`));
}

// A prefix length valid for the given address family.
export function asPrefixLen(value: unknown, family: 'ipv4' | 'ipv6', path: string): number {
  return asInt(value, path, { min: 0, max: family === 'ipv4' ? 32 : 128 });
}

// An IP address string of the expected family (when given), rejecting
// cross-family values such as an IPv4 route with an IPv6 gateway.
export function asIpAddress(
  value: unknown,
  path: string,
  expectedFamily?: 'ipv4' | 'ipv6',
): string {
  const raw = asString(value, path);
  if (!isIpAddress(raw)) {
    fail(path, `expected an IP address, got ${JSON.stringify(raw)}`);
  }
  if (expectedFamily !== undefined && ipFamily(raw) !== expectedFamily) {
    fail(path, `expected an ${expectedFamily} address, got ${JSON.stringify(raw)}`);
  }
  return raw;
}
