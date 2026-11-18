// TypeScript mirror of the Rust observation model (araiguma-core).
//
// SYNC CONTRACT (docs/development.md): these types are locked to
// fixtures/snapshots/*.json from both sides — Rust tests parse every fixture
// with serde (the authoritative model), and the frontend tests parse the
// same files with these types. A change on either side that breaks the
// fixtures fails the suite. When editing this file, update the Rust model
// and the fixtures in the same commit.

export type AddressFamily = 'ipv4' | 'ipv6';
export type ObservationSource = 'netlink' | 'sysfs' | 'procfs';

export type InterfaceKind =
  | 'loopback'
  | 'ethernet'
  | 'wireless'
  | 'bridge'
  | 'bond'
  | 'dummy'
  | 'wireguard'
  | 'tunnel'
  | 'veth'
  | 'vlan'
  | 'virtual'
  | 'other';

export type OperState =
  | 'up'
  | 'down'
  | 'unknown'
  | 'testing'
  | 'dormant'
  | 'notPresent'
  | 'lowerLayerDown';

// Hardware-address observation (mirror of the Rust MacObservation, schema
// v2): a normalized address string ("aa:bb:cc:dd:ee:ff"), or one of the
// sentinels 'absent' (observed to have NO hardware address, e.g. wireguard)
// / 'notObserved'. JSON null is invalid: v1 null was ambiguous between the
// two states and is rejected by the loaders.
export type MacObservation = string;

export type MacObservationKind = 'present' | 'absent' | 'notObserved';

export type AdminState = 'up' | 'down';

export type AddressScope = 'global' | 'site' | 'link' | 'host';

export type RouteProtocol =
  | 'unspec'
  | 'redirect'
  | 'kernel'
  | 'boot'
  | 'static'
  | 'ra'
  | 'dhcp'
  | { other: number };

export type RouteScope =
  | 'universe'
  | 'site'
  | 'link'
  | 'host'
  | 'nowhere'
  | { other: number };

export type RouteType =
  | 'unspec'
  | 'unicast'
  | 'local'
  | 'broadcast'
  | 'anycast'
  | 'multicast'
  | 'blackhole'
  | 'unreachable'
  | 'prohibit'
  | 'throw'
  | 'nat'
  | 'xresolve'
  | { other: number };

export interface IpNet {
  address: string;
  prefixLen: number;
}

export interface Interface {
  ifindex: number;
  name: string;
  kind: InterfaceKind;
  kindDetail: string | null;
  operState: OperState | null;
  adminState: AdminState | null;
  macAddress: MacObservation;
  mtu: number | null;
  txQueueLen: number | null;
  masterIfindex: number | null;
  parentIfindex: number | null;
  sources: ObservationSource[];
  notes: string[];
}

export interface Address {
  ifindex: number;
  family: AddressFamily;
  address: string;
  prefixLen: number;
  scope: AddressScope | null;
  flags: string[] | null;
  label: string | null;
  sources: ObservationSource[];
}

export interface Route {
  family: AddressFamily;
  destination: IpNet | null;
  gateway: string | null;
  outputInterface: number | null;
  outputInterfaceName: string | null;
  protocol: RouteProtocol | null;
  scope: RouteScope | null;
  routeType: RouteType | null;
  metric: number | null;
  table: number | null;
  flags: string[] | null;
  sources: ObservationSource[];
  notes: string[];
}

export type ProviderId = 'netlink' | 'sysfs';

export type ProviderState =
  | 'notYetObserved'
  | 'available'
  | 'degraded'
  | 'partial'
  | 'unavailable'
  | 'notSupported';

export type SectionKind =
  | 'interfaces'
  | 'addressesV4'
  | 'addressesV6'
  | 'routesV4'
  | 'routesV6';

export type SectionOutcome =
  | { kind: 'observed'; count: number }
  | { kind: 'failed'; error: string }
  | { kind: 'unsupported' }
  | { kind: 'notObserved' };

export interface SectionReport {
  section: SectionKind;
  outcome: SectionOutcome;
}

export interface ProviderReport {
  provider: ProviderId;
  state: ProviderState;
  sections: SectionReport[];
  notes: string[];
}

export interface NetworkSnapshot {
  schemaVersion: number;
  timestampMs: number;
  interfaces: Interface[];
  addresses: Address[];
  routes: Route[];
  providers: ProviderReport[];
}

// ── Topology documents (mirror of crates/core/src/topology.rs) ────────────

export type NodeId =
  | { interface: { ifindex: number } }
  | { gateway: { address: string } };

export type TopologyNodeKind = 'interface' | 'gateway';

export type TopologyNodeState = 'active' | 'inactive' | 'unknown';

export type TopologyEdgeKind =
  | 'bridgeMember'
  | 'bondMember'
  | 'masterMember'
  | 'linkParent'
  | 'gatewayRoute'
  | 'gatewayInferred';

export type Evidence =
  | { observed: { source: ObservationSource } }
  | { inferred: { reason: string } };

export interface NodeDetail {
  ifindex: number | null;
  addresses: Address[];
  macAddress: MacObservation;
  mtu: number | null;
  operState: OperState | null;
  adminState: AdminState | null;
  masterIfindex: number | null;
  parentIfindex: number | null;
  routesUsing: number;
  defaultRoutes: number;
  sources: ObservationSource[];
  notes: string[];
}

export interface TopologyNode {
  id: NodeId;
  label: string;
  nodeKind: TopologyNodeKind;
  interfaceKind: InterfaceKind | null;
  kindDetail: string | null;
  state: TopologyNodeState;
  annotations: string[];
  detail: NodeDetail;
}

export interface TopologyEdge {
  from: NodeId;
  to: NodeId;
  edgeKind: TopologyEdgeKind;
  evidence: Evidence;
  detail: string | null;
}

export interface TopologyDocument {
  schemaVersion: number;
  sourceTimestampMs: number;
  nodes: TopologyNode[];
  edges: TopologyEdge[];
}
