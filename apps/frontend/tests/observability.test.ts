import { describe, expect, it } from 'vitest';
import { fixtures } from '../src/lib/fixtures';
import type { NetworkSnapshot, SectionOutcome } from '../src/types';
import {
  connectionsAvailability,
  connectionsForInterface,
  dnsAvailability,
  dnsForInterface,
  dnsServerText,
  gatewayObservations,
  gatewaysForInterface,
  interfaceStateCounts,
  isDefaultRoute,
  optionalSectionAvailability,
  outcomeSummary,
  problemSections,
  providerDisplayName,
  providerOwnerLabel,
  providerStateCounts,
  routeGateways,
  routesForInterface,
  sectionOwner,
  SECTION_ORDER,
  sectionLabel,
} from '../src/lib/observability';

const workstation = fixtures.workstation.snapshot;

function snapshot(overrides: Partial<NetworkSnapshot> = {}): NetworkSnapshot {
  return { ...workstation, ...overrides };
}

describe('SECTION vocabulary', () => {
  it('covers the seven v4 sections in wire order', () => {
    expect(SECTION_ORDER).toEqual([
      'interfaces',
      'addressesV4',
      'addressesV6',
      'routesV4',
      'routesV6',
      'dns',
      'connections',
    ]);
  });

  it('names providers and section ownership without inventing any', () => {
    expect(providerDisplayName('systemdResolved')).toBe('systemd-resolved');
    expect(providerDisplayName('networkManager')).toBe('NetworkManager');
    expect(providerDisplayName('some-other-source')).toBe('some-other-source');
    expect(providerOwnerLabel('systemdResolved')).toBe('owns DNS');
    expect(providerOwnerLabel('networkManager')).toBe('owns connections');
    expect(providerOwnerLabel('netlink')).toBeNull();
    expect(sectionOwner('dns')).toBe('systemdResolved');
    expect(sectionOwner('connections')).toBe('networkManager');
    expect(sectionOwner('interfaces')).toBeNull();
    expect(sectionLabel('addressesV4')).toBe('IPv4 addresses');
  });
});

describe('interfaceStateCounts', () => {
  it('derives up/down/unknown from observed admin + oper state only', () => {
    const counts = interfaceStateCounts(workstation);
    expect(counts.total).toBe(6);
    expect(counts.up).toBe(2);
    expect(counts.down).toBe(1);
    expect(counts.unknown).toBe(3);
    expect(counts.up + counts.down + counts.unknown).toBe(counts.total);
  });

  it('treats empty observations as zero totals, not missing data', () => {
    expect(interfaceStateCounts(snapshot({ interfaces: [] }))).toEqual({
      total: 0,
      up: 0,
      down: 0,
      unknown: 0,
    });
  });
});

describe('routes and gateways (no invented edges)', () => {
  it('isDefaultRoute requires an observed /0 destination', () => {
    expect(isDefaultRoute({ ...workstation.routes[0], destination: { address: '0.0.0.0', prefixLen: 0 } })).toBe(true);
    expect(isDefaultRoute({ ...workstation.routes[0], destination: null })).toBe(false);
    expect(isDefaultRoute({ ...workstation.routes[0], destination: { address: '0.0.0.0', prefixLen: 32 } })).toBe(false);
  });

  it('routeGateways reads single gateways and multipath hops without fabricating', () => {
    const single = workstation.routes.find((r) => r.gateway !== null)!;
    expect(routeGateways(single)).toEqual([single.gateway!]);
    const multipath = workstation.routes.find((r) => typeof r.nextHops === 'object' && 'multi' in r.nextHops)!;
    expect(routeGateways(multipath)).toEqual(['198.51.100.1']);
    expect(routeGateways({ ...single, gateway: null })).toEqual([]);
  });

  it('gatewayObservations counts distinct named gateways with sources', () => {
    const gateways = gatewayObservations(workstation);
    expect(gateways.map((g) => g.address).sort()).toEqual(['192.168.1.1', '198.51.100.1', 'fe80::1']);
    const shared = gateways.find((g) => g.address === '192.168.1.1')!;
    expect(shared.routes).toBeGreaterThanOrEqual(2);
    expect(shared.sources).toContain('netlink');
  });

  it('routesForInterface includes multipath hops; gatewaysForInterface deduplicates', () => {
    const eth0Routes = routesForInterface(workstation, 2);
    expect(eth0Routes.some((r) => r.outputInterface === 2)).toBe(true);
    const mp = workstation.routes.find((r) => typeof r.nextHops === 'object' && 'multi' in r.nextHops)!;
    expect(routesForInterface(workstation, 3).includes(mp)).toBe(true);
    expect(gatewaysForInterface(workstation, 2)).toContain('192.168.1.1');
    expect(gatewaysForInterface(workstation, 9999)).toEqual([]);
  });
});

