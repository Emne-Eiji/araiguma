//! Snapshot fixtures are schema-locked: every JSON file must parse into the
//! authoritative Rust model and round-trip losslessly. The frontend unit
//! tests parse the same files with the TypeScript mirror of the model —
//! both sides must stay in sync with these files.

use std::path::PathBuf;

use araiguma_core::NetworkSnapshot;

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/snapshots")
}

#[test]
fn all_snapshot_fixtures_roundtrip() {
    let dir = fixture_dir();
    let mut checked = 0;
    for entry in std::fs::read_dir(&dir).expect("fixtures/snapshots must exist") {
        let path = entry.expect("readable dir entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let raw = std::fs::read_to_string(&path).expect("readable fixture");
        let parsed: NetworkSnapshot =
            serde_json::from_str(&raw).unwrap_or_else(|e| panic!("{}: {}", path.display(), e));
        assert_eq!(parsed.schema_version, 1, "{}", path.display());

        // Round-trip: re-serialize and parse again; must be identical.
        let serialized = serde_json::to_string_pretty(&parsed).expect("serializable");
        let reparsed: NetworkSnapshot = serde_json::from_str(&serialized)
            .unwrap_or_else(|e| panic!("{} reparse: {}", path.display(), e));
        assert_eq!(
            parsed,
            reparsed,
            "{} must round-trip losslessly",
            path.display()
        );
        checked += 1;
    }
    assert!(
        checked >= 2,
        "expected at least two fixtures, checked {checked}"
    );
}

#[test]
fn workstation_fixture_covers_required_scenarios() {
    let raw =
        std::fs::read_to_string(fixture_dir().join("workstation.json")).expect("fixture present");
    let snapshot: NetworkSnapshot = serde_json::from_str(&raw).expect("valid workstation fixture");

    // Interface kinds required by the fixture plan.
    let kinds: Vec<InterfaceKindAlias> = snapshot.interfaces.iter().map(|i| i.kind).collect();
    for expected in [
        InterfaceKindAlias::Loopback,
        InterfaceKindAlias::Ethernet,
        InterfaceKindAlias::Wireless,
        InterfaceKindAlias::Bridge,
        InterfaceKindAlias::Wireguard,
        InterfaceKindAlias::Dummy,
    ] {
        assert!(kinds.contains(&expected), "missing kind {expected:?}");
    }

    // Multiple default routes, both families.
    let defaults = snapshot.default_routes(None);
    assert!(
        defaults.len() >= 2,
        "workstation fixture must include multiple default routes"
    );
    assert!(!snapshot
        .default_routes(Some(AddressFamilyAlias::Ipv6))
        .is_empty());

    // Both main and local tables present.
    let tables = snapshot.tables();
    assert!(tables.contains(&254));
    assert!(tables.contains(&255));

    // The wlp3s0 card is asserted by the frontend design check; the fixture
    // must keep it.
    assert!(snapshot.interfaces.iter().any(|i| i.name == "wlp3s0"));
}

// Local aliases keep the assert! Debug formatting readable.
use araiguma_core::model::{
    AddressFamily as AddressFamilyAlias, InterfaceKind as InterfaceKindAlias,
};

#[test]
fn incomplete_fixture_has_netlink_failure_and_null_fields() {
    let raw = std::fs::read_to_string(fixture_dir().join("incomplete-observations.json"))
        .expect("fixture present");
    let snapshot: NetworkSnapshot =
        serde_json::from_str(&raw).expect("valid incomplete-observations fixture");

    let netlink = snapshot
        .providers
        .iter()
        .find(|p| p.provider == araiguma_core::ProviderId::Netlink)
        .expect("netlink report present");
    assert_eq!(netlink.state, araiguma_core::ProviderState::Unavailable);

    // procfs-only routes: protocol/scope/type remain None (not fabricated).
    let route = snapshot
        .routes
        .iter()
        .find(|r| r.is_default())
        .expect("default route present");
    assert!(route.protocol.is_none());
    assert!(route.scope.is_none());
    assert!(route.route_type.is_none());
    assert_eq!(route.table, Some(254), "procfs table is implied main");

    // Interfaces carry classification caveats.
    assert!(snapshot
        .interfaces
        .iter()
        .any(|i| i.notes.iter().any(|n| n.contains("sysfs cannot"))));
}
