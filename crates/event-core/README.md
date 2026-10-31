# M04 — durable event inbox

Implementation reference for [M04](../../docs/development/modules/M04-signed-events.md), [C01](../../docs/development/contracts/C01-storage-runtime.md) and [C02](../../docs/development/contracts/C02-identity-events.md). The roadmap and current user instructions remain canon. This is the first part of B; semantic integration and C's replay evidence are still pending.

## Retained bytes and provenance

Register `migrations()` alongside the existing storage/identity migrations. `Inbox::receive` runs inside the caller's single-writer transaction; propagate errors so quota, raw bytes, body provenance and authenticated links roll back together. Use the storage content floor with an appropriate growth estimate (the tests reserve 128 KiB per maximum event). No HTTP or peer route is added here.

The inbox checks the envelope signature and actual body commitment before retaining a current-profile body, even when its device credential is missing. The first valid full body gets its own receiver-supplied source/transfer record. A previous header-only carrier, malformed body or repeated later carrier cannot replace it. This records input provenance, **not** an acceptance receipt or Lump Drop. Module-defined relevance and atomic integration/credit hooks remain B work.

Unknown structural profiles are retained as opaque, unsupported envelope bytes within quota; their uncheckable bodies are discarded. They produce no authenticated stream positions. Invalid signatures in a known profile are rejected. Unknown kinds/extensions within the known byte profile must still pass the codec's registry and module policy before any future integration. Retention never grants semantic authorization.

The current intake budget defaults to 64 MiB across all retained envelopes and verified bodies, with a transactionally maintained byte count. It deliberately includes ready intake until the later semantic retention implementation explicitly handles that transition; the inbox is not an unlimited accepted-history store. Duplicate bytes do not charge twice. SQLite metadata/WAL use and actual disk failures remain subject to C01's separate free-space admission and rollback. There is no automatic deletion of fork evidence or history.

## Continuity and forks

Only an actor bound to a valid public delegation/root identity enters `events_links`. Positions are eight-byte big-endian BLOBs, preserving the full unsigned u64 range and index order. A predecessor must belong to the same stream at exactly sequence minus one. Cross-resource/audience/distribution/incarnation chains cannot connect. A commitment without its body can establish continuity, while staying `CommitmentOnly`; the following independent event can become `Ready`. Features must stage any later action that needs the hidden semantic state.

Two authenticated hashes at one stream position preserve both raw envelopes and establish a persistent minimum fork position. `readiness` immediately reports `Forked` for either branch and all later positions, including events previously ready. Earlier positions and other streams continue. A missing/mismatched delegation cannot poison another authenticated stream. `Ready` only means bytes, actor binding and chain continuity are available. It says nothing about kind support, membership, causal authorization, content relevance or delivery credit.

`reconcile` processes at most 100 unresolved rows per transaction with an explicit ID cursor; missing proofs cannot trap the scan on its first row. Repeat complete scans after new dependencies arrive until no readiness transitions remain. Already bound/connected rows do not require another resolver call. Callers schedule this work on C01's background queue; no autonomous polling loop is introduced. A missing immediate predecessor may itself be waiting on an earlier gap.

`inventory` pages authenticated `(sequence,event_id)` positions within an exact supplied scope/stream and includes both fork hashes. This backend API does not authorize export. M15/M16 must apply current audience/local/shared policy before accessing or sending it, commitments or bodies. The public-protocol lookup helpers likewise supply no UI, private-content or transport permission.

## Evidence and remaining work

Twelve integration tests cover reopen/deduplication, first-body provenance, corrupt-body rejection, proof arrival, reversed histories, suppressed-body continuity, scope/sequence violations, late forks, invalid-delegation isolation, opaque future profiles, quota/lookup rollback, bounded scans, full-width positions and range pagination. They use the real M02 database and M03 credential resolver. M04 A's independent signature vectors remain separate.

Next in B: typed semantic dependency staging, atomic reducer/result/provenance integration, duplicate original results and projection invalidation/rebuild after late forks. C then proves arrival-order convergence, immutable edits/tombstones and unsupported semantic negotiation using real reducer fixtures. The inbox alone does not complete AC07 or the M04 provider.
