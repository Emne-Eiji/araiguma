# 08 — Decision and assumption register

Prompt 03 deliverable: the consolidated register separating approved
decisions, this package's recommendations (not yet approved), unresolved
decisions (including the still-open OD-04..OD-07), assumptions, and
future work — with traceability. **Nothing in this register converts a
recommendation into an approved decision**; RD-xx items await Emne's
review exactly as OD-04..OD-07 do (work order §2, binding).

## 1. Approved decisions (Emne) and how this package implements them

| ID | Approved decision (as recorded in the WO-02 closeout correction, Table 11.2) | Implementation in this package |
|---|---|---|
| OD-01 | Tauri 2 is the desktop application shell | Layer 2 = `apps/desktop`, the only Tauri-dependent crate (01 §2); runtime ownership rules (01 §8); Tauri capability discipline (06 §5); thin-slice Track 2 check (07 §9) |
| OD-02 | NetworkManager and systemd-resolved are optional, capability-probed providers; graceful degradation when either is unavailable | Four-axis availability/permission/support/health model (02 §3); capability reports in every document (02 §5, 04 §2); degradation invariants (02 §1, 03 §1); provider-failure tests (07 §8) |
| OD-03 | Tauri IPC provides live snapshots and events; file import retained; one JSON document contract across CLI, import, IPC | Document 04 in full: single contract, v4 envelope, watch/event envelopes with seq, four-command IPC surface (Prompt 03-FC FC-01: `unsubscribe_watch` added), validation at every boundary, atomic replacement |
| OD-08 | Configuration under `~/.config/araiguma`; logs and session state under `~/.local/state/araiguma` | Document 05 §1 (binding paths, XDG overrides, ownership/upgrade rules) |
| OD-09 | Prompt 03 defines logging locations, rotation, diagnostics-export design | Document 05 in full |
| OD-10 | First local Ubuntu 24.04 validation after Prompt 03, before Tauri implementation: thin Tauri slice, .deb install/upgrade/removal, live NM/resolved probes including permissions | Document 07 §9 — the checkpoint's seven checks, including the real-host read-only invariant (07 §8) |

## 2. Open decisions (not pre-empted; dependencies made explicit)

| ID (this register) | Open decision | Where this package depends on it | Recommendation (for Emne, when the owning prompt opens) |
|---|---|---|---|
| UD-01 (= OD-04) | D-Bus crate choice (zbus) — owned by Prompt 05 | 02 §11, 06 §4: the contract is crate-agnostic by construction — a single `dbus-client` module per service isolates the choice behind the allow-listed proxy; nothing in documents 01–07 requires zbus specifically | adopt zbus 6.4.x exact-pinned (WO-02 §11 Table 11.1 recommendation) |
| UD-02 (= OD-05) | Distribution format at 1.0.0 — owned by the packaging prompt | 07 §9 check 3 (`.deb` lifecycle) is written for the recommended format; the architecture (XDG state, no privileged helpers) is format-agnostic | Debian `.deb` only, Ubuntu 24.04 (WO-02 recommendation) |
| UD-03 (= OD-06) | Update mechanism at 1.0.0 — owned by the packaging prompt | 05 §1 upgrade-safety and 07 §9 check 3's upgrade step assume **manual** upgrade only; no updater surface exists anywhere in this design | no auto-updater (WO-02 recommendation) |
| UD-04 (= OD-07) | Tray timing / WO-TRAY-01 scope | 01 §9 and 03 §11 preserve the compatibility constraints and reserve the status vocabulary without implementing any tray behavior | defer to WO-TRAY-01 (WO-02 recommendation); every constraint it needs is preserved here |

## 3. Recommendations introduced by this package (RD — require Emne's approval before the implementing prompt treats them as settled)

