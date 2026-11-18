import { describe, expect, it } from 'vitest';
import {
  formatGateway,
  formatIpNet,
  formatMissing,
  formatProtocol,
  interfaceStateChip,
  kindLabel,
  providerStateLabel,
  summarizeAddresses,
} from '../src/lib/format';
import type { Address, Interface } from '../src/types';

function iface(overrides: Partial<Interface>): Interface {
  return {
    ifindex: 1,
    name: 'test0',
    kind: 'ethernet',
    kindDetail: null,
    operState: null,
    adminState: null,
    macAddress: null,
    mtu: null,
    txQueueLen: null,
    masterIfindex: null,
    parentIfindex: null,
    sources: ['netlink'],
    notes: [],
    ...overrides,
  };
}

describe('interfaceStateChip', () => {
  it('shows up when operstate is up', () => {
    expect(interfaceStateChip(iface({ operState: 'up', adminState: 'up' }))).toEqual({
      label: 'up',
      tone: 'up',
    });
  });

  it('admin down wins over operstate', () => {
    expect(interfaceStateChip(iface({ operState: 'up', adminState: 'down' })).tone).toBe(
      'down',
    );
  });

  it('unknown operstate stays unknown, never up (loopback/dummy kernel behavior)', () => {
    const chip = interfaceStateChip(iface({ operState: 'unknown', adminState: 'up' }));
    expect(chip.tone).toBe('unknown');
    expect(chip.label).toContain('unknown');
  });

  it('unobserved states stay unknown', () => {
    expect(interfaceStateChip(iface({ operState: null, adminState: null })).tone).toBe(
      'unknown',
    );
  });
});

describe('formatters', () => {
  it('formats default routes as default', () => {
    expect(formatIpNet({ address: '0.0.0.0', prefixLen: 0 })).toBe('default');
    expect(formatIpNet({ address: '::', prefixLen: 0 })).toBe('default');
    expect(formatIpNet({ address: '192.168.1.0', prefixLen: 24 })).toBe('192.168.1.0/24');
    expect(formatIpNet(null)).toBe('—');
  });

  it('gateway None means on-link, not missing', () => {
    expect(formatGateway('192.168.1.1')).toBe('192.168.1.1');
    expect(formatGateway(null)).toBe('on-link');
  });

  it('formats unknown protocol ids verbatim', () => {
    expect(formatProtocol('dhcp')).toBe('dhcp');
    expect(formatProtocol({ other: 42 })).toBe('protocol-42');
    expect(formatProtocol(null)).toBe('—');
  });

  it('renders missing values explicitly', () => {
    expect(formatMissing('aa:bb')).toBe('aa:bb');
    expect(formatMissing(null)).toBe('—');
    expect(formatMissing('')).toBe('—');
  });

  it('labels kinds humanely', () => {
    expect(kindLabel('wireless')).toBe('Wi-Fi');
    expect(kindLabel('virtual')).toBe('Virtual (unclassified)');
  });
});

describe('providerStateLabel', () => {
  it('covers all six states', () => {
    expect(providerStateLabel('available').tone).toBe('up');
    expect(providerStateLabel('degraded').tone).toBe('unknown');
    expect(providerStateLabel('partial').tone).toBe('down');
    expect(providerStateLabel('unavailable').tone).toBe('down');
    expect(providerStateLabel('notSupported').tone).toBe('unavailable');
    expect(providerStateLabel('notYetObserved').tone).toBe('unknown');
  });
});

describe('summarizeAddresses', () => {
  const addr = (address: string, scope: Address['scope']): Address => ({
    ifindex: 1,
    family: address.includes(':') ? 'ipv6' : 'ipv4',
    address,
    prefixLen: 64,
    scope,
    flags: null,
    label: null,
    sources: ['netlink'],
  });

  it('prioritizes global addresses and reports the remainder', () => {
    const { shown, more } = summarizeAddresses(
      [
        addr('fe80::1', 'link'),
        addr('192.168.1.42', 'global'),
        addr('fd00::1', 'global'),
        addr('::1', 'host'),
      ],
      2,
    );
    expect(shown.map((a) => a.address)).toEqual(['192.168.1.42', 'fd00::1']);
    expect(more).toBe(2);
  });
});
