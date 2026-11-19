//! Model-driven topology projection (WO-02).
//!
//! A pure, deterministic projection from a [`NetworkSnapshot`] to a
//! [`TopologyDocument`]: nodes and edges that are **only** backed by
//! evidence. The rules (see `docs/design/wo-02-implementation.md`):
//!
//! * Every observed interface becomes a node — including unrecognized
//!   kinds, which are preserved, never dropped.
//! * Gateway nodes exist only when a route explicitly names a next-hop IP.
//!   There are deliberately **no** internet/upstream/cloud nodes: nothing in
//!   the observation model can prove an upstream, so none is invented.
//! * Edges require evidence: `bridgeMember`/`bondMember`/`masterMember`
//!   from the kernel's own master statements — the relationship exists
//!   because IFLA_MASTER names the master, and its flavor (bridge port,
//!   bond member, or neutral) comes from the master's own kind
//!   observation, never from an assumption; `linkParent` from IFLA_LINK
//!   (never for veth — their IFLA_LINK is the peer); `vethPeer` when the
//!   kernel classified an endpoint as veth and named the peer's ifindex;
//!   `gatewayRoute` from a main-table route naming both oif and gateway;
//!   and `gatewayInferred` only when a gateway without an oif falls
//!   inside exactly one interface's subnet (the derivation is recorded).
//! * Relationships that cannot be established (VPN peer, upstream beyond a
//!   gateway) stay **unknown**: they appear as node annotations, never as
//!   fabricated edges.

use std::net::IpAddr;

use serde::{Deserialize, Serialize};

use crate::model::{
    Address, AdminState, Interface, InterfaceKind, IpNet, MacObservation, NextHops,
    ObservationSource, OperState, Route, RouteType, RT_TABLE_MAIN,
};
use crate::snapshot::NetworkSnapshot;

/// Current topology document schema revision.
///
/// * **v3** (WO-03): edge kind `vethPeer` (the observed veth peer
///   relationship) and `peerIfindex` in node details.
/// * **v2** (WO-02 correction): `macAddress` in node details became the
///   three-state [`MacObservation`] string representation (address /
///   `"absent"` / `"notObserved"`); edge kinds gained `bondMember` and
///   `masterMember` (a master relationship's flavor comes from the
///   master's own kind observation, never an assumption).
/// * v1: everything else, as originally defined.
pub const TOPOLOGY_SCHEMA_VERSION: u32 = 3;

/// Identifies a topology node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NodeId {
    Interface { ifindex: u32 },
    Gateway { address: IpAddr },
}

/// What a node represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NodeKind {
    Interface,
    Gateway,
}

/// Coarse node state for layout/styling (honest derivation; gateway nodes
/// are always `unknown` because reachability is never tested).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NodeState {
    Active,
    Inactive,
    Unknown,
}

/// The kind of relationship an edge represents.
///
/// veth peer semantics (WO-03 M2, verified live on kernel 5.10): a veth
/// device's `IFLA_LINK` is its **peer's** ifindex — the kernel's veth
/// implementation reports the peer as the link, and dumps do not carry
/// `IFLA_INFO_DATA` on this kernel (the netlink crate models
/// `IFLA_VETH_PEER_IFINDEX` only on the creation path, where the peer
/// arrives as a full link message). The translator records the peer from
/// either source, and only for interfaces the kernel itself classified as
/// `veth` — for other kinds `IFLA_LINK` means something else (vlan parent,
/// tunnel underlay) and stays in `parent_ifindex`, which is why
/// [`EdgeKind::LinkParent`] is never drawn for veth interfaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EdgeKind {
    /// Interface is a port of a master interface that the observation
    /// established to be a **bridge** (IFLA_MASTER / sysfs `master`, plus
    /// the master's own kind being Bridge).
    BridgeMember,
    /// Interface is a member of a master interface that the observation
    /// established to be a **bond**. IFLA_MASTER is shared between bridges
    /// and bonds; only the master's own kind observation separates them —
    /// a master is never assumed to be a bridge.
    BondMember,
    /// Interface is a port of a master interface whose type could not be
    /// established. The kernel states the relationship (IFLA_MASTER), so
    /// the edge exists, but its flavor stays unknown and is presented
    /// neutrally.
    MasterMember,
    /// Interface's parent link (IFLA_LINK): vlan parent, tunnel underlay,
    /// or another link-layer parent. Never drawn for veth interfaces —
    /// their IFLA_LINK is the peer, represented by [`EdgeKind::VethPeer`].
    LinkParent,
    /// veth peer relationship: the kernel itself classified the endpoint as
    /// veth and named the peer's ifindex (IFLA_LINK on a veth, or the
    /// creation-path IFLA_VETH_PEER_IFINDEX). The peer may live in another
    /// network namespace — its ifindex is only meaningful when the peer is
    /// part of the same observation.
    VethPeer,
    /// A main-table route names both the output interface and the gateway.
    GatewayRoute,
    /// A route names the gateway but no output interface, and the gateway
    /// falls inside exactly one interface's subnet.
    GatewayInferred,
}