| ID | Recommendation | Package section | Rationale (condensed) |
|---|---|---|---|
| RD-01 | New `crates/engine` owns observation orchestration (providers, debounce, resync, envelopes) consumed by CLI and desktop alike | 01 §4 | removes CLI/shell duplication; engine testable headless; watch semantics stay identical [REPO] |
| RD-02 | Session + collection identity model; ordered cooperative shutdown with grace period | 01 §6 | correlation root for logs/diagnostics; deterministic teardown |
| RD-03 | Provider identity as stable strings; unknown ids preserved verbatim in readers | 02 §2 | E-1 resolved without breaking serialized reports; matches `Other(..)` discipline [REPO] |
| RD-04 | `permissionDenied` as a first-class section outcome and capability axis | 02 §3 | distinguishes not-installed / not-permitted / broke — exactly what OD-10 probes must show |
| RD-05 | Capability `probe()` separated from `observe()`; probing is read-only presence-only | 02 §4–5 | capability without observation; no traffic from probing |
| RD-06 | `ChangeFeed` abstraction; engine owns debounce/poll scheduling | 02 §6 | one debounce policy; providers stay sources, not schedulers |
| RD-07 | Providers own their resources; Stopped is terminal (fresh instance to restart) | 02 §10 | trivially testable lifecycle; no half-restarted providers |
| RD-08 | Poll-fallback default 30 s | 02 §6 | honest freshness without busy polling; configurable |
| RD-09 | Retry backoff 250 ms → 8 s (existing monitor values [REPO]) | 02 §7 | proven values; no new tuning |
| RD-10 | Re-probe cadence 60 s for dead providers | 02 §7 | conservative; no bus hammering |
| RD-11 | Shared D-Bus connections only with a single explicit owner | 02 §10 | deterministic shutdown if a connection is ever shared |
| RD-12 | Conflict records: field and existence conflicts retained with all assertions; precedence resolves presentation only | 03 §9 | work order §3.5 requirement — evidence never discarded |
| RD-13 | `RoutingRule` entity (rtnetlink rule dump); no policy-routing inference | 03 §6.2 | "where observable" — honest raw co-observation |
| RD-14 | Shutdown grace default 5 s (configurable) | 01 §6.3 | bounded, visible, never hangs |
| RD-15 | Schema v4 additive `meta` envelope (identity/producer/synthetic/collection) | 04 §2 | v2/v3 additive precedent [REPO]; identity + synthetic labels structural |
| RD-16 | No binary transport for v1.0.0 | 04 §3 | payload sizing [WO-02 §2.3]; JSON is the one contract |
| RD-17 | Import time recorded by the importer in session state, never in the document | 04 §2 | documents are immutable artifacts; the two times stay distinct facts |
| RD-18 | Envelope `seq` (monotonic, per session) + snapshot-first gap recovery | 04 §7–8 | continuity detection without new transports; R-10 |
| RD-19 | `get_topology` also projects caller-supplied validated snapshots | 04 §9 | projection authority stays in the backend |
| RD-20 | `config.toml` (TOML); defaults on absence; warn + defaults on malformed | 05 §2 | never crash on own config; inspectable everywhere |
| RD-21 | JSONL structured logs with a fixed field vocabulary | 05 §3 | machine-parseable; the redaction allowlist is defined over it |
| RD-22 | Rotation 5 MiB × N, default bound 50 MiB, oldest-first deletion | 05 §5 | bounded storage by construction |
| RD-23 | Append-only session registry with unclean-crash detection | 05 §6 | crash forensics without new infrastructure |
| RD-24 | Diagnostics export: field allowlist + per-category privacy policy (§9.1); full MAC redaction default (partial disclosure only by explicit user opt-in — Prompt 03-FC FC-03); pattern pass over free-form strings; self-describing manifest | 05 §9 | structural privacy; user-initiated only |
| RD-25 | Restricted-proxy D-Bus design with the per-operation (service/path/interface/member/operation) allow-list; named-`Get`-only property access; `GetAll`/`GetManagedObjects` excluded from v1.0.0 (Prompt 03-FC FC-02) | 06 §4 | Q-1 resolved; R-03 mitigated structurally |
| RD-26 | Tauri capability set = `core:event:allow-listen` and `core:event:allow-unlisten` only; no event emit, dialog, shell, fs, or http permissions | 06 §5 | deny-by-default made reviewable; the frontend cannot spoof backend-owned monitor-health facts |
| RD-27 | `scripts/check-readonly.sh`: eight mechanical audits in the gate | 06 §6 | WO-02 Layer 4 mechanized |
| RD-28 | Shared provider conformance suite (mock sources injected) | 07 §4 | every provider provably honors the same contract |
| RD-29 | Mock D-Bus services offer prohibited members; proxies proven not to call them | 07 §4 | the R-03 backstop, executable in Track 1 |
| RD-30 | CLI/IPC envelope equality asserted by construction tests | 07 §6 | OD-03's "one contract" made executable |

