# ARAIGUMA — WORK ORDER 01
## Project Bootstrap, Core Network Discovery Foundation & Test Infrastructure

**Work Order:** WO-01  
**Project:** Araiguma  
**Phase:** Foundation / Initial Implementation  
**Status:** Authorized for execution, subject to the mandatory inspection-and-approval gate  
**Environment:** Online development sandbox only  
**Product Owner:** Emne  
**Primary Engineering Agent:** GLM 5.3 Max  
**Independent Reviewer:** OpenCode / separately assigned review agent  
**Architecture and Product Advisor:** ChatGPT

---

# 1. Authority and governing instructions

Emne is the sole product owner and retains final authority over project direction, architecture decisions, scope changes, acceptance, and deliverables.

The engineering agent is authorized to implement the work described in this work order only after completing the initial inspection and receiving Emne's approval of the proposed implementation plan.

The agent may investigate, analyze, recommend, implement approved work, test, document, and report. It may not independently redefine product requirements, approve architectural deviations, expand scope, or treat its own recommendations as approved decisions.

The agent must distinguish clearly between:

- Existing facts discovered through inspection.
- Proposed implementation decisions.
- Decisions already authorized by Emne.
- Decisions still deferred or unresolved.
- Work completed and verified.
- Work that is incomplete, blocked, or only simulated.

Do not interpret silence as approval.

## 1.1 Approved decisions

The following decisions have been explicitly approved by Emne:

**D1 — User-space toolchain and dependency provisioning: APPROVED**

The agent may download and install the necessary user-space development tools and dependencies inside the online sandbox, within the scope of WO-01.

This authorization is strictly limited to the online sandbox.

It does NOT authorize:
- Installing anything on Emne's local Pop!_OS workstation.
- Modifying Emne's local machine, files, packages, system settings, network configuration, or development environment.
- Privileged host-level installation or system modification.
- Installing a privileged daemon or system service.
- Using credentials or accessing external accounts without explicit authorization.

Prefer project-local toolchains and dependencies where practical. Document what is installed, where it is installed, why it is needed, and how it can be reproduced.

**D5 — Polling fallback: APPROVED**

Polling may be implemented as a fallback if the sandbox's inotify limits prove restrictive.

Do not automatically make polling the default. First determine whether normal file-watching mechanisms are sufficient. Use the simplest reliable approach and document the trade-offs.

**D2 — External build infrastructure: DEFERRED**

Do not create or configure GitHub Actions, external CI, external build machines, or other build infrastructure during WO-01.

A provisional future strategy may be discussed in documentation, but it is not an authorization to implement it.

**D3 — Repository hosting and remote delivery: DEFERRED**

No GitHub repository creation, remote repository configuration, GitHub authentication, pushing, or other remote operations are authorized.

However, the agent MUST establish a proper local Git repository for the project, including meaningful commits and a clean, transferable repository structure.

The intention is to preserve the complete development history so the repository can be transferred to GitHub later.

Git must be used properly from the beginning. Do not postpone version control until a later phase.

**D4 — Primary target platform: UPDATED**

Ubuntu 24.04 LTS is the primary initial target.

Cross-version packaging and compatibility validation are not a priority in WO-01. Do not spend significant effort on Ubuntu 22.04 or 26.04 packaging at this stage.

Keep the architecture reasonably portable, but concentrate on building the foundation for Ubuntu 24.04.

---

# 2. Product definition

Araiguma is a modern Linux desktop network observability application.

Its purpose is to help users understand, inspect, and monitor their computer's network environment without requiring specialist networking knowledge.

The application should eventually provide:

1. Network topology visualization.
2. Physical and virtual network interface discovery.
3. Network connections, routes, gateways, and routing paths.
4. VPN and tunnel visibility.
5. DNS and resolver visibility.
6. Internet-sharing and upstream connection visibility.
7. Live network traffic monitoring.
8. Per-internet-source data usage monitoring.
9. Configurable data allowances and usage notifications.
10. Network event history and live notifications.
11. A compact desktop system-tray interface.
12. Useful diagnostics and clear explanations when information is unavailable.

WO-01 does not implement the complete product. It establishes the initial technical foundation on which those capabilities can be developed.

## 2.1 Non-negotiable product principle: Observe, do not control

Araiguma is a read-only network observer.

It must not:

- Modify network configuration.
- Change IP addresses, routes, gateways, DNS settings, or interface states.
- Connect to, disconnect from, or reconfigure VPNs.
- Modify firewall rules.
- Install or configure proxies, packet interceptors, or DNS resolvers.
- Intercept, modify, or redirect user traffic.
- Install kernel modules.
- Require a privileged daemon.
- Collect passwords, private keys, authentication tokens, or other credentials.
- Introduce undisclosed external communications.

Prefer unprivileged access and existing operating-system interfaces.

If a particular observation requires permissions or services that are unavailable, report that limitation and degrade gracefully.

**Important testing exception:** WO-01 may create and manipulate network interfaces, addresses, and routes inside explicitly created, transient, isolated network namespaces for testing. Such changes must remain confined to those test namespaces, which must be destroyed after testing. They must never modify the host's network configuration.

---

# 3. WO-00 baseline and known sandbox limitations

WO-00 environment reconnaissance has been completed and accepted as the initial environment baseline.

The agent must use its findings rather than repeating the entire reconnaissance exercise unnecessarily.

The following are important known facts.

## 3.1 Sandbox environment

- Debian 13.4 (trixie).
- Linux kernel 5.10.134, Alibaba Cloud Linux host kernel.
- x86_64 / amd64.
- 2 vCPU.
- Approximately 4.1 GiB RAM.
- No swap.
- Approximately 9.9 GiB overlay root filesystem, with approximately 8 GiB free at reconnaissance time.
- Unprivileged user.
- No effective Linux capabilities.
- Active seccomp restrictions.
- cgroup v1.
- No usable systemd init.
- No usable host D-Bus or NetworkManager service.
- No Linux desktop session.
- Only `/home/z/my-project` is guaranteed persistent.

These figures are reconnaissance-time observations, not permanent guarantees. Recheck available disk space before significant downloads or builds.

## 3.2 Available capabilities

The sandbox already has useful development tools, including:

- Node.js 24.
- npm 11.
- Bun.
- Python 3.12.
- GCC/G++.
- Make.
- Git.
- pkg-config.
- dpkg and apt.
- iproute2.
- D-Bus utilities.
- SQLite and OpenSSL development libraries.
- Playwright and Chromium.
- Xvfb.
- GLib, Pango, Cairo, and related libraries.

Rust toolchains, Cargo, rustfmt, and Clippy were not available at the time of reconnaissance.

Tauri's required desktop dependencies are also incomplete.

## 3.3 Important positive discovery

Unprivileged user namespaces work.

The reconnaissance agent successfully created a transient network namespace, created a dummy interface, assigned an IP address, and configured a route inside that namespace.

The test namespace was destroyed afterwards.

The following are available for unprivileged network observation:

- `/sys/class/net`
- Relevant `/proc/net` interfaces
- rtnetlink reads
- rtnetlink monitoring
- Network namespace-based test isolation

This provides a useful foundation for developing and testing the initial network discovery layer.

## 3.4 Important limitations

The sandbox cannot provide genuine host-service integration testing at this stage.

Unavailable or unverified capabilities include:

- A working NetworkManager service.
- A working systemd-resolved service.
- A usable system D-Bus session or system bus.
- systemd-networkd.
- A Linux desktop environment.
- Real Tauri GUI execution.
- Real desktop notification integration.
- Real system-tray integration.
- Full `.deb` installation and upgrade testing.
- Privileged container runtimes.
- QEMU/KVM.

Do not claim that these integrations have been tested when they have not.

Use mocks, fixtures, isolated test environments, and clearly labeled simulated service responses where appropriate.

---

# 4. Mandatory Phase 0 — Inspect before creating anything

**This phase is mandatory. Do not skip it.**

Before creating project files, installing additional dependencies, initializing a repository, or modifying existing project contents, inspect the persistent workspace.

The first task is to inspect:

- `/home/z/my-project`
- Existing project directories.
- Existing source files.
- Existing Git repositories.
- Existing Git configuration and repository status.
- Existing toolchain installations.
- Available disk space.
- Existing build artifacts and caches.
- Existing documentation and project notes.
- Any files that might conflict with the proposed project structure.

Do not assume that the directory is empty.

Do not overwrite, delete, rename, or reorganize existing files without approval.

Do not initialize Git over an existing repository without inspecting it first.

## 4.1 Inspection report

Submit a concise but sufficiently detailed report containing:

