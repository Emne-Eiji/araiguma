# 03 — Network data model and evidence semantics

Prompt 03 deliverable: the normalized, typed model of Araiguma's
observations, and the evidence/uncertainty vocabulary that governs every
representation of them (providers → merge → documents → CLI → IPC → UI).

Sources: the existing authoritative model [REPO — `crates/core/src/model.rs`,
schema v1–v3 history], OD-02/OD-03, WO-02 §5.4 (evidence provenance), the
work order §3.4–3.5 requirements, verified kernel-behavior findings
(WO-03 M2/M3, WO-03-F AUD-01..05/13/14).

Conventions carried from the existing model (binding, unchanged):

- `Option<T>` on a field means **not observed by any provider** — never a
  negative fact, with the two documented kernel-semantics exceptions
  (`Route::gateway` on-link absence; `NextHops::Multi` route-level
  fields) [REPO].
- Every item carries `sources` (observing providers, priority-first) and
  may carry verbatim `notes` [REPO].
- Unmapped raw kernel values are preserved (`Other(u8)` pattern), never
  collapsed [REPO].
- Synthetic/mock/fixture data must be labeled as such by its producer
  [REPO]; schema v4 makes the label a structured field (document 04).

## 1. Evidence vocabulary (work order §3.5, complete)

Ten evidence states, each with a fixed meaning. Every layer (provider
sections, normalized entities, documents, CLI output, IPC, UI) uses this
one vocabulary; no layer invents synonyms.

| State | Meaning | Negation? | Model representation |
|---|---|---|---|
| `observedPresent` | a successful observation returned this value | — | a field's `Some(value)`; a section's `Observed { count ≥ 1 }` |
| `observedAbsent` | a successful, sufficiently complete observation established the value's absence | **negative fact** | `MacObservation::Absent`; `Route::gateway = None` on an observed single-path route (on-link); `Observed { count: 0 }` for a section whose scope was successfully enumerated [REPO] |
| `notObserved` | no evidence either way — the field/section was not observed by anyone | no | `None`; `SectionOutcome::NotObserved`; per-entity absence from a document |
| `unsupported` | the provider cannot observe this in principle (this environment) | no | `SectionOutcome::Unsupported`; capability report `supportedSections` |
| `unavailable` | the provider could not run (service/binary/interface missing) | no | capability `available: Unavailable`; state `Unavailable` |
| `permissionDenied` | observation was attempted and access was denied | no | section outcome `permissionDenied` (new, document 02 §3); capability `permission: Denied` |
| `collectionFailed` | observation was attempted and failed (non-permission) | no | `SectionOutcome::Failed { error }`; state `Partial`/`Unavailable` [REPO] |
| `stale` | an observation exists but is older than the applicable freshness bound | no (an *assessment* of an existing observation) | derived: document/provider timestamps vs. freshness policy (§10) |
| `unresolved` | an observation references an identity that cannot be resolved within this document | no | relationship markers (§7); never silently dropped [REPO — annotation discipline] |
| `conflicting` | two or more providers assert different values for one fact | no | conflict records (§9, schema v4) |

### 1.1 Absence: the four-way distinction (binding)

The four states that are most easily conflated, and their invariants:

1. **Absence established by a successful, sufficiently complete
   observation** — `observedAbsent`. Requires (a) the observation
   succeeded, and (b) the source's scope provably covers the question
   (e.g. a full rtnetlink route dump covers "routes in table 254"; a
   missing `/proc/net/ipv6_route` file does **not** cover "IPv6 routes"
   and is a failure, never an observed zero [REPO — WO-03 M4 rule]).
2. **Absence of evidence** — `notObserved`. No claim in either
   direction.
3. **Failure to collect evidence** — `collectionFailed` /
   `permissionDenied` / `unavailable`. Never reinterpreted as 1 or 2.
4. **Provider cannot support the observation** — `unsupported`. The
   entity is simply not this provider's business in this environment.

**Unknown must remain unknown.** No layer coerces any of 2–4 (or stale,
unresolved, conflicting) into false, empty, absent, disconnected, or
normal. Provider health is never network health; attention lists state
evidence limitations, not network faults.

## 2. Interface identity and core fields

Identity (existing, binding [REPO — incl. WO-03-F AUD-01 scoping]):

- **Primary identity: `ifindex` (kernel-assigned, u32)** — stable within
  one observation and one network namespace.
- **Human identity: `name`** — may be absent only when a provider saw the
  index without a name (never invented; interface rows require ifindex,
  name is required-by-netlink, optional in principle).
