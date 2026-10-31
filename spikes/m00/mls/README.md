# M00 encrypted-group and recovery experiment

Implementation/evidence reference for the [canonical M00 brief](../../../docs/development/modules/M00-foundation-spikes.md), [private-content contract](../../../docs/development/contracts/C05-private-content.md) and the user’s instructions. This harness is a disposable feasibility experiment, not a shipped forum/client or an independent source of product scope.

## Run locally

From `spikes/m00`, use the existing Rust toolchain setup. On this Windows workspace, dot-source `tools/local-env.ps1` first.

```sh
cargo test --locked --offline --manifest-path mls/Cargo.toml
cargo clippy --locked --offline --manifest-path mls/Cargo.toml --all-targets -- -D warnings
```

The first dependency fetch requires developer internet access (`cargo fetch --locked --manifest-path mls/Cargo.toml`); tests themselves use separate in-memory OpenMLS clients and manually delivered serialized bytes. No network service is involved. Rust 1.99.1, OpenMLS 0.9.2, RustCrypto/basic-credential/traits providers 0.6.0 and the mandatory X25519/AES-128-GCM/SHA-256/Ed25519 suite are the selected test configuration. `Cargo.lock` freezes dependencies. Content/key debug features are disabled. Fixed recovery keys appear only in tests with disposable identities.

## Experiments and observed behavior

| Fixture | Evidence and design consequence |
| --- | --- |
| Three disconnected writers | Each creates ciphertext before receiving anyone else's work. Reordered same-sender messages decrypt once; replay, tampering and a host without client state fail. There is no global posting halt. |
| Missing commit | An old-epoch writer keeps writing. Another member already on the next epoch reads retained old work. A future-epoch message cannot be read until the missing commit arrives; retrying those same bytes then succeeds. Transport must preserve pending work. |
| Removal and new join | A stale removed member can still write under its old view and read retained old-epoch ciphertext. It cannot read new epochs; after learning removal it cannot create new messages. A fresh owner-authorized join reads current work without automatically obtaining old history. Raw decryption is distinct from BBSBOX causal admission. |
| Retention | With the selected 32-past-epoch setting, an unread old message works after 32 updates and fails after the 33rd. This is an epoch count, not a calendar duration. Current work remains usable. |
| Competing commits | Concurrent valid commits create incompatible branches. Raw MLS also accepts a non-owner member's commit. Forum-owner authorization and serialization must be enforced by the BBSBOX control/approval layer; membership alone is insufficient. |
| Encrypted snapshot | AES-256-GCM protects serialized client storage with a separate test recovery key, random nonce and versioned authenticated context. A fresh provider reloads the group and signing key and reads subsequent traffic. Wrong keys, ciphertext tampering and mismatched fixture identity fail. This tests storage mechanics, not a production recovery-kit format or person-identity proof. |
| Backup rollback | A snapshot predating a received message can decrypt that message again after restore. Lost post-snapshot replay knowledge cannot be reconstructed from the old snapshot alone. Exact delivery/application IDs need their own reconciliation policy. |
| Recovery guard | The harness restores into receive-only mode. Even a subsequent commit does not automatically enable sending. A new owner-authorized join with fresh client state can send while other members continue using the group. The production private client must enforce this across all mutation paths. |

## Limits and next integration

The test labels are basic MLS credentials, not the C02 person-root/device chain. There is no forum-owner approval envelope, membership-role UI, transport, durable client transaction adapter or Tauri distribution implementation here. M13 must bind the inner MLS operation to the authenticated outer governance proof and validate roles before merging. M15/M16 must retain missing-epoch ciphertext and replay it after dependencies arrive. M12/M13 must implement user-facing recovery with fresh membership state and an honest recovery horizon.

These passing fixtures support continuing local development. They do not pass G2 or claim Android/iOS, independently trusted client code, capacity, long-term storage compatibility or a security audit. Phone tests follow the integrated milestone schedule in the roadmap. Native device evidence, actual membership policy and complete recovery remain release requirements.

Primary references: [OpenMLS 0.9.2](https://docs.rs/openmls/0.9.2/openmls/), [OpenMLS book](https://book.openmls.tech/), [group retention configuration](https://docs.rs/openmls/0.9.2/openmls/group/struct.MlsGroupJoinConfigBuilder.html), [RustCrypto provider](https://docs.rs/openmls_rust_crypto/0.6.0/openmls_rust_crypto/struct.OpenMlsRustCrypto.html) and [RFC 9420](https://www.rfc-editor.org/rfc/rfc9420.html). The fixture outcomes above come from our executed tests, not an upstream support guarantee.