### 3.1 Implementation disposition (Prompt 03-FC FC-04 — classification, not approval)

The approval status of **every** RD-01..RD-30 is **NOT APPROVED —
recommendation awaiting Emne's review**; nothing in this package, this
correction, or a future acceptance of the package text silently
promotes, merges, or reclassifies any of them as an approved decision.
The table below classifies each recommendation's **implementation
disposition only** — what kind of choice it is and what it would block
if implemented. Vocabulary (exactly these values):

- **REQUIRES EMNE DECISION** — implementation-blocking design choice on
  an approved-decision surface: the implementing prompt must not adopt
  it (or any alternative) without Emne approving this specific design;
- **NON-BLOCKING DESIGN GUIDANCE** — advisory content (typically
  default values or engineering practice) an implementer may follow
  while Emne retains the option to retune; adopting it is not a product
  decision;
- **DEFERRED TO A LATER PROMPT** — the choice is owned by a named
  future prompt; this package's text is input to that decision.

| ID | Disposition | Affected subsystem | Blocks a future implementation work order? | Dependencies | Note |
|---|---|---|---|---|---|
| RD-01 | REQUIRES EMNE DECISION | crate layout, `crates/engine`, CLI watch refactor | **yes** — the first implementation WO's crate boundaries | RD-06, RD-02 | the spec recommends `crates/engine` as a new crate; an implementer must not invent its own boundary — adopt as specified or Emne decides otherwise |
| RD-02 | REQUIRES EMNE DECISION | engine lifecycle, logging correlation | **yes** — engine and logging implementation | RD-01, RD-23 | session/collection identity is the correlation root for documents 04–05 |
| RD-03 | REQUIRES EMNE DECISION | schema v4 serialization, provider identity | **yes** — schema v4 and the provider contract | RD-15 | identity format is wire contract; changing it later means another schema bump |
| RD-04 | REQUIRES EMNE DECISION | provider contract, schema v4, UI states | **yes** — provider contract implementation; OD-10 probe reporting | OD-02, OD-10 | the four-axis model (document 02 §3) depends on this outcome type |
| RD-05 | REQUIRES EMNE DECISION | provider lifecycle | **yes** — provider implementation | OD-02 | probe/observe separation is contract-level, not internal |
| RD-06 | REQUIRES EMNE DECISION | engine, netlink refactor (Monitor → `ChangeFeed`) | **yes** — engine and netlink changes | RD-01 | the abstraction reshapes existing working code; needs explicit approval |
| RD-07 | REQUIRES EMNE DECISION | provider lifecycle and tests | **yes** — provider implementation | — | resource-ownership rule drives the conformance suite (document 07 §4) |
| RD-08 | NON-BLOCKING DESIGN GUIDANCE | engine poll scheduling | no — a default value, configurable (document 05 §2) | RD-06 | Emne may retune; implementation proceeds with the default |
| RD-09 | NON-BLOCKING DESIGN GUIDANCE | engine retry policy | no — default value (existing proven backoff [REPO]) | — | values already proven by the existing netlink monitor |
| RD-10 | NON-BLOCKING DESIGN GUIDANCE | engine re-probe cadence | no — default value, configurable | — | conservative default; no bus hammering |
| RD-11 | DEFERRED TO A LATER PROMPT | D-Bus connection sharing | no — not needed until Prompt 05 | RD-25 | the constraint (single explicit owner) is stated here; the choice is Prompt 05's |
| RD-12 | REQUIRES EMNE DECISION | schema v4, merge, UI | **yes** — schema v4 and merge implementation | RD-15 | conflict records change the document shape |
| RD-13 | REQUIRES EMNE DECISION | schema v4, netlink (rule dump) | **yes** — v4 entity and provider section | RD-15 | new entity; deliberately no inference semantics attached |
| RD-14 | NON-BLOCKING DESIGN GUIDANCE | shutdown behavior | no — default value, configurable | RD-02 | a bounded grace is the requirement; 5 s is the default |
| RD-15 | REQUIRES EMNE DECISION | schema v4 envelope — the core document contract | **yes** — everything downstream of documents | OD-03 | see the schema-version note below |
| RD-16 | NON-BLOCKING DESIGN GUIDANCE | IPC transport policy | no — the near-term substance (one JSON contract) is already approved via OD-03; this RD adds the explicit v1.0.0 binary exclusion | OD-03 | binary transports stay excluded unless Emne later decides otherwise |
| RD-17 | REQUIRES EMNE DECISION | import path, session state | **yes** — imported-documents store and session state | RD-23, OD-03 | the *separation* of observation/import time is work-order-mandated; the *mechanism* needs approval |
| RD-18 | REQUIRES EMNE DECISION | IPC events, engine envelopes, CLI watch | **yes** — envelope construction everywhere | RD-15, OD-03 | `seq` + snapshot-first recovery is the continuity contract (document 04 §7–8) |
| RD-19 | REQUIRES EMNE DECISION | IPC command semantics | **yes** — IPC implementation | OD-03, RD-18 | caller-supplied projection keeps projection authority in the backend |
| RD-20 | REQUIRES EMNE DECISION | configuration | **yes** — configuration implementation | OD-08 | format choice is user-facing |
| RD-21 | REQUIRES EMNE DECISION | logging implementation | **yes** — logging implementation | OD-09, RD-23 | OD-09 assigns this design to Prompt 03; the specifics need acceptance |
| RD-22 | NON-BLOCKING DESIGN GUIDANCE | log rotation | no — the rotation mechanism is OD-09-mandated; the values are defaults | OD-09 | tunable via configuration |
| RD-23 | REQUIRES EMNE DECISION | session registry, diagnostics | **yes** — logging/diagnostics implementation | RD-02 | registry schema is a persistence contract |
| RD-24 | REQUIRES EMNE DECISION | diagnostics export | **yes** — export implementation | RD-21, OD-09 | FC-03 updated the policy (full MAC redaction default); still a recommendation |
| RD-25 | REQUIRES EMNE DECISION | D-Bus enforcement, Prompt 05 foundation | **yes** — any D-Bus provider work (Prompt 05) | OD-02 | FC-02 refined the allow-list to per-operation scoping; still a recommendation |
| RD-26 | REQUIRES EMNE DECISION | desktop shell capabilities | **yes** — `apps/desktop` implementation | OD-01, RD-27 | capability set is security-relevant (document 06 §5) |
| RD-27 | REQUIRES EMNE DECISION | validation gate | **yes** — gate extension at implementation time | RD-25, RD-26 | adds eight mechanical audits; a process change needing approval |
| RD-28 | NON-BLOCKING DESIGN GUIDANCE | test architecture | no — engineering practice for the implementing WO | RD-07 | adopting the conformance suite requires no product decision |
| RD-29 | NON-BLOCKING DESIGN GUIDANCE | test architecture | no — engineering practice | RD-25, RD-28 | the R-03 backstop, executable in Track 1 |
| RD-30 | NON-BLOCKING DESIGN GUIDANCE | test architecture | no — engineering practice | OD-03, RD-18 | one-contract proof by construction (document 07 §6) |