1. Workspace contents and directory structure.
2. Existing Git repository status, if any.
3. Existing toolchain availability.
4. Available disk space and likely storage constraints.
5. Relevant existing files and potential conflicts.
6. Proposed project directory structure.
7. Proposed implementation sequence.
8. Required downloads and installations.
9. Expected storage requirements.
10. Identified technical risks and unresolved questions.

## 4.2 Proposed file plan

Before creating files, provide a proposed file and directory plan.

For each proposed major component, explain:

- Its purpose.
- Why it belongs in the project.
- Its dependencies.
- Whether it is source code, configuration, test data, documentation, or build infrastructure.

Identify which files will be created, which existing files would be modified, and which existing files will remain untouched.

## 4.3 Approval gate

After submitting the inspection report and proposed file plan:

**STOP AND WAIT FOR EMNE'S APPROVAL.**

Do not begin implementation, initialize or alter Git, install tools, create directories, or generate project files before approval of this plan.

The D1 authorization permits the necessary user-space provisioning for WO-01, but it does not bypass this inspection-and-approval gate.

---

# 5. WO-01 implementation objectives

After Emne approves Phase 0, implement the following objectives.

## 5.1 Establish the development foundation

Provision a reproducible development environment suitable for Rust and Svelte-based development.

Preferred toolchain:

- Rust stable.
- Cargo.
- rustfmt.
- Clippy.
- Node.js and npm, reusing the existing compatible installations where practical.
- Svelte 5.
- TypeScript.
- Vite.
- Relevant test tooling.

Prefer a project-local Rust toolchain installation under the persistent workspace if feasible.

Avoid unnecessary global installations.

Pin or otherwise record important toolchain and dependency versions. Do not use unbounded dependency updates.

Create a reproducible bootstrap process that can be executed again in a fresh compatible sandbox.

The bootstrap process should:

1. Check prerequisites.
2. Check available disk space.
3. Install or provision missing user-space tools.
4. Verify tool versions.
5. Set up project dependencies.
6. Report success or actionable errors.
7. Avoid unnecessary downloads when dependencies are already available.

Use a shared Cargo target directory to reduce duplicate build artifacts.

Monitor storage consumption throughout the work. Do not allow large build caches or duplicated toolchains to consume the sandbox's limited storage without justification.

Do not install Tauri's complete desktop dependency stack merely for the appearance of progress if it cannot be used in this environment. Record what would be needed for the later desktop-build stage.

## 5.2 Establish a proper local Git repository

Create a clean, conventional Git repository for Araiguma, subject to approval of the Phase 0 plan.

Requirements:

- Local Git only.
- No GitHub or other remote hosting.
- No remote URL.
- No credentials or authentication setup.
- No pushing or fetching from an external project repository.
- Meaningful commit history.
- Clear, descriptive commit messages.
- A sensible `.gitignore`.
- No generated build artifacts or caches committed.
- No credentials, secrets, personal tokens, or sensitive environment information committed.

The repository must be suitable for later transfer to GitHub without needing to reconstruct its history.

Do not make one enormous initial commit containing all work if the project can be separated into meaningful milestones.

Suggested commit sequence:

1. Initial project structure and development documentation.
2. Toolchain and bootstrap infrastructure.
3. Rust workspace and core abstractions.
4. Initial Linux network discovery providers.
5. Test fixtures and automated tests.
6. Frontend foundation and smoke tests.
7. Documentation and WO-01 completion.

Adapt this sequence if inspection reveals a better arrangement.

Every commit must represent a coherent, reviewable state.

At completion, report the commit hashes, repository status, and commit history.

## 5.3 Establish a maintainable project structure

Create a structure that supports future growth without unnecessary complexity.

The preferred direction is:

- Rust backend.
- Svelte 5 frontend.
- TypeScript.
- Vite.
- Tauri 2 as the intended desktop application framework.
- SQLite as the intended local persistence layer.
- Cytoscape.js as the intended topology visualization library.
- Apache ECharts as the intended charting library.

These are the current architectural preferences, not permission to introduce every dependency immediately.

For WO-01, only introduce dependencies that serve an actual implementation or test requirement.

Do not create a large collection of empty modules, speculative abstractions, or placeholder subsystems.

A possible high-level structure is:

