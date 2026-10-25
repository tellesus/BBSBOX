# C04 — Game host and Game Passport

Version 0.1.0 · Proposed · Owners M11 and M18.

## Local game integration

A game manifest declares game ID, implementation version, supported state/passport schemas, launch entry point, local multiplayer mode and supported profile components. First-party game assets are installed from local packages. Prefer separate game worker processes and an HTTP/WebSocket API; a game owns its state and rules. The core owns accounts and issues short-lived game-scoped sessions. Durable game commands carry idempotency IDs. The game cannot update core tables directly.

`launch`, `join_session`, `submit_command`, `read_state`, `profile_summary`, `export_character`, `inspect_import`, `accept_import`, and optional `apply_reward` are proposed boundaries. Local session membership and game commands are authorized independently. Displayed character data is opt-in; achievements/progress require P24. A lightweight local turn-based demo can prove multiplayer without selecting a permanent game title or engine.

## Passport and local group associations

A signed export binds game ID/state schema, character ID, owning person, issuing game/node authority, export ID and predecessor, snapshot hash, and declared transfer semantics. An export is evidence; receiving GM/game policy decides acceptance. Unsupported versions produce an actionable refusal. Proposed first implementation uses copy/import semantics explicitly shown to the user: import becomes a destination branch, re-import of the same export is idempotent, concurrent branches are not silently merged. Exclusive one-active-copy transfer needs a separate game-specific protocol and is not claimed offline.

`local_memberships(person, node)` returns that node's live clan/cohort associations under chosen visibility/authorization. Passport history may mention an origin group but never automatically enrolls the character at another node or creates a federated clan. Profile components query an authorized game summary and honor the user's display choice.

## Rewards

For P20, an issuer creates a random one-use code; its issuing node atomically records redemption and durable game reward delivery. A worker retry cannot apply the reward twice. Wrong-node redemption fails. Spending never changes social reach. Game-earned coupons are a separate conditional slice. P23 Lump Drop rewards remain disabled until separately approved; M16 only records/displays credit.

## Fixtures and completion

Two local players, reconnect after command retry, export/import with configured GM approval, incompatible schema, duplicate import, two diverging branches, local membership isolation, opt-in profile display, and reward retry at the issuer. A manifest claiming portability is valid only after that game's adapter passes its own import/export fixtures.
