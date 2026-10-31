# M04 — durable event inbox

Implementation reference for [M04](../../docs/development/modules/M04-signed-events.md), [C01](../../docs/development/contracts/C01-storage-runtime.md) and [C02](../../docs/development/contracts/C02-identity-events.md). The roadmap and current user instructions remain canon. A/B have local/Linux evidence; C's local domain replay/convergence proof awaits Linux confirmation.

## Retained bytes and provenance

Register `migrations()` alongside the existing storage/identity migrations. `Inbox::receive` runs inside the caller's single-writer transaction; propagate errors so quota, raw bytes, body provenance and authenticated links roll back together. Use the storage content floor with an appropriate growth estimate (the tests reserve 128 KiB per maximum event). No HTTP or peer route is added here.

The inbox checks the envelope signature and actual body commitment before retaining a current-profile body, even when its device credential is missing. The first valid full body gets its own receiver-supplied source/transfer record. A previous header-only carrier, malformed body or repeated later carrier cannot replace it. This records input provenance, **not** an acceptance receipt or Lump Drop. Semantic integration below owns acceptance and the future credit-consumer hook.

Unknown structural profiles are retained as opaque, unsupported envelope bytes within quota; their uncheckable bodies are discarded. They produce no authenticated stream positions. Invalid signatures in a known profile are rejected. Unknown kinds/extensions within the known byte profile must still pass the codec's registry and module policy before any future integration. Retention never grants semantic authorization.

The pending intake budget defaults to 64 MiB across retained envelopes and verified bodies, with a transactionally maintained byte count. Readiness alone does not release it. Semantic acceptance atomically releases an event's pending charge exactly once; accepted history remains stored and subject to C01's disk floors. Duplicate bytes do not charge twice. SQLite metadata/WAL use and actual disk failures remain subject to separate free-space admission and rollback. There is no automatic deletion of fork evidence or history.

## Continuity and forks

Only an actor bound to a valid public delegation/root identity enters `events_links`. Positions are eight-byte big-endian BLOBs, preserving the full unsigned u64 range and index order. A predecessor must belong to the same stream at exactly sequence minus one. Cross-resource/audience/distribution/incarnation chains cannot connect. A commitment without its body can establish continuity, while staying `CommitmentOnly`; the following independent event can become `Ready`. Features must stage any later action that needs the hidden semantic state.

Two authenticated hashes at one stream position preserve both raw envelopes and establish a persistent minimum fork position. `readiness` immediately reports `Forked` for either branch and all later positions, including events previously ready. Earlier positions and other streams continue. A missing/mismatched delegation cannot poison another authenticated stream. `Ready` only means bytes, actor binding and chain continuity are available. It says nothing about kind support, membership, causal authorization, content relevance or delivery credit.

`reconcile` processes at most 100 unresolved rows per transaction with an explicit ID cursor; missing proofs cannot trap the scan on its first row. Repeat complete scans after new dependencies arrive until no readiness transitions remain. Already bound/connected rows do not require another resolver call. Callers schedule this work on C01's background queue; no autonomous polling loop is introduced. A missing immediate predecessor may itself be waiting on an earlier gap.

`inventory` pages authenticated `(sequence,event_id)` positions within an exact supplied scope/stream and includes both fork hashes. This backend API does not authorize export. M15/M16 must apply current audience/local/shared policy before accessing or sending it, commitments or bodies. The public-protocol lookup helpers likewise supply no UI, private-content or transport permission.

## Evidence and remaining work

Thirty-three integration tests cover reopen/deduplication, first-body provenance, corrupt-body rejection, proof arrival, reversed histories, suppressed-body continuity, scope/sequence violations, late forks, invalid-delegation isolation, opaque future profiles, quota/lookup rollback, bounded scans, full-width positions and range pagination. They also cover the semantic/authoring/replay APIs with the real M02 writer/database and M03 public resolver/node signer. M04 A's independent signature vectors remain separate.

Local replay evidence is described below. Linux confirmation is pending before provider completion; no HTTP/private-forum/federation integration is claimed.

## Atomic semantic integration

`integration::integrate` checks readiness, the codec registry's role/purpose/version/extension gates and the compiled module's current causal/resource policy. Explicit event dependencies need accepted, effective domain state; credential dependencies need the matching verified public credential; blob dependencies require the module's existence/visibility check. Same-scope commitments can supply chain continuity, but cannot supply absent semantic state. Cross-scope content dependencies fail by default. A module can explicitly admit only separately authorized control proofs, checked from their signed commitment before any body access.

