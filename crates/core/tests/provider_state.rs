//! Provider-state classification: all six states must be reachable and
//! correctly derived from section results.

use araiguma_core::provider::{ProviderId, ProviderObservation, ProviderState, SectionResult};

fn base() -> ProviderObservation {
    ProviderObservation {
        provider: ProviderId::Netlink,
        priority: 0,
        interfaces: SectionResult::Unsupported,
        addresses_v4: SectionResult::Unsupported,
        addresses_v6: SectionResult::Unsupported,
        routes_v4: SectionResult::Unsupported,
        routes_v6: SectionResult::Unsupported,
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
fn all_supported_failed_is_unavailable() {
    let mut o = base();
    o.interfaces = SectionResult::Failed("permission denied".into());
    o.routes_v4 = SectionResult::Failed("permission denied".into());
    // Unsupported sections must not rescue the state.
    o.addresses_v6 = SectionResult::Unsupported;
    assert_eq!(o.state(), ProviderState::Unavailable);
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
    assert_eq!(report.sections.len(), 5);
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
