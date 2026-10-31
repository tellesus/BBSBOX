# M03 B — public credentials

Implementation reference for [M03](../../docs/development/modules/M03-identity-access.md) and [C02](../../docs/development/contracts/C02-identity-events.md). The roadmap and user instructions remain product authority. This page records the implementation profile and conformance recipe.

## Public definitions

Person/device/node/forum/courier IDs are distinct Rust types. Each is SHA-256 of its ASCII domain (respectively `bbsbox.person/v1`, `bbsbox.device/v1`, `bbsbox.node/v1`, `bbsbox.forum/v1`, `bbsbox.courier/v1`), one NUL byte, and its 32-byte Ed25519 public key. Text form is `person|device|node|forum|courier:<64 lowercase hex digits>`. Invalid/weak public keys fail construction. An identifier/fingerprint proves no role or key possession by itself. Local account IDs/usernames are separate, and no username participates in portable ID derivation.

`ContactCard` holds only a validated root public key. QR text is exactly `bbsbox-person-v1:<64 lowercase hex public-key digits>`; the person fingerprint is derived as above. Comparing an expected fingerprint detects substitution. Scanning/storing a card does not establish membership or link an account. It includes no root secret or recovery material.

## Current device credential v2 byte profile

The credential is an untagged deterministic COSE_Sign1 four-item array. Its protected map is `{1:-19, 4:person_digest_bytes}` (explicit Ed25519 and 32-byte key ID), unprotected map empty, embedded payload and 64-byte Ed25519 signature. External AAD is UTF-8 `bbsbox.device-credential/v2`. All CBOR lengths/integers use shortest encoding; indefinite forms, extra headers/fields and trailing bytes fail canonical reconstruction. Maximum encoded size is 512 bytes.

Payload is exactly `["bbsbox.device-credential/v2", 2, root_public_bstr32, device_public_bstr32, writer_incarnation_bstr16, purpose_bits_uint]`. Root/device keys differ and reject weak Ed25519 encodings. Incarnation is a nonzero fresh random 16-byte value when the client root issues a credential. Purpose bits: 1 authorship, 2 forum governance, 4 game ownership, 8 identity proof. A nonzero subset of those four bits is required; unknown bits/version are unsupported. There is no wall-clock permission or expiry field. Credential ID is SHA-256 of the exact encoded COSE bytes, displayed `credential:<64 lowercase hex digits>`.

Verification rebuilds both payload and the entire envelope before strict Ed25519 signature verification. `check_subject_purpose` checks the expected person and delegation purpose. It does not decide forum membership/ownership, local hosting policy, game permissions or causal revocation. M04/M13 feature authorizers supply those checks; storing a valid credential never grants them. Separate device certificates/incarnations do not share stream counters.

`crates/identity` migration 2 stores only verified public credential bytes and indexed derived IDs. Its resolver re-verifies and checks IDs on read; missing IDs remain dependencies. Inserts are transactional/idempotent and never link accounts. M03 C's separate destination-bound root possession proof supplies account linking; the [client core](../../client/identity/README.md) supplies encrypted user-held recovery. Restored-node activation and native private-client trust remain M12/M00/G2.

## Immutable legacy v1

Current issuance is v2 after the explicit algorithm update in [RFC 9864](https://www.rfc-editor.org/rfc/rfc9864.html). Existing device/link v1 uses its original -8 header, domain/version 1 and exact signed bytes; verification remains fixed to 32-byte Ed25519 keys. Issuing v1 or relabeling an existing algorithm fails. Both legacy vectors remain in the fixture directory and independent tests. IDs/cards do not change because their key derivation is unchanged. See [ADR-0002](../../docs/decisions/ADR-0002-wire-profiles.md).

## Conformance

The [public vector](tests/fixtures/device-v2.json) uses fixed, publicly known fixture seeds. `cargo run --locked --offline -p bbsbox-credentials --example device_vector` reproduces it. Rust tests reject altered signatures/issuers, weak/reused keys, unprotected/unknown headers, nonminimal CBOR, unsupported versions/purposes and excessive/trailing data. Client tests exercise root issuance, fresh incarnations, expected-person/device matching and purpose-limited signing. Resolver tests check persistence, corruption, no account creation and identity migration 1→2 preserving accounts/receipts.

`node tools/verify_credentials.mjs` independently rebuilds payload/signature structure using cbor-x and Node crypto, checks hashes/public keys, reproduces the signature and rejects tampering. Install the existing M00 web dependencies first; CI runs it after their prepared dependency install. Node 24 is pinned by that workspace. The verifier has no network calls. Ed25519 implementation: [ed25519-dalek 3](https://docs.rs/ed25519-dalek/3.0.0/ed25519_dalek/); [COSE](https://www.rfc-editor.org/rfc/rfc9052.html).

The [account-link vector](tests/fixtures/link-v2.json) uses the same fixed public test root and a distinct destination-bound payload. `cargo run --locked --offline -p bbsbox-credentials --example link_vector` reproduces it. Both Rust and Node check exact payload/signature bytes; changing the destination account invalidates the proof. The [client reference](../../client/identity/README.md#destination-account-proof) records its byte profile.
