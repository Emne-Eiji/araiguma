//! Snapshot fixtures are schema-locked: every JSON file must parse into the
//! authoritative Rust model and round-trip losslessly. The frontend unit
//! tests parse the same files with the TypeScript mirror of the model —
//! both sides must stay in sync with these files.

use std::path::PathBuf;

use araiguma_core::model::MacObservation;
use araiguma_core::snapshot::SNAPSHOT_SCHEMA_VERSION;
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
        assert_eq!(
            parsed.schema_version,
            SNAPSHOT_SCHEMA_VERSION,
            "{}",
            path.display()
        );

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

// ── MAC observation semantics (WO-02 correction §5) ─────────────────────

#[test]
fn mac_observation_serializes_as_plain_string_and_sentinels() {
    let cases = [
        (
            MacObservation::Present("52:54:00:12:34:56".into()),
            "\"52:54:00:12:34:56\"",
        ),
        (MacObservation::Absent, "\"absent\""),
        (MacObservation::NotObserved, "\"notObserved\""),
    ];
    for (value, expected_json) in cases {
        let serialized = serde_json::to_string(&value).expect("serializable");
        assert_eq!(serialized, expected_json);
        let reparsed: MacObservation = serde_json::from_str(&serialized).expect("round-trips");
        assert_eq!(reparsed, value);
    }
}

#[test]
fn mac_observation_rejects_null_and_malformed_values() {
    // Genuinely malformed values are rejected. (Length is deliberately
    // permissive: the kernel's addr_len varies by hardware class, e.g.
    // 20-byte IPoIB addresses, so "52:54:00" is a valid — if unusual —
    // observation and must not be silently dropped.)
    for bad in [
        "null",
        "\"zzz\"",
        "\"52:54:00:12:34:5g\"", // non-hex digit
        "\"aa:bb:cc:dd:ee:\"",   // trailing separator
        "\"aa-bb-cc-dd-ee-ff\"", // wrong separator
        "\"aabbccddeeff\"",      // missing separators
        "123",
        "[]",
    ] {
        let result: Result<MacObservation, _> = serde_json::from_str(bad);
        assert!(result.is_err(), "{bad} must be rejected");
        let error = result.expect_err("checked above").to_string();
        assert!(
            error.contains("absent")
                || error.contains("hardware address")
                || error.contains("null"),
            "error explains the accepted forms: {error}"
        );
    }
}

#[test]
fn mac_observation_normalizes_case_and_accepts_long_hardware_addresses() {
    let upper: MacObservation = serde_json::from_str("\"AA:BB:CC:DD:EE:FF\"").expect("parses");
    assert_eq!(upper, MacObservation::Present("aa:bb:cc:dd:ee:ff".into()));
    // IPoIB-style 20-byte addresses are valid hardware addresses.
    let long: MacObservation =
        serde_json::from_str("\"00:11:22:33:44:55:66:77:88:99:aa:bb:cc:dd:ee:ff:00:11:22:33\"")
            .expect("20-byte address parses");
    assert!(long.present().is_some());
}

#[test]
fn fixtures_cover_all_three_mac_states() {
    // workstation has a present MAC (eth0), an observed absence (wg0 —
    // wireguard has no hardware address), and the model's NotObserved is
    // exercised by the unit tests and provider behavior.
    let snapshot: NetworkSnapshot = serde_json::from_str(
        &std::fs::read_to_string(fixture_dir().join("workstation.json")).expect("fixture"),
    )
    .expect("valid");
    let eth0 = snapshot
        .interfaces
        .iter()
        .find(|i| i.name == "eth0")
        .expect("eth0");
    assert_eq!(eth0.mac_address.present(), Some("52:54:00:12:34:56"));
    let wg0 = snapshot
        .interfaces
        .iter()
        .find(|i| i.name == "wg0")
        .expect("wg0");
    assert_eq!(
        wg0.mac_address,
        MacObservation::Absent,
        "wireguard is observed to have no hardware address"
    );
}
