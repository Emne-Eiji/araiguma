//! Topology projection tests: per-fixture assertions including the
//! negative cases that keep the projection honest (no invented nodes, no
//! unevidenced edges).

use std::net::IpAddr;
use std::path::PathBuf;

use araiguma_core::model::ObservationSource;
use araiguma_core::topology::{
    build_topology, EdgeKind, Evidence, NodeId, NodeKind, NodeState, TopologyDocument,
    TOPOLOGY_SCHEMA_VERSION,
};
use araiguma_core::NetworkSnapshot;

fn fixture(name: &str) -> NetworkSnapshot {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/snapshots")
        .join(format!("{name}.json"));
    let raw = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path:?}: {e}"));
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("{path:?}: {e}"))
}

fn gateway(address: &str) -> NodeId {
    NodeId::Gateway {
        address: address.parse().unwrap(),
    }
}

fn interface(ifindex: u32) -> NodeId {
    NodeId::Interface { ifindex }
}

fn edges_between(
    doc: &TopologyDocument,
    from: NodeId,
    to: NodeId,
) -> Vec<&araiguma_core::TopologyEdge> {
    doc.edges
        .iter()
        .filter(|e| e.from == from && e.to == to)
        .collect()
}

#[test]
fn workstation_topology() {
    let doc = build_topology(&fixture("workstation"));

    // 6 interface nodes + 2 gateway nodes (192.168.1.1 shared by two
    // defaults, fe80::1 for IPv6). No other node kinds exist.
    assert_eq!(doc.schema_version, TOPOLOGY_SCHEMA_VERSION);
    assert_eq!(doc.source_timestamp_ms, fixture("workstation").timestamp_ms);
    let interface_nodes = doc
        .nodes
        .iter()
        .filter(|n| n.node_kind == NodeKind::Interface)
        .count();
    let gateway_nodes = doc
        .nodes
        .iter()
        .filter(|n| n.node_kind == NodeKind::Gateway)
        .count();
    assert_eq!(interface_nodes, 6, "every interface becomes a node");
    assert_eq!(
        gateway_nodes,
        2,
        "exactly two gateways observed: {:?}",
        doc.nodes
            .iter()
            .filter(|n| n.node_kind == NodeKind::Gateway)
            .map(|n| &n.label)
            .collect::<Vec<_>>()
    );

    // Both defaults through the same gateway share one gateway node, with
    // one edge per interface.
    let gw = gateway("192.168.1.1");
    let eth0 = edges_between(&doc, interface(2), gw);
    let wlp3s0_v4 = edges_between(&doc, interface(3), gw);
    assert_eq!(eth0.len(), 1);
    assert_eq!(eth0[0].edge_kind, EdgeKind::GatewayRoute);
    assert_eq!(
        eth0[0].evidence,
        Evidence::Observed {
            source: ObservationSource::Netlink
        }
    );
    assert!(eth0[0].detail.as_deref().unwrap().contains("default"));
    assert_eq!(wlp3s0_v4.len(), 1);
    // And the IPv6 default through fe80::1 on wlp3s0.
    assert_eq!(
        edges_between(&doc, interface(3), gateway("fe80::1")).len(),
        1
    );

    // No bridge members in this scenario: br0 has no ports, so no
    // bridgeMember edges at all.
    assert!(!doc
        .edges
        .iter()
        .any(|e| e.edge_kind == EdgeKind::BridgeMember));

    // wg0's connected route (10.10.0.0/24) has no gateway: no edge, no
    // gateway node, and NO invented internet node.
    assert!(!doc.nodes.iter().any(|n| n.label.contains("internet")));
    assert!(!doc.nodes.iter().any(|n| n.id == gateway("10.10.0.0")));
    let wg0 = doc.nodes.iter().find(|n| n.label == "wg0").unwrap();
    assert_eq!(
        wg0.state,
        NodeState::Unknown,
        "up wireguard reports operstate unknown"
    );
    assert!(wg0
        .annotations
        .iter()
        .any(|a| a.contains("peers and endpoints are not observable")));

    // State derivation: eth0/wlp3s0 active, br0 inactive (admin down),
    // dummy0 unknown (admin up, operstate unknown — kernel behavior).
    let state = |label: &str| doc.nodes.iter().find(|n| n.label == label).unwrap().state;
    assert_eq!(state("eth0"), NodeState::Active);
    assert_eq!(state("br0"), NodeState::Inactive);
    assert_eq!(state("dummy0"), NodeState::Unknown);

    // Gateway nodes never claim reachability.
    let gw_node = doc.nodes.iter().find(|n| n.id == gw).unwrap();
    assert_eq!(gw_node.state, NodeState::Unknown);
    assert!(gw_node
        .annotations
        .iter()
        .any(|a| a.contains("reachability")));
}

