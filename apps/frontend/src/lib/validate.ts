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
  // Complete RFC 4291 textual grammar (WO-03-F / AUD-10 + §5.3), matching
  // what the Rust model (std Ipv6Addr) accepts and rejects — the previous
  // hand-written check accepted malformed shapes such as ":::", "1:::2",
  // ":1:2::" and "1:2:3:4:5:6:7:8:".
  //
  // Rules:
  // * hex groups of 1-4 digits separated by single colons;
  // * at most ONE "::" compression (which stands for one or more zero
  //   groups), so with compression at most 7 explicit groups, without it
  //   exactly 8 (an IPv4 dotted tail counts as two groups);
  // * an IPv4 dotted tail is allowed only as the last group;
  // * no leading or trailing single colon, no empty groups, no zone
  //   identifiers ("::1%eth0" — the document contract has no zones; the
  //   Rust model rejects them too), no stray characters.
  if (value.length === 0 || /[^0-9a-fA-F:.]/.test(value)) return false;
  const hasV4Tail = value.includes('.');
  const firstCompression = value.indexOf('::');
  const lastCompression = value.lastIndexOf('::');

  // Validate one side of the address (a list of colon-separated groups,
  // possibly ending in an IPv4 tail on the RIGHT side only).
  const checkSide = (side: string, allowTail: boolean): { groups: number; ok: boolean } => {
    if (side === '') return { groups: 0, ok: true };
    if (side.startsWith(':') || side.endsWith(':')) return { groups: 0, ok: false };
    const groups = side.split(':');
    for (let i = 0; i < groups.length; i++) {
      const group = groups[i];
      if (group.includes('.')) {
        // An IPv4 tail may appear only as the very last group of the
        // whole address.
        if (!allowTail || i !== groups.length - 1 || !isIpv4Address(group)) {
          return { groups: 0, ok: false };
        }
      } else if (!/^[0-9a-fA-F]{1,4}$/.test(group)) {
        return { groups: 0, ok: false };
      }
    }
    return { groups: groups.length, ok: true };
  };

  if (firstCompression !== lastCompression) return false; // more than one "::"
  if (firstCompression === -1) {
    // No compression: all groups explicit; the tail counts as one group
    // here (checkSide consumed it) but represents two.
    const { groups, ok } = checkSide(value, hasV4Tail);
    if (!ok) return false;
    return groups + (hasV4Tail ? 1 : 0) === 8;
  }
  // One "::": at most 7 explicit group-equivalents on both sides combined.
  const left = checkSide(value.slice(0, firstCompression), false);
  const right = checkSide(value.slice(firstCompression + 2), hasV4Tail);
  if (!left.ok || !right.ok) return false;
  return left.groups + right.groups + (hasV4Tail ? 1 : 0) <= 7;
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
