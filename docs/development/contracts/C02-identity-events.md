# C02 — Identity, signed events and authorization

Version 1.4.0 · Accepted for implementation · Owners M03 (identity), M04 (events).

## Identity and credentials

`PersonId` derives from a root public key. Keep local accounts/usernames, devices, nodes, forums and courier credentials distinct. Root-authorized devices carry bounded purposes; local sessions authorize HTTP only. Node keys sign node actions/receipts; forums have separate signed governance. Offline client recovery protects the root and enables device replacement/rotation without cloud services.

Sessions expire on restart. Local login/SysOp powers grant no portable/forum authority.

Account linking requires a destination-bound root possession proof.

New credential/link profiles v2 use COSE Ed25519 (-19); v1 (-8) remains verification-only without rewriting bytes/IDs. Credentials bind root/device keys, fresh 16-byte incarnation and purpose bits 1/2/4/8 (authorship/forum governance/game ownership/identity proof), grant no resource permission themselves, and cap at 512 bytes. Root/device keys differ. Root/node runtime incarnations also change on startup/restore.

Authorization: `authorize(actor, action, resource, causal_context) -> allow | deny | missing_dependency`. Host suppression/peer policy are node-local, never membership grants. Unrecoverable root loss creates a new identity. Known compromised/revoked keys follow causal policy; R17 accepts disconnected stale state, never known-invalid transitions.

## Event envelope

Selected test implementation: deterministic CBOR, COSE_Sign1 with Ed25519, SHA-256 event IDs. Define exact byte fixtures before persisting a public version: `domain`, `protocol_major/minor`, `kind`, `schema_version`, `actor/device`, `stream_id`, `sequence`, `previous_hash`, `resource_id`, `dependencies`, `audience/distribution`, `claimed_time`, `body_hash`, `body_length`. The signed envelope commits to a detached plaintext/ciphertext body; its ID hashes the exact signed envelope. Bind routing/resource/version information into signed bytes. Domain separation prevents cross-protocol use. Human timestamps are display data, never permission or conflict authority.

Each stream binds actor role/subject/key, resource/audience/distribution and writer incarnation, with its own sequence/hash chain. Private/local actions never become required predecessors of unrelated public/shared actions. Cross-scope references expose only separately authorized credential/control proofs, never hidden content. Concurrent devices do not share counters; scope changes start new streams. Immutable events express creates, updates and tombstones; features own reducers/authorization. Typed event/credential/blob dependency gaps are staged. Same stream/sequence with different hashes is a fork: preserve evidence, quarantine affected advancement, let unaffected streams continue. Never transform signed bytes during upgrade; projections can be rebuilt.

## Integration result contract

`integrate(event, provenance) -> rejected(reason) | unsupported(version) | staged(missing) | duplicate(original_result) | accepted(relevance)`.

Acceptance atomically records canonical identity/authority, projection effect, receipt provenance and earned credit. Relevant means a new authorized message or meaningful module-defined state/control transition; obsolete no-op edits and merely stored dependencies do not qualify. Hosts validate encrypted outer admission/governance; clients validate/decrypt inner content. M13 must prove exact encrypted admission before encrypted deliveries earn credit.

Equal known valid events/policies yield equal replicated content/control projections regardless of arrival order. Receiver-local receipts/credit intentionally depend on arrival history. Conflicts are domain-specific: concurrent replies coexist; competing governance changes never select a wall-clock winner. Stable post IDs and immutable revision/event IDs support complaints.

Suppression may withhold a body while an authorized same-scope consumer receives the signed commitment needed to check later chain continuity. A commitment alone is neither content acceptance nor credit. Do not export it to an unauthorized audience; if a later action requires hidden semantic state, stage that action rather than infer permission. M04/M14/M15 test a suppressed predecessor followed by an otherwise independent valid post.

## Version boundary

Unknown required event semantics are retained as unsupported within quota and never applied/credited. Optional extension fields cannot silently alter authority. C03 negotiates supported versions; old peers keep working with compatible kinds. Normative schema, algorithm identifiers, limits, test vectors, and revocation/conflict rules must be finalized in M04/M13 ADRs. [CBOR](https://www.rfc-editor.org/rfc/rfc8949.html), [COSE](https://www.rfc-editor.org/rfc/rfc9052.html).
