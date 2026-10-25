# C04 — Game host and Game Passport

Version 1.0.0 · Accepted for implementation · Owners M11 and M18.

## Local game integration

A game manifest declares game ID, implementation version, supported state/passport schemas, launch entry point, local multiplayer mode and supported profile components. First-party game assets are installed from local packages. Use separate game worker processes and an HTTP/WebSocket API; a game owns its state and rules. The core owns accounts and issues short-lived game-scoped sessions. Durable game commands carry idempotency IDs. The game cannot update core tables directly.

`launch`, `join_session`, `submit_command`, `read_state`, `profile_summary`, `export_character`, `inspect_import`, `accept_import`, and optional `apply_reward` are selected boundaries. Local session membership and game commands are authorized independently. Displayed character data is opt-in; achievements/progress are delivered by approved P24/O12. A lightweight local turn-based demo can prove multiplayer without selecting a permanent game title or engine.

## Passport and local group associations

A signed export binds game ID/state schema, character ID, owning person, issuing game/node authority, export ID and predecessor, snapshot hash, and declared transfer semantics. An export is evidence; receiving GM/game policy decides acceptance. Unsupported versions produce an actionable refusal. Selected first implementation uses copy/import semantics explicitly shown to the user: import becomes a destination branch, re-import of the same export is idempotent, concurrent branches are not silently merged. Exclusive one-active-copy transfer needs a separate game-specific protocol and is not claimed offline.

`local_memberships(person, node)` returns that node's live clan/cohort associations under chosen visibility/authorization. Passport history may mention an origin group but never automatically enrolls the character at another node or creates a federated clan. Profile components query an authorized game summary and honor the user's display choice.

## Rewards

For P20, an issuer creates a random one-use code; its issuing node atomically records redemption and durable game reward delivery. A worker retry cannot apply the reward twice. Wrong-node redemption fails. Spending never changes social reach. Game-earned coupons are an approved O11 slice, with issuer-local redemption and durable retry handling. P23 manual rewards are approved in O12 and remain an operator opt-in; M16 only records/displays credit.

## Fixtures and completion

Two local players, reconnect after command retry, export/import with configured GM approval, incompatible schema, duplicate import, two diverging branches, local membership isolation, opt-in profile display, and reward retry at the issuer. A manifest claiming portability is valid only after that game's adapter passes its own import/export fixtures.


## Import authority and browser boundary

An origin signature certifies a snapshot, not the current importing user's right to claim it. Require an owner-authorized signed import intent bound to export hash, destination NodeId, game ID and a fresh receiver challenge. Replay returns the original receipt; a different account cannot attach it without an explicit owner delegation. Lost-owner-key cases follow M03 recovery rather than GM approval forging identity.

Workers use a separate game origin/sandbox and least-privilege game tokens. Core session cookies and private forum keys never cross that boundary. Game checkpoints participate in C01 coordinated backup; receipt redemption uses an outbox plus worker-side durable deduplication rather than pretending two databases share a transaction.