- `apps/desktop/` — future desktop application shell.
- `apps/frontend/` — Svelte frontend.
- `crates/` — Rust workspace and backend components.
- `crates/core/` — shared domain models and core abstractions.
- `crates/network-linux/` — Linux-specific discovery.
- `crates/network-providers/` — provider interfaces and implementations where justified.
- `tests/` — integration tests.
- `fixtures/` — representative network-state fixtures.
- `scripts/` — bootstrap, validation, and development scripts.
- `docs/` — architecture, development, and implementation documentation.

This is a proposal, not a mandatory directory layout. Finalize the structure through the Phase 0 file plan.

Keep the architecture understandable and avoid unnecessary microservices or excessive crate fragmentation.

---

# 6. Initial Rust backend foundation

The Rust backend must become the authoritative source of network observations.

The frontend must not independently discover or reconstruct network state.

Establish a minimal but extensible backend architecture.

## 6.1 Core domain models

Create appropriate domain models for the information that the initial discovery providers can actually observe.

Potential entities include:

- Network interface.
- Interface address.
- Interface state.
- Link type.
- Route.
- Gateway.
- Network namespace.
- Observation timestamp.
- Discovery source.
- Observation confidence or status.
- Provider availability.

Models should distinguish known information from unknown or unavailable information.

Do not invent fields or values simply to make the frontend appear complete.

Where possible, preserve the distinction between:

- Directly observed information.
- Operating-system-reported information.
- Inferred information.
- Simulated test information.
- Unknown information.

Do not conflate physical and logical interfaces.

Do not assume that an interface represents an independent internet connection.

Do not assume that every route represents an active internet path.

## 6.2 Provider abstraction

Create a modest provider architecture that allows additional Linux network-information sources to be added later.

The initial implementation should focus on:

1. Linux sysfs.
2. Relevant procfs information.
3. rtnetlink.

Design the provider boundary so future integrations can include:

- NetworkManager D-Bus.
- systemd-resolved D-Bus.
- systemd-networkd where available.
- Other supported Linux networking environments.

Do not make NetworkManager a mandatory dependency.

Do not assume all Ubuntu installations use the same network management stack.

Do not create a monolithic discovery implementation that will be difficult to extend.

## 6.3 Initial discovery capabilities

Implement the first practical read-only discovery capabilities.

At minimum, investigate and implement where feasible:

- Interface enumeration.
- Interface names and indices.
- Interface operational state.
- Interface type where discoverable.
- MAC address where available.
- MTU where available.
- IP addresses.
- IPv4 and IPv6 routes.
- Default routes.
- Gateway information where available.
- Route/interface relationships.
- Relevant link information exposed by rtnetlink.

Use rtnetlink as the primary structured interface for kernel network-state discovery where practical.

Use sysfs and procfs as complementary sources.

Do not make shell-command parsing the primary implementation architecture.

The application should not need to execute `ip`, `nmcli`, `resolvectl`, or similar commands to perform its core discovery.

Those commands may be used during development for independent verification, not as the application's primary data source.

## 6.4 Live monitoring

Implement a minimal event-driven discovery mechanism using rtnetlink monitoring where feasible.

The initial backend should be capable of observing relevant changes such as:

- Interface creation and removal.
- Interface state changes.
- Address changes.
- Route changes.

Ensure that monitoring can recover from interruption or failed subscriptions.

Avoid busy loops and unnecessary polling.

If a polling fallback is needed, use the approved D5 decision. Document the fallback conditions and its resource implications.

Do not implement the complete notification system in this work order.

The purpose is to establish a reliable source of network change events that later components can consume.

---

# 7. Testing and validation

Testing is a major part of WO-01.

Do not treat successful compilation as sufficient evidence of correctness.

## 7.1 Rust tests

Implement unit and integration tests for:

- Domain models.
- Parsing and normalization.
- Interface discovery.
- Address discovery.
- Route discovery.
- Default-route handling.
- Missing or incomplete information.
- Provider failures.
- Duplicate or conflicting observations where applicable.
- Event handling.
- Error handling.

Use fixtures for representative network states.

Fixtures should cover ordinary and unusual cases, including:

- One physical Ethernet interface.
- One Wi-Fi interface.
- Multiple interfaces.
- IPv4 and IPv6.
- Multiple addresses on one interface.
- Loopback.
- Virtual interfaces.
- Bridges.
- VPN-like interfaces.
- Default routes.
- Multiple routes.
- Missing gateway information.
- Interfaces that are down.
- Incomplete or unavailable observations.