#[test]
fn server_tunnel_topology_missing_gateway_default() {
    let doc = build_topology(&fixture("server-tunnel"));

    // tun0's default route has NO gateway (on-link next hop): an annotation,
    // not a fabricated edge or gateway node.
    let tun0 = doc.nodes.iter().find(|n| n.label == "tun0").unwrap();
    assert!(tun0
        .annotations
        .iter()
        .any(|a| a.contains("default route without a gateway")));
    assert!(!doc.edges.iter().any(|e| e.from == interface(5)));

    // ens3's default through 203.0.113.1 is an observed edge.
    assert_eq!(
        edges_between(&doc, interface(2), gateway("203.0.113.1")).len(),
        1
    );

    // Only one gateway node exists.
    assert_eq!(
        doc.nodes
            .iter()
            .filter(|n| n.node_kind == NodeKind::Gateway)
            .count(),
        1
    );

    // Two default routes exist in the snapshot; both are visible in the
    // details (one edge + one annotation).
    let ens3 = doc.nodes.iter().find(|n| n.label == "ens3").unwrap();
    assert_eq!(ens3.detail.default_routes, 1);
    assert_eq!(tun0.detail.default_routes, 1);
}

#[test]
fn dummy_virtual_topology_structural_edges() {
    let doc = build_topology(&fixture("dummy-virtual"));

    // eth1 is a bridge port of br0.
    let bridge_edges: Vec<_> = doc
        .edges
        .iter()
        .filter(|e| e.edge_kind == EdgeKind::BridgeMember)
        .collect();
    assert_eq!(bridge_edges.len(), 1);
    assert_eq!(bridge_edges[0].from, interface(5));
    assert_eq!(bridge_edges[0].to, interface(4));

    // eth0.100's parent is eth0.
    let parent_edges: Vec<_> = doc
        .edges
        .iter()
        .filter(|e| e.edge_kind == EdgeKind::LinkParent)
        .collect();
    assert_eq!(parent_edges.len(), 1);
    assert_eq!(parent_edges[0].from, interface(3));
    assert_eq!(parent_edges[0].to, interface(2));
    assert!(parent_edges[0]
        .detail
        .as_deref()
        .unwrap()
        .contains("vlan parent"));

    // veth kinds are preserved as nodes; veth PAIRING is not observable in
    // the current model, so there must be no edge between veth0 and veth1.
    assert!(doc.nodes.iter().any(|n| n.label == "veth0"));
    assert!(doc.nodes.iter().any(|n| n.label == "veth1"));
    assert!(!doc.edges.iter().any(|e| {
        (e.from == interface(6) && e.to == interface(7))
            || (e.from == interface(7) && e.to == interface(6))
    }));

    // Unrecognized/virtual kinds are all preserved.
    let kinds: Vec<String> = doc
        .nodes
        .iter()
        .filter_map(|n| n.interface_kind.map(|k| format!("{k:?}")))
        .collect();
    for expected in ["Loopback", "Ethernet", "Vlan", "Bridge", "Veth", "Dummy"] {
        assert!(kinds.iter().any(|k| k == expected), "missing {expected}");
    }
}