/// Why an edge exists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Evidence {
    /// A single kernel fact states the relationship.
    Observed { source: ObservationSource },
    /// Derived from more than one observation; the derivation is recorded.
    Inferred { reason: String },
}

/// Contextual detail attached to a node (enough for a details panel without
/// re-joining the snapshot).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeDetail {
    pub ifindex: Option<u32>,
    pub addresses: Vec<Address>,
    /// Hardware address observation (three states; see
    /// [`crate::model::MacObservation`]).
    pub mac_address: MacObservation,
    pub mtu: Option<u32>,
    pub oper_state: Option<OperState>,
    pub admin_state: Option<AdminState>,
    pub master_ifindex: Option<u32>,
    pub parent_ifindex: Option<u32>,
    /// veth peer ifindex, when the kernel named one (interface nodes only).
    pub peer_ifindex: Option<u32>,
    /// Netnsid of the namespace containing the link target (veth peer, or
    /// parent for other kinds) when the kernel reported one
    /// (`IFLA_LINK_NETNSID`); `None` = not reported (WO-03-F / AUD-01).
    #[serde(default)]
    pub link_netnsid: Option<i32>,
    /// Routes whose output interface is this node (interface nodes only).
    pub routes_using: usize,
    /// Default routes among them.
    pub default_routes: usize,
    pub sources: Vec<ObservationSource>,
    /// Provider-recorded caveats (verbatim from the interface).
    pub notes: Vec<String>,
}

/// A topology node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopologyNode {
    pub id: NodeId,
    pub label: String,
    pub node_kind: NodeKind,
    /// Interface classification (interface nodes only).
    pub interface_kind: Option<InterfaceKind>,
    pub kind_detail: Option<String>,
    pub state: NodeState,
    /// Honest notes: unknown relationships, classification caveats,
    /// on-link default routes, and similar.
    pub annotations: Vec<String>,
    pub detail: NodeDetail,
}

/// A topology edge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopologyEdge {
    pub from: NodeId,
    pub to: NodeId,
    pub edge_kind: EdgeKind,
    pub evidence: Evidence,
    /// Human-readable context (destinations for gateway edges, parent
    /// flavor for link-parent edges).
    pub detail: Option<String>,
}

/// The projection result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopologyDocument {
    pub schema_version: u32,
    /// Timestamp of the source snapshot (the projection itself is
    /// timeless/deterministic).
    pub source_timestamp_ms: u64,
    pub nodes: Vec<TopologyNode>,
    pub edges: Vec<TopologyEdge>,
}

const GATEWAY_NOTE: &str =
    "gateway observed via a routing-table entry; reachability is not tested — observation only";
const NO_UPSTREAM_NOTE: &str =
    "anything beyond this gateway (upstream or internet) is not observed";