Do not claim that a fixture proves the corresponding real-world service integration works.

Label synthetic data clearly.

## 7.2 Network namespace tests

Use transient unprivileged network namespaces to test actual kernel networking behavior where feasible.

Potential test cases:

- Create a namespace.
- Create a dummy interface inside it.
- Assign addresses.
- Add routes.
- Observe the resulting interface and route state.
- Monitor changes through rtnetlink.
- Destroy the namespace.

All changes must remain inside the transient test namespace.

The test must clean up after itself, including on ordinary test failure where feasible.

Never modify the host network namespace.

Never use host networking or privileged containers as a workaround.

If a test cannot safely guarantee isolation, do not run it.

## 7.3 D-Bus test foundation

The sandbox does not provide usable NetworkManager or systemd-resolved services.

Do not claim real service integration testing.

Where useful, establish a private D-Bus test harness or a mock-provider boundary for future NetworkManager and systemd-resolved integrations.

Keep this limited to the foundation needed for future work.

Do not build complete fake implementations of these services.

The purpose is to make later integration testing possible without depending on the sandbox's host services.

## 7.4 Frontend tests

Establish a minimal frontend with:

- Svelte 5.
- TypeScript.
- Vite.
- A basic application entry point.
- A basic layout.
- A clear separation between frontend presentation and backend observations.

Use Vitest or an appropriate lightweight test framework where justified.

Use the existing Playwright installation for a basic browser-based smoke test if practical.

The smoke test should establish that:

- The frontend starts.
- The application renders.
- The basic navigation or layout is present.
- The interface can display representative mock network observations.
- The page does not produce critical runtime errors.

A mock frontend is acceptable for this phase, but label it as mock data.

Do not create a misleading frontend that presents fabricated network state as real discovery.

## 7.5 Test report

For every test, distinguish:

- Passed.
- Failed.
- Skipped.
- Not executable in this environment.
- Simulated or mock-only.

Include the relevant command, result, and limitations.

Do not silently skip tests.

---

# 8. Frontend foundation and design direction

WO-01 should establish the frontend foundation, not attempt to complete the final UI.

The design direction is already defined and should guide the initial scaffold.

## 8.1 Product design principles

Araiguma should feel like a polished, modern desktop utility.

It must not look like:

- A generic web dashboard.
- A cybersecurity control room.
- A gaming interface.
- A terminal emulator.
- A collection of oversized cards.
- A complex network-engineering tool that assumes expert knowledge.

The UI must be approachable for ordinary users while retaining access to advanced information.

Prioritize:

- Clear hierarchy.
- Readable typography.
- Minimal clutter.
- Strong use of central workspace.
- Restrained color and visual effects.
- Consistent spacing.
- Responsive desktop layout.
- Clear labels and useful explanations.

## 8.2 Layout direction

The eventual application should use:

- A narrow, icon-led left navigation area that can expand.
- A large central workspace.
- A contextual right-side information panel that is collapsible and hidden when unnecessary.

Avoid permanent oversized sidebars.

The topology workspace should receive the majority of available screen space.

Selecting a network node should expose contextual information without permanently reducing the central workspace.

## 8.3 Initial frontend scope

For WO-01, implement only a modest foundation:

- Application shell.
- Navigation structure or a small representative subset.
- Overview page placeholder.
- Central workspace.
- Basic network observation presentation.
- A minimal contextual details area if useful.
- Consistent theme and design tokens.

Use a restrained dark visual foundation with near-black or charcoal surfaces and subtle cyan, blue, or green accents.

Do not spend time creating elaborate illustrations, mascots, animations, or decorative effects.

Do not implement the final topology visualization yet.

Do not implement the complete navigation structure unless it can be done without unnecessary scaffolding.

The frontend must be structured to consume backend observations later.

---

# 9. Resource and dependency management

The sandbox has limited disk capacity and only 2 vCPU.

Resource management is therefore an engineering requirement.

The agent must:

- Check available disk space before major downloads.
- Avoid installing unnecessary packages.
- Reuse existing compatible tools.
- Prefer project-local installations.
- Use a shared Cargo target directory.
- Avoid duplicate browser downloads.
- Avoid redundant dependency trees.
- Keep build artifacts out of Git.
- Avoid unnecessary parallel compilation that could exhaust memory.
- Record significant disk usage changes.
- Clean temporary artifacts when safe.