#[test]
fn empty_snapshot_projects_to_empty_topology() {
    let doc = build_topology(&fixture("empty"));
    assert!(doc.nodes.is_empty());
    assert!(doc.edges.is_empty());
    assert_eq!(doc.schema_version, TOPOLOGY_SCHEMA_VERSION);
}

#[test]
fn incomplete_observations_project_with_honest_caveats() {
    let doc = build_topology(&fixture("incomplete-observations"));

    // 4 sysfs-observed interfaces.
    assert_eq!(
        doc.nodes
            .iter()
            .filter(|n| n.node_kind == NodeKind::Interface)
            .count(),
        4
    );

    // The procfs default route names eth0 and gateway 192.168.0.1: an
    // observed edge with procfs provenance.
    let edges = edges_between(&doc, interface(2), gateway("192.168.0.1"));
    assert_eq!(edges.len(), 1);
    assert_eq!(
        edges[0].evidence,
        Evidence::Observed {
            source: ObservationSource::ProcFs
        }
    );

    // Classification caveats from sysfs survive onto the nodes.
    let eth0 = doc.nodes.iter().find(|n| n.label == "eth0").unwrap();
    assert!(eth0.annotations.iter().any(|a| a.contains("sysfs cannot")));
    let wg0 = doc.nodes.iter().find(|n| n.label == "wg0").unwrap();
    assert_eq!(
        wg0.interface_kind,
        Some(araiguma_core::model::InterfaceKind::Virtual)
    );
    assert!(wg0.annotations.iter().any(|a| a.contains("ARPHRD_NONE")));
}

#[test]
fn gateway_without_oif_is_inferred_only_when_unambiguous() {
    // Build a synthetic snapshot: default route via 10.0.0.1 with no oif.
    // eth0 (10.0.0.5/24) contains the gateway -> inferred edge.
    // A second route via 192.168.5.1 with no oif and no containing subnet
    // -> gateway node exists, no edge, ambiguity annotation.
    let mut snapshot = fixture("empty");
    snapshot.interfaces = vec![interface_fixture(2, "eth0", "ethernet")];
    snapshot.addresses = vec![address_fixture(2, "10.0.0.5", 24)];
    snapshot.routes = vec![
        route_fixture(None, "0.0.0.0", 0, Some("10.0.0.1")),
        route_fixture(None, "0.0.0.0", 0, Some("192.168.5.1")),
    ];

    let doc = build_topology(&snapshot);

    let inferred = edges_between(&doc, interface(2), gateway("10.0.0.1"));
    assert_eq!(inferred.len(), 1);
    assert_eq!(inferred[0].edge_kind, EdgeKind::GatewayInferred);
    assert!(
        matches!(&inferred[0].evidence, Evidence::Inferred { reason } if reason.contains("eth0"))
    );

    let ambiguous = doc
        .nodes
        .iter()
        .find(|n| n.id == gateway("192.168.5.1"))
        .expect("gateway node kept");
    assert!(ambiguous
        .annotations
        .iter()
        .any(|a| a.contains("ambiguous or unknown")));
    assert!(edges_between(&doc, interface(2), gateway("192.168.5.1")).is_empty());
}

// --- small builders -------------------------------------------------------

use araiguma_core::model::{
    Address, AddressFamily, AddressScope, AdminState, Interface, InterfaceKind, IpNet,
    MacObservation, OperState, Route, RouteProtocol, RouteScope, RouteType,
};

