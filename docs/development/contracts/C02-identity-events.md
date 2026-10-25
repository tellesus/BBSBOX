# C02 — Identity, signed events and authorization

Version 0.1.0 · Proposed · Owners M03 (identity), M04 (events).

## Identity and credentials

Keep `PersonId`, local account ID/username, device signing key, `NodeId`, `ForumId`, and courier credential distinct. Proposed stable person identity starts from a root public key; root-authorized device certificates carry bounded purposes. Local account sessions authorize HTTP access but are not portable identity proof. Node credentials sign node actions/receipts, not arbitrary user authorship. Forum governance is a separate signed authority chain. An offline recovery kit protects the root; rotating/replacing devices must not require a cloud service.

Proposed authorization interface: `authorize(actor, action, resource, causal_context) -> allow | deny | missing_dependency`. Local host suppression and peer policy are node-local records, never forum membership grants. Root loss without a previously authorized recovery method yields a new identity, not invented continuity. A known compromised/revoked key follows the recorded causal policy; R17 permits disconnected stale state, not ignoring known invalid transitions.

## Event envelope

Preferred prototype: deterministic CBOR, COSE_Sign1 with Ed25519, SHA-256 event IDs. Define exact byte fixtures before persisting a public version: `domain`, `protocol_major/minor`, `kind`, `schema_version`, `actor/device`, `stream_id`, `sequence`, `previous_hash`, `resource_id`, `dependencies`, `audience/distribution`, `claimed_time`, `payload_or_ciphertext`. Bind routing/resource/version information into signed bytes. Domain separation prevents cross-protocol use. Human timestamps are display data, never permission or conflict authority.

Every device has its own sequence/hash chain; concurrent devices do not share a counter. Immutable events express creates, updates and tombstones; each feature owns its reducer and authorization checks. Dependency gaps are staged. Same stream/sequence with different hashes is a fork: preserve evidence, quarantine affected advancement, let unaffected streams continue. Signed bytes cannot be transformed during upgrade; derived projections can be rebuilt.

## Integration result contract

`integrate(event, provenance) -> rejected(reason) | unsupported(version) | staged(missing) | duplicate(original_result) | accepted(relevance)`.

Acceptance atomically records canonical identity, authority result, projection effect, receipt provenance and any earned credit. Relevance means a newly admitted authorized message or meaningful state/control transition under the module's reducer. An obsolete no-op edit or mere stored dependency is not automatically relevant. For encrypted content the host checks the authenticated outer admission/governance contract, not plaintext; clients separately validate/decrypt inner content. Exact encrypted admission proof is an M13 gate before encrypted deliveries earn credit.

For equal known valid events and local policies, reducers must converge independent of arrival order. Specify conflicts per domain: concurrent discussion replies coexist; unresolved competing governance changes do not silently pick a wall-clock winner. Stable post ID plus immutable revision/event IDs support complaints.

## Version boundary

Unknown required event semantics are retained as unsupported within quota and never applied/credited. Optional extension fields cannot silently alter authority. C03 negotiates supported versions; old peers keep working with compatible kinds. Normative schema, algorithm identifiers, limits, test vectors, and revocation/conflict rules must be finalized in M04/M13 ADRs. [CBOR](https://www.rfc-editor.org/rfc/rfc8949.html), [COSE](https://www.rfc-editor.org/rfc/rfc9052.html).
