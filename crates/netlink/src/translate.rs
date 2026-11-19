//! Pure translation from rtnetlink messages to the core observation model.
//!
//! No I/O happens here: every function takes a constructed
//! `netlink_packet_route` message and produces core-model items, so the
//! kernel-message mapping is unit-testable without a socket. The mapping is
//! deliberately conservative — anything the message does not carry stays
//! `None` (not observed) rather than being inferred.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use araiguma_core::model::{
    Address, AddressFamily, AddressScope, AdminState, Interface, InterfaceKind, IpNet,
    MacObservation, NextHop, NextHops, ObservationSource, OperState, Route, RouteProtocol,
    RouteScope, RouteType, RT_TABLE_MAIN,
};
use netlink_packet_route::address::{
    AddressAttribute, AddressFlags, AddressMessage, AddressScope as NlAddressScope,
};
use netlink_packet_route::link::{
    InfoKind, LinkAttribute, LinkFlags, LinkMessage, State as NlOperState,
};
use netlink_packet_route::route::RouteNextHop;
use netlink_packet_route::route::{RouteAddress, RouteAttribute, RouteFlags, RouteMessage};
use netlink_packet_route::{AddressFamily as NlAddressFamily, RouteNetlinkMessage};

/// Classify a multicast notification into a short change kind, or `None`
/// for messages that do not describe a relevant change (ACKs, rule
/// changes, queueing-discipline changes, ...).
pub fn classify_notification(message: &RouteNetlinkMessage) -> Option<&'static str> {
    use RouteNetlinkMessage as M;
    Some(match message {
        M::NewLink(_) => "newLink",
        M::DelLink(_) => "delLink",
        M::SetLink(_) => "setLink",
        M::NewAddress(_) => "newAddress",
        M::DelAddress(_) => "delAddress",
        M::NewRoute(_) => "newRoute",
        M::DelRoute(_) => "delRoute",
        _ => return None,
    })
}

fn nl_family(family: NlAddressFamily) -> Option<AddressFamily> {
    match family {
        NlAddressFamily::Inet => Some(AddressFamily::Ipv4),
        NlAddressFamily::Inet6 => Some(AddressFamily::Ipv6),
        _ => None,
    }
}

fn format_mac(bytes: &[u8]) -> Option<String> {
    if bytes.is_empty() {
        return None;
    }
    Some(
        bytes
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<Vec<_>>()
            .join(":"),
    )
}

fn map_oper_state(state: NlOperState) -> OperState {
    match state {
        NlOperState::Up => OperState::Up,
        NlOperState::Down => OperState::Down,
        NlOperState::Unknown => OperState::Unknown,
        NlOperState::Testing => OperState::Testing,
        NlOperState::Dormant => OperState::Dormant,
        NlOperState::NotPresent => OperState::NotPresent,
        NlOperState::LowerLayerDown => OperState::LowerLayerDown,
        // Future/unknown kernel states are honestly reported as unknown.
        _ => OperState::Unknown,
    }
}

fn map_info_kind(kind: &InfoKind) -> Option<InterfaceKind> {
    use InfoKind as K;
    Some(match kind {
        K::Bridge => InterfaceKind::Bridge,
        K::Bond => InterfaceKind::Bond,
        K::Dummy => InterfaceKind::Dummy,
        K::Wireguard => InterfaceKind::Wireguard,
        K::Vlan => InterfaceKind::Vlan,
        K::Veth => InterfaceKind::Veth,
        K::Tun
        | K::Vxlan
        | K::Geneve
        | K::GreTap
        | K::GreTap6
        | K::GreTun
        | K::GreTun6
        | K::IpIp
        | K::Ip6Tnl
        | K::SitTun
        | K::Vti
        | K::Vti6
        | K::ErSpan => InterfaceKind::Tunnel,
        K::MacVlan | K::IpVlan | K::MacVtap | K::IpVtap => InterfaceKind::Virtual,
        _ => return None,
    })
}

fn kind_detail_string(kind: &InfoKind) -> String {
    match kind {
        InfoKind::Other(raw) => raw.clone(),
        other => format!("{other:?}").to_lowercase(),
    }
}