- **Namespace scoping**: ifindex and name are namespace-scoped.
  `link_netnsid` (when present) marks that the interface's link target —
  veth peer or vlan/tunnel parent — lives in **another** namespace; the
  referenced ifindex can then never be resolved locally, regardless of
  index collisions [REPO].

Fields (existing [REPO], summarized): `kind` (classified) +
`kind_detail` (raw kernel string; unrecognized kinds stay `Other` with
the raw value — never confidently misclassified [REPO — WO-02 correction
§9.2]); `oper_state` (RFC 2863; `unknown` stays unknown — never
"down"); `admin_state`; `mac_address` — the three-state
`MacObservation` (`present` / `absent` / `notObserved`; v1 `null`
rejected [REPO — WO-02 correction §5]); `mtu`; `tx_queue_len`;
relationship ifindexes (§4); `sources`; `notes`.

Behavior when information is missing/conflicting/stale/unsupported:
missing → `notObserved` fields, no fabrication; conflicting → conflict
records (§9); stale → whole-entity freshness via the document's
timestamps (§10); unsupported → entity simply absent from that provider's
sections, explained by the capability report.

## 3. Interface type and operational state

- `InterfaceKind` (existing set [REPO]): loopback, ethernet, wireless,
  bridge, bond, dummy, wireguard, tunnel, veth, vlan, virtual, other.
  Classification precedence: netlink `IFLA_INFO_KIND` string wins; ARPHRD
  fallback only for physical devices; sysfs-only classification carries
  its ambiguity as a fidelity note (Degraded) [REPO].
- Oper/admin states are independent facts (admin-down wins presentation;
  `unknown` operstate is never rendered as operational) [REPO].
- No state is ever inferred from another (an interface with addresses is
  not "up" unless observed up).

## 4. Interface relationships (existing discipline, formalized)

| Relationship | Evidence required | Representation |
|---|---|---|
| master/port (bridge, bond, unestablished master) | kernel master statement (IFLA_MASTER / sysfs `master`) + the master's own kind observation for the flavor | `master_ifindex` + topology edge `bridgeMember`/`bondMember`/neutral `masterMember` [REPO — WO-02 correction §6] |
| parent link (vlan parent, tunnel underlay) | kernel parent statement (IFLA_LINK), never for veth (their IFLA_LINK is the peer [REPO]) | `parent_ifindex` + `linkParent` edge |
| veth peer | kernel-classified veth + named peer ifindex | `peer_ifindex` + `vethPeer` edge; cross-namespace peers stay unresolved (§7) [REPO — WO-03 M2, AUD-01] |

Invariants (binding): a relationship edge exists **only** on kernel
evidence; a master/parent/peer ifindex naming a missing interface (or one
in another namespace) produces an `unresolved` marker/annotation on the
referencing entity — never a dangling edge, never a guessed resolution
[REPO]. Self-referential indexes are invalid references, never drawn
[REPO].

## 5. IP addresses

Existing model [REPO], binding: identity = (ifindex, family, address,
prefix); fields scope/flags/label with observed-absence vs not-observed
discipline (`Some(vec![])` = observed no flags [REPO]); family
consistency validated at the document boundary [REPO]. Addresses attach
to interfaces by ifindex within this observation; an address on an
unobserved interface is retained (its own observation) and flagged
`unresolved` for presentation (the interface row is absent — the address
is not deleted; the UI shows the reference honestly). No subnet, "LAN",
or allocation facts are derived beyond `IpNet::contains` for the
*documented, recorded* `gatewayInferred` topology inference [REPO].

## 6. Routes, routing tables, policy routing

### 6.1 Existing (binding, [REPO])

Route identity is complete and evidence-based (WO-03-F AUD-02): family,
destination, table, metric, protocol, scope, type, TOS, flags AND the
complete next-hop representation (gateway, `RTA_VIA` with its own family,
oif, per-hop weight/flags; multipath as full hop lists — never flattened
[WO-03 M3]; undecodable multipath preserved as `Undecoded(reason)`
[REPO]). Tables: all tables observed; u32 ids; procfs table-less rows
carry explicit ambiguity notes, never invented tables [REPO — AUD-03/
05]. Route ordering for display is deterministic [REPO].

A default route is evidence of a **configured next hop, not of Internet
connectivity**; no connectivity claims are derived anywhere (work order
§3.4).

### 6.2 New: routing-policy rules (target, schema v4)

Where observable (rtnetlink `RTM_GETRULE` dump — read-only; availability
probed per document 02):

