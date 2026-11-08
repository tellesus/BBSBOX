# Private forum host providers

Implementation reference for [M13](../docs/development/modules/M13-encrypted-forums.md) under C02 v1.4.1 and C05 v1.1.4. The [host crate](../crates/private-forum-host/src/lib.rs) stores public routing indexes over retained signed opaque events. It stores no member decryption keys. This provider is not yet registered in the node runtime or exposed through transport endpoints.

## Owner-led governance

`Host` implements the `private-forum` M04 module. Its caller explicitly configures forum/distribution pairs and a control-history budget from 1 through 4,096 accepted controls per scope. Migration 1 indexes control IDs and their scope. Every history read reconstructs the accepted owner-led chain after checking M04 readiness and credential proofs. Missing credentials preserve the unambiguous prefix. Competing successors withhold that branch and its descendants. Receipts remain immutable.

## Opaque create and reply admission

`posts::Posts` implements the separate `private-forum-posts` M04 module and shares the configured `Host` and credential resolver. Migration 2 adds an index of application event IDs and scope. Active intake must include both modules in the same M04 fork-synchronization transaction. Route control events to `Host` and application events to `Posts`.

The initial application provider supports `forum.post.create` and `forum.post.reply`, schema 1 and required extension `0x1301`. Edits and withdrawals remain unsupported and receive no acceptance receipt. The caller selects a separate application-history budget from 1 through 4,096 accepted records per scope. Counts include retained records invalidated by later evidence. These are bounded provider limits, not measured appliance capacity claims. Paged history remains open.

Admission requires a verified authorship credential, exact forum/distribution, the matching accepted control and epoch membership. Known cross-scope event dependencies are rejected. Missing credentials, controls and semantic predecessors remain pending. M04 validates detached body commitments and stream continuity. A fresh accepted opaque create or reply is relevant; duplicate deliveries return the original receipt. Host acceptance does not assert that MLS decryption will succeed or that plaintext is meaningful.

## Signed causal context

Each derived post records the greatest epoch observed through its own control, signed stream predecessor and reply parent, transitively. A disconnected post anchored to an older accepted epoch may remain valid after removal arrives at the host. If the post's signed causal references observe an epoch in which that credential was removed, anchoring it to an earlier membership state does not authorize it. This check uses the signed context, not wall-clock time or a host assumption about what a disconnected client has seen.

The provider reconstructs the complete bounded application graph with iterative passes. It rechecks current M04 readiness and accepted governance before returning `admitted` metadata or effective dependencies. Late device forks and owner conflicts can therefore retract a post and its semantic dependents while preserving unrelated earlier work. They neither rewrite receipts nor award credit again. The result is admission metadata; future runtime queries must apply session/audience access and M14 host suppression separately.

## Integration evidence and next work

SQLite fixtures cover missing proofs/parents, restart, duplicates, wrong membership/epoch, accepted disconnected work, causally observed removal, cross-scope denial, unsupported revisions, late device/control conflicts, capacity and failed index-write rollback. Client cryptographic framing remains covered by the separate pinned OpenMLS experiment. Application edits/withdrawals, delegated governance proposals, runtime/transport access, suppression and actual trusted-client MLS state remain later slices.
