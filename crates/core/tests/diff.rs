//! Snapshot diffing: events between two observation runs.

use araiguma_core::model::{
    AdminState, Interface, InterfaceKind, MacObservation, ObservationSource, OperState,
};
use araiguma_core::snapshot::{diff_snapshots, NetworkEvent, NetworkSnapshot};

fn empty_snapshot() -> NetworkSnapshot {
    NetworkSnapshot {
        schema_version: 1,
        timestamp_ms: 0,
        interfaces: vec![],
        addresses: vec![],
        routes: vec![],
        providers: vec![],
    }
}

fn iface(ifindex: u32, name: &str, oper: OperState) -> Interface {
    Interface {
        ifindex,
        name: name.into(),
        kind: InterfaceKind::Ethernet,
        kind_detail: None,
        oper_state: Some(oper),
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

#[test]
fn identical_snapshots_produce_no_events() {
    let a = empty_snapshot();
    let b = empty_snapshot();
    assert!(diff_snapshots(&a, &b).is_empty());
}

#[test]
fn interface_lifecycle_events() {
    let mut before = empty_snapshot();
    before.interfaces = vec![
        iface(1, "lo", OperState::Up),
        iface(2, "eth0", OperState::Up),
    ];

    let mut after = empty_snapshot();
    // lo unchanged; eth0 operstate changed; wlan0 added; (nothing removed here)
    after.interfaces = vec![
        iface(1, "lo", OperState::Up),
        iface(2, "eth0", OperState::Down),
        iface(3, "wlan0", OperState::Down),
    ];

    let events = diff_snapshots(&before, &after);
    assert!(events.contains(&NetworkEvent::InterfaceAdded {
        interface: iface(3, "wlan0", OperState::Down)
    }));
    assert!(events.iter().any(|e| matches!(
        e,
        NetworkEvent::InterfaceChanged { interface, changed }
            if interface.ifindex == 2 && changed == &vec!["operState".to_string()]
    )));
    assert!(!events
        .iter()
        .any(|e| matches!(e, NetworkEvent::InterfaceRemoved { .. })));

    // Removal in the other direction.
    let events = diff_snapshots(&after, &before);
    assert!(events.iter().any(|e| matches!(
        e,
        NetworkEvent::InterfaceRemoved { interface } if interface.ifindex == 3
    )));
}

#[test]
fn empty_to_empty_observation_is_valid() {
    // A namespace with zero interfaces must diff cleanly.
    let events = diff_snapshots(&empty_snapshot(), &empty_snapshot());
    assert!(events.is_empty());
}
