//! Provider-state classification: all six states must be reachable and
//! correctly derived from section results.

use araiguma_core::provider::{
    ProviderId, ProviderObservation, ProviderState, SectionOutcome, SectionResult,
};

fn base() -> ProviderObservation {
    ProviderObservation {
        provider: ProviderId::Netlink,
        priority: 0,
        interfaces: SectionResult::Unsupported,
        addresses_v4: SectionResult::Unsupported,
        addresses_v6: SectionResult::Unsupported,
        routes_v4: SectionResult::Unsupported,
        routes_v6: SectionResult::Unsupported,
        dns: SectionResult::Unsupported,
        connections: SectionResult::Unsupported,
        fidelity_limited: false,
        notes: vec![],
    }
}

#[test]
fn all_observed_without_limits_is_available() {
    let mut o = base();
    o.interfaces = SectionResult::Observed(vec![]);
    o.routes_v4 = SectionResult::Observed(vec![]);
    assert_eq!(o.state(), ProviderState::Available);
}

#[test]
fn all_observed_with_fidelity_limits_is_degraded() {
    let mut o = base();
    o.interfaces = SectionResult::Observed(vec![]);
    o.routes_v4 = SectionResult::Observed(vec![]);
    o.fidelity_limited = true;
    assert_eq!(o.state(), ProviderState::Degraded);
}

#[test]
fn some_failed_is_partial() {
    let mut o = base();
    o.interfaces = SectionResult::Observed(vec![]);
    o.routes_v4 = SectionResult::Failed("dump interrupted".into());
    assert_eq!(o.state(), ProviderState::Partial);
}

#[test]
fn all_supported_failed_is_degraded() {
    let mut o = base();
    o.interfaces = SectionResult::Failed("permission denied".into());
    o.routes_v4 = SectionResult::Failed("permission denied".into());
    // Unsupported sections must not rescue the state.
    o.addresses_v6 = SectionResult::Unsupported;
    assert_eq!(o.state(), ProviderState::Degraded);
}

#[test]
fn nothing_supported_is_not_supported() {
    let o = base();
    assert_eq!(o.state(), ProviderState::NotSupported);
}

#[test]
fn empty_observation_is_still_available() {
    // Zero items is a valid observation (e.g. a namespace with no routes).
    let mut o = base();
    o.routes_v6 = SectionResult::Observed(vec![]);
    assert_eq!(o.state(), ProviderState::Available);
    let report = o.summarize();
    assert_eq!(report.sections.len(), 7);
}

#[test]
fn report_summarizes_section_outcomes() {
    let mut o = base();
    o.interfaces = SectionResult::Observed(vec![]);
    o.routes_v4 = SectionResult::Failed("boom".into());
    let report = o.summarize();
    assert_eq!(report.state, ProviderState::Partial);
    let failed = report
        .sections
        .iter()
        .find(|s| {
            matches!(
                s.outcome,
                araiguma_core::provider::SectionOutcome::Failed { .. }
            )
        })
        .expect("failed section must be reported");
    assert_eq!(
        failed.section,
        araiguma_core::provider::SectionKind::RoutesV4
    );
}

// ── NotObserved semantics (WO-02 correction §7) ─────────────────────────────
// An unattempted observation is not proof of unsupported functionality, and
// provider health is never claimed from missing observations.

#[test]
fn all_not_observed_is_not_yet_observed_not_not_supported() {
    let mut o = base();
    o.interfaces = SectionResult::NotObserved;
    o.addresses_v4 = SectionResult::NotObserved;
    o.addresses_v6 = SectionResult::NotObserved;
    o.routes_v4 = SectionResult::NotObserved;
    o.routes_v6 = SectionResult::NotObserved;
    assert_eq!(o.state(), ProviderState::NotYetObserved);
}

#[test]
fn mixed_observed_and_not_observed_is_partial_never_available() {
    let mut o = base();
    o.interfaces = SectionResult::Observed(vec![]);
    o.routes_v4 = SectionResult::Observed(vec![]);
    o.addresses_v6 = SectionResult::NotObserved; // never attempted
    assert_eq!(
        o.state(),
        ProviderState::Partial,
        "a run with unattempted sections is incomplete, not healthy"
    );
}

#[test]
fn mixed_observed_unsupported_and_not_observed_is_partial() {
    let mut o = base();
    o.interfaces = SectionResult::Observed(vec![]);
    o.addresses_v4 = SectionResult::Unsupported; // impossible in principle
    o.routes_v6 = SectionResult::NotObserved; // possible but unattempted
    assert_eq!(
        o.state(),
        ProviderState::Partial,
        "the unattempted supported section keeps the run incomplete"
    );
}