Modules own SQL reducers, relevance, effective dependency state and fork invalidation. A successful reduction, immutable acceptance receipt, first-body provenance, optional relevant-acceptance hook and pending-budget release commit in the caller's one transaction. Reducer/hook failures roll everything back. Accepted receipts return their exact original result on retries, even after restart or later fork invalidation; effects and relevance hooks never run twice. Obsolete/dependency-only reductions produce receipts without calling the relevance hook. This hook is infrastructure for M16; these fixtures do not implement real Lump Drop counting or encrypted admission.

Policy/extension/dependency lookup failures propagate as operational errors and roll back; they are never converted to a missing dependency or a successful denial. The pure codec's authority gate is bridged to the module's fallible database policy without losing that distinction.

Rejected/unsupported/staged assessments remain retryable after dependency, policy or compiled-module changes; their retained raw bytes provide durable staging. `pending` pages unaccepted candidates with a maximum of 100, so missing/unsupported semantics do not trap a scan at its first row. The caller persists/schedules background work through C01 and repeats scans when relevant dependencies change. No timestamp selects authority or conflict winners.

An active projection database must use `integration::receive`/`reconcile`, supplying **all compiled affected modules, including disabled ones**, rather than bypassing them through the raw inbox. A discovered fork and each module's invalidation then commit together. Each adapter must hide/rebuild affected prior effects and semantic dependents; the test adapter invalidates a recursive dependency closure in SQL. Historical acceptance receipts remain unchanged. Failure in invalidation rolls back the new intake/fork as well.

Fork synchronization handles at most 100 new/earlier stream floors per call. When initializing a module over existing retained forks, run bounded `synchronize_forks` transactions until complete; `ensure_projection_current` guards ordinary projection reads during that initialization. Active intake rejects an unrepaired backlog and can be retried after that work. Module callback costs must remain bounded or immediately fence affected views and schedule their own rebuild. This provider does not make arbitrary feature reducers converge automatically; C and each feature must prove their causal/conflict rules.

## Local authorship

`authoring::author` takes an action without a caller-selected counter/previous hash. Within the single-writer transaction it derives the scoped stream, refuses known forks/gaps/counter exhaustion, selects the next full-width position, invokes the owning key holder and persists the verified result. Apply a local action's reducer in the same C01 idempotent command transaction and publish its bytes after commit. A failed reducer consumes no position; a retried command returns the original signed event. Twenty queued authoring operations prove contiguous, non-reused positions.

Actor identity/incarnation comes from the current key holder, including M03's node signer and independently held member keys. Fresh node startup uses a fresh stream; an old incarnation passed to the new signer fails before persistence. This is the counter/signing integration, not M12's complete backup activation or M00/M13's trusted-client distribution. Migration 2 adds receipts/fork synchronization and the pending-charge flag without changing migration 1 or existing signed records; its upgrade test retains the original first-body source.

## Fresh-generation replay

`ReplayModule::project` rebuilds a feature-owned projection generation. `integration::accepted` pages that module's immutable accepted IDs; `replay` rechecks bytes, current causal/resource policy, kind support and effective dependencies before projecting one event. It never calls the first-acceptance reducer/relevance hook, rewrites receipts or changes quota. A feature repeats bounded causal passes, preserving unsupported/missing outcomes. Each projection's policy/dependency reads and writes must use its same fresh generation.

Migration 3 adds an indexed accepted-ID scan and a monotonic event revision. New raw/body input, identity/chain binding and first acceptance advance it transactionally; exact duplicates and projection replay do not. Capture `revision` before rebuilding and call `require_revision` inside the generation-publication transaction. Features must check their own policy/schema revision there too. New input or changed policy cancels stale publication while the existing view remains intact; the feature retries/catches up. M06 and later consumers own their actual views, job scheduling, checkpoints and generation cleanup.

The [revision fixture](tests/support/replay.rs) uses real SQL generations and immutable create/edit/tombstone/reply facts. All 120 orders of one five-event history produce equal heads despite inverted/extreme human timestamps. Concurrent replies coexist; separate devices' competing author edits remain visible conflicts rather than selecting a clock winner. An edit delivered before its original stages and later becomes current; replaying the older original cannot roll it back. Equal forked sets converge whether a branch was accepted early or neither was; historical receipts may differ as C02 permits. Known membership withdrawal rebuilds an empty view without rewriting those receipts. Another person's edit is denied.

Compatibility fixtures retain a required unsupported extension uncredited, activate its explicitly registered validator later without rewriting signed bytes, and preserve an unknown minor version as unsupported through restart. Rebuilds preserve receipt/hook counts. These are M04 domain-conformance fixtures, not M06's final post schema or M13's membership/governance protocol; each consumer must prove its own causal rules and bounded work.