describe('optional v4 section availability (no collapse of outcomes)', () => {
  it('observed vs observed-empty is preserved', () => {
    expect(dnsAvailability(workstation).state).toBe('observed');
    expect(connectionsAvailability(workstation).state).toBe('observed');
    expect(dnsAvailability(snapshot({ dns: [] })).state).toBe('observed-empty');
    expect(connectionsAvailability(snapshot({ connections: [] })).state).toBe('observed-empty');
  });

  it('absent collection is not observed; unsupported is not inferred from absence', () => {
    const noDns = snapshot({ dns: undefined });
    const result = dnsAvailability(noDns);
    expect(result.state).not.toBe('observed');
    expect(result.state).not.toBe('observed-empty');
    // Workstation's other providers mark dns unsupported; systemdResolved
    // would still be required to own it. Nothing is invented.
    const baselineOnly = snapshot({ dns: undefined, providers: workstation.providers.filter((p) => p.provider !== 'systemdResolved') });
    const unsupported = dnsAvailability(baselineOnly);
    expect(unsupported.state).toBe('unsupported');
  });

  it('per-kind outcomes are not flattened into unavailable', () => {
    const make = (outcome: SectionOutcome, provider: string = 'systemdResolved'): NetworkSnapshot =>
      snapshot({
        dns: undefined,
        providers: [
          {
            provider,
            state: 'partial',
            sections: [{ section: 'dns', outcome }],
            notes: [],
          },
        ],
      });
    expect(dnsAvailability(make({ kind: 'notObserved' })).state).toBe('not-observed');
    expect(dnsAvailability(make({ kind: 'unsupported' })).state).toBe('unsupported');
    expect(dnsAvailability(make({ kind: 'unavailable' })).state).toBe('unavailable');
    expect(dnsAvailability(make({ kind: 'permissionDenied' })).state).toBe('permission-denied');
    expect(dnsAvailability(make({ kind: 'failed', error: 'source-error' })).state).toBe('failed');
  });

  it('optionalSectionAvailability matches dns/connections wrappers', () => {
    expect(optionalSectionAvailability(workstation, 'dns')).toEqual(dnsAvailability(workstation));
    expect(optionalSectionAvailability(workstation, 'connections')).toEqual(connectionsAvailability(workstation));
  });
});

describe('DNS helpers', () => {
  it('dnsForInterface returns link-scoped observations for an ifindex only', () => {
    const link2 = dnsForInterface(workstation, 2);
    expect(link2).toHaveLength(1);
    expect(link2[0].scope).toEqual({ kind: 'link', ifindex: 2 });
    expect(dnsForInterface(workstation, 9999)).toEqual([]);
    expect(dnsForInterface(snapshot({ dns: undefined }), 2)).toEqual([]);
  });

  it('dnsServerText does not invent a default port', () => {
    expect(dnsServerText({ address: '192.168.1.1' })).toBe('192.168.1.1');
    expect(dnsServerText({ address: '192.168.1.1', port: 53 })).toBe('192.168.1.1:53');
  });
});

describe('connection helpers', () => {
  it('connectionsForInterface resolves devices against current observations', () => {
    expect(connectionsForInterface(workstation, 2).length).toBeGreaterThan(0);
    expect(connectionsForInterface(workstation, 9999)).toEqual([]);
    expect(connectionsForInterface(snapshot({ connections: undefined }), 2)).toEqual([]);
  });
});

describe('provider outcomeSummary keeps all six outcome kinds', () => {
  it('maps each SectionOutcome kind distinctly', () => {
    expect(outcomeSummary({ kind: 'observed', count: 3 }).label).toBe('3 observed');
    expect(outcomeSummary({ kind: 'observed', count: 0 }).label).toBe('0 observed');
    expect(outcomeSummary({ kind: 'failed', error: 'source-error' }).label).toContain('failed');
    expect(outcomeSummary({ kind: 'unavailable' }).label).toBe('unavailable');
    expect(outcomeSummary({ kind: 'permissionDenied' }).label).toBe('permission denied');
    expect(outcomeSummary({ kind: 'unsupported' }).label).toBe('not supported by this source');
    expect(outcomeSummary({ kind: 'notObserved' }).label).toContain('not observed');
    const kinds = new Set([
      outcomeSummary({ kind: 'observed', count: 0 }).tone,
      outcomeSummary({ kind: 'failed', error: 'x' }).tone,
      outcomeSummary({ kind: 'unavailable' }).tone,
      outcomeSummary({ kind: 'permissionDenied' }).tone,
      outcomeSummary({ kind: 'unsupported' }).tone,
      outcomeSummary({ kind: 'notObserved' }).tone,
    ]);
    expect(kinds.has('ok')).toBe(true);
    expect(kinds.has('bad')).toBe(true);
    expect(kinds.has('muted')).toBe(true);
  });

  it('problemSections surfaces failed/unavailable/denied, not unsupported or notObserved', () => {
    const reports = workstation.providers;
    const problems = problemSections(reports);
    for (const problem of problems) {
      expect(['failed', 'unavailable', 'permissionDenied']).toContain(problem.outcome.kind);
    }
    const synthetic = problemSections([
      {
        provider: 'x',
        state: 'partial',
        notes: [],
        sections: [
          { section: 'dns', outcome: { kind: 'unsupported' } },
          { section: 'dns', outcome: { kind: 'notObserved' } },
          { section: 'dns', outcome: { kind: 'unavailable' } },
          { section: 'dns', outcome: { kind: 'permissionDenied' } },
          { section: 'dns', outcome: { kind: 'failed', error: 'boom' } },
        ],
      },
    ]);
    expect(synthetic.map((p) => p.outcome.kind)).toEqual(['unavailable', 'permissionDenied', 'failed']);
  });

  it('providerStateCounts groups real provider states', () => {
    const counts = providerStateCounts(workstation.providers);
    const available = counts.find((c) => c.state === 'available');
    expect(available?.count).toBe(3);
    expect(counts.find((c) => c.state === 'degraded')?.count).toBe(1);
  });
});
