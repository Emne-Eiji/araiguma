import { describe, expect, it } from 'vitest';
import {
  SECTIONS,
  SECTION_META,
  isSectionId,
  navigation,
  navigateTo,
  navigate,
  type SectionId,
} from '../src/lib/navigation.svelte';
import { findRouteByRef } from '../src/lib/routes';
import { scenarioSnapshot } from '../src/lib/scenarios';

// WO-04 Stage A + WO-08: the stable navigation destinations (the tray-
// compatibility foundation) and selected-object reference resolution.
// WO-08 adds `dns` and `connections` for the optional v4 collections.

describe('section registry', () => {
  it('defines the eight sections in the authorized order', () => {
    expect(SECTIONS).toEqual([
      'overview',
      'topology',
      'interfaces',
      'routes',
      'dns',
      'connections',
      'providers',
      'documents',
    ]);
  });

  it('every section has complete metadata with unique rail test ids', () => {
    const testids = SECTIONS.map((s) => SECTION_META[s].testid);
    expect(new Set(testids).size).toBe(SECTIONS.length);
    for (const section of SECTIONS) {
      const meta = SECTION_META[section];
      expect(meta.id).toBe(section);
      expect(meta.label.length).toBeGreaterThan(0);
      expect(meta.blurb.length).toBeGreaterThan(0);
      expect(meta.testid.startsWith('nav-')).toBe(true);
    }
  });

  it('preserves the pre-existing rail test ids (e2e compatibility)', () => {
    expect(SECTION_META.overview.testid).toBe('nav-overview');
    expect(SECTION_META.topology.testid).toBe('nav-topology');
    expect(SECTION_META.routes.testid).toBe('nav-routes');
    expect(SECTION_META.providers.testid).toBe('nav-providers');
    expect(SECTION_META.documents.testid).toBe('nav-documents');
  });

  it('exposes the WO-08 DNS and connections sections with stable ids', () => {
    expect(SECTION_META.dns.testid).toBe('nav-dns');
    expect(SECTION_META.dns.label).toBe('DNS Configuration');
    expect(SECTION_META.connections.testid).toBe('nav-connections');
    expect(SECTION_META.connections.label).toBe('Connections');
  });

  it('uses the authorized section labels', () => {
    expect(SECTION_META.routes.label).toBe('Routes & Gateways');
    expect(SECTION_META.providers.label).toBe('Providers & Observation Health');
    expect(SECTION_META.documents.label).toBe('Documents & Import');
    expect(SECTION_META.interfaces.label).toBe('Interfaces');
  });

  it('isSectionId accepts exactly the eight sections', () => {
    for (const section of SECTIONS) {
      expect(isSectionId(section)).toBe(true);
    }
    expect(isSectionId('settings')).toBe(false);
    expect(isSectionId('')).toBe(false);
  });
});

describe('navigation store (destinations)', () => {
  it('navigateTo switches the section with a plain destination', () => {
    navigateTo('interfaces');
    expect(navigation.section).toBe('interfaces');
    expect(navigation.destination).toEqual({ section: 'interfaces' });

    navigateTo('documents');
    expect(navigation.section).toBe('documents');
  });

  it('navigate carries a selected-object destination unchanged', () => {
    const destination = {
      section: 'topology' as SectionId,
      selection: {
        kind: 'route' as const,
        family: 'ipv4' as const,
        destination: { address: '0.0.0.0', prefixLen: 0 },
        gateway: '192.168.1.1',
        metric: 100,
        tos: null,
      },
    };
    navigate(destination);
    expect(navigation.section).toBe('topology');
    expect(navigation.destination).toEqual(destination);
    // The store keeps no resolver, no queue, no notification state — only
    // the destination; the shell resolves it against the current documents.
    expect(Object.keys(navigation).sort()).toEqual(['destination', 'section']);
  });

  it('every section accepts navigation (a future tray can reach each one)', () => {
    for (const section of SECTIONS) {
      navigateTo(section);
      expect(navigation.section).toBe(section);
    }
  });
});

describe('selected-object reference resolution (findRouteByRef)', () => {
  const routes = scenarioSnapshot('workstation').routes;

  it('resolves the default route via 192.168.1.1 with metric 100', () => {
    const route = findRouteByRef(routes, {
      family: 'ipv4',
      destination: { address: '0.0.0.0', prefixLen: 0 },
      gateway: '192.168.1.1',
      metric: 100,
      tos: null,
    });
    expect(route).not.toBeNull();
    expect(route!.gateway).toBe('192.168.1.1');
    expect(route!.metric).toBe(100);
  });

  it('metric is part of the navigation identity (the two defaults differ)', () => {
    const route = findRouteByRef(routes, {
      family: 'ipv4',
      destination: { address: '0.0.0.0', prefixLen: 0 },
      gateway: '192.168.1.1',
      metric: 600,
      tos: null,
    });
    expect(route).not.toBeNull();
    expect(route!.metric).toBe(600);
  });

  it('an unresolvable reference returns null (never a nearest match)', () => {
    expect(
      findRouteByRef(routes, {
        family: 'ipv4',
        destination: null,
        gateway: '203.0.113.9',
        metric: null,
        tos: null,
      }),
    ).toBeNull();
    expect(
      findRouteByRef([], {
        family: 'ipv6',
        destination: { address: '::', prefixLen: 0 },
        gateway: null,
        metric: null,
        tos: null,
      }),
    ).toBeNull();
  });
});
