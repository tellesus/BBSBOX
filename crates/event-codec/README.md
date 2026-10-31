# M04 A — signed event codec and authority gates

Implementation reference for [M04](../../docs/development/modules/M04-signed-events.md), [C01](../../docs/development/contracts/C01-storage-runtime.md) and [C02](../../docs/development/contracts/C02-identity-events.md). Product authority remains the roadmap and user instructions; [ADR-0002](../../docs/decisions/ADR-0002-wire-profiles.md) records rationale.

## Event v1 bytes

An event is an untagged deterministic COSE_Sign1 array with protected map `{1:-19,4:signing_public_bstr32}`, empty unprotected map, metadata payload and 64-byte Ed25519 signature. External AAD is UTF-8 `bbsbox.event/v1`. The body is detached. Event ID is SHA-256 of the exact COSE bytes (`event:<64 lowercase hex>`). Rebuilding the metadata and complete envelope rejects nonminimal CBOR, extra/unprotected headers and trailing bytes. The decoder bounds input before parsing and caps nested metadata depth at eight.

Metadata is a 17-item array in this order:

| Index | Field |
| --- | --- |
| 0–2 | Domain `bbsbox.event/v1`, protocol major 1, minor 0 |
| 3–4 | Kind token (1–64 ASCII identifier characters), schema version (nonzero u16) |
| 5 | Actor `[role,subject_digest_bstr32,signing_public_bstr32,credential_digest_bstr32_or_null,incarnation_bstr16]` |
| 6 | Stream digest bstr32 |
| 7–8 | Sequence (nonzero u64), previous event digest bstr32 or null; null iff sequence 1 |
| 9–11 | Resource token, audience token (each 1–256 ASCII identifier characters), distribution (0 local, 1 selected shared) |
| 12 | Sorted unique dependency pairs `[type,digest_bstr32]`; type 1 event, 2 credential, 3 blob |
| 13 | Claimed human time, signed i64; display data only |
| 14–15 | SHA-256 body digest bstr32, body length |
| 16 | Sorted unique extensions `[nonzero_u16_id,required_bool,data_bstr]` |

Tokens accept ASCII alphanumeric plus `_ - . : /` and remain case-sensitive. Role codes: 1 person device, 2 node, 3 forum root, 4 person root. Root roles derive their subject digest from the appropriate role-specific key domain. Person devices require a verified root delegation matching credential ID, subject, signing key and incarnation. Credential versions 1/2 verify; new issuance is v2. The explicit Ed25519 algorithm identifier (-19) follows [RFC 9864](https://www.rfc-editor.org/rfc/rfc9864.html).

Stream digest is SHA-256 of canonical CBOR `["bbsbox.stream/v1",role,subject_digest,signing_public,incarnation,resource,audience,distribution]`. Changing identity/scope/distribution/incarnation starts another stream; separate devices never share counters. History resolution and cross-scope dependency permission are B work. A typed blob dependency does not grant access or authorize file-library export; feature-owned scope/retention policy still applies.

Limits: 4,096-byte envelope, 65,536-byte body, 32 dependencies, 16 extensions, 1,024-byte extension data (all still within envelope budget). Invalid/oversized producer input fails before invoking the key holder. Unknown domain/major/role/dependency type and unsupported extended structures have explicit errors. Within the known major/layout, an unknown minor, kind/schema or required extension stays unsupported at the authority gate. Durable quota/staging behavior belongs to B.

## API boundaries

`sign` derives actor/stream/body commitments, calls the supplied client/node key holder, and verifies the resulting signature. The crate's production dependencies contain only public protocol/crypto code; identity/client libraries are test dependencies. Normal signing identity and incarnation come from their owning runtime. Root control contexts are fresh on root creation/recovery; node contexts are fresh on startup. M04 B will own durable stream positions and previous-event checks.

`verify` returns `Commitment`, proving the exact signed envelope and delegated identity. It has no body and cannot be passed to `Registry::authorize`. `with_body` checks actual size/hash and returns `CompleteEvent`. A commitment alone supplies no content acceptance, semantic permission or credit. It supports the later suppression/continuity implementation without inventing hidden state. `verify_with_resolver` distinguishes an unavailable credential lookup from an actually missing credential.

For staging, `verify_signature` returns `SignedEnvelope` with explicitly unbound actor claims. It can check actual body bytes while the delegation is missing, so corrupt bytes cannot take the first valid body slot. `bind` verifies that delegation/subject before yielding `Commitment`; the signature-only value cannot enter the authority gate.

The compiled `Registry` selects kind/schema/actor rules and required device purpose. Signature validity never skips those checks. The feature's `Authority` then validates current causal/resource policy; it may deny or report typed missing dependencies. Required extensions need known IDs and explicit validation; a missing handler denies, and a recognized authority-bearing extension cannot be relabeled optional. Unknown optional annotations cannot grant authority. `Authorized` means permission to run the reducer, not committed integration/relevance/credit. B/C and each feature own that work.

## Evidence and next slice

`cargo test --locked --offline -p bbsbox-event-codec` covers role/purpose/resource gates, missing proofs, namespaces, private/public scope separation, optional/required extensions, version handling, tampering/noncanonical input, size/sequence bounds, false CBOR lengths/depth, detached commitments and a fixed vector using u64::MAX/i64::MIN. `cargo run --locked --offline -p bbsbox-event-codec --example event_vector` reproduces [the public vector](../../protocol/fixtures/event-v1.json); its predecessor/dependency IDs are conformance data, not a claimed accepted history.

`node tools/verify_event_vectors.mjs` independently rebuilds payload/COSE/stream hashes with cbor-x/Node crypto, preserves 64-bit values, reproduces Ed25519 and rejects resource alteration. It uses M00's prepared web dependencies and Node 24. B next adds raw persistence, durable staging/deduplication, fork evidence/isolation, provenance and atomic reducer integration; C proves replay/convergence with real domain fixtures. No HTTP, federation or encrypted-forum integration is claimed by A.