#[test]
fn master_relationships_classified_by_the_masters_own_kind() {
    // WO-02 correction §6: IFLA_MASTER names the master, but only the
    // master's OWN kind observation decides the flavor. A master is never
    // assumed to be a bridge.
    let mut snapshot = fixture("empty");
    let mut bridge = interface_fixture(2, "br0", "other");
    bridge.kind = InterfaceKind::Bridge;
    let mut bond = interface_fixture(3, "bond0", "other");
    bond.kind = InterfaceKind::Bond;
    let mut team = interface_fixture(4, "team0", "other"); // unclassified master type
    team.kind = InterfaceKind::Other;
    let mut port_of_bridge = interface_fixture(10, "eth1", "ethernet");
    port_of_bridge.master_ifindex = Some(2);
    let mut member_of_bond = interface_fixture(11, "eth2", "ethernet");
    member_of_bond.master_ifindex = Some(3);
    let mut member_of_team = interface_fixture(12, "eth3", "ethernet");
    member_of_team.master_ifindex = Some(4);
    snapshot.interfaces = vec![
        bridge,
        bond,
        team,
        port_of_bridge,
        member_of_bond,
        member_of_team,
    ];

    let doc = build_topology(&snapshot);

    let bridge_edges: Vec<_> = doc
        .edges
        .iter()
        .filter(|e| e.edge_kind == EdgeKind::BridgeMember)
        .collect();
    assert_eq!(bridge_edges.len(), 1, "one bridge port edge");
    assert_eq!(bridge_edges[0].from, interface(10));
    assert_eq!(bridge_edges[0].to, interface(2));
    assert!(bridge_edges[0]
        .detail
        .as_deref()
        .unwrap()
        .contains("master observed as a bridge"));

    let bond_edges: Vec<_> = doc
        .edges
        .iter()
        .filter(|e| e.edge_kind == EdgeKind::BondMember)
        .collect();
    assert_eq!(bond_edges.len(), 1, "one bond member edge");
    assert_eq!(bond_edges[0].from, interface(11));
    assert_eq!(bond_edges[0].to, interface(3));
    assert!(bond_edges[0]
        .detail
        .as_deref()
        .unwrap()
        .contains("master observed as a bond"));

    let neutral_edges: Vec<_> = doc
        .edges
        .iter()
        .filter(|e| e.edge_kind == EdgeKind::MasterMember)
        .collect();
    assert_eq!(neutral_edges.len(), 1, "one neutral master/member edge");
    assert_eq!(neutral_edges[0].from, interface(12));
    assert_eq!(neutral_edges[0].to, interface(4));
    assert!(neutral_edges[0]
        .detail
        .as_deref()
        .unwrap()
        .contains("the master's type is not established"));
}

#[test]
fn missing_master_interface_keeps_the_member_and_records_the_unknown() {
    // The member names a master that is not part of the observation: the
    // member node stays, the limitation is annotated, and no dangling edge
    // is fabricated (the peer may be unobserved for many reasons).
    let mut snapshot = fixture("empty");
    let mut orphan = interface_fixture(10, "eth9", "ethernet");
    orphan.master_ifindex = Some(99); // not in this observation
    let mut orphan_parent = interface_fixture(11, "eth9.100", "other");
    orphan_parent.kind = InterfaceKind::Vlan;
    orphan_parent.parent_ifindex = Some(99); // also not observed
    snapshot.interfaces = vec![orphan, orphan_parent];

    let doc = build_topology(&snapshot);

    assert!(
        doc.edges
            .iter()
            .all(|e| e.edge_kind == EdgeKind::GatewayRoute
                || e.edge_kind == EdgeKind::GatewayInferred),
        "no structural edges to unobserved interfaces: {:?}",
        doc.edges
    );
    let eth9 = doc.nodes.iter().find(|n| n.label == "eth9").unwrap();
    assert!(eth9
        .annotations
        .iter()
        .any(|a| a.contains("names master ifindex 99, which is not part of this observation")));
    let vlan = doc.nodes.iter().find(|n| n.label == "eth9.100").unwrap();
    assert!(
        vlan.annotations
            .iter()
            .any(|a| a
                .contains("names parent link ifindex 99, which is not part of this observation"))
    );
}