```
RoutingRule {
  family: AddressFamily,
  priority: Option<u32>,        // rule preference; None = not observed
  action: Option<RuleAction>,   // to-table(n) / blackhole / unreachable / prohibit; None = not observed
  selector: RuleSelector,       // observed fragments only:
                                //   from/to prefixes, tos, fwmark/mask, iif/oif (name or ifindex), uidrange
  table: Option<u32>,           // for to-table actions
  sources, notes,
}
```

Semantics: every selector fragment is independently optional
(`notObserved` when the kernel attribute is absent); rules are listed,
never interpreted into reachability claims; unsupported (no rule dump
available — e.g. procfs-only environments) is a section-level
`unsupported`, and rule absence is asserted only from a successful
complete dump. Policy-routing *context* for routes (which rule would
select which table) is **not** computed — it would be an inference beyond
the evidence; the raw co-observation of rules and routes is the honest
representation.

### 6.3 Gateways and next hops (view semantics, existing)

"Gateway" is not a stored entity: gateway nodes/edges are a pure
projection of route next-hop facts (observed `gatewayRoute`, or
`gatewayInferred` with the recorded subnet-containment reason — the only
inference, always labeled [REPO]). Gateway nodes are annotated
"reachability not tested" and never receive "up" states [REPO].

## 7. Unresolved relationships (binding rules)

An observation may reference an identity this document cannot resolve.
Unresolved is a **first-class state**, never silently repaired:

- **veth peers**: a peer ifindex that does not resolve to an interface
  in this document (including every cross-namespace peer — netnsid
  present [REPO]) stays `unresolved`. **Never** infer the peer from
  naming conventions, matching MACs, complementary indexes, or any other
  weak evidence (work order §3.4).
- **missing masters/parents**: marker/annotation on the member, no edge
  [REPO].
- **addresses/routes referencing unobserved interfaces**: retained with
  an `unresolved` presentation marker (the observation stands; the
  relationship cannot be drawn).
- The `unresolved` marker names what was referenced and why it cannot
  resolve (e.g. `peer ifindex 7 not in this observation; linkNetnsid
  present — peer lives in another network namespace`).

## 8. DNS and resolver observations (new, schema v4; target sections)

Modeled as **configuration observations only**. DNS configuration is
never proof of successful name resolution, and no resolution probing
exists in the observation model (a name-resolution query would generate
network traffic — an active operation outside the read-only passive
observation posture; excluded from v1.0.0 providers, work order §3.4).

```
DnsObservation {
  scope: DnsScope,              // global | link { ifindex } | processHint { source } (see below)
  mode: Option<DnsMode>,        // stub | uplink | direct | none | notObserved  (as the source exposes it)
  servers: [DnsServer],         // observed, ordered; DnsServer { address, port?: notObserved-able, kind?: doT/doH/plain/unknown }
  searchDomains: [String],      // observed list (may be empty = observed empty)
  routingDomains: Option<Vec<String>>,  // systemd-resolved split-DNS domains; None = not exposed by source
  sources, notes,
}
```

- Sources: systemd-resolved (per-link + global state), and — as a
  fallback-class observation — statically read resolver configuration
  files (`/etc/resolv.conf`, `systemd-resolved` stub detection) where
  reading them is permitted; each source's fidelity limits are fidelity
  notes (e.g. "resolv.conf shows the stub, not the upstream servers").
- `scope: processHint` exists to represent *file-derived* observations
  without inventing a process model; it is labeled with its derivation
  and never presented as the system's effective configuration when a
  resolved provider is available (precedence: resolved wins field
  conflicts; the file observation is retained).
- No entity may claim "DNS works" or "DNS is broken"; the model holds
  configuration and its evidence, nothing else.

## 9. Merge, precedence, and conflicting evidence

The merge is the existing priority-ordered, gap-filling, identity-based
dedup [REPO], extended by **conflict recording** (schema v4):

- When two providers assert different non-null values for the same field
  of an identity-matched item, the merged item takes the higher-priority
  value (unchanged rule) **and** records a conflict entry:
  `Conflict { field, assertions: [ { provider, value } ], resolution: "priority" }`.
- Existence conflicts (one provider sees an entity, another's successful
  full-scope observation does not) are **not** resolvable by precedence
  and are recorded as conflicts on the item with both assertions (an
  honest "the sources disagree about reality").
- Conflicts are never dropped, aggregated away, or turned into errors;
  they surface in documents, and the UI renders them as evidence
  limitations. Precedence resolves *presentation*, never *evidence*.
- Provenance is never overwritten: `sources` unites; a conflict entry
  preserves each asserting provider's value verbatim.