/// Translate an `RTM_GETLINK` dump entry into a core interface.
///
/// Notes:
/// * `Controller` is `IFLA_MASTER` (bridge/bond master) under its modern
///   kernel name.
/// * veth peers (WO-03 M2): for interfaces the kernel classified as veth,
///   `IFLA_LINK` is the peer's ifindex (verified live on kernel 5.10 —
///   veth reports its peer as the link; dumps do not carry
///   `IFLA_INFO_DATA` on this kernel). The creation path can also deliver
///   the peer as a full link message inside
///   `LinkInfo::Data(InfoData::Veth(InfoVeth::Peer(..)))`. Both are
///   recorded in `peer_ifindex`; for veth devices the link is NOT kept in
///   `parent_ifindex` (it is not a parent). For every other kind,
///   `IFLA_LINK` keeps its usual parent/underlay meaning.
/// * Classification: a recognized `IFLA_INFO_KIND` string wins. An
///   **unrecognized** kind string proves the device is a specific kind
///   this model does not map: the raw string is preserved in `kind_detail`,
///   the classification stays `Other` with an explicit note, and the
///   ARPHRD fallback is deliberately NOT consulted — an unrecognized
///   virtual device whose hardware type happens to be ARPHRD_ETHER would
///   otherwise be confidently misclassified as Ethernet. The fallback
///   applies only when the kernel exposed no kind string at all (typical
///   for physical devices).
pub fn translate_link(message: &LinkMessage) -> Option<Interface> {
    if message.header.index == 0 {
        return None;
    }
    let ifindex = message.header.index;

    let mut name: Option<String> = None;
    let mut mac_bytes: Option<&[u8]> = None;
    let mut mtu: Option<u32> = None;
    let mut tx_queue_len: Option<u32> = None;
    let mut master_ifindex: Option<u32> = None;
    let mut parent_ifindex: Option<u32> = None;
    let mut link_netnsid: Option<i32> = None;
    let mut peer_from_info_data: Option<u32> = None;
    let mut oper_state: Option<OperState> = None;
    let mut kind: Option<InterfaceKind> = None;
    let mut kind_detail: Option<String> = None;
    let mut kind_string_unmapped = false;

    for attribute in &message.attributes {
        match attribute {
            LinkAttribute::IfName(value) => name = Some(value.clone()),
            LinkAttribute::Address(bytes) => mac_bytes = Some(bytes),
            LinkAttribute::Mtu(value) => mtu = Some(*value),
            LinkAttribute::TxQueueLen(value) => tx_queue_len = Some(*value),
            LinkAttribute::Controller(value) => master_ifindex = Some(*value),
            LinkAttribute::Link(value) => parent_ifindex = Some(*value),
            // IFLA_LINK_NETNSID: the network namespace containing the link
            // target (the veth peer, or the vlan/tunnel parent). Live-verified
            // on kernel 5.10: present (with an allocated id) exactly when the
            // target lives in another namespace; absent for same-namespace
            // targets. The raw signed value is preserved verbatim — including
            // a hypothetically unassigned -1 — never clamped or guessed.
            LinkAttribute::LinkNetNsId(value) => link_netnsid = Some(*value),
            LinkAttribute::OperState(state) => oper_state = Some(map_oper_state(*state)),
            LinkAttribute::LinkInfo(infos) => {
                for info in infos {
                    match info {
                        netlink_packet_route::link::LinkInfo::Kind(info_kind) => {
                            match map_info_kind(info_kind) {
                                Some(mapped) => kind = Some(mapped),
                                None => kind_string_unmapped = true,
                            }
                            kind_detail = Some(kind_detail_string(info_kind));
                        }
                        // Creation-path veth peer: a full link message whose
                        // header index is the peer's ifindex.
                        netlink_packet_route::link::LinkInfo::Data(
                            netlink_packet_route::link::InfoData::Veth(
                                netlink_packet_route::link::InfoVeth::Peer(peer),
                            ),
                        ) => peer_from_info_data = Some(peer.header.index),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    let kind = match kind {
        Some(mapped) => mapped,
        None if kind_string_unmapped => InterfaceKind::Other,
        None => InterfaceKind::from_arphrd(message.header.link_layer_type as u16),
    };

    // veth peer semantics: the kernel classified this interface as veth and
    // named a link — that link is the PEER, not a parent. The peer is
    // recorded (InfoData wins when both sources appear; they name the same
    // device), and the link value is removed from parent_ifindex so no
    // parent/underlay relationship is fabricated for a veth. IFLA_LINK_NETNSID
    // qualifies the same link target: when present, the peer (or parent) is
    // in another network namespace and its ifindex is namespace-scoped.
    let mut peer_ifindex = None;
    if kind == InterfaceKind::Veth {
        peer_ifindex = peer_from_info_data.or(parent_ifindex);
        if peer_ifindex.is_some() {
            parent_ifindex = None;
        }
    }
    // A netnsid without a link target is still recorded verbatim (the kernel
    // can report the link-netns of devices whose IFLA_LINK is absent); it is
    // never interpreted here.
    let admin_state = Some(if message.header.flags.contains(LinkFlags::Up) {
        AdminState::Up
    } else {
        AdminState::Down
    });

    let mut notes = Vec::new();
    if kind_string_unmapped {
        notes.push(format!(
            "unrecognized link kind \"{}\"; classification uncertain — raw kind preserved, not guessed from the hardware type",
            kind_detail.as_deref().unwrap_or("?")
        ));
    }

    // MAC observation (v2). Within a successfully dumped link message, an
    // absent or empty IFLA_ADDRESS is the kernel stating the device has no
    // hardware address (e.g. wireguard, ARPHRD_NONE) — an observed absence,
    // not missing data. A dump that failed entirely produces no message at
    // all (the section reports the failure).
    let mac_address = match mac_bytes {
        Some(bytes) if !bytes.is_empty() => {
            MacObservation::Present(format_mac(bytes).expect("non-empty is formattable"))
        }
        Some(_) | None => MacObservation::Absent,
    };

    Some(Interface {
        ifindex,
        name: name?,
        kind,
        kind_detail,
        oper_state,
        admin_state,
        mac_address,
        mtu,
        tx_queue_len,
        master_ifindex,
        parent_ifindex,
        peer_ifindex,
        link_netnsid,
        sources: vec![ObservationSource::Netlink],
        notes,
    })
}

fn map_address_scope(scope: NlAddressScope) -> Option<AddressScope> {
    match scope {
        NlAddressScope::Universe => Some(AddressScope::Global),
        NlAddressScope::Site => Some(AddressScope::Site),
        NlAddressScope::Link => Some(AddressScope::Link),
        NlAddressScope::Host => Some(AddressScope::Host),
        // Nowhere and unmapped values do not map to a meaningful address
        // scope: leave the field not-observed rather than guess.
        _ => None,
    }
}

fn address_flags(flags: &AddressFlags) -> Vec<String> {
    let mut names = Vec::new();
    let checks: [(AddressFlags, &str); 12] = [
        (AddressFlags::Secondary, "secondary"),
        (AddressFlags::Nodad, "nodad"),
        (AddressFlags::Optimistic, "optimistic"),
        (AddressFlags::Dadfailed, "dadfailed"),
        (AddressFlags::Homeaddress, "homeaddress"),
        (AddressFlags::Deprecated, "deprecated"),
        (AddressFlags::Tentative, "tentative"),
        (AddressFlags::Permanent, "permanent"),
        (AddressFlags::Managetempaddr, "managetempaddr"),
        (AddressFlags::Noprefixroute, "noprefixroute"),
        (AddressFlags::Mcautojoin, "mcautojoin"),
        (AddressFlags::StablePrivacy, "stableprivacy"),
    ];
    for (bit, name) in checks {
        if flags.contains(bit) {
            names.push(name.to_string());
        }
    }
    names
}

/// Translate an `RTM_GETADDR` dump entry into a core address.
pub fn translate_address(message: &AddressMessage) -> Option<Address> {
    if message.header.index == 0 {
        return None;
    }
    let family = nl_family(message.header.family)?;
    let mut address: Option<IpAddr> = None;
    let mut label: Option<String> = None;
    let mut flags: Option<Vec<String>> = None;
    for attribute in &message.attributes {
        match attribute {
            // Local is the canonical address; Address is the legacy alias
            // (equal for ordinary addresses).
            AddressAttribute::Local(ip) => address = Some(*ip),
            AddressAttribute::Address(ip) => {
                if address.is_none() {
                    address = Some(*ip);
                }
            }
            AddressAttribute::Label(value) => label = Some(value.clone()),
            AddressAttribute::Flags(value) => flags = Some(address_flags(value)),
            _ => {}
        }
    }
    Some(Address {
        ifindex: message.header.index,
        family,
        address: address?,
        prefix_len: message.header.prefix_len,
        scope: map_address_scope(message.header.scope),
        flags,
        label,
        sources: vec![ObservationSource::Netlink],
    })
}

fn route_address_ip(value: &RouteAddress) -> Option<IpAddr> {
    Some(match value {
        RouteAddress::Inet(ip) => IpAddr::V4(*ip),
        RouteAddress::Inet6(ip) => IpAddr::V6(*ip),
        _ => return None,
    })
}

fn map_route_protocol(protocol: netlink_packet_route::route::RouteProtocol) -> RouteProtocol {
    use netlink_packet_route::route::RouteProtocol as P;
    match protocol {
        P::Unspec => RouteProtocol::Unspec,
        P::IcmpRedirect => RouteProtocol::Redirect,
        P::Kernel => RouteProtocol::Kernel,
        P::Boot => RouteProtocol::Boot,
        P::Static => RouteProtocol::Static,
        P::Ra => RouteProtocol::Ra,
        P::Dhcp => RouteProtocol::Dhcp,
        other => RouteProtocol::Other(u8::from(other)),
    }
}

fn route_flags(flags: &RouteFlags) -> Vec<String> {
    let mut names = Vec::new();
    let checks: [(RouteFlags, &str); 15] = [
        (RouteFlags::Dead, "dead"),
        (RouteFlags::Pervasive, "pervasive"),
        (RouteFlags::Onlink, "onlink"),
        (RouteFlags::Linkdown, "linkdown"),
        (RouteFlags::Unresolved, "unresolved"),
        (RouteFlags::Trap, "trap"),
        (RouteFlags::Notify, "notify"),
        (RouteFlags::Cloned, "cloned"),
        (RouteFlags::Equalize, "equalize"),
        (RouteFlags::Prefix, "prefix"),
        (RouteFlags::LookupTable, "lookupTable"),
        (RouteFlags::FibMatch, "fibMatch"),
        (RouteFlags::RtOffload, "offload"),
        (RouteFlags::RtTrap, "rtTrap"),
        (RouteFlags::OffloadFailed, "offloadFailed"),
    ];
    for (bit, name) in checks {
        if flags.contains(bit) {
            names.push(name.to_string());
        }
    }
    names
}

fn next_hop_flags(flags: &netlink_packet_route::route::RouteNextHopFlags) -> Vec<String> {
    let mut names = Vec::new();
    let checks: [(netlink_packet_route::route::RouteNextHopFlags, &str); 7] = [
        (netlink_packet_route::route::RouteNextHopFlags::Dead, "dead"),
        (
            netlink_packet_route::route::RouteNextHopFlags::Pervasive,
            "pervasive",
        ),
        (
            netlink_packet_route::route::RouteNextHopFlags::Onlink,
            "onlink",
        ),
        (
            netlink_packet_route::route::RouteNextHopFlags::Offload,
            "offload",
        ),
        (
            netlink_packet_route::route::RouteNextHopFlags::Linkdown,
            "linkdown",
        ),
        (
            netlink_packet_route::route::RouteNextHopFlags::Unresolved,
            "unresolved",
        ),
        (netlink_packet_route::route::RouteNextHopFlags::Trap, "trap"),
    ];
    for (bit, name) in checks {
        if flags.contains(bit) {
            names.push(name.to_string());
        }
    }
    names
}

/// Translate one `RTA_MULTIPATH` next-hop entry. Per-hop weight: the
/// kernel stores `rtnh_hops` (weight − 1); the model carries the
/// effective weight as iproute2 displays it.
fn translate_next_hop(hop: &RouteNextHop, family: AddressFamily) -> NextHop {
    let mut gateway: Option<IpAddr> = None;
    for attribute in &hop.attributes {
        if let RouteAttribute::Gateway(value) = attribute {
            gateway = route_address_ip(value);
        }
    }
    let _ = family; // hop gateways are validated by the kernel to match the route family
    NextHop {
        gateway,
        output_interface: Some(hop.interface_index),
        output_interface_name: None,
        weight: Some(u16::from(hop.hops) + 1),
        flags: Some(next_hop_flags(&hop.flags)),
    }
}

/// Translate an `RTM_GETROUTE` dump entry into a core route.
///
/// Notes:
/// * A route with prefix length 0 and no `RTA_DST` is a default route and
///   is materialized as `0.0.0.0/0` / `::/0`.
/// * `RTA_TABLE` (u32, extended ids) is preferred over the header's u8
///   compat field; an unset compat field means the main table.
/// * An absent `RTA_PRIORITY` is the kernel's encoding of "effective metric
///   0" (the attribute is omitted when zero), so the metric is recorded as
///   `Some(0)` — this is what `/proc/net/route`'s Metric column reports for
///   the same route, and keeps cross-source deduplication correct.
/// * Multipath (`RTA_MULTIPATH`) routes carry their FULL next-hop list
///   (WO-03 M3): every observed hop with its gateway, output interface,
///   weight, and per-hop flags. The route-level gateway/output interface
///   are `None` for multipath routes — the hops carry them, and multiple
///   hops are never flattened into one gateway. An empty `RTA_MULTIPATH`
///   (malformed kernel data) is recorded as an explicit undecodable
///   marker rather than silently dropped.
pub fn translate_route(message: &RouteMessage) -> Option<Route> {
    let family = nl_family(message.header.address_family)?;

    let mut destination_ip: Option<IpAddr> = None;
    let mut gateway: Option<IpAddr> = None;
    let mut output_interface: Option<u32> = None;
    let mut priority: Option<u32> = None;
    let mut table: Option<u32> = None;
    let mut multipath: Option<Vec<RouteNextHop>> = None;

    for attribute in &message.attributes {
        match attribute {
            RouteAttribute::Destination(value) => destination_ip = route_address_ip(value),
            RouteAttribute::Gateway(value) => gateway = route_address_ip(value),
            RouteAttribute::Oif(value) => output_interface = Some(*value),
            RouteAttribute::Priority(value) => priority = Some(*value),
            RouteAttribute::Table(value) => table = Some(*value),
            RouteAttribute::MultiPath(next_hops) => multipath = Some(next_hops.clone()),
            _ => {}
        }
    }

    let prefix_len = message.header.destination_prefix_length;
    let destination = destination_ip
        .map(|ip| IpNet::new(ip, prefix_len))
        .or_else(|| {
            // Default route: the kernel sends prefix length 0 without a
            // destination attribute.
            (prefix_len == 0).then(|| {
                IpNet::new(
                    match family {
                        AddressFamily::Ipv4 => IpAddr::V4(Ipv4Addr::UNSPECIFIED),
                        AddressFamily::Ipv6 => IpAddr::V6(Ipv6Addr::UNSPECIFIED),
                    },
                    0,
                )
            })
        });

    let mut notes = Vec::new();

    // Multipath representation (WO-03 M3). For a decoded hop list the
    // hops carry the gateways and output interfaces; route-level fields
    // are cleared so the hops are never mistaken for a single gateway.
    let next_hops = match multipath {
        None => NextHops::Single,
        Some(hops) if hops.is_empty() => {
            notes.push(
                "kernel sent RTA_MULTIPATH with no next hops (malformed); next-hop set not decodable"
                    .to_string(),
            );
            NextHops::Undecoded("RTA_MULTIPATH attribute carried no next hops".to_string())
        }
        Some(hops) => NextHops::Multi(
            hops.iter()
                .map(|hop| translate_next_hop(hop, family))
                .collect(),
        ),
    };
    if matches!(next_hops, NextHops::Multi(_)) {
        gateway = None;
        output_interface = None;
    }

    let table = match table.or(Some(message.header.table as u32)) {
        Some(0) | None => Some(RT_TABLE_MAIN as u32),
        other => other,
    };

    Some(Route {
        family,
        destination,
        // `None` here is the kernel's own "no gateway" (on-link) encoding —
        // except for multipath routes, where the hops carry the gateways.
        gateway,
        output_interface,
        output_interface_name: None,
        protocol: Some(map_route_protocol(message.header.protocol)),
        scope: Some(RouteScope::from_raw(u8::from(message.header.scope))),
        route_type: Some(RouteType::from_raw(u8::from(message.header.kind))),
        metric: Some(priority.unwrap_or(0)),
        table,
        // rtm_tos is part of the kernel's route key: two routes with the
        // same destination but different TOS are distinct routes, so the
        // value participates in merge identity (WO-03-F / AUD-02).
        tos: Some(message.header.tos),
        flags: Some(route_flags(&message.header.flags)),
        next_hops,
        sources: vec![ObservationSource::Netlink],
        notes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use netlink_packet_route::address::AddressHeader;
    use netlink_packet_route::link::{InfoData, InfoVeth, LinkHeader, LinkInfo, LinkLayerType};
    use netlink_packet_route::route::{
        RouteHeader, RouteScope as NlRouteScope, RouteType as NlRouteType,
    };

    fn link_message() -> LinkMessage {
        let mut message = LinkMessage::default();
        message.header = LinkHeader {
            interface_family: NlAddressFamily::Inet,
            index: 11,
            link_layer_type: LinkLayerType::Ether,
            flags: LinkFlags::Up,
            change_mask: LinkFlags::empty(),
        };
        message
    }

    #[test]
    fn translates_link_attributes() {
        let mut message = link_message();
        message.attributes = vec![
            LinkAttribute::IfName("dummy0".into()),
            LinkAttribute::Mtu(1500),
            LinkAttribute::TxQueueLen(1000),
            LinkAttribute::Address(vec![0x66, 0x2d, 0x1a, 0x55, 0xaa, 0xbb]),
            LinkAttribute::OperState(NlOperState::Unknown),
            LinkAttribute::LinkInfo(vec![LinkInfo::Kind(InfoKind::Dummy)]),
        ];
        let interface = translate_link(&message).expect("valid link");
        assert_eq!(interface.ifindex, 11);
        assert_eq!(interface.name, "dummy0");
        assert_eq!(interface.kind, InterfaceKind::Dummy);
        assert_eq!(interface.kind_detail.as_deref(), Some("dummy"));
        assert_eq!(interface.admin_state, Some(AdminState::Up));
        assert_eq!(interface.oper_state, Some(OperState::Unknown));
        assert_eq!(interface.mac_address.present(), Some("66:2d:1a:55:aa:bb"));
        assert_eq!(interface.mtu, Some(1500));
    }

    #[test]
    fn link_kind_falls_back_to_arphrd_without_link_info() {
        let mut message = link_message();
        message.header.link_layer_type = LinkLayerType::Loopback;
        message.attributes = vec![LinkAttribute::IfName("lo".into())];
        let interface = translate_link(&message).expect("valid link");
        assert_eq!(interface.kind, InterfaceKind::Loopback);
        assert_eq!(interface.kind_detail, None);
        assert_eq!(
            interface.mac_address,
            MacObservation::Absent,
            "no address attribute"
        );
    }

    #[test]
    fn unrecognized_link_kind_stays_other_with_the_raw_kind() {
        // WO-02 correction §9.2: an unknown IFLA_INFO_KIND on a device whose
        // hardware type is ARPHRD_ETHER must NOT be confidently classified
        // as Ethernet — the device is a specific (virtual) kind the model
        // does not map, and that uncertainty is exposed.
        let mut message = link_message(); // link_layer_type: Ether
        message.attributes = vec![
            LinkAttribute::IfName("weird0".into()),
            LinkAttribute::LinkInfo(vec![LinkInfo::Kind(InfoKind::Other("weird0".into()))]),
        ];
        let interface = translate_link(&message).expect("valid link");
        assert_eq!(interface.kind, InterfaceKind::Other, "never guessed");
        assert_eq!(
            interface.kind_detail.as_deref(),
            Some("weird0"),
            "raw kind preserved"
        );
        assert!(interface
            .notes
            .iter()
            .any(|n| n.contains("unrecognized link kind \"weird0\"")));
        assert!(interface
            .notes
            .iter()
            .any(|n| n.contains("classification uncertain")));
    }

    #[test]
    fn arphrd_fallback_is_legitimate_without_any_kind_string() {
        // No LinkInfo at all: a plain physical Ethernet device — the ARPHRD
        // fallback is the only available evidence and applies, without a
        // caveat note.
        let mut message = link_message(); // Ether + no LinkInfo
        message.attributes = vec![LinkAttribute::IfName("enp3s0".into())];
        let interface = translate_link(&message).expect("valid link");
        assert_eq!(interface.kind, InterfaceKind::Ethernet);
        assert!(interface.notes.is_empty());
    }

    #[test]
    fn link_without_an_address_attribute_has_an_observed_absent_mac() {
        // WO-02 correction §5: inside a successfully dumped message, a
        // missing or empty IFLA_ADDRESS is the kernel stating the device
        // has no hardware address — distinct from "not observed".
        let mut message = link_message();
        message.attributes = vec![
            LinkAttribute::IfName("wg0".into()),
            LinkAttribute::Mtu(1420),
        ];
        let interface = translate_link(&message).expect("valid link");
        assert_eq!(interface.mac_address, MacObservation::Absent);

        let mut empty = link_message();
        empty.attributes = vec![
            LinkAttribute::IfName("wg1".into()),
            LinkAttribute::Address(Vec::new()),
        ];
        let interface = translate_link(&empty).expect("valid link");
        assert_eq!(interface.mac_address, MacObservation::Absent);
    }

    #[test]
    fn link_without_a_name_is_skipped() {
        let mut message = link_message();
        message.attributes = vec![LinkAttribute::Mtu(1500)];
        assert!(translate_link(&message).is_none());
    }

    #[test]
    fn translates_bridge_master_and_vlan_parent() {
        let mut port = link_message();
        port.header.index = 6;
        port.attributes = vec![
            LinkAttribute::IfName("eth1".into()),
            LinkAttribute::Controller(5),
        ];
        let port = translate_link(&port).expect("valid link");
        assert_eq!(port.master_ifindex, Some(5), "Controller is IFLA_MASTER");

        let mut vlan = link_message();
        vlan.header.index = 7;
        vlan.attributes = vec![
            LinkAttribute::IfName("eth1.100".into()),
            LinkAttribute::Link(6),
            LinkAttribute::LinkInfo(vec![LinkInfo::Kind(InfoKind::Vlan)]),
        ];
        let vlan = translate_link(&vlan).expect("valid link");
        assert_eq!(vlan.parent_ifindex, Some(6));
        assert_eq!(vlan.kind, InterfaceKind::Vlan);
    }

    fn address_message(family: NlAddressFamily, index: u32) -> AddressMessage {
        let mut message = AddressMessage::default();
        message.header = AddressHeader {
            family,
            prefix_len: 24,
            flags: Default::default(),
            scope: NlAddressScope::Universe,
            index,
        };
        message
    }

    #[test]
    fn veth_link_attribute_is_the_peer_not_a_parent() {
        // WO-03 M2 (verified live, kernel 5.10): a veth's IFLA_LINK is its
        // peer's ifindex. The translator records it as the peer and does
        // NOT keep it as a parent.
        let mut peer_message = link_message();
        peer_message.header.index = 7;
        let mut message = link_message();
        message.header.index = 6;
        message.attributes = vec![
            LinkAttribute::IfName("veth0".into()),
            LinkAttribute::Link(7),
            LinkAttribute::LinkInfo(vec![
                LinkInfo::Kind(InfoKind::Veth),
                LinkInfo::Data(InfoData::Veth(InfoVeth::Peer(peer_message))),
            ]),
        ];
        let interface = translate_link(&message).expect("valid link");
        assert_eq!(interface.kind, InterfaceKind::Veth);
        assert_eq!(interface.peer_ifindex, Some(7));
        assert_eq!(
            interface.parent_ifindex, None,
            "a veth's link is its peer, not a parent"
        );

        // Without InfoData (the dump shape on this kernel), IFLA_LINK
        // alone still names the peer.
        let mut dump_only = link_message();
        dump_only.header.index = 6;
        dump_only.attributes = vec![
            LinkAttribute::IfName("veth0".into()),
            LinkAttribute::Link(7),
            LinkAttribute::LinkInfo(vec![LinkInfo::Kind(InfoKind::Veth)]),
        ];
        let interface = translate_link(&dump_only).expect("valid link");
        assert_eq!(interface.peer_ifindex, Some(7));
        assert_eq!(interface.parent_ifindex, None);
    }

    #[test]
    fn non_veth_link_attribute_stays_a_parent() {
        // A vlan's IFLA_LINK is its parent — no peer is invented.
        let mut message = link_message();
        message.header.index = 3;
        message.attributes = vec![
            LinkAttribute::IfName("eth0.100".into()),
            LinkAttribute::Link(2),
            LinkAttribute::LinkInfo(vec![LinkInfo::Kind(InfoKind::Vlan)]),
        ];
        let interface = translate_link(&message).expect("valid link");
        assert_eq!(interface.kind, InterfaceKind::Vlan);
        assert_eq!(interface.parent_ifindex, Some(2));
        assert_eq!(interface.peer_ifindex, None);
    }

    #[test]
    fn link_netnsid_is_preserved_verbatim_for_veth_and_parents() {
        // WO-03-F / AUD-01: IFLA_LINK_NETNSID qualifies the link target —
        // the veth peer, or the vlan/tunnel parent. The raw signed value is
        // recorded verbatim (never clamped, never guessed): a veth whose
        // peer lives in another namespace carries the allocated netnsid,
        // and so does a cross-namespace vlan parent.
        let mut veth = link_message();
        veth.header.index = 6;
        veth.attributes = vec![
            LinkAttribute::IfName("veth0".into()),
            LinkAttribute::Link(7),
            LinkAttribute::LinkNetNsId(0),
            LinkAttribute::LinkInfo(vec![LinkInfo::Kind(InfoKind::Veth)]),
        ];
        let interface = translate_link(&veth).expect("valid link");
        assert_eq!(interface.kind, InterfaceKind::Veth);
        assert_eq!(interface.peer_ifindex, Some(7));
        assert_eq!(
            interface.link_netnsid,
            Some(0),
            "the kernel's netnsid statement is preserved"
        );
        assert_eq!(interface.parent_ifindex, None);

        // A cross-namespace vlan parent carries the netnsid instead; a
        // hypothetically unassigned id (-1) is preserved verbatim rather
        // than turned into "not observed".
        let mut vlan = link_message();
        vlan.header.index = 8;
        vlan.attributes = vec![
            LinkAttribute::IfName("eth0.100".into()),
            LinkAttribute::Link(2),
            LinkAttribute::LinkNetNsId(-1),
            LinkAttribute::LinkInfo(vec![LinkInfo::Kind(InfoKind::Vlan)]),
        ];
        let interface = translate_link(&vlan).expect("valid link");
        assert_eq!(interface.kind, InterfaceKind::Vlan);
        assert_eq!(interface.parent_ifindex, Some(2));
        assert_eq!(interface.link_netnsid, Some(-1));

        // Without the attribute: not observed (same-namespace target on
        // this kernel — verified live).
        let mut plain = link_message();
        plain.header.index = 9;
        plain.attributes = vec![
            LinkAttribute::IfName("veth1".into()),
            LinkAttribute::Link(10),
            LinkAttribute::LinkInfo(vec![LinkInfo::Kind(InfoKind::Veth)]),
        ];
        let interface = translate_link(&plain).expect("valid link");
        assert_eq!(interface.peer_ifindex, Some(10));
        assert_eq!(interface.link_netnsid, None);
    }

    #[test]
    fn translates_address_local_and_flags() {
        let mut message = address_message(NlAddressFamily::Inet, 2);
        message.attributes = vec![
            AddressAttribute::Local("192.168.1.42".parse().unwrap()),
            AddressAttribute::Label("eth0".into()),
            AddressAttribute::Flags(AddressFlags::Permanent | AddressFlags::Noprefixroute),
        ];
        let address = translate_address(&message).expect("valid address");
        assert_eq!(address.ifindex, 2);
        assert_eq!(address.family, AddressFamily::Ipv4);
        assert_eq!(address.address.to_string(), "192.168.1.42");
        assert_eq!(address.prefix_len, 24);
        assert_eq!(address.scope, Some(AddressScope::Global));
        assert_eq!(
            address.flags,
            Some(vec!["permanent".to_string(), "noprefixroute".to_string()])
        );
        assert_eq!(address.label.as_deref(), Some("eth0"));
    }

    fn route_message(family: NlAddressFamily) -> RouteMessage {
        let mut message = RouteMessage::default();
        message.header = RouteHeader {
            address_family: family,
            destination_prefix_length: 0,
            source_prefix_length: 0,
            tos: 0,
            table: 254,
            protocol: netlink_packet_route::route::RouteProtocol::Dhcp,
            scope: NlRouteScope::Universe,
            kind: NlRouteType::Unicast,
            flags: RouteFlags::default(),
        };
        message
    }

    #[test]
    fn translates_default_route_without_destination_attribute() {
        let mut message = route_message(NlAddressFamily::Inet);
        message.attributes = vec![
            RouteAttribute::Gateway(RouteAddress::Inet("192.168.1.1".parse().unwrap())),
            RouteAttribute::Oif(2),
            RouteAttribute::Priority(600),
        ];
        let route = translate_route(&message).expect("valid route");
        assert_eq!(route.family, AddressFamily::Ipv4);
        assert_eq!(
            route.destination,
            Some(IpNet::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0)),
            "default route materialized as 0.0.0.0/0"
        );
        assert_eq!(route.gateway.unwrap().to_string(), "192.168.1.1");
        assert_eq!(route.output_interface, Some(2));
        assert_eq!(route.metric, Some(600));
        assert_eq!(route.table, Some(254));
        assert_eq!(route.protocol, Some(RouteProtocol::Dhcp));
        assert_eq!(route.scope, Some(RouteScope::Universe));
        assert_eq!(route.route_type, Some(RouteType::Unicast));
        assert_eq!(route.flags, Some(vec![]));
    }

    #[test]
    fn translates_connected_ipv6_route_with_effective_metric_zero() {
        let mut message = route_message(NlAddressFamily::Inet6);
        message.header.destination_prefix_length = 64;
        message.header.protocol = netlink_packet_route::route::RouteProtocol::Kernel;
        message.header.scope = NlRouteScope::Universe;
        message.header.kind = NlRouteType::Unicast;
        message.attributes = vec![
            RouteAttribute::Destination(RouteAddress::Inet6("fe80::".parse().unwrap())),
            RouteAttribute::Oif(2),
        ];
        let route = translate_route(&message).expect("valid route");
        assert_eq!(route.destination.unwrap().to_string(), "fe80::/64");
        assert_eq!(route.gateway, None, "no gateway attribute means on-link");
        assert_eq!(
            route.metric,
            Some(0),
            "absent RTA_PRIORITY is the kernel's effective metric 0"
        );
        assert_eq!(route.protocol, Some(RouteProtocol::Kernel));
    }

    #[test]
    fn translates_local_table_route() {
        let mut message = route_message(NlAddressFamily::Inet);
        message.header.table = 255;
        message.header.destination_prefix_length = 32;
        message.header.protocol = netlink_packet_route::route::RouteProtocol::Kernel;
        message.header.scope = NlRouteScope::Host;
        message.header.kind = NlRouteType::Local;
        message.attributes = vec![
            RouteAttribute::Destination(RouteAddress::Inet("127.0.0.1".parse().unwrap())),
            RouteAttribute::Oif(1),
            RouteAttribute::Table(255),
        ];
        let route = translate_route(&message).expect("valid route");
        assert_eq!(route.table, Some(255));
        assert_eq!(route.route_type, Some(RouteType::Local));
        assert_eq!(route.scope, Some(RouteScope::Host));
    }

    #[test]
    fn notification_classification() {
        use RouteNetlinkMessage as M;
        assert_eq!(
            classify_notification(&M::NewLink(link_message())),
            Some("newLink")
        );
        assert_eq!(
            classify_notification(&M::DelRoute(route_message(NlAddressFamily::Inet))),
            Some("delRoute")
        );
        assert_eq!(
            classify_notification(&M::GetRule(
                netlink_packet_route::rule::RuleMessage::default()
            )),
            None,
            "irrelevant messages are not classified"
        );
    }
}