Do not delete existing files or caches outside the approved project scope.

Do not sacrifice reproducibility for temporary convenience.

If a dependency cannot be installed or a build cannot be completed due to resource limitations, document the exact blocker and propose the smallest practical alternative.

---

# 10. Documentation deliverables

Documentation is part of the implementation, not an afterthought.

Create or update the following documents as appropriate to the approved file plan:

## 10.1 README

Must explain:

- What Araiguma is.
- Its read-only operating principle.
- Current development status.
- Supported development environment.
- How to bootstrap the environment.
- How to build and test the current implementation.
- Known limitations.

Do not describe planned functionality as already implemented.

## 10.2 Architecture overview

Document:

- Frontend/backend separation.
- Rust workspace structure.
- Core domain models.
- Provider abstraction.
- Initial Linux discovery implementation.
- Event flow.
- Test architecture.
- Future integration points.

Include a simple architecture diagram if useful.

## 10.3 Development guide

Document:

- Required tools.
- Toolchain versions.
- Bootstrap procedure.
- Build commands.
- Test commands.
- Frontend development commands.
- Git workflow.
- Resource limitations and troubleshooting.

## 10.4 Network discovery notes

Document:

- What each discovery source provides.
- Which information comes from sysfs, procfs, and rtnetlink.
- Known limitations.
- Permissions required.
- Information that cannot currently be observed.
- How the provider architecture can later accommodate NetworkManager and systemd-resolved.

## 10.5 Environment and dependency inventory

Record:

- Installed tool versions.
- Dependency versions.
- Installation locations.
- Bootstrap procedure.
- Important environment variables.
- Build limitations.
- Any dependencies that remain unavailable.

Do not include secrets, credentials, or private environment data.

---

# 11. Explicit exclusions

The following are NOT WO-01 deliverables.

Do not implement them merely because they appear in the long-term product vision.

1. Complete network topology engine.
2. Final Cytoscape topology visualization.
3. Full traffic accounting and upstream attribution.
4. Per-source data allowances and billing-cycle management.
5. Data usage forecasting.
6. Full notification manager.
7. Desktop native notifications.
8. System-tray implementation.
9. Startup/background monitoring.
10. SQLite persistence architecture beyond what is strictly needed for the foundation.
11. Full NetworkManager integration.
12. Full systemd-resolved integration.
13. Full systemd-networkd integration.
14. Tauri desktop packaging and `.deb` release.
15. Cross-Ubuntu-version packaging validation.
16. GitHub repository creation.
17. GitHub Actions or external CI setup.
18. Remote Git configuration or pushing.
19. Privileged services or daemons.
20. Network configuration control features.

These are future work unless Emne explicitly authorizes a scope change.

---

# 12. Engineering quality requirements

The implementation must prioritize maintainability, correctness, and clear boundaries over rapid feature accumulation.

Requirements:

- Clear module responsibilities.
- Explicit error handling.
- No silent failures.
- No fabricated network state.
- No unnecessary dependencies.
- No speculative abstraction layers.
- No duplicated sources of truth.
- No frontend-owned network discovery logic.
- No hard-coded assumptions that NetworkManager is always present.
- No unnecessary shell-command dependencies.
- No unexplained unsafe Rust.
- No unbounded background polling.
- No unbounded event accumulation.
- No hidden network communications.
- No unnecessary telemetry.
- No credential collection.

Use conventional Rust practices and formatting.

Run rustfmt and Clippy where available.

Use appropriate TypeScript checks and frontend linting where practical.

Pin important dependencies and preserve a reproducible lockfile.

If an implementation decision would materially change the architecture or product behavior, stop and request approval.

---

# 13. Milestones and acceptance criteria

WO-01 is divided into reviewable milestones.

## Milestone 0 — Inspection and implementation plan

Deliver:

- Workspace inspection report.
- Existing repository status.
- Toolchain inventory.
- Disk-space assessment.
- Proposed directory structure.
- Proposed file creation/modification plan.
- Dependency and installation plan.
- Risk assessment.

**Acceptance:** Emne approves the implementation plan.

No files or tools may be created or installed before this approval.

## Milestone 1 — Development foundation and Git

Deliver:

- Reproducible bootstrap mechanism.
- Rust toolchain.
- Frontend toolchain.
- Local Git repository.
- `.gitignore`.
- Initial project structure.
- Initial documentation.

