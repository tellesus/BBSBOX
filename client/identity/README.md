# Client identity signing core

Implementation reference for [M03](../../docs/development/modules/M03-identity-access.md) and the [public credential profile](../../crates/credentials/README.md). Product authority remains the canonical roadmap and user instructions.

This Rust library generates separate client-held person roots and device signing keys. A root issues a credential with a fresh random incarnation and explicit bounded purposes. A device activates only with the expected person's credential matching its own public key, then signs only delegated purposes. Consumers construct their domain-separated/resource-bound signed structure; receivers still verify resource authority and causal revocation.

Secret types have no Debug, Clone or plaintext serialization interface. Seeds and signing keys zeroize on drop through the maintained libraries. Public contact cards contain root public keys only. The node depends on the public verifier; this client key holder appears in node tests only.

This is a cryptographic library with real tests, not a shipped native private client. M03 C adds protected client recovery and account-link proofs; M04 owns stream/counter integration; M13 owns MLS state. Native installation/update trust and phone tests remain their roadmap gates. Run the root Cargo workspace checks from the repository root.
