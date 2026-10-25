# C03 — Selected exchange, Pigeon delivery and credit

Version 0.1.0 · Proposed · Owners M15 and M16; consumes C02 v0.1.0.

## Transport-independent exchange

`inventory(subscription, ranges)`, `want(event_ids/ranges)`, `offer(manifest)`, `receive_chunk`, `integrate`, `receipt` form a resumable exchange. Both authenticated offline IP peers and Pigeon packages feed the same event importer. Each endpoint applies local peer/subscription policy. No file-library blobs enter this exchange. Start with sequence-range inventories and explicit gap requests; Merkle trees and advanced routing remain D05/D07.

Package identity is `(origin_node, signing_incarnation, dispatch_sequence)` plus signed manifest hash. Reusing a stamp for different bytes is an error. Manifests bind versions, declared destinations/subscriptions, contained hashes/lengths and per-chunk hashes. Compression is optional after measurement; enforce limits on expanded size. Reject unknown destination/version at an understandable boundary. Private payloads remain encrypted; do not add carriers to groups. Public-content confidentiality and package-at-rest encryption are separate undecided P05 options.

## Persistent delivery and attribution

At receiver intake an authenticated delivering person confirms attribution. Persist `(receiver, package_id, manifest_hash, person_id, receiving_delivery_id)` before processing. Carrier identity is separate from device owner and authors. Retries look up the same delivery and cannot reassign an already accepted attribution. If no account exists, require local account creation/identity linking before credited import; exact visitor flow is an M16 decision.

Each newly staged event gets one durable first-source delivery association at that receiver. Repackaging or a later duplicate cannot steal that provenance. When missing dependencies later arrive, acceptance credits the recorded source delivery once, if the event qualifies under C02. Concurrent imports serialize this association. A later direct mesh copy does not mint a second courier credit.

`credit(receiver, receiving_delivery_id)` has a unique constraint. Many relevant events in one delivery yield +1; no relevant event yields 0; staged-only yields pending. More events accepted later do not increment again. Different receivers may each award their own receiving-delivery credit. Proposed first profile total is the local receiving BBS's count, clearly labeled; network aggregation is deferred pending a separate identity/receipt decision. A private count stays recorded and is hidden from public queries.

## User-visible state

Keep local save, courier handoff, remote receipt, accepted integration, and optional human acknowledgment distinct. A missing acknowledgment leaves delivery unknown, not failed or read. Receipts carry results and reasons; returning them can itself take a Pigeon trip. Present last-contact evidence without claiming that a stale clock gives exact network freshness.

## Required fixtures

Overlapping 108/107 packages; one relevant event among duplicates; all stale/rejected; pending acceptance later; two couriers supplying the same event; a maliciously changed manifest; interrupted credit transaction; incompatible module version; restoring a receiver and retrying. Cross-node totals, multi-hop routing and Pigeon-to-Pigeon transfer are later choices.