## 10. Observation time, provenance, and staleness

- Every document carries observation timing (start/completion, per-
  provider completion times — document 04 §5) and per-item provenance
  (`sources`) [existing discipline extended].
- **Staleness is derived, never stored as a fact**: a consumer computes
  `now − observationTime` against an explicit freshness policy (per
  section/provider, from configuration; defaults in document 05 §3).
  A stale observation remains an observation — labeled with its age, it
  is never rewritten, hidden, or counted as absent.
- The only freshness *signal* in the stream is discontinuity: a
  `resynchronized` envelope resets freshness assumptions (changes during
  the gap were not observed) [REPO]. Consumers must not interpret the
  post-resync snapshot as "nothing changed meanwhile" (R-10).

## 11. Provider health, coverage, and the reserved status vocabulary

Provider health and coverage per document are the contract of document
02 §8 (capability report + per-run state + health metadata). From them,
documents and views can always answer: *what could have been observed,
what was attempted, what succeeded, what failed and why, and how fresh is
each part* — the complete evidence picture behind every `notObserved`.

**Reserved for WO-TRAY-01 (vocabulary only, no implementation):** a
status vocabulary over evidence — status values { `critical`, `unread`,
`newConnection`, `attention`, `normal`, `unknown` } with a fixed priority
order `critical > unread > newConnection > attention > normal` and
**`unknown` absorbing** (any unresolved input yields `unknown`; `unknown`
is never coerced to `normal`). Any future evidence-to-status mapping must
be a **total, non-lossy function**: every evidence state of §1 maps to
exactly one status, and an evidence gap maps to `unknown`. This reserves
the semantic contract; the resolver, its inputs' weighting, and all tray
behavior remain unauthorized (OD-07 open; document 01 §9).

## 12. VPN-related interfaces and provider evidence

- VPN **interface** evidence: interface kind (wireguard, tunnel/tun)
  [REPO] — an interface classification, nothing more.
- VPN **service** evidence (schema v4, target): NetworkManager active-
  connection type (`vpn`) and its vpn-type where exposed, attached to
  the observed device/connection mapping — an observation with its
  source, not a topology edge.
- Binding invariants: no VPN peer node, no "VPN tunnel" edge, no
  underlay inference for WireGuard (unprivileged observation cannot
  establish it [REPO — WO-02 §2.2]); VPN presence is presented as
  evidence on the interface/connection ("kind: wireguard (observed)";
  "NM active connection of type vpn on eth0 (observed)"), never as a
  relationship to an unknown remote endpoint.

## 13. Entity summary table (identity, provenance, evidence)

| Entity | Identity | Provenance | Key evidence states |
|---|---|---|---|
| Interface | ifindex (+namespace scoping) | sources | present; mac observed-absent; kind-other; unresolved relationships |
| Address | (ifindex, family, address, prefix) | sources | present; flags observed-empty; unresolved interface |
| Route | complete identity (§6.1) | sources | present; gateway observed-absent (on-link); multipath full/undecoded; table-less ambiguity notes |
| RoutingRule | (family, priority, action, selector fragments) (new) | sources | present; fragment-wise notObserved; section unsupported |
| DnsObservation | (scope, source) (new) | sources | present; mode/servers partial; fidelity notes |
| ProviderReport / health / capability | provider id per document | self | six states + permission/unavailable axes |
| Conflict entries (new) | (item identity, field) | both asserting providers | conflicting |
| Topology nodes/edges | projection keys [REPO] | derived from item sources | observed/inferred evidence classes; never fabricated |

## 14. Data-model decision summary

| Decision | Source | Status |
|---|---|---|
| Keep v3 entities and semantics verbatim; extend additively (v4) | existing verified model [REPO]; schema policy [REPO] | approved-derived |
| Ten-state evidence vocabulary; four-way absence distinction | work order §3.5 | approved-derived |
| Conflict records (existence + field) with priority presentation, evidence preserved | work order §3.2/§3.5 | RD-12 |
| RoutingRule entity, no policy-routing inference | work order §3.4 (policy-routing "where observable") | RD-13 |
| DnsObservation: configuration-only, no resolution probing | work order §3.4 (no DNS-works claims); read-only posture | approved-derived |
| Unresolved as first-class state; veth peers never inferred | work order §3.4 (explicit) | approved-derived (binding) |
| Staleness derived from timestamps + policy, never stored/fabricated | work order §3.5 | approved-derived |
| Status vocabulary reserved (tray) with absorbing unknown | work order §4; WO-02 ch. 8 | approved-derived (scope) |
