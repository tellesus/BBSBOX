# C05 — Private content, profiles and complaint access

Version 0.1.0 · Proposed · Owners M07, M13 and M14.

## Privacy surfaces

Separate audience, distribution and presentation. Local-only means not federated; it does not imply encryption. Public profile means optional fields chosen for display; private profile still exposes the common directory baseline. Proposed baseline is username and stable local profile link; additional baseline fields require a decision. Lump Drop count has its own public/private switch. Never serialize private contact/game/event fields and rely on CSS to hide them. Feed links and event references obey their own audience authorization.

Client-held forum decryption keys never enter node backups, server search indexes or game APIs. Proposed MLS/OpenMLS group state is owned by trusted member clients; public governance proofs establish outer admission and moderator roles for the host. Sharing plaintext voluntarily remains the member's choice under R17. Exact metadata disclosure and proof format need a recorded M13 decision.

## Client trust and delayed state

Secure browser access and trust in delivered JavaScript are different questions. Offline HTTPS is necessary for the chosen browser features but does not prevent a hosting SysOp from altering that site's client. M00/M13 must demonstrate an independently trusted client distribution path for the stronger hosting threat model. An installed signed client using bundled UI/crypto is the leading candidate; OS/browser/mobile coverage and offline installation are unproven until tested. Do not claim a service worker by itself solves code substitution.

MLS commit ordering, retained old epochs and offline writers require explicit fixtures. Proposed owner-led serialization applies to governance changes only. Disconnected users may continue posting under locally known valid state. Preserve pending/concurrent governance work; never impose a global posting halt. Document finite history/key retention and what can actually be decrypted after a long absence. If the candidate fails the required behavior, resolve an alternative through an ADR before calling encrypted groups complete.

## Complaint case

`Complaint` binds case ID, forum ID, stable post IDs, reported immutable revision IDs where available, reporter explanation, optional alleged-illegal flag, and role-visible responses/status. The explanation goes to the SysOp; encrypted target plaintext is not attached automatically. Explicit voluntary disclosure, if added, is a separate P03 choice.

Only the forum's authorized owner/moderators may fetch suppressed target ciphertext through the review endpoint, after proving role according to the signed governance state known locally. A user account's SysOp role is insufficient. They decrypt using existing member keys. Ordinary queries/downloads on this host suppress selected posts; cached copies/other hosts are outside that local action. Host suppression is reversible and separate from forum edits/removal.

If an exact revision is not present, show missing/pending and request it through permitted transport; do not substitute another post. Preserve any locally available reported revision under the documented case-retention policy. Keep later edits visible as later revisions to the moderator. Cases move through reported, forwarded, awaiting-target/review, responded and locally-resolved states; suppression is an independent flag. Reporting alone changes no content visibility.
