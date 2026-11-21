# Prompt 03 — backend architecture and network data model specification

Status: specification (design only — no implementation authorized by this
package). Produced under the Prompt 03 work order (Emne, 2026-11-21) on the
verified baseline `main` @ `98081962403f55e332fc8b13f5209594bb04ca2e`
(WO-04 Stage A, 50 commits).

This package translates the accepted product requirements, the WO-02
technical feasibility findings, and Emne's approved decisions (OD-01, OD-02,
OD-03, OD-08, OD-09, OD-10) into a coherent backend design that a later
implementation agent can build without independently inventing architectural
boundaries, provider contracts, data structures, evidence semantics,
document serialization, IPC contracts, error and lifecycle behavior,
security boundaries, or testable invariants.

## Reading order

| # | Document | Contents |
|---|---|---|
| 01 | [backend architecture](01-backend-architecture.md) | layers, crates, dependency direction, observation engine, runtime ownership, application lifecycle, CLI / import / Tauri integration, tray compatibility constraints |
| 02 | [provider contract](02-provider-contract.md) | provider identity, capability declaration and discovery, availability vs permission, observation, refresh / polling / events, errors, health metadata, graceful degradation, shutdown, cross-provider reconciliation |
| 03 | [data model and evidence semantics](03-data-model-and-evidence.md) | normalized entity model (interfaces, addresses, routes, rules, DNS, VPN evidence, provider health), the evidence vocabulary, uncertainty, conflicts, staleness, unresolved relationships |
| 04 | [document and IPC contract](04-document-and-ipc-contract.md) | the one shared JSON document contract (CLI, import, IPC), schema v4 envelope, validation and atomic replacement, the IPC surface, event continuity and resynchronization, Tokio runtime ownership |
| 05 | [logging and diagnostics](05-logging-and-diagnostics.md) | OD-09/OD-08 design: locations, levels, structured fields, rotation, session state, diagnostics export, redaction, unwritable-directory behavior |
| 06 | [security and read-only enforcement](06-security-read-only-enforcement.md) | the read-only invariant, structural enforcement, the concrete D-Bus allow-list, Tauri capabilities, invariant tests, review gates, runtime degradation |
| 07 | [testing strategy](07-testing-strategy.md) | test architecture, the two-track validation model, the OD-10 Ubuntu 24.04 checkpoint, the read-only state-comparison approach, validation matrix |
| 08 | [decision register](08-decision-register.md) | approved decisions, new recommendations, unresolved decisions, future work — with full traceability |

## Standing rules for this package

1. **Design, not implementation.** Every statement describes a target
   contract or invariant. Nothing here has been built; where a statement
   describes the *current* repository it is marked as existing behavior
   (with its WO/AUD reference) rather than as a proposal.
2. **The Rust backend is authoritative.** The frontend renders
   backend-provided documents; it never discovers, invents, or silently
   reinterprets network facts (existing, enforced principle — WO-01R/WO-02).
3. **Unknown stays unknown.** No design element coerces unknown, unavailable,
   failed, or not-observed into false, empty, absent, disconnected, or
   normal (existing first principle, extended).
4. **Read-only is structural.** Observation never mutates network state, not
   even transiently as a discovery technique (see document 06).
5. **One document contract.** CLI output, file import, and IPC deliver the
   same JSON documents (OD-03); no competing formats.
6. **Approved, recommended, and unresolved are never conflated.** OD-xx IDs
   are Emne's approved decisions; RD-xx are this package's recommendations
   awaiting approval; UD-xx are unresolved, explicitly including the still
   open OD-04..OD-07. See document 08.

## Traceability

Every important design decision in documents 01–07 carries an inline
reference to its source: an approved decision (OD-xx), a WO-02 feasibility
finding (chapter, extension point E-n, risk R-n, or question Q-n), an
existing verified repository behavior (WO/AUD reference), or a clearly
marked recommendation (RD-xx). Document 08 is the consolidated register.

## Out of scope (not this package)

Prompt 04 UI/UX specification; Prompt 05 discovery specification (provider
internals — this package defines their *contract* only); Prompt 06 traffic
accounting (only architectural extension points are reserved here); Prompt
07 security/notifications/testing beyond the backend security and test
architecture required here; WO-TRAY-01 tray implementation; any source-code
change, dependency addition, or application configuration change.
