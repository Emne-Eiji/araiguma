# ARAIGUMA — WO-01R + CONTINUATION INTO WO-02
## RECONSTRUCTION AUTHORIZATION AND IMMEDIATE IMPLEMENTATION

**Authority:** Emne — Product Owner and sole acceptance authority
**Implementation agent:** GLM 5.3 Max
**Architecture advisor / independent reviewer:** ChatGPT ("The Brain")
**Status:** AUTHORIZED — PROCEED WITHOUT FURTHER APPROVAL FOR WORK WITHIN THIS SCOPE

(Preserved verbatim as the authorization record for the reconstruction and
the WO-02 continuation. Source: Emne's work order issued 2026-11-19.)

## 1. Executive instruction

The WO-02 incident report dated 2026-11-18 has been reviewed.

The original Araiguma repository was lost because the online sandbox's persistence layer exceeded its boot-restore capacity. The report establishes that the original source code and Git history are no longer available in the sandbox.

Do not waste time attempting speculative recovery.

Emne authorizes:

1. The narrowly scoped removal of the three regenerable cache items identified in the incident report.
2. Reconstruction of the WO-01 foundation under a new work order, WO-01R.
3. Full testing and verification of the reconstructed foundation.
4. Immediate continuation into the already-authorized WO-02 implementation.

**Do not stop after reconstruction to request another work order or authorization.** Complete WO-01R, verify it, and proceed directly to WO-02.

The priority is engineering quality, correctness, maintainability, and recoverability. There is no pressure to publish to GitHub.

## 2. Explicit project-owner decisions

### 2.1 Git identity

Use repository-local configuration:

- `user.name = Emne`
- `user.email = https://github.com/Emne-Eiji`

Every Araiguma commit must use this identity for both author and committer.

Do not rely on the sandbox's global Git identity.

Do not modify the parent accounting repository or its Git configuration.

### 2.2 GitHub publication

GitHub publication is explicitly deferred.

- No remote configuration.
- No push.
- No GitHub credentials or tokens.
- No CI configuration requiring GitHub access.
- No attempt to publish the reconstructed history.

The eventual repository destination is:

`https://github.com/Emne-Eiji/araiguma`

It is reserved for later publication after the application has been transferred to Emne's local machine, independently audited, built, run, tested, and corrected through repeated validation cycles.

### 2.3 Product authority

Emne remains the sole authority for product direction, scope changes, and final acceptance.

Agents may implement, inspect, test, and recommend. They cannot independently change product direction or declare final product acceptance.

## 3. M0 — Protect the persistence environment

Perform the narrowly authorized cleanup proposed in the incident report.

The only approved deletion targets are:

1. `/home/z/my-project/.cache/npm`
2. `/home/z/my-project/toolchains/rustup-init`
3. `/home/z/my-project/toolchains/cargo/registry/src`

Remove their corresponding copies from the persistent store if they exist and can be safely accessed.

These are approved because they are regenerable. Do not delete anything else.

Before cleanup, verify the exact paths and record their sizes. After cleanup:

- Verify the Rust toolchain still works.
- Verify the Cargo crate archive cache remains intact.
- Verify the frontend dependency cache can be regenerated.
- Measure both working-directory and persistent-store sizes.
- Target a durable store below 950 MB, with sufficient headroom for source files and Git history.

If any target contains unexpected project data, stop and report that specific conflict rather than deleting it.

Do not delete or modify the parent accounting repository, `.env`, `.gitignore`, `download/`, `upload/`, surviving work-order materials, or recovery artifacts.

### 3.1 Durable storage rules

From this point forward:

- Source code, Git metadata, essential scripts, fixtures, and documentation belong in the persistent project area.
- Cargo build output must live outside the persistent tree, using `CARGO_TARGET_DIR=/tmp/araiguma-target`.
- Frontend `node_modules` must live outside the persistent tree, using a gitignored symlink to `/tmp/araiguma-node-modules`.
- Regenerable npm caches must remain outside the persistent tree.
- Bootstrap scripts must recreate missing build artifacts and dependencies when necessary.
- Add an automated durable-storage size check.
- Maintain the persistent project footprint below 950 MB.

Do not assume that a file existing in the working directory guarantees that it will survive a session boundary.

## 4. M1 — Reconstruct WO-01 foundation

Use the original WO-01 requirements and the surviving evidence described in the incident report.

The complete original WO-01 work-order text, surviving fixtures, frontend design-verification script, toolchain, and dependency information are the reconstruction references.

Reconstruct the following architecture.

### 4.1 Rust workspace

Create the workspace with:

- `crates/core`
- `crates/sysfs`
- `crates/netlink`
- `crates/cli`
- `apps/frontend`

Use pinned dependencies and maintain a committed Cargo.lock.

### 4.2 Core domain

Reconstruct:

- Network interface models.
- Address models.
- Route models.
- Provider abstraction.
- Observation provenance.
- Merge layer.
- Explicit unknown/not-observed semantics.

`None` must not be silently interpreted as a negative observation.

Preserve provider provenance and distinguish missing observations from provider failures.

### 4.3 Discovery providers

Implement:

- rtnetlink-based interface, address, and route discovery.
- Sysfs/procfs fallback observations.
- Merge behavior in which netlink is primary and sysfs fills missing information.
- Provider failure degradation without discarding valid observations.

Do not build a parallel discovery architecture.

### 4.4 Netlink monitoring

Implement:

- Initial state dump.
- Multicast monitoring.
- Reconnection with exponential backoff.
- Resynchronization after reconnect.
- Pure translation functions where appropriate.

### 4.5 CLI

Provide read-only snapshot and watch functionality.

JSON output must preserve the observation model and provenance.

### 4.6 Frontend foundation

Reconstruct the Svelte 5 + TypeScript + Vite frontend.

Preserve the established design direction:

- Dark charcoal/near-black visual system.
- Narrow 56 px icon-led navigation rail.
- Central workspace occupying approximately 96% of the available width.
- Contextual collapsible details panel.
- Explicitly visible MOCK DATA indication.
- Accessible, clear interface labels.
- Dev server port 5199.

This is an application foundation, not a generic dashboard.

### 4.7 Tests and fixtures

Recreate the surviving fixture scenarios and expand them where necessary.

Include:

- Basic workstation.
- Loopback, Ethernet, and Wi-Fi.
- Virtual interfaces.
- Bridge, WireGuard, and dummy interfaces.
- IPv4 and IPv6 observations.
- Routes and incomplete observations.

Use unprivileged transient network namespaces for integration testing where supported.

Never modify the host network namespace.

## 5. M2 — WO-01R validation gate

Before proceeding into WO-02, run and record:

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `svelte-check`
- Vitest
- Frontend production build
- Playwright smoke tests
- Private D-Bus harness capability check
- Live read-only observation and cross-validation where practical
- Durable-store size check
- Git identity and repository-integrity checks

Do not claim that sandbox testing establishes compatibility with real NetworkManager, systemd-resolved, systemd-networkd, Tauri, or desktop environments.

Document what is implemented, tested, mocked, deferred, and unsupported.

## 6. M3 — Recovery and Git integrity

Initialize a fresh Araiguma repository under:

`/home/z/my-project/araiguma`

Use one `main` branch.

Configure the approved repository-local Git identity before the first commit.

Create meaningful milestone commits. Do not create artificial commits simply to reproduce the old history.

At every meaningful milestone:

1. Confirm the working tree is clean.
2. Verify author and committer identity.
3. Create a Git bundle outside the repository, under the persistent `download/` directory.
4. Create a source archive outside the repository.
5. Record the current commit hash and SHA-256 checksums.
6. Verify the recovery artifacts exist and are readable.

Use a predictable naming convention, for example:

- `download/araiguma-WO01R-M1.bundle`
- `download/araiguma-WO01R-M1.tar.gz`
- `download/araiguma-WO01R-M1.sha256`

Adapt milestone names as appropriate.

The Git bundle must preserve the complete repository history and the relevant branch references.

Never store recovery archives inside the repository itself.

Create:

`docs/incident-2026-11-18-repository-loss.md`

Record the incident, the evidence-backed cause, the reconstruction approach, and the fact that the previous 11-commit history could not be recovered.

Do not invent a reconciliation of the original 11-versus-10 commit discrepancy. Record it as permanently unresolved.

## 7. M4 — Continue directly into WO-02

After WO-01R passes its validation gate, continue directly into the existing WO-02 scope.

The original WO-02 requirements remain applicable except for the obsolete assumption that the original repository and history still exist.

Implement:

### 7.1 Repository and architecture reassessment

Inspect the reconstructed architecture before feature implementation.

Document topology, routing-table, provider-status, frontend/backend consistency, and testing decisions.

Do not build a parallel network-discovery mechanism.

### 7.2 Network topology

Implement the first functional, model-driven topology.

Use the Rust observation model as the authoritative source.

Cytoscape.js remains the preferred candidate, subject to a brief dependency and architecture assessment.

Relationships must be evidence-based. Distinguish observed, inferred, and unknown relationships.

Never invent a gateway, upstream, VPN relationship, bridge relationship, or internet connection.

Unknown relationships must remain unknown.

Support physical, logical, virtual, loopback, bridge, VPN/tunnel, gateway, and other observed kinds where the available evidence supports them.

Preserve unrecognized interface types.

Selecting a node must reveal its available contextual details without permanently reducing the central workspace.

### 7.3 Main routing-table presentation

Reuse the existing route model.

Display available fields, including:

- Destination and prefix.
- Gateway/next hop.
- Output interface.
- Protocol.
- Scope.
- Type.
- Metric/priority.
- Address family.
- Routing table.
- Relevant route attributes.

The default presentation must distinguish main-table routes from routes in other tables.

Do not discard valid observations merely because they are not part of the default presentation.

Handle multiple default routes, missing gateways, missing interfaces, IPv4, IPv6, and incomplete observations.

### 7.4 Provider status

Expose meaningful provider states using actual provider results.

Distinguish available, unavailable, degraded, partial/incomplete, unsupported, and not-yet-observed states where applicable.

Explain missing observations and provider failures without inventing health information.

NetworkManager and systemd-resolved integrations remain outside this work order.

### 7.5 Testing and documentation

Implement the feature-specific tests and documentation specified in the original WO-02 work order.

Extend `scripts/test-all.sh`.

Do not weaken existing validation merely to obtain a green result.

## 8. Engineering quality requirements

These requirements apply throughout WO-01R and WO-02.

- Prefer maintainable, comprehensible architecture over rushed implementation.
- Avoid unnecessary dependencies.
- Avoid duplicate data models and conflicting interpretations.
- Keep backend observations authoritative.
- Keep mocks visibly separate from live observations.
- Do not conceal incomplete functionality.
- Do not claim unsupported integration.
- Do not introduce network-control functionality.
- Do not introduce privileged services, root daemons, kernel modules, traffic interception, or undisclosed external communications.
- Do not use Emne's local Pop!_OS machine.
- Do not configure GitHub remotes or publish anything.
- Do not modify the parent accounting repository.

If a genuinely unexpected architecture issue, destructive operation outside the authorized cleanup, or material security concern arises, stop only the affected operation and report the issue. Do not use routine implementation questions as reasons to pause.

## 9. Closeout and handoff

Produce two clearly separated closeout sections:

**WO-01R reconstruction closeout**
- Architecture and workspace.
- Implemented functionality.
- Exact tests and results.
- Git commits and identity verification.
- Durable storage measurements.
- Recovery bundle and archive paths.
- Known limitations and technical debt.
- Incident documentation.

**WO-02 implementation closeout**
- Topology implementation and semantics.
- Route presentation and filtering.
- Provider-status behavior.
- Frontend/backend consistency.
- Exact tests and results.
- Dependency changes and security advisories.
- Remaining technical debt.
- Scope compliance.

At final handoff, create a complete compressed repository archive that includes `.git`, all source, tests, fixtures, lockfiles, and documentation. Also create a SHA-256 checksum and manifest identifying the final commit.

The final archive must be suitable for transfer to Emne's local machine for independent OpenCode audit and real-environment validation.

## 10. Final instruction

**Proceed now.**

First perform the narrowly authorized persistence cleanup and verify the resulting storage budget.

Then reconstruct WO-01, validate it, preserve it through Git and recovery artifacts, and continue directly into WO-02.

Do not spend time on GitHub publication, credentials, remote configuration, or deployment.

The priority is a correct, maintainable, fully documented, testable, and recoverable application.

Emne will decide when the application is ready for local transfer and subsequent publication. Only Emne accepts the final deliverable.
