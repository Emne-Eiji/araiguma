// Scenario registry (WO-04 Stage A): the single descriptive catalog of the
// synthetic demonstration scenarios. This is a REGISTRY, not an engine —
// it holds stable identifiers, presentation metadata, fixture references,
// and the explicit mock provenance; nothing here derives, merges, or
// simulates anything.
//
// Every scenario is a pair of checked-in synthetic fixtures (snapshot +
// topology golden) that are schema-locked by both the Rust and frontend
// test suites. Scenario content is therefore exactly as labeled: synthetic,
// never an observation of any real machine.

import type { FixtureName } from './fixtures';
import { fixtures } from './fixtures';
import { topologyGoldens } from './topology';
import type { NetworkSnapshot, TopologyDocument } from '../types';

/** Stable scenario identifier (never renamed once published — the tray
 *  destination foundation and any persisted references rely on it). */
export type ScenarioId = FixtureName;

/** Coarse, honest classification used for presentation only. */
export type ScenarioClassification =
  | 'well-observed'
  | 'degraded-observation'
  | 'empty-observation';

export interface Scenario {
  id: ScenarioId;
  /** Short display name (rail/status/documents presentation). */
  name: string;
  /** One-line description of what the synthetic observation contains. */
  description: string;
  /** Coarse classification (presentation only; never a health claim). */
  classification: ScenarioClassification;
  /** What the classification means, spelled out for the UI. */
  classificationLabel: string;
  /** Snapshot fixture reference (the merged NetworkSnapshot). */
  snapshotFixture: FixtureName;
  /** Topology golden reference (Rust-projected document). */
  topologyGolden: string;
  /** Explicit provenance line — always states synthetic/mock. */
  provenance: string;
}

const SYNTHETIC_PROVENANCE =
  'Synthetic demonstration scenario (labeled mock data) — not an observation of any machine';

const SCENARIOS: Scenario[] = [
  {
    id: 'workstation',
    name: 'Synthetic workstation',
    description:
      'Laptop-style machine: loopback, ethernet, Wi-Fi, WireGuard, bridge, dummy; IPv4/IPv6, multiple default routes, local table',
    classification: 'well-observed',
    classificationLabel: 'well-observed (both providers report)',
    snapshotFixture: 'workstation',
    topologyGolden: 'workstation',
    provenance: SYNTHETIC_PROVENANCE,
  },
  {
    id: 'incomplete-observations',
    name: 'Degraded observation',
    description:
      'The netlink provider failed entirely; only sysfs/procfs observations survive, with nulls preserved where nothing was observed',
    classification: 'degraded-observation',
    classificationLabel: 'degraded observation (primary provider failed)',
    snapshotFixture: 'incomplete-observations',
    topologyGolden: 'incomplete-observations',
    provenance: SYNTHETIC_PROVENANCE,
  },
  {
    id: 'server-tunnel',
    name: 'Server with tunnel',
    description:
      'Server with an on-link tunnel default (no gateway) alongside an ethernet default; loopback, ens3, tun0',
    classification: 'well-observed',
    classificationLabel: 'well-observed (both providers report)',
    snapshotFixture: 'server-tunnel',
    topologyGolden: 'server-tunnel',
    provenance: SYNTHETIC_PROVENANCE,
  },
  {
    id: 'dummy-virtual',
    name: 'Virtual interface structures',
    description:
      'Bridge with a port, vlan parent, veth pair, and dummy: the structural relations the topology projection draws',
    classification: 'well-observed',
    classificationLabel: 'well-observed (both providers report)',
    snapshotFixture: 'dummy-virtual',
    topologyGolden: 'dummy-virtual',
    provenance: SYNTHETIC_PROVENANCE,
  },
  {
    id: 'empty',
    name: 'Empty observation',
    description:
      'No interfaces, addresses, or routes at all — a valid observation of an empty environment, not an error',
    classification: 'empty-observation',
    classificationLabel: 'empty observation (valid, not an error)',
    snapshotFixture: 'empty',
    topologyGolden: 'empty',
    provenance: SYNTHETIC_PROVENANCE,
  },
];

/** Ordered registry (display order). */
export const scenarioRegistry: readonly Scenario[] = SCENARIOS;

export const DEFAULT_SCENARIO_ID: ScenarioId = 'workstation';

export function scenarioById(id: ScenarioId): Scenario {
  const found = SCENARIOS.find((s) => s.id === id);
  if (!found) {
    throw new Error(`unknown scenario: ${id}`);
  }
  return found;
}

export function isScenarioId(value: string): value is ScenarioId {
  return SCENARIOS.some((s) => s.id === value);
}

/** The scenario's synthetic snapshot document. */
export function scenarioSnapshot(id: ScenarioId): NetworkSnapshot {
  return fixtures[id].snapshot;
}

/** The scenario's synthetic topology golden (projected by the Rust code,
 *  never derived by the UI). */
export function scenarioTopology(id: ScenarioId): TopologyDocument {
  const golden = topologyGoldens.find((g) => g.name === id);
  if (!golden) {
    throw new Error(`no topology golden for scenario: ${id}`);
  }
  return golden.document;
}