#[test]
fn not_observed_alongside_failures_stays_partial() {
    let mut o = base();
    o.interfaces = SectionResult::Observed(vec![]);
    o.routes_v4 = SectionResult::Failed("dump interrupted".into());
    o.routes_v6 = SectionResult::NotObserved;
    assert_eq!(o.state(), ProviderState::Partial);
}

#[test]
fn all_unsupported_plus_not_observed_is_not_yet_observed() {
    // Nothing ran at all; the provider is not "not supported here" — it
    // simply was never queried (its one supportable section is unattempted).
    let mut o = base();
    o.addresses_v4 = SectionResult::NotObserved;
    assert_eq!(o.state(), ProviderState::NotYetObserved);
}

#[test]
fn unsupported_sections_do_not_lower_an_otherwise_complete_run() {
    // The sysfs shape: sections impossible in principle (addresses_v4,
    // routes_v6) coexist with fully observed sections.
    let mut o = base();
    o.interfaces = SectionResult::Observed(vec![]);
    o.addresses_v6 = SectionResult::Observed(vec![]);
    o.routes_v4 = SectionResult::Observed(vec![]);
    assert_eq!(o.state(), ProviderState::Available);
    o.fidelity_limited = true;
    assert_eq!(o.state(), ProviderState::Degraded);
}

#[test]
fn failure_dominates_fidelity_limits() {
    // A run with both a failure and fidelity limits reports the failure
    // (Partial), the more severe fact — never Degraded.
    let mut o = base();
    o.interfaces = SectionResult::Observed(vec![]);
    o.routes_v4 = SectionResult::Failed("nope".into());
    o.fidelity_limited = true;
    assert_eq!(o.state(), ProviderState::Partial);
}

// ── WO-06 outcomes and precedence ────────────────────────────────────────────

#[test]
fn unavailable_without_observation_is_unavailable() {
    let mut o = base();
    o.interfaces = SectionResult::Unavailable;
    o.routes_v4 = SectionResult::Unavailable;
    assert_eq!(o.state(), ProviderState::Unavailable);
}

#[test]
fn permission_denied_without_observation_is_degraded() {
    let mut o = base();
    o.interfaces = SectionResult::PermissionDenied;
    o.routes_v4 = SectionResult::PermissionDenied;
    assert_eq!(o.state(), ProviderState::Degraded);
}

#[test]
fn failure_and_permission_denied_without_observation_are_degraded() {
    let mut o = base();
    o.interfaces = SectionResult::Failed("timeout".into());
    o.routes_v4 = SectionResult::PermissionDenied;
    assert_eq!(o.state(), ProviderState::Degraded);
}

#[test]
fn unavailable_and_permission_denied_without_observation_are_degraded() {
    let mut o = base();
    o.interfaces = SectionResult::Unavailable;
    o.routes_v4 = SectionResult::PermissionDenied;
    assert_eq!(o.state(), ProviderState::Degraded);
}

#[test]
fn unavailable_dominates_not_observed_without_observation() {
    let mut o = base();
    o.interfaces = SectionResult::Unavailable;
    o.routes_v4 = SectionResult::NotObserved;
    assert_eq!(o.state(), ProviderState::Unavailable);
}

#[test]
fn observed_with_unavailable_or_denied_is_partial() {
    let mut o = base();
    o.interfaces = SectionResult::Observed(vec![]);
    o.routes_v4 = SectionResult::Unavailable;
    assert_eq!(o.state(), ProviderState::Partial);

    let mut o = base();
    o.interfaces = SectionResult::Observed(vec![]);
    o.routes_v4 = SectionResult::PermissionDenied;
    assert_eq!(o.state(), ProviderState::Partial);
}

#[test]
fn all_unsupported_is_not_supported() {
    let o = ProviderObservation {
        provider: ProviderId::Netlink,
        priority: 0,
        interfaces: SectionResult::Unsupported,
        addresses_v4: SectionResult::Unsupported,
        addresses_v6: SectionResult::Unsupported,
        routes_v4: SectionResult::Unsupported,
        routes_v6: SectionResult::Unsupported,
        dns: SectionResult::Unsupported,
        connections: SectionResult::Unsupported,
        fidelity_limited: false,
        notes: vec![],
    };
    // The fixed seven-section observation shape represents an empty
    // section list as all-Unsupported: nothing was attempted and nothing
    // is supported here.
    assert_eq!(o.state(), ProviderState::NotSupported);
}

#[test]
fn new_outcomes_serialize_with_exact_wire_shapes() {
    assert_eq!(
        serde_json::to_value(SectionOutcome::Unavailable).unwrap(),
        serde_json::json!({ "kind": "unavailable" })
    );
    assert_eq!(
        serde_json::to_value(SectionOutcome::PermissionDenied).unwrap(),
        serde_json::json!({ "kind": "permissionDenied" })
    );
}