#[test]
fn self_referential_master_and_parent_are_invalid_not_edges() {
    let mut snapshot = fixture("empty");
    let mut self_master = interface_fixture(10, "weird0", "ethernet");
    self_master.master_ifindex = Some(10); // itself
    let mut self_parent = interface_fixture(11, "weird1", "ethernet");
    self_parent.parent_ifindex = Some(11); // itself
    snapshot.interfaces = vec![self_master, self_parent];

    let doc = build_topology(&snapshot);

    assert!(
        !doc.edges.iter().any(|e| e.from == e.to),
        "a self-referential relationship is never drawn as an edge"
    );
    let weird0 = doc.nodes.iter().find(|n| n.label == "weird0").unwrap();
    assert!(weird0
        .annotations
        .iter()
        .any(|a| a.contains("names itself as master")));
    let weird1 = doc.nodes.iter().find(|n| n.label == "weird1").unwrap();
    assert!(weird1
        .annotations
        .iter()
        .any(|a| a.contains("names itself as parent link")));
}

#[test]
fn bond_members_are_never_labeled_bridge_ports() {
    // The exact defect from the review: a bond member rendered as a
    // "bridge port". With the corrected classification the bond edge is
    // BondMember and the bridge edge is BridgeMember — the two never mix.
    let mut snapshot = fixture("empty");
    let mut bridge = interface_fixture(2, "br0", "other");
    bridge.kind = InterfaceKind::Bridge;
    let mut bond = interface_fixture(3, "bond0", "other");
    bond.kind = InterfaceKind::Bond;
    let mut p1 = interface_fixture(10, "eth1", "ethernet");
    p1.master_ifindex = Some(2);
    let mut p2 = interface_fixture(11, "eth2", "ethernet");
    p2.master_ifindex = Some(3);
    snapshot.interfaces = vec![bridge, bond, p1, p2];

    let doc = build_topology(&snapshot);
    for edge in &doc.edges {
        if edge.to == interface(3) {
            assert_eq!(edge.edge_kind, EdgeKind::BondMember);
        }
        if edge.to == interface(2) {
            assert_eq!(edge.edge_kind, EdgeKind::BridgeMember);
        }
    }
}

fn interface_fixture(ifindex: u32, name: &str, kind: &str) -> Interface {
    Interface {
        ifindex,
        name: name.into(),
        kind: match kind {
            "ethernet" => InterfaceKind::Ethernet,
            _ => InterfaceKind::Other,
        },
        kind_detail: None,
        oper_state: Some(OperState::Up),
        admin_state: Some(AdminState::Up),
        mac_address: MacObservation::NotObserved,
        mtu: Some(1500),
        tx_queue_len: None,
        master_ifindex: None,
        parent_ifindex: None,
        sources: vec![ObservationSource::Netlink],
        notes: vec![],
    }
}

fn address_fixture(ifindex: u32, address: &str, prefix: u8) -> Address {
    Address {
        ifindex,
        family: AddressFamily::Ipv4,
        address: address.parse().unwrap(),
        prefix_len: prefix,
        scope: Some(AddressScope::Global),
        flags: None,
        label: None,
        sources: vec![ObservationSource::Netlink],
    }
}

fn route_fixture(oif: Option<u32>, dest: &str, prefix: u8, gateway: Option<&str>) -> Route {
    Route {
        family: AddressFamily::Ipv4,
        destination: Some(IpNet::new(IpAddr::V4(dest.parse().unwrap()), prefix)),
        gateway: gateway.map(|g| IpAddr::V4(g.parse().unwrap())),
        output_interface: oif,
        output_interface_name: None,
        protocol: Some(RouteProtocol::Static),
        scope: Some(RouteScope::Universe),
        route_type: Some(RouteType::Unicast),
        metric: Some(0),
        table: Some(254),
        flags: Some(vec![]),
        sources: vec![ObservationSource::Netlink],
        notes: vec![],
    }
}