Acceptance:

- Tool versions can be verified.
- Bootstrap process works or has clearly documented environment-specific blockers.
- Git history is meaningful.
- Repository is clean at the milestone commit.
- No remote repository is configured.

## Milestone 2 — Rust core and network discovery

Deliver:

- Rust workspace.
- Core domain models.
- Provider abstraction.
- Initial sysfs/procfs discovery.
- Initial rtnetlink discovery.
- Basic error handling.
- Initial tests.

Acceptance:

- Rust code compiles.
- Formatting checks pass.
- Clippy runs and relevant issues are resolved or documented.
- Unit tests pass.
- Discovery distinguishes unavailable information from actual absence.
- No host network configuration is changed.

## Milestone 3 — Live events and isolated testing

Deliver:

- Initial rtnetlink event monitoring.
- Interface/address/route change handling.
- Transient network namespace tests.
- Representative fixtures.
- D-Bus test foundation where feasible.

Acceptance:

- Event monitoring works in the capabilities available in the sandbox.
- Namespace tests demonstrate actual isolated network observations.
- Tests clean up their test environments.
- Host network state remains untouched.
- Unsupported service integrations are clearly identified.

## Milestone 4 — Frontend foundation

Deliver:

- Svelte 5 and TypeScript frontend.
- Basic application shell.
- Initial visual design foundation.
- Representative network observation presentation.
- Basic frontend test.
- Playwright smoke test where feasible.

Acceptance:

- Frontend starts and renders.
- Mock observations are explicitly identified as mock data.
- The layout follows the agreed design direction.
- The frontend does not claim to have live backend integration if that integration is not yet implemented.

## Milestone 5 — Integration, cleanup, and final review

Deliver:

- Final build and test results.
- Updated documentation.
- Dependency inventory.
- Git history.
- Known limitations.
- Unresolved issues.
- Recommended next work order.

Acceptance:

- All completed milestones have evidence.
- No unexplained uncommitted changes.
- No accidental secrets or build artifacts in Git.
- Repository is in a clean, transferable state.
- The agent has not exceeded WO-01 scope.

---

# 14. Reporting requirements

Provide a short report after each milestone.

Each report must include:

1. What was completed.
2. What files or components were created or modified.
3. Important implementation decisions.
4. Commands executed.
5. Test results.
6. Git commit hash.
7. Known limitations.
8. Outstanding problems.
9. Any decision requiring Emne's approval.

At the end of WO-01, provide a complete closeout report.

The closeout report must include:

- Final project directory tree.
- Git repository status.
- Commit history.
- Toolchain inventory.
- Dependency inventory.
- Build results.
- Test results.
- Test coverage at a meaningful high level.
- Sandbox limitations.
- Outstanding technical debt.
- Deferred integrations.
- Recommended WO-02 scope.

Clearly distinguish between completed, partially completed, blocked, and deferred work.

Do not mark an item complete solely because a placeholder, interface, mock, or documentation entry exists.

---

# 15. Scope-change and escalation protocol

Stop and consult Emne before:

- Changing the approved architecture materially.
- Adding a major dependency not in the approved plan.
- Introducing privileged operations.
- Modifying host networking.
- Accessing Emne's local workstation.
- Creating remote repositories.
- Configuring external CI.
- Using credentials or external accounts.
- Expanding into excluded features.
- Making an irreversible change to existing project files.
- Making a change that would compromise the read-only product principle.

If a technical obstacle prevents an objective, do not silently replace it with a different implementation.

Explain:

- The obstacle.
- Evidence that it exists.
- Available alternatives.
- The recommended option.
- The impact on scope and future architecture.

Wait for approval when the decision belongs to Emne.

---

# 16. Final instruction

Begin with **Milestone 0 only**.

Inspect the existing persistent online sandbox workspace and prepare the inspection report and proposed implementation/file plan.

Do not create files, install dependencies, initialize Git, or begin implementation until Emne reviews and approves the proposed plan.

After approval, execute WO-01 incrementally, preserving meaningful Git commits and reporting each milestone.

The goal is not to produce the largest amount of code possible.

The goal is to establish a technically sound, reproducible, testable, maintainable foundation for Araiguma, with a clean development history and no compromise to the product's read-only principle.

**Emne is the final authority on all project decisions and deliverable acceptance.**