**Schema-version note (Prompt 03-FC §7.4).** The specification uses
schema version 4; the rationale is documented, not invented: the
repository's snapshot contract has shipped **v1 → v2 → v3**
(`SNAPSHOT_SCHEMA_VERSION = 3` at the baseline; each bump recorded in
`crates/core/src/snapshot.rs` and `docs/development.md` — v2 is the
MAC-ambiguity fix of the WO-02 correction, v3 the WO-03 additive
`peerIfindex`/multipath bump). Schema v4 is the **next additive bump in
that existing series**, extending v3 with the `meta` envelope (RD-15).
The version number is not a new product line, no v1/v2/v3 compatibility
obligation is invented by this package (reader acceptance of older
versions follows the pre-existing policy — document 04 §4), and the
version is left unchanged by this correction.

Tally: 20 × REQUIRES EMNE DECISION (RD-01..07, RD-12, RD-13, RD-15,
RD-17..21, RD-23..27), 9 × NON-BLOCKING DESIGN GUIDANCE (RD-08, RD-09,
RD-10, RD-14, RD-16, RD-22, RD-28, RD-29, RD-30), 1 × DEFERRED TO A
LATER PROMPT (RD-11). None approved; none superseded (no documented
replacement exists for any of them).

## 4. Assumptions (explicit, with basis and validation path)

