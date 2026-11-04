# Private forum application framing v1

Exact byte reference for [C05](../docs/development/contracts/C05-private-content.md) and the [M13 implementation](../crates/private-forum/src/lib.rs).

The C02 envelope uses kind schema `1`, a canonical ForumId resource, the `forum-members` audience and a person device delegated for authorship. The detached body is an MLS private application message. The forum digest is the 32-byte MLS GroupId. A post or reply event's ID is its stable post ID. Each subsequent edit or withdrawal has its own immutable revision ID.

Required extension `0x1301` holds an 8-byte big-endian epoch, 32-byte control event ID and a random nonzero 16-byte action ID. Append the target IDs from the table in order. The dependency list contains the control ID and all target IDs, sorted and deduplicated as C02 Event dependencies. Targets and the control ID are nonzero and distinct from each other where their meanings differ. The stable post and its initial revision may be the same ID.

| Kind | Action byte | Target IDs | Extension bytes |
| --- | --- | --- | --- |
| `forum.post.create` | 1 | None | 56 |
| `forum.post.reply` | 2 | Stable parent | 88 |
| `forum.post.edit` | 3 | Stable post, prior revision | 120 |
| `forum.post.withdraw` | 4 | Stable post, prior revision | 120 |

The sender passes the following concatenated bytes to MLS before encrypting the application message.

1. ASCII `bbsbox.forum-aad/v1` followed by one NUL byte.
2. One action byte, the 32-byte ForumId digest and one C02 distribution byte.
3. The 32-byte credential digest, 32-byte PersonId digest, 32-byte device signing key and 16-byte device incarnation.
4. The 8-byte big-endian C02 sequence, a one-byte previous-event indicator and, when present, the 32-byte previous event ID. The indicator is `0` for absent and `1` for present.
5. The exact required extension bytes.

The C02 signature binds the detached ciphertext hash and length. Recipients compare the unverified MLS group ID, epoch, application framing and authenticated-data field with the outer record before processing. After MLS verification they compare the authenticated data again and verify that the MLS leaf matches the authorized device for that epoch. Host admission separately checks the public governance chain, resource permission and distribution policy. Passing the framing check alone grants no membership, delivery credit or permission to share.

The [fixed test vector](../crates/private-forum/tests/outer.rs) verifies the exact authenticated-data bytes. The [pinned OpenMLS fixture](../spikes/m00/mls/tests/async_groups.rs) checks that a relabeled ciphertext fails without consuming the original message.