/// Project a snapshot into a topology document. Pure and deterministic.
pub fn build_topology(snapshot: &NetworkSnapshot) -> TopologyDocument {
    let mut nodes: Vec<TopologyNode> = Vec::new();
    let mut edges: Vec<TopologyEdge> = Vec::new();

    // 1. Interface nodes — every observed interface, every kind.
    for iface in &snapshot.interfaces {
        nodes.push(interface_node(snapshot, iface));
    }

    // 2. Structural edges from the kernel's own master/parent statements.
    //    A master relationship's FLAVOR comes from the master's own kind
    //    observation (bridge / bond); without that evidence the edge is
    //    presented neutrally as a master/member relationship — a master is
    //    never assumed to be a bridge.
    for iface in &snapshot.interfaces {
        if let Some(master) = iface.master_ifindex {
            if master == iface.ifindex {
                annotate_interface(
                    &mut nodes,
                    iface.ifindex,
                    format!(
                        "names itself as master (ifindex {master}) — an invalid reference; no relationship drawn"
                    ),
                );
            } else if let Some(master_iface) = snapshot.interface_by_ifindex(master) {
                let (edge_kind, flavor) = match master_iface.kind {
                    InterfaceKind::Bridge => {
                        (EdgeKind::BridgeMember, "master observed as a bridge")
                    }
                    InterfaceKind::Bond => (EdgeKind::BondMember, "master observed as a bond"),
                    _ => (
                        EdgeKind::MasterMember,
                        "the master's type is not established",
                    ),
                };
                edges.push(TopologyEdge {
                    from: NodeId::Interface {
                        ifindex: iface.ifindex,
                    },
                    to: NodeId::Interface { ifindex: master },
                    edge_kind,
                    evidence: Evidence::Observed {
                        source: primary_source(&iface.sources),
                    },
                    detail: Some(format!(
                        "interface is a port of its master (IFLA_MASTER); {flavor}"
                    )),
                });
            } else {
                // The master itself is unobserved: keep the member node,
                // record the unknown, draw no dangling edge.
                annotate_interface(
                    &mut nodes,
                    iface.ifindex,
                    format!(
                        "names master ifindex {master}, which is not part of this observation (no relationship drawn)"
                    ),
                );
            }
        }
        // veth peer relationships (WO-03 M2; namespace identity corrected by
        // WO-03-F / AUD-01). ifindex is NAMESPACE-SCOPED: a peer ifindex never
        // identifies an interface of this observation on its own. The edge is
        // drawn only when the evidence establishes the relationship:
        //
        // * `IFLA_LINK_NETNSID` present (link_netnsid Some) — the kernel
        //   itself states the peer lives in ANOTHER network namespace. The
        //   peer is not part of this observation; an unrelated local
        //   interface that happens to carry the same ifindex is NEVER
        //   connected. The peer stays explicitly unresolved.
        // * netnsid absent — on this kernel a cross-namespace peer always
        //   carries an allocated netnsid (verified live), so absence places
        //   the peer in THIS namespace. The local interface at the peer
        //   ifindex is then the peer (ifindex uniqueness within a namespace)
        //   — but only if the kernel's own mutual statement confirms it:
        //   the candidate must itself be a veth naming THIS interface as its
        //   peer. A bare ifindex match (an unrelated dummy or veth with a
        //   colliding ifindex) cannot satisfy that and draws nothing.
        if iface.kind == InterfaceKind::Veth {
            if let Some(peer) = iface.peer_ifindex {
                if peer == iface.ifindex {
                    annotate_interface(
                        &mut nodes,
                        iface.ifindex,
                        "names itself as veth peer (ifindex {peer}) — an invalid reference; no relationship drawn".to_string(),
                    );
                } else if let Some(netnsid) = iface.link_netnsid {
                    // The kernel states the peer is in another network
                    // namespace. Never a local edge — a local ifindex
                    // coincidence is meaningless across namespaces.
                    annotate_interface(
                        &mut nodes,
                        iface.ifindex,
                        format!(
                            "veth peer ifindex {peer} lives in another network namespace (kernel netnsid {netnsid}); the peer is not part of this observation — no relationship drawn"
                        ),
                    );
                } else {
                    match snapshot
                        .interfaces
                        .iter()
                        .find(|candidate| candidate.ifindex == peer)
                    {
                        Some(candidate)
                            if candidate.kind == InterfaceKind::Veth
                                && candidate.peer_ifindex == Some(iface.ifindex)
                                && candidate.link_netnsid.is_none() =>
                        {
                            // Mutual kernel statements with no cross-namespace
                            // marker: both endpoints name each other. One edge
                            // represents the pair (from the lower ifindex).
                            let already_drawn = edges.iter().any(|e| {
                                e.edge_kind == EdgeKind::VethPeer
                                    && ((e.from == NodeId::Interface { ifindex: peer }
                                        && e.to
                                            == NodeId::Interface {
                                                ifindex: iface.ifindex,
                                            })
                                        || (e.from
                                            == NodeId::Interface {
                                                ifindex: iface.ifindex,
                                            }
                                            && e.to == NodeId::Interface { ifindex: peer }))
                            });
                            if !already_drawn {
                                edges.push(TopologyEdge {
                                    from: NodeId::Interface {
                                        ifindex: iface.ifindex,
                                    },
                                    to: NodeId::Interface { ifindex: peer },
                                    edge_kind: EdgeKind::VethPeer,
                                    evidence: Evidence::Observed {
                                        source: primary_source(&iface.sources),
                                    },
                                    detail: Some(
                                        "veth peer (mutual kernel-named ifindexes, same namespace)"
                                            .to_string(),
                                    ),
                                });
                            }
                        }
                        Some(candidate) => {
                            // An interface with the peer's ifindex exists
                            // locally, but it is not the peer: a same-
                            // namespace veth peer would itself be a veth
                            // naming this interface back. Either the true peer
                            // is in another namespace the dump could not mark
                            // (no netnsid observed), or the observation is
                            // incomplete — both stay unresolved rather than
                            // guessed.
                            annotate_interface(
                                &mut nodes,
                                iface.ifindex,
                                format!(
                                    "names veth peer ifindex {peer}, but interface \"{}\" at that ifindex does not name this interface back — the true peer is not established (it may live in another network namespace); no relationship drawn",
                                    candidate.name
                                ),
                            );
                        }
                        None => {
                            // The peer itself is unobserved: keep the node,
                            // record the unknown, draw no dangling edge.
                            annotate_interface(
                                &mut nodes,
                                iface.ifindex,
                                format!(
                                    "veth peer ifindex {peer} is not part of this observation (it may live in another network namespace); no relationship drawn"
                                ),
                            );
                        }
                    }
                }
            }
        }

        if let Some(parent) = iface.parent_ifindex {
            if iface.kind == InterfaceKind::Veth {
                // Defensive: the translator never sets parent_ifindex on a
                // veth, but a hand-written or foreign document might — a
                // veth's IFLA_LINK is its peer, not a parent, and drawing a
                // parent edge would misrepresent it.
                if parent != iface.peer_ifindex.unwrap_or(0) {
                    annotate_interface(
                        &mut nodes,
                        iface.ifindex,
                        format!(
                            "carries both a veth peer and a parent ifindex ({parent}); the peer relationship is authoritative, no parent edge drawn"
                        ),
                    );
                }
            } else if let Some(netnsid) = iface.link_netnsid {
                // The parent link lives in another network namespace
                // (IFLA_LINK_NETNSID): its ifindex is namespace-scoped and
                // never resolves to a local interface, even on collision.
                annotate_interface(
                    &mut nodes,
                    iface.ifindex,
                    format!(
                        "parent link ifindex {parent} lives in another network namespace (kernel netnsid {netnsid}); no relationship drawn"
                    ),
                );
            } else if parent == iface.ifindex {
                annotate_interface(
                    &mut nodes,
                    iface.ifindex,
                    format!(
                        "names itself as parent link (ifindex {parent}) — an invalid reference; no relationship drawn"
                    ),
                );
            } else if snapshot.interface_by_ifindex(parent).is_some() {
                let flavor = match iface.kind {
                    InterfaceKind::Tunnel => "tunnel underlay (IFLA_LINK)",
                    InterfaceKind::Vlan => "vlan parent (IFLA_LINK)",
                    _ => "parent link (IFLA_LINK)",
                };
                edges.push(TopologyEdge {
                    from: NodeId::Interface {
                        ifindex: iface.ifindex,
                    },
                    to: NodeId::Interface { ifindex: parent },
                    edge_kind: EdgeKind::LinkParent,
                    evidence: Evidence::Observed {
                        source: primary_source(&iface.sources),
                    },
                    detail: Some(flavor.to_string()),
                });
            } else {
                annotate_interface(
                    &mut nodes,
                    iface.ifindex,
                    format!(
                        "names parent link ifindex {parent}, which is not part of this observation (no relationship drawn)"
                    ),
                );
            }
        }
    }

    // 3. Gateway nodes and edges from main-table routes.
    let main_routes: Vec<&Route> = snapshot
        .routes
        .iter()
        .filter(|r| r.table == Some(RT_TABLE_MAIN as u32))
        .collect();

    // Group routes with a gateway by (oif, gateway) so one relationship is
    // one edge, with the destinations aggregated in the detail.
    //
    // A multipath route (WO-03 M3) contributes EVERY hop as the same
    // evidence class — an observed output interface plus an observed
    // gateway — so its hops draw gateway relationships exactly like
    // single-path routes, with the kernel weight recorded in the detail.
    // (The route-level gateway/oif are None for multipath routes by
    // design; reading only them would silently drop every hop.)
    let mut gateway_routes: Vec<GatewayGroup> = Vec::new();
    let mut inferred_candidates: Vec<(IpAddr, &Route)> = Vec::new();

    for route in &main_routes {
        match &route.next_hops {
            NextHops::Single => record_gateway(
                snapshot,
                &mut nodes,
                &mut gateway_routes,
                &mut inferred_candidates,
                route,
                route.output_interface,
                // An RTA_VIA next hop (AUD-13) is a gateway too — with its
                // own family — and is never an on-link absence.
                route
                    .gateway
                    .or_else(|| route.via.as_ref().and_then(|via| via.as_ip())),
                None,
            ),
            NextHops::Multi(hops) => {
                for hop in hops {
                    record_gateway(
                        snapshot,
                        &mut nodes,
                        &mut gateway_routes,
                        &mut inferred_candidates,
                        route,
                        hop.output_interface,
                        hop.gateway
                            .or_else(|| hop.via.as_ref().and_then(|via| via.as_ip())),
                        hop.weight,
                    );
                }
            }
            // An undecodable hop set carries no usable (oif, gateway)
            // evidence; the routes view shows the limitation. Nothing is
            // invented here.
            NextHops::Undecoded(_) => {}
        }
    }

    for (oif, gateway, routes) in gateway_routes {
        let destinations = routes
            .iter()
            .map(|(route, weight)| {
                let label = destination_label(route);
                match weight {
                    Some(weight) => format!("{label} (multipath next hop, kernel weight {weight})"),
                    None => label,
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
        ensure_gateway_node(&mut nodes, gateway);
        edges.push(TopologyEdge {
            from: NodeId::Interface { ifindex: oif },
            to: NodeId::Gateway { address: gateway },
            edge_kind: EdgeKind::GatewayRoute,
            evidence: Evidence::Observed {
                source: routes
                    .first()
                    .and_then(|(route, _)| route.sources.first().copied())
                    .unwrap_or(ObservationSource::Netlink),
            },
            detail: Some(format!(
                "{} via this gateway (main table 254)",
                plural_routes(routes.len(), &destinations)
            )),
        });
    }

    for (gateway, route) in inferred_candidates {
        let candidates = interfaces_containing(snapshot, gateway);
        match candidates.as_slice() {
            [only] => {
                ensure_gateway_node(&mut nodes, gateway);
                edges.push(TopologyEdge {
                    from: NodeId::Interface { ifindex: only.ifindex },
                    to: NodeId::Gateway { address: gateway },
                    edge_kind: EdgeKind::GatewayInferred,
                    evidence: Evidence::Inferred {
                        reason: format!(
                            "route names the gateway but no output interface; the gateway falls inside {}'s subnet",
                            only.name
                        ),
                    },
                    detail: Some(format!(
                        "inferred from {} (main table 254)",
                        destination_label(route)
                    )),
                });
            }
            _ => {
                // Zero or several possible interfaces: the relationship
                // stays unknown; no edge is invented.
                ensure_gateway_node(&mut nodes, gateway);
                annotate_gateway(
                    &mut nodes,
                    gateway,
                    "the route has no output interface and the reaching interface is ambiguous or unknown".into(),
                );
            }
        }
    }

    // Gateways referenced only through unobserved oifs must still exist.
    // (annotate_gateway above already ensures their nodes.)

    TopologyDocument {
        schema_version: TOPOLOGY_SCHEMA_VERSION,
        source_timestamp_ms: snapshot.timestamp_ms,
        nodes,
        edges,
    }
}

/// A route that cannot carry traffic: the kernel's unreachable encodings.
/// Two independent observations carry this: the netlink route type
/// (`blackhole` / `unreachable` / `prohibit`) and `RTF_REJECT` in
/// `/proc/net/ipv6_route` flags (the file's unreachable-default sentinel —
/// verified live, WO-03 M4). Such a default route must not be presented as
/// an on-link next hop.
fn is_unusable_route(route: &Route) -> bool {
    if let Some(RouteType::Unreachable | RouteType::Blackhole | RouteType::Prohibit) =
        route.route_type
    {
        return true;
    }
    route
        .flags
        .as_ref()
        .is_some_and(|flags| flags.iter().any(|flag| flag == "reject"))
}

/// One aggregated (oif, gateway) relationship: the contributing routes,
/// each tagged with its kernel weight when the evidence is a multipath
/// next hop (WO-03 M3; `None` for single-path route-level pairs).
type GatewayGroup<'a> = (u32, IpAddr, Vec<(&'a Route, Option<u16>)>);

/// Record one observed (oif, gateway) pair of a route — the route-level
/// pair for single-path routes, or one hop of a multipath route (WO-03
/// M3). A pair with a known oif and observed interface groups into an
/// edge; a gateway without an oif becomes an inference candidate; an
/// unobserved oif keeps the gateway node and records the unknown.
#[allow(clippy::too_many_arguments)]
fn record_gateway<'a>(
    snapshot: &NetworkSnapshot,
    nodes: &mut Vec<TopologyNode>,
    gateway_routes: &mut Vec<GatewayGroup<'a>>,
    inferred_candidates: &mut Vec<(IpAddr, &'a Route)>,
    route: &'a Route,
    oif: Option<u32>,
    gateway: Option<IpAddr>,
    weight: Option<u16>,
) {
    let Some(gateway) = gateway else { return };
    match oif {
        Some(oif) => {
            if snapshot.interface_by_ifindex(oif).is_some() {
                match gateway_routes
                    .iter_mut()
                    .find(|(existing_oif, existing_gw, _)| {
                        *existing_oif == oif && *existing_gw == gateway
                    }) {
                    Some((_, _, routes)) => routes.push((route, weight)),
                    None => gateway_routes.push((oif, gateway, vec![(route, weight)])),
                }
            } else {
                // The oif itself is unobserved: keep the gateway node,
                // record the unknown, draw no edge.
                annotate_gateway(
                    nodes,
                    gateway,
                    format!(
                        "one route reaches this gateway through an unobserved interface (ifindex {oif})"
                    ),
                );
            }
        }
        None => inferred_candidates.push((gateway, route)),
    }
}

fn interface_node(snapshot: &NetworkSnapshot, iface: &Interface) -> TopologyNode {
    let addresses: Vec<Address> = snapshot
        .addresses_on(iface.ifindex)
        .into_iter()
        .cloned()
        .collect();
    let routes_using = snapshot.routes_using_interface(iface.ifindex);
    let default_routes = routes_using.iter().filter(|r| r.is_default()).count();
    let usable_default_routes = routes_using
        .iter()
        .filter(|r| r.is_default() && !is_unusable_route(r))
        .count();

    let state = node_state(iface);
    let mut annotations = Vec::new();
    if default_routes > 0 {
        let via_gateway = routes_using
            .iter()
            .any(|r| r.is_default() && r.gateway.is_some());
        if usable_default_routes > 0 {
            if !via_gateway {
                annotations.push(
                    "has a default route without a gateway (on-link next hop); anything beyond it is not observed"
                        .into(),
                );
            }
            annotations.push(NO_UPSTREAM_NOTE.into());
        } else {
            // Only the kernel's unreachable encodings observed (route type
            // from netlink; RTF_REJECT from /proc/net/ipv6_route — the
            // file's unreachable-default sentinel, WO-03 M4). Claiming an
            // "on-link next hop" here would be false: a reject route
            // carries no traffic.
            annotations.push(
                "only an unreachable (reject) default route observed; no usable default next hop — upstream is not observed"
                    .into(),
            );
        }
    }
    match iface.kind {
        InterfaceKind::Wireguard => annotations.push(
            "WireGuard peers and endpoints are not observable without privileges; the tunnel underlay is unknown"
                .into(),
        ),
        InterfaceKind::Tunnel if iface.parent_ifindex.is_none() => {
            annotations.push("tunnel underlay interface not observed".into());
        }
        _ => {}
    }
    annotations.extend(iface.notes.iter().cloned());

    TopologyNode {
        id: NodeId::Interface {
            ifindex: iface.ifindex,
        },
        label: iface.name.clone(),
        node_kind: NodeKind::Interface,
        interface_kind: Some(iface.kind),
        kind_detail: iface.kind_detail.clone(),
        state,
        annotations,
        detail: NodeDetail {
            ifindex: Some(iface.ifindex),
            addresses,
            mac_address: iface.mac_address.clone(),
            mtu: iface.mtu,
            oper_state: iface.oper_state,
            admin_state: iface.admin_state,
            master_ifindex: iface.master_ifindex,
            parent_ifindex: iface.parent_ifindex,
            peer_ifindex: iface.peer_ifindex,
            link_netnsid: iface.link_netnsid,
            routes_using: routes_using.len(),
            default_routes,
            sources: iface.sources.clone(),
            notes: iface.notes.clone(),
        },
    }
}

fn node_state(iface: &Interface) -> NodeState {
    if iface.admin_state == Some(AdminState::Down) {
        return NodeState::Inactive;
    }
    match iface.oper_state {
        Some(OperState::Up) => NodeState::Active,
        Some(OperState::Down) | Some(OperState::LowerLayerDown) | Some(OperState::NotPresent) => {
            NodeState::Inactive
        }
        // Includes the kernel's UNKNOWN for loopback/dummy while up:
        // honest, never dressed up as active.
        _ => NodeState::Unknown,
    }
}

fn ensure_gateway_node(nodes: &mut Vec<TopologyNode>, gateway: IpAddr) {
    if nodes
        .iter()
        .any(|n| n.id == NodeId::Gateway { address: gateway })
    {
        return;
    }
    nodes.push(TopologyNode {
        id: NodeId::Gateway { address: gateway },
        label: gateway.to_string(),
        node_kind: NodeKind::Gateway,
        interface_kind: None,
        kind_detail: None,
        state: NodeState::Unknown,
        annotations: vec![GATEWAY_NOTE.to_string(), NO_UPSTREAM_NOTE.to_string()],
        detail: NodeDetail::default(),
    });
}

fn annotate_gateway(nodes: &mut Vec<TopologyNode>, gateway: IpAddr, note: String) {
    ensure_gateway_node(nodes, gateway);
    if let Some(node) = nodes
        .iter_mut()
        .find(|n| n.id == NodeId::Gateway { address: gateway })
    {
        if !node.annotations.contains(&note) {
            node.annotations.push(note);
        }
    }
}

/// Attach an honest annotation to an interface node (unknown relationships,
/// invalid references) without fabricating an edge.
fn annotate_interface(nodes: &mut [TopologyNode], ifindex: u32, note: String) {
    if let Some(node) = nodes
        .iter_mut()
        .find(|n| n.id == NodeId::Interface { ifindex })
    {
        if !node.annotations.contains(&note) {
            node.annotations.push(note);
        }
    }
}

fn primary_source(sources: &[ObservationSource]) -> ObservationSource {
    sources
        .first()
        .copied()
        .unwrap_or(ObservationSource::Netlink)
}

/// Interfaces with an address whose prefix contains `address`.
fn interfaces_containing(snapshot: &NetworkSnapshot, address: IpAddr) -> Vec<&Interface> {
    let mut result = Vec::new();
    for iface in &snapshot.interfaces {
        let contained = snapshot
            .addresses_on(iface.ifindex)
            .iter()
            .any(|a| IpNet::new(a.address, a.prefix_len).contains(address));
        if contained {
            result.push(iface);
        }
    }
    result
}

fn destination_label(route: &Route) -> String {
    route
        .destination
        .as_ref()
        .map(|d| {
            if d.is_default() {
                "default".to_string()
            } else {
                d.to_string()
            }
        })
        .unwrap_or_else(|| "unknown destination".to_string())
}

fn plural_routes(count: usize, destinations: &str) -> String {
    if count == 1 {
        destinations.to_string()
    } else {
        format!("{count} routes: {destinations}")
    }
}
