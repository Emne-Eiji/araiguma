import { beforeEach, describe, expect, it } from 'vitest';
import {
  DEFAULT_SCENARIO_ID,
  scenarioRegistry,
  scenarioById,
  scenarioSnapshot,
  scenarioTopology,
  isScenarioId,
  type ScenarioClassification,
} from '../src/lib/scenarios';
import { topologyGoldens } from '../src/lib/topology';
import { fixtures } from '../src/lib/fixtures';
import { selectMockScenario, returnToMock, dataSource } from '../src/lib/dataMode.svelte';

// WO-04 Stage A: the scenario registry. Every entry is a checked-in
// synthetic fixture pair with stable identifiers and explicit mock
// provenance — the registry describes, it never derives or simulates.

const CLASSIFICATIONS: ScenarioClassification[] = [
  'well-observed',
  'degraded-observation',
  'empty-observation',
];

describe('scenario registry integrity', () => {
  it('registers all five fixture pairs, in a stable order, with unique ids', () => {
    expect(scenarioRegistry.map((s) => s.id)).toEqual([
      'workstation',
      'incomplete-observations',
      'server-tunnel',
      'dummy-virtual',
      'empty',
    ]);
  });

  it('the default scenario is workstation and is the first entry', () => {
    expect(DEFAULT_SCENARIO_ID).toBe('workstation');
    expect(scenarioRegistry[0].id).toBe(DEFAULT_SCENARIO_ID);
  });

  it('every entry is complete and self-describing', () => {
    for (const scenario of scenarioRegistry) {
      expect(scenario.id.length).toBeGreaterThan(0);
      expect(scenario.name.length).toBeGreaterThan(0);
      expect(scenario.description.length).toBeGreaterThan(0);
      expect(CLASSIFICATIONS).toContain(scenario.classification);
      expect(scenario.classificationLabel.length).toBeGreaterThan(0);
      // The fixture references name the checked-in pair exactly.
      expect(scenario.snapshotFixture).toBe(scenario.id);
      expect(scenario.topologyGolden).toBe(scenario.id);
      // Provenance always states synthetic/mock explicitly.
      expect(scenario.provenance).toMatch(/synthetic/i);
      expect(scenario.provenance).toMatch(/not an observation/i);
    }
  });

  it('snapshot fixtures match the loaded fixture documents', () => {
    for (const scenario of scenarioRegistry) {
      const snapshot = scenarioSnapshot(scenario.id);
      expect(snapshot).toBe(fixtures[scenario.snapshotFixture].snapshot);
    }
  });

  it('topology references resolve to the Rust-projected goldens', () => {
    for (const scenario of scenarioRegistry) {
      const golden = topologyGoldens.find((g) => g.name === scenario.topologyGolden);
      expect(golden, `golden for ${scenario.id}`).toBeDefined();
      expect(scenarioTopology(scenario.id)).toBe(golden!.document);
    }
  });

  it('scenario content is what the descriptions claim (honesty spot checks)', () => {
    // workstation: well-observed, six interfaces, both providers report.
    const workstation = scenarioSnapshot('workstation');
    expect(workstation.interfaces).toHaveLength(6);
    expect(workstation.providers.map((p) => p.state).sort()).toEqual(['available', 'degraded']);

    // incomplete-observations: the primary provider failed entirely.
    const degraded = scenarioSnapshot('incomplete-observations');
    const netlink = degraded.providers.find((p) => p.provider === 'netlink');
    expect(netlink?.state).toBe('unavailable');
    expect(scenarioById('incomplete-observations').classification).toBe('degraded-observation');

    // empty: a valid observation of nothing.
    const empty = scenarioSnapshot('empty');
    expect(empty.interfaces).toHaveLength(0);
    expect(empty.routes).toHaveLength(0);
    expect(scenarioTopology('empty').nodes).toHaveLength(0);
  });

  it('scenarioById throws on unknown ids; isScenarioId is exact', () => {
    expect(() => scenarioById('nope' as never)).toThrow(/unknown scenario/);
    expect(isScenarioId('workstation')).toBe(true);
    expect(isScenarioId('nope')).toBe(false);
  });
});

describe('scenario selection through the data-mode store', () => {
  beforeEach(() => {
    selectMockScenario(DEFAULT_SCENARIO_ID);
    returnToMock();
  });

  it('selecting a scenario switches the rendered mock documents', () => {
    selectMockScenario('server-tunnel');
    expect(dataSource.mode).toBe('mock');
    expect(dataSource.mockScenario).toBe('server-tunnel');
    expect(scenarioSnapshot('server-tunnel').interfaces.map((i) => i.name)).toContain('tun0');
  });

  it('selecting a scenario is an explicit return to mock mode from imported documents', () => {
    selectMockScenario('empty');
    dataSource.mode = 'imported'; // direct state probe: simulate an import
    selectMockScenario('dummy-virtual');
    expect(dataSource.mode).toBe('mock');
    expect(dataSource.mockScenario).toBe('dummy-virtual');
    expect(dataSource.generation).toBeGreaterThan(0);
  });

  it('returning to mock keeps the selected scenario (explicitly shown by the UI)', () => {
    selectMockScenario('server-tunnel');
    returnToMock();
    expect(dataSource.mockScenario).toBe('server-tunnel');
  });
});