| ID | Assumption | Basis | Validation |
|---|---|---|---|
| A-1 | Tauri IPC events arrive ordered within a session; `seq` guards regardless | Tauri 2 event semantics | 07 §6 tests + Track 2 slice (07 §9 check 2) |
| A-2 | NetworkManager property gets are permitted unprivileged on default Ubuntu 24.04 | D-Bus/polkit defaults; WO-02 §4.3 | OD-10 live probes (07 §9 check 4); code never assumes it (capability probing handles denial — 02 §3) |
| A-3 | systemd-resolved manager/link properties are readable unprivileged | same | OD-10 check 5; same denial handling |
| A-4 | Workstation-scale documents keep JSON-over-IPC adequate at v1.0.0 | WO-02 §2.3 [ASSESS] | 07 §6 sizing assertions; revisit only with evidence |
| A-5 | 30 s poll / 60 s re-probe / 5 s grace defaults are reasonable starting values | engineering judgment (RD-08/10/14) | configurable (05 §2); Prompt 04 UX feedback may refine |
| A-6 | Target hosts expose sysfs/procfs and rtnetlink (Ubuntu 24.04 default kernels) | WO-02 ch. 4; verified baseline behavior [REPO] | Track 2 checkpoint; degradation paths exist regardless (02 §1) |

## 5. Future work (explicitly out of this package)

- WO-TRAY-01: status resolver, tray event bus, notifications, unread/
  acknowledgement lifecycle (incl. its persistence location — WO-02
  §8.1), tray menu/icons. Reserved constraints: 01 §9, 03 §11.
- Prompt 04: UI/UX specification (may refine freshness/poll defaults,
  presentation of conflicts and unresolved states).
- Prompt 05: discovery specification — provider internals, zbus
  adoption (UD-01), concrete NM/resolved section mappings.
- Prompt 06: traffic accounting and allowance subsystem (extension
  points reserved: 01 §10).
- Prompt 07: security/notifications/testing beyond backend architecture
  (this package's documents 06–07 cover the backend scope only).
- Packaging prompt: .deb pipeline, upgrade mechanics (UD-02/UD-03).
- Post-1.0.0 hardening: seccomp (06 §3 L5).

## 6. Traceability map (spec section → governing source)

| Spec section | Governing source |
|---|---|
| 01 §2 layers, only-shell-depends-on-Tauri | OD-01; WO-02 §2.2 |
| 01 §4 engine | RD-01 ← WO-02 §2.3 thin-command principle |
| 01 §8 runtime ownership | OD-01; WO-02 §2.3 runtime note [REPO — monitor.rs] |
| 01 §9 tray constraints | OD-07 (open); WO-02 ch. 8; work order §4 |
| 01 §10 accounting extension points | work order §3.4 (Prompt 06 deferral) |
| 02 §2–§10 provider contract | OD-02; E-1/E-2; work order §3.2 |
| 03 §1 evidence vocabulary | work order §3.5 |
| 03 §6.2 rules; §8 DNS; §12 VPN | work order §3.4 |
| 03 §7 unresolved (veth) | work order §3.4 (explicit no-inference rule) |
| 03 §9 conflicts | work order §3.2/§3.5 |
| 03 §11 status vocabulary reservation | work order §4; WO-02 ch. 8 |
| 04 entire | OD-03; work order §3.6–3.7 |
| 05 entire | OD-08; OD-09; work order §3.8 |
| 06 §2 prohibitions | work order §3.3; WO-02 Table 5.1 |
| 06 §4 D-Bus allow-list | work order §3.3; Q-1; R-03 |
| 07 §8 state comparison | work order §5; WO-02 Layer 3 |
| 07 §9 checkpoint | OD-10 |

## 7. Register status

- Approved decisions consumed: OD-01, OD-02, OD-03, OD-08, OD-09,
  OD-10 (six, all recorded in the accepted WO-02 closeout correction).
- Open decisions preserved as open: OD-04/05/06/07 (UD-01..04).
- New recommendations awaiting Emne's review: RD-01..RD-30 — **all
  unapproved**; their implementation dispositions are classified in
  §3.1 (Prompt 03-FC FC-04), which confers no approval.
- Assumptions: A-1..A-6 (each carries its validation path).
- Every design element in documents 01–07 traces to a row of this
  register; no untraced requirement exists in the package.
