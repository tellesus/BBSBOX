# BBSBOX — Tier 1 Master Product Design & Reference

| Document control | Value |
| --- | --- |
| Document ID | BBSBOX-T1-001 |
| Version | **0.3.0** |
| Date | **2026-10-25** |
| Status | Source-audited baseline with approved community additions; remaining proposals unapproved |
| Product owner | Michael |
| Canonical location | `BBSBOX_MASTER_SPEC.md` in [tellesus/BBSBOX](https://github.com/tellesus/BBSBOX) |
| Authority | Tier 1: product purpose, requirements, boundaries, and scope control |
| Implementation status | Specification only; no implementation or test results claimed |
| Software license | **TBD — open source with a sharealike/reciprocal ethos** |

## 1. Purpose and authority

BBSBOX is a community platform built around local ownership, personal identity, individual SysOps, distinctive community spaces, and independent connections between them. A lightweight repurposed computer and an internet-isolated Wi-Fi network give nearby people a place to communicate, cooperate, create, play, and gather. Nodes may connect through an offline mesh; Pigeons physically carry messages between disconnected places.

This document consolidates the project discussion into one scope reference. It governs more detailed architecture, protocol, interface, operations, and game-integration documents. Those documents may refine approved requirements; they may not quietly expand them.

The design question is:

> Does this feature help real people communicate, cooperate, create, play, or gather?

Fun and creative expression are valid answers. Time spent looking at the screen is not the project's optimization target.

### 1.1 How to read status

| Label | Meaning | Implementation consequence |
| --- | --- | --- |
| **Settled — R** | A product requirement or boundary established by the user's statements, clear agreement, or the current consolidation request | Preserve it; changing it requires a recorded product decision |
| **Proposed — P** | A possible feature or implementation approach, including assistant and fictional-roundtable suggestions | May be evaluated or prototyped; not a promised feature or selected technology |
| **Deferred — D** | An idea parked outside the proposed foundation release | No current implementation commitment; revisit only through change control |
| **Non-goal — N** | An explicit exclusion or boundary derived from settled direction | Do not add it incidentally through another feature |

Status and scheduling are separate. A settled capability can ship later. An optional feature can still be a settled requirement to support; opting in is an operator/user choice, not uncertainty about its status. A proposal is not approved merely because it appears in a phase or acceptance scenario.

The phase plan, acceptance scenarios, and editorial deferral of unapproved ideas are planning recommendations prepared to make the scope actionable. They do not imply that the user previously selected a delivery schedule or every detailed implementation rule.

### 1.2 Evidence and precedence

The source is [Build A Local BBS](https://chatgpt.com/c/6adc295f-a36c-83e9-a133-64b770da8a2a), supplemented by the requests to create this specification, audit it against the exported conversation, and integrate subsequent explicit product additions. Version 0.2.0 was checked directly against all 16 user turns and their assistant responses in `ChatGPT-Build A Local BBS-20261025-0035.md`, not just the cached preview or conversation summaries. Export timestamps and line references are recorded in §17.4.

Read the conversation as a developing brainstorm: later user conclusions and corrections take precedence over earlier formulations. Later synthesis helps identify the resulting direction, but an assistant's assertion that something is settled is not sufficient evidence by itself. Broad agreement establishes the direction of a discussion without selecting every named library or implementation detail. The simulated roundtable is part of the brainstorm: retain its useful ideas as proposals, and give them settled status where the user subsequently adopts or clarifies them. Neither the fictional speakers' assertions nor their simulated agreement constitute independent approvals.

The source anchors below use distinctive user wording so they can be found in the original conversation. They are references, not a transcript.

| Source | User decision or request |
| --- | --- |
| U01 | Initial request: lightweight repurposed computer, offline Wi-Fi router, classic BBS features/feel, mouse navigation |
| U02 | Permanent neighborhood/community operation; mesh links and synchronization of selected forums; interest in event/gathering installations |
| U03 | SysOp customization, optional mesh forums/feeds, agreement that files should not sync, public/private-key encrypted forums |
| U04 | Agreement with independent offline communities; a Pigeon installation solely for physically carrying messages between unconnected nodes |
| U05 | Explicitly no internet operation or internet connection seeking; exposure reports so other nodes can choose banishment |
| U06 | Agreement with offline infrastructure direction, then request for additional security/community ideas; subsequent fictional roundtable is brainstorming |
| U07 | Internet-connected clients are expected; BBSBOX hardware stays disconnected; possible identity-backed exposure reports; each SysOp decides trust; encrypted forum owner/moderator responsibility; contact through mail; sequence stamps and protected Pigeon Lumps |
| U08 | Request for a pitch on signed event format, node/forum identities, and Pigeon reconciliation; the response calls itself an architecture proposal |
| U09 | Explicit acceptance of stale-credential access risk after removal from disconnected encrypted groups; owners manage membership trust |
| U10 | Readiness to move on to feature design; remaining recovery, retention, client, and extensibility details were presented as proposals |
| U11 | Name question settled for this project; no further name/trademark investigation requested |
| U12 | Modern devices/Wi-Fi/UI, neighborhood mission, all-open-source/sharealike intent, friends' posts and event invitations without paid reach, games, multiplayer, desired character portability; possible inter-node competition, signature dungeons, business game promotions |
| U13 | Redemption at issuing BBS only, optional purchase-related game bonuses supporting the host business; approval of mission/design test; explicit objection to invented caveats becoming canon |
| U14 | Request for a Tier 1 master design/reference document, updates when scope changes, and storage in new BBSBOX GitHub repository |
| U15 | Initial specification request in this chat: explicit status separation, versioned scope control, development phases, acceptance criteria, and repository commit if access permits |
| U16 | Source-audit request in this chat: inspect the export directly, give eventual conclusions more weight than early brainstorming, and treat the simulated discussion as part of that brainstorming |
| U17 | 2026-10-25 additions in this chat: Lump Drop credit/privacy and possible discretionary rewards; post-linked complaints and optional temporary suppression of posts alleged illegal; account directory and optional profile information/activity; local clans/cohorts with profiles, directories, rosters and Game Passport integration; shipped dark theme with vibrant colors on black |

## 2. Mission and people

**R01 — Neighborhood community and enjoyable participation.** BBSBOX serves real local communities: neighbors, friends, clubs, organizers, local businesses, and people with shared interests. It should support everyday communication, useful local services, creative activity, games, and gathering in person. It is designed around small communities whose participants can form meaningful relationships, with local control over unwanted spam and coordinated manipulation. This mission is not a promise that internet-connected visitors cannot bring in outside material. [U02, U07, U09, U12–U13]

**R02 — Independent places with individual SysOps.** Every full BBS is useful on its own and has its own character, local content, administration, and chosen relationships. Joining a mesh does not turn nodes into interchangeable fronts for one centrally controlled service. [U03–U04, U12–U13]

**R03 — Modern interaction and a vibrant dark default.** The primary experience is a modern browser interface on phones, tablets, and computers, using mouse, touch, and keyboard. The shipped interface always defaults to dark mode: black backgrounds with bright, vibrant, pleasant colors. It does not automatically switch the shipped default to light mode based on the device theme. SysOps can customize the colors and presentation. Classic BBS structure and visual creativity are inspiration; terminal-era constraints must not dictate ordinary navigation. [U01, U03–U04, U12, U17]

The people and roles are:

| Role | Purpose and authority |
| --- | --- |
| Resident/member | Read, participate, maintain chosen relationships, join groups, send messages, organize, and play |
| SysOp | Operate and customize a node; manage local accounts and services; choose hosted forums, peers, and local trust policy |
| Forum owner | Govern a forum, including its membership and appointed moderators; ownership is distinct from hosting |
| Appointed moderator | Exercise the forum powers delegated by its owner |
| Pigeon operator/carrier | Carry permitted message bundles between places; carriage confers no forum ownership or plaintext access |
| Game master/operator | Apply a game's rules and approve character/account imports where that game requires approval |
| Business host | Operate a community node at its premises and optionally offer game-related purchase bonuses |
| Developer/contributor | Build open software and compatible features within the Tier 1 scope |

A person may hold several roles. Possession of one role does not automatically grant the others.

## 3. Infrastructure and installation modes

### 3.1 Offline infrastructure

**R04 — Internet-isolated operation is foundational.** BBSBOX infrastructure operates without the public internet and must not seek an internet connection. The deployment must enforce this intent, not merely work during a temporary outage. Local Wi-Fi, approved offline mesh links, and physical carriage are the communication environment. [U01, U05–U07]

This applies to full nodes and participating dedicated relay/courier infrastructure. Internet-connected phones and laptops may be clients: their independent internet connectivity does not itself make the hosting BBS noncompliant. The node must handle untrusted client input and cannot assume visitors will keep information inside the mesh. [U07]

Required deployment outcomes:

- UI code, fonts, game assets, authentication, and other runtime resources are available locally.
- Normal operation does not depend on cloud login, public DNS, internet time services, online certificate issuance/renewal, telemetry endpoints, CDNs, or online package downloads.
- Node networking prevents unauthorized internet routing and forwarding.
- Installation, identity establishment, operation, and recovery can be performed offline using prepared local materials.
- Software maintenance uses locally supplied, verified packages. Infrastructure does not fetch updates from the internet.
- Connectivity diagnostics use local evidence; they must not probe a public website to test whether the internet is reachable.

Exact operating-system image, firewall rules, certificate/client distribution, and monitoring implementation remain Tier 2 decisions. Repository hosting and development work on GitHub do not create an internet dependency for deployed BBSBOX equipment.

**R05 — Modest, repurposed hardware.** A full node should be practical on a lightweight reused computer with an ordinary isolated Wi-Fi router. No hardware brand, RAM minimum, price target, user capacity, or power budget has been selected. Those require a measured prototype profile. [U01]

### 3.2 Installation profiles

| Profile | Status | Scope |
| --- | --- | --- |
| Full BBS node | Settled | Local community services, SysOp administration/customization, optional selected federation |
| Pigeon | Settled | Lightweight installation solely for message carriage; no requirement to host a full BBS |
| Dedicated mesh relay | Proposed P01 | Forward selected traffic without providing a full community interface |
| Temporary event/gathering node | Deferred D01 | Desired possible use of the platform; permanent neighborhoods remain the primary deployment |
| Public kiosk client | Proposed P02 | Walk-up public reading/interaction with a simple interface and session reset |

The initial implementation need not ship every profile at once.

## 4. SysOp experience and local services

**R06 — Meaningful SysOp customization and control.** A SysOp controls the node's appearance, welcome material, navigation, local services, local account permissions, and participation in shared forums/feeds. Customization must preserve modern usability and compatible message exchange. [U03–U04, U12]

Welcome art, themes, custom menus, message-of-the-day content, layouts, and locally installed games are the intended customization surface. The exact theme/package interface and editor are proposed implementation work, not selected standards.

**R07 — Classic community BBS foundation.** Provide local discussion boards, member identity/accounts, private mail, local file areas, games, and administration as the core product direction. These are delivered through the modern interface. Local mail also gives SysOps a way to contact forum owners and other operators. [U01–U04, U07, U12]

Chat, notifications, search, online presence, and caller-history details remain proposals (P03). The user directory and profile capabilities in R26 are now settled; their exact field schema and layout remain design work. “Classic features” is not blanket approval for every feature found in old BBS packages.

**R08 — File libraries remain local.** Do not automatically synchronize file libraries through mesh federation or Pigeons. Local uploads/downloads are separate from message synchronization. The conversation did not settle a general policy for message attachments or compact art embedded in messages; define any such support and its limits in Tier 2 without turning it into file-library replication. [U03]

An offline software-release package is a deliberate maintenance artifact, not an automatically federated file library. Its packaging and transport workflow must be specified separately.

Usability target for the implementation: a nontechnical host can understand node status and perform ordinary administration; a resident can discover discussions, events, mail, and games without understanding routing or cryptography. Exact onboarding, backup, retention, and recovery workflows are tracked in P04 and the phase plan.

## 5. Opt-in federation and Pigeons

### 5.1 Federation

**R09 — Selective, optional mesh participation.** SysOps choose which peers, forums, and feeds their node participates in. Local-only content stays local. Nodes retain usable local copies of subscribed information during disconnection and exchange missing information when a permitted path becomes available. No central registry, internet identity provider, or mandatory central server is required. [U02–U06]

The same product-level delivery behavior should apply whether information arrives through an offline network link or a Pigeon. Specific radio stacks and adapter implementations are not selected.

Forum visibility and distribution are separate dimensions:

| Reading access | Local only | Selected federation |
| --- | --- | --- |
| Public under the node/forum's access rules | Local public board | Shared public forum/feed |
| Authorized encrypted-group members | Local encrypted group | Encrypted group replicated as ciphertext |

This is a conceptual model. UI naming and treatment of any additional access-controlled but unencrypted forum type remain open.

### 5.2 Pigeon role

**R10 — Dedicated physical message carriage.** A Pigeon installation carries messages between nodes without a mesh path. It can collect permitted outgoing information, transport it while disconnected, deliver it, and collect return information. Carriers do not need forum membership or private-message keys. [U04–U07]

**R11 — Protected Pigeon Lumps and sequence stamps.** Each delivery package (“Pigeon Lump”) has a unique sequence-based identity rather than depending on accurate real-world time. Contents must be authenticated/protected against undetected carrier tampering; private-group content remains encrypted in carriage. Repeated or stale delivery must not overwrite newer authorized state. [U07]

Active portable devices and passive removable-media packages are proposed forms (P05), not a requirement to support every phone, USB drive, or radio immediately. Multi-hop carriage, Pigeon-to-Pigeon exchange, route/status screens, storage-at-rest encryption, and configurable delivery budgets are also in P05.

### 5.3 Delivery principles and unresolved mechanics

**R12 — Signed-event and asynchronous-delivery foundation.** Signed information, distinct identities/authority, interrupted connectivity, protected carriage, and sequence-based ordering form the resulting design direction. Out-of-order delivery preserves valid missing information without rolling back newer authorized state; accurate synchronized clocks and courier arrival order must not decide authoritative state. These are the later discussion's delivery principles, distinct from the specific protocol implementation pitched beneath them. [U03–U08, U10, U15–U16]

The exact event format and reconciliation algorithm were requested as a **pitch**. They are retained below as **P06**, not presented as a ratified wire protocol.

U07 initially suggested rejecting an older delivery after a newer sequence had arrived. The subsequent explanation, protocol pitch, and later foundation recap consistently developed event-level reconciliation because an older package can contain unseen valid messages. The resulting direction preserves the user's anti-rollback goal while accepting missing valid history. Keep the earlier whole-package rejection idea in the correction history, not as a competing current requirement. The specific stream structure, encoding, conflict-handling machinery, and algorithm below remain P06; the conversation did not separately ratify each implementation choice.

The proposed reconciliation contract is:

1. Validate package identity, integrity, destination, version, and local acceptance policy.
2. Recognize a repeated identical package and return the recorded outcome without duplicating effects.
3. Inspect events individually, retaining valid missing information even from an older package.
4. Verify signatures and action authority separately. Stage records with missing dependencies.
5. Detect conflicting records at the same stream position; retain evidence and stop affected automatic state advancement rather than select by arrival time.
6. Apply authorized edits, withdrawals, governance changes, and encryption transitions according to their own dependency/order rules.
7. Persist progress durably so interruption and retry cannot partially duplicate delivery.
8. Record receipts that distinguish receipt of the package from integration of its contents.

Example: Lump 108 arrives with A, B, C. Lump 107 later arrives with A and unseen D. Skip duplicate A; preserve valid D. A later-arriving original post must not reverse an already-authorized edit.

Proposed synchronization metadata separates:

| Record | Purpose |
| --- | --- |
| Dispatch stamp | Identify a package and detect replay/conflicting reuse |
| Author/device stream sequence | Track event history and missing/conflicting events |
| Forum governance version | Order authorized control changes |
| Encryption state/version | Interpret the chosen encryption protocol's membership/key transitions |
| Human date/time | Display posts and schedule gatherings; not an authority for protocol state |

**Delivery promise:** material becomes available to its intended authorized audience when it reaches their node/client under the applicable subscription and moderation rules. There is no promise of instant arrival, a route that does not exist, or proof of reading.

Proposed delivery labels are queued, handed to courier, received at destination, integrated, and—only if separately supported—acknowledged/read. Pending and rejected outcomes need understandable explanations. Labels must not conflate those events.


### 5.4 Lump Drops

**R25 — Credit useful Pigeon deliveries.** A user who delivers a Pigeon Lump receives one **Lump Drop** when at least one event from that delivery is newly accepted as a relevant change by the receiving BBS. One qualifying event is sufficient, even when every other event in the lump is already known or no longer relevant. If none qualifies, the delivery earns no credit. Record the cumulative count on the delivering user's account and display it on their user info/profile page unless they set that count to private. [U17]

The credit is for the person delivering the lump, rather than automatically for the message authors or the Pigeon device's owner. The delivery workflow must associate the delivery with that user's identity/account; the exact attribution and receipt format remain Tier 2 work.

| Delivery outcome | Lump Drop result |
| --- | --- |
| One newly accepted, relevant event | +1 |
| Several newly accepted, relevant events in the same delivery | +1 total |
| A mixture of duplicates/stale changes and one qualifying new event | +1 |
| All events are already known, no longer relevant, invalid, or rejected | 0 |
| Events only staged pending missing dependencies | No credit yet; award once if the attributed delivery later supplies a qualifying accepted event |
| Retry/replay of a delivery already credited by that receiving BBS | No additional credit |

Receipt or storage of the package alone is insufficient. An older-sequence event that is newly accepted as relevant still qualifies; age alone does not deny credit. It need not create a newly visible post: any qualifying accepted change counts. Integration and the associated credit must survive retry/restart without double-counting; a delivery that later has more events accepted still earns only one credit at that receiving BBS. Credit does not depend on the carrier being able to decrypt the contents.

Private-count settings control public display, not whether earned drops are recorded. The exact identity mapping across nodes and whether profile totals aggregate credits from several receiving BBSes remain implementation decisions; no network-wide leaderboard is specified.

**Future possibility — P23:** SysOps may eventually reward accumulated Lump Drops with things they choose at their discretion. For now, implement the count and its display/privacy control. No reward schedule or automatic reward mechanism is approved.

## 6. Identity, security, and trust

### 6.1 Authority boundaries

**R13 — Separate hosting, membership, authorship, and carriage.** A node identity, a forum's ownership/identity, a user's identity, and a courier's role are distinct. A valid signature identifies a signer; it does not grant that signer permission to perform every action. Hosting an encrypted forum does not grant membership, plaintext access, or moderation authority. [U03–U04, U07–U08]

The four named cryptographic namespaces, per-device signing feeds, genesis-record-derived forum IDs, portable personas, and a particular ownership-transfer record format are **P07**. They are plausible ways to implement these boundaries.

Desired continuity is that a community can retain its identity and available history across hosting changes. Precise portability, succession, key backup, and recovery procedures remain P04/P07; no recovery method has been selected.

### 6.2 Infrastructure trust

**R14 — Local trust and exposure reporting.** Evidence of a node's internet exposure can be reported to other participating nodes, with signed reports identifying their origin in the resulting design direction. Each receiving SysOp independently chooses whether to investigate, contact the other operator, restrict, banish, or later restore association. No node's report compels the whole network to ban another node. The report format, diagnostic implementation, and any aggregation thresholds remain P08. [U05–U08, U10]

The later user clarification supersedes any interpretation that clients with internet connections must be banished. A report is evidence for human/local policy judgment, not proof of compromise or a mandatory vote result.

The user suggested a diagnostic/report action associated with a user's identity. The UI, evidence format, aggregation thresholds, and automation remain **P08**. The assistant's “Trust Ledger,” severity ladder, dedicated control channel, and priority rules are proposals too. Cryptographic continuity, technical observations, and an operator's social trust choice should remain distinguishable.

### 6.3 Security scope

The accepted operating context includes untrusted visitors, internet-connected clients, possible spam/manipulation or exfiltration by clients, untrusted carriers, delayed messages, inaccurate clocks, and recipients who may share plaintext. The implementation must validate incoming data and enforce authority within the chosen protocol. [U07–U09]

Concrete design work remains on local admission/rate/storage controls, offline browser security, trusted client distribution, key handling, recovery, and authenticated updates. These are engineering questions in P04/P09, not a mandate to build an extensive governance or surveillance system.

No claim of protection from a malicious hosting SysOp's supplied client code is valid until the client trust/distribution approach has been designed and tested. This is the specific unresolved browser/client issue raised in the conversation, not a requirement that everyone install a native app.

## 7. Encrypted forums and moderation

**R15 — Private groups use public/private-key encryption.** Support private discussion within authorized groups, including groups hosted across selected nodes. Members' clients decrypt; relaying or storing ciphertext alone does not confer reading access. The cryptographic construction and library are not yet selected. [U03–U04, U07, U09]

**R16 — Forum owners govern; SysOps control hosting and local suppression.** Encrypted forum owners and their appointed moderators control membership, rules, and moderation. A member can raise a concern with a hosting SysOp. The complaint is tied to one or more specific posts, even when the SysOp cannot read those posts. The SysOp can contact the forum owner/moderation contact through mail, pass along the complaint reference, request review, and independently decide whether to keep hosting that forum. Hosting does not automatically appoint the SysOp as a forum moderator or confer decryption access. [U07, U17]

Required complaint/review flow:

1. Associate the original complaint with the forum, the specific post or posts, and the reporter's explanation. Preserve stable references so the complaint remains tied to its targets.
2. Let the hosting SysOp inspect the complaint reference and explanation without requiring access to the encrypted post contents.
3. Include that complaint reference when the SysOp contacts the owner or appointed moderator.
4. Let an authorized moderator open the reference, retrieve the specific posts through their existing forum access, and see what generated the complaint.
5. Let the moderator review and respond on the same complaint record. The SysOp can relate that response to the original report and make their local hosting decision.

A reporter can explicitly mark that they **believe the referenced post or posts contain something illegal**. This records the reporter's allegation. The SysOp may choose to temporarily suppress those specific posts on their BBS while the forum moderator reviews them and gets back to the SysOp. Reporting does not automatically suppress a post.

Suppression works by the post references without decrypting the content for the SysOp. It temporarily withholds the posts from ordinary display/access through that hosting node while preserving the complaint association and an authorized moderator's ability to retrieve the targets for review. After the moderator responds, the SysOp can lift suppression or make a further local hosting decision. The status and response stay associated with the complaint.

This is a local host action. It does not appoint the SysOp as a group moderator, change encrypted-group membership, or impose the same decision on other BBSes. Reference schemas, handling of edited/missing posts, and the authorized review path during suppression are Tier 2 details required to implement this workflow.

**R17 — Communities manage membership trust and delayed revocation.** Members can disclose plaintext they have decrypted. Removed members may retain old messages/keys and may read new material encrypted with stale credentials on disconnected nodes. This is an expressly accepted limitation. Owners choose whom to admit and manage that risk. [U07, U09]

Provide understandable membership/key-state information and workable invitation/removal/key-update tools. Exact workflows are Tier 2 design work. Correct cryptographic processing is still required, but immediate universal revocation is not a release condition. Do not impose global membership synchronization or a network-wide posting halt merely to manufacture that guarantee.

The scope decision does not instruct clients to ignore known invalid cryptographic transitions. It removes the promise of instant exclusion everywhere while keeping asynchronous operation usable.

## 8. Friends, posts, events, and community features

**R18 — Direct chronological social experience.** People see posts from the friends/people they choose to connect with, in chronological presentation, without engagement ranking, paid amplification, or payment to reach their existing audience. Communities and feeds are chosen explicitly. [U12–U13, U15]

Chronological display and authoritative protocol ordering solve different problems. The implementation must define how displayed dates, late-arriving posts, and clock uncertainty appear; causal/sequence verification must not be replaced with sorting by a timestamp. The exact friend/follow model, sorting direction, tie-break rules, and late-arrival indication remain **P10**.

**R19 — Local events and invitations.** People can announce gatherings and invite their chosen audience: concerts, protests, exhibitions, art shows, product launches, game nights, and ordinary community meetings. Events and invitations must reach the relevant available view when delivered, without a paid gatekeeper or engagement selection. [U12–U13]

Event creation, invitation, and chronological visibility are settled direction. Calendar layouts, RSVP states, reminders, cancellation/update propagation, and provisional attendance counts are **P10** details. Physical event dates still matter even though delivery correctness cannot require synchronized clocks.

### 8.1 Community feature register

These services were discussed; their status is explicit.

| ID | Feature | Status and boundary |
| --- | --- | --- |
| R07 | Local forums, mail, file areas, games, basic accounts/admin | Settled foundation; detailed flows to be designed |
| R09 | Selected shared forums and announcement feeds | Settled; opt-in |
| R15–R17 | Encrypted private groups and owner moderation | Settled; accepted asynchronous membership limitation |
| R18–R19 | Friends' posts, event announcements, invitations | Settled direction; detailed interaction design open |
| R25 | Lump Drop count on user profiles | Settled; qualifying deliveries only; user can make the count private; rewards remain P23 |
| R26 | Account directory and user profiles with optional personal/game/event information and a feed link | Settled; all local accounts remain listed with their public baseline; optional profile information respects privacy |
| R27 | Local clans and cohorts, profiles, directories, membership rosters, and Game Passport integration | Settled; groups belong to their local BBS; additional fun features remain P24 |
| P03 | Live local chat, SysOp paging, presence, search, notifications, welcome/MOTD options | Proposed elaboration of classic BBS experience; approved profile scope is R26 |
| P11 | Cross-node encrypted “Post Office” mail with delivery status | Proposed extension of settled local mail and message carriage |
| P12 | Neighborhood Exchange: lending, barter, giveaways, odd jobs, skills, mutual-aid/project listings | Proposed optional service; lifecycle/withdrawal/staleness rules needed |
| P13 | Community newspaper, printable editions, selected article syndication | Proposed optional publishing feature |
| P14 | Local knowledge base/wiki, local history, selected text-article synchronization | Proposed; does not authorize file-library federation |
| P15 | “Meshcasts”: compact publisher-controlled announcement feeds | Proposed specialization of settled feeds |
| P16 | “Mesh Atlas”: optional discovery/directory of participating nodes and advertised forums | Proposed; no central mandatory registry |
| P02 | Public guest reading and kiosk mode | Proposed accessibility/deployment feature |
| D02 | Postcards, custom node stamps, guestbooks, souvenirs, traveling exhibitions/stories, scavenger hunts | Deferred creative ideas; no foundation commitment |

The simulated roundtable contributes feature ideas to this register. Later user statements settle the direction where they adopt or clarify an idea—for example, owner-led encrypted moderation, local trust decisions, modern usability, events, and games. Classifieds, kiosks, newspapers, and other ideas without that later adoption remain available proposals.

### 8.2 User directory and profiles

**R26 — Find local members and share chosen profile information.** Every BBS has a directory listing everyone who has an account there. Making an account/profile private does not remove it from the directory: its entry still shows its username and the account fields defined as public for all accounts. A public profile can additionally show information the person chooses to share. [U17]

| Profile component | Required behavior |
| --- | --- |
| Directory baseline | List every local account by username and the defined common public fields; the complete baseline field list remains to be designed |
| Private profile | Show the directory baseline; hide optional profile details from other users |
| Public profile | Show the baseline and the optional details the user has chosen to publish |
| Optional personal fields | Support real-life contact information, a bio, and a short message; these are supplied/displayed by the user's choice |
| Games and characters | Let users choose to display the games they are playing and their characters, using participating games' integration |
| Hosted events | Let users choose to show events they are hosting |
| Personal feed | Link the profile to the user's feed of updates; the link follows the feed's own visibility rules |
| Lump Drops | Display the recorded count unless the user sets it private, following R25; directory/profile views respect that setting |

Optional contact details or game activity do not become common public fields merely because an account exists. The profile's private setting hides optional profile content; the separately controlled Lump Drop statistic follows R25. Public profile does not mean that every available field must be filled in or displayed.

Achievements/progress displays are **P24**, reflecting the user's “maybe”; additional logical profile components can be proposed through the same scope process. Exact fields, layouts, feed visibility settings, and game data interfaces remain design work. This user directory is distinct from the proposed Mesh Atlas of BBS nodes (P16).

### 8.3 Local clans and cohorts

**R27 — Local organizations and friend groups.** Users can form **clans**, larger organizations in the style of classic gamer clans, and **cohorts**, smaller friend groups in the style of game squads. Both are local to a BBS. Each has its own profile page, a directory listing, and a membership roster. [U17]

The clan/cohort directory makes these groups discoverable on their home BBS and links to their profile pages and rosters. The distinction describes their intended scale; numeric membership limits, hierarchy between clans and cohorts, membership roles, invitations, and roster visibility rules have not been selected.

Clans and cohorts work with the Game Passport capability so participating games can associate characters/accounts with their local group membership. Their precise game interface is designed alongside R21/P17. Portable characters do not by themselves turn a local clan or cohort into a federated organization or automatically create membership on another BBS.

Other fun group features remain **P24** until specified. The approved foundation is group formation, local profiles/directories, rosters, and Game Passport integration.

## 9. Games and business-hosted play

**R20 — Games are part of the product.** BBSBOX supports a game system and locally integrated games, including games created specifically for it. Some games will support multiplayer. Fun and social play are legitimate product goals. [U12]

**R21 — Portable characters/accounts where supported.** Support the desired ability to move a character or game account between nodes when the game supports it, subject to game-master approval if that game is configured for approval. Receiving games retain authority over acceptance and rules. This is a capability target, not a promise that every game shares one character format. [U12]

The **Game Passport capability and its integration with local clans/cohorts are now part of the required direction** under U17, alongside portable characters/accounts. Participating games must be able to associate their account/character information with the relevant local group membership and the user's chosen profile display (R26–R27). **P17** remains the proposed signed record/interface design: game ID/version, state, origin, import validation, and group-association representation. Transfer versus copy, concurrent play, re-export, incompatible versions, reward recognition, and the exact approval workflow need game-specific design. No universal item economy or cross-game compatibility has been selected.

| Game idea | Status |
| --- | --- |
| Local single-player and multiplayer games | Settled direction |
| Portable game accounts/characters in participating games | Settled desired capability |
| Inter-node competition and asynchronous multiplayer | Proposed P18; explicitly tentative in the user discussion |
| A signature dungeon per node with unique rewards | Proposed P19, user idea; not mandatory for every node |
| Shared worlds, traveling tournaments, correspondence chess, collaborative fiction | Deferred D03 examples |
| Legacy DOS door compatibility and dedicated terminal client | Deferred D04; no requirement for the first release |
| Named sample worlds/dungeons from assistant illustrations | Illustrations only; no committed game titles or content |

**R22 — Local business game bonuses: settled rules for an optional idea.** A café or other business can host its own BBSBOX and offer optional game bonuses associated with purchases, such as a receipt code for a healing potion. Redemption occurs at the issuing BBS. This supports the business providing the gathering place—informal “table rent.” It is not mandatory spending and does not buy placement or reach in social/community views. [U12–U13]

Implementation of receipt rewards, game-earned coupons, and promotion tools remains **P20**. The issuing-node-only rule and optional, game-related nature are settled if this feature is built. Promotion mechanics belong in a game or optional integration rather than expanding the BBS core into a retail platform. That placement follows the user's stated preference; the exact shared API is still open.

The BBS platform supplies the relevant identity, game integration, and transport facilities. Each game owns its rules, rewards, progression, and supported portability. A purchase-processing or point-of-sale integration has not been designed; it is not a prerequisite for the optional receipt-code idea. Issuing-node redemption is the settled boundary.

## 10. Open-source and project identity

**R23 — Fully open-source project with reciprocal intent.** BBSBOX is explicitly an open-source project with a sharealike ethos. The exact license will be chosen later. This includes the intended core, standard clients, Pigeon implementation, and documented interoperability work. [U12]

This document selects no license. Do not add an arbitrary LICENSE file or present a candidate as adopted. License selection and compatibility review belong to an explicit future decision. Software licensing and user-authored content are separate matters; no content licensing policy is created here.

**R24 — BBSBOX name and Tier 1 source of truth.** Use BBSBOX as the project name. The user has closed the naming investigation for present purposes. Store and version the master scope in the BBSBOX repository, updating it when decisions change. [U11, U14–U15]

No tagline, component brand beyond BBSBOX/Pigeon, product logo, or specific game name is approved by this document.

## 11. Proposed implementation register

This register retains useful choices without confusing them with requirements.

| ID | Proposal | Decision needed before implementation commitment |
| --- | --- | --- |
| P01 | Dedicated relay profile | Is a separate install/profile useful for the first federation release? |
| P02 | Guest reading, kiosk mode, session reset | Which public actions require accounts; kiosk scope and supported devices |
| P03 | Remaining classic BBS details; local blocks, leaving groups, invitation permissions, voluntary report disclosure | Chat/search/notification/presence and personal-control scope; profiles are settled in R26 and post-linked host complaints in R16 |
| P04 | Offline backup/recovery/export, retention quotas, node succession, simple installer and restart behavior | Recovery ownership, retained-history behavior, installation/update/recovery workflow |
| P05 | Active and USB Pigeons, multi-hop/Pigeon exchanges, route/status UI, storage and transparent queue controls; optional aggregate public-mail counters | First supported device/transport; routing, capacity, retention, acknowledgment behavior; useful status without unnecessary courier movement or private-group activity disclosure |
| P06 | Immutable signed events; per-key hash-linked feeds; event reconciliation; durable integration; compatible mesh/Pigeon sync | Exact format, authorization model, dependency rules, fork handling, and conformance examples |
| P07 | Node/forum/user-device/Pigeon key namespaces; portable personas; forum genesis/governance chain | Key ownership, delegation, migration, succession, device separation, recovery, and optional new-member access to earlier group history; preserve user identity separately from a banished home node |
| P08 | Identity-backed exposure diagnostics/reports; Trust Ledger; alert severity/channel/priority | Evidence, sharing, local automation, reinstatement, and clear distinction from compulsory voting |
| P09 | Offline secure browser delivery, independently verifiable client, established encryption library, hardened deployment | Client trust model, group-key scheme, supported secure contexts, release packaging |
| P10 | Friend/follow semantics, feed chronology details, event calendars/RSVPs/reminders | Minimum social/event workflow and treatment of delayed updates |
| P11–P16 | Community services in §8.1 | Approve each independently; optional service boundaries and phase |
| P17 | Signed Game Passport record/interface design; capability adopted in R21/R27 | Game interface, import approval, version/state transfer semantics, local group association, and chosen profile display |
| P18–P19 | Cross-node competition; signature dungeons/rewards | Select a game prototype and its rules; do not make every node a game world |
| P20 | Receipt-code game rewards and game-earned business coupons | Optional game integration and issuing-node redemption workflow |
| P21 | Small versioned protocol core, extension negotiation, independent implementations | Required interoperability surface and upgrade compatibility policy |
| P22 | Physical key introductions through QR codes/fingerprints/signed invitations; public Node Passport/identity cards | Operator/user introduction UX without a central authority; public identity details separate from recovery secrets |
| P23 | Discretionary SysOp rewards for accumulated Lump Drops | Whether/how to provide reward tools; currently track the number only (R25) |
| P24 | Optional game achievements/progress on profiles; further profile components and clan/cohort fun features | Choose specific additions without changing the approved R26/R27 foundation |

Additional brainstorm detail remains attached to these proposals rather than becoming new requirements: P02 includes readable public announcements, screen-reader/semantic navigation and descriptions for art, and simple walk-up use; P04 includes headless operation, measured power needs, graceful shutdown, and ordinary power-failure recovery; P13 includes printable editions and a possible paper-flyer-to-local-post workflow; P16 includes operator-chosen public directory details and location disclosure; P21 includes offline interoperability checks and clear core/optional/extension boundaries. D02 also retains ANSI-art galleries/competitions, custom welcome-screen challenges, and collaborative local-history projects. These details are candidates to assess when their parent feature is designed.

### 11.1 Technology candidates, not selections

The conversation considered a new application, reuse of existing BBS components, and a hybrid. No build-versus-reuse or language decision was finalized.

Candidates mentioned include Node.js/browser tooling or other languages, SQLite, deterministic CBOR, COSE/Ed25519 signatures, SHA-256 addressing, MLS/OpenMLS, Wi-Fi mesh/OpenWrt/802.11s/BATMAN-adv, Reticulum, and constrained radio transports. Existing BBS projects and signed-feed/delay-tolerant networking systems were references for investigation.

This is a record of candidate ideas, not a current technical recommendation, dependency list, or claim that any library already meets BBSBOX's needs. Tier 2 selection must assess offline operation, modest hardware, asynchronous behavior, usability, interoperability, and the eventual license.

## 12. Deferred ideas and non-goals

### 12.1 Deferred register

These entries are parked in this edition to protect the foundation. “Deferred” does not mean that the user rejected them or promised to build them later.

| ID | Idea | Revisit when |
| --- | --- | --- |
| D01 | Temporary gathering/festival installations and traveling full BBSes | Permanent-node deployment and ordinary Pigeon carriage work |
| D02 | Postcards/stamps, guestbooks/souvenirs, scavenger hunts, traveling art and collaborative artifacts | Core communication and game integration are useful |
| D03 | Large shared worlds, cross-node tournaments and richer asynchronous game experiments | A small game and portable-state interface have been exercised |
| D04 | Legacy door emulation, dedicated terminal access, simulated modem-era effects | Modern browser experience and game interface are stable |
| D05 | Merkle inventory optimization, authenticated pruning/checkpoints at scale | Measured history size or reconciliation cost justifies complexity |
| D06 | Hardware attestation, measured boot, read-only appliance variants, reproducible-build program, advanced recovery/quorum governance | A working deployment reveals a concrete need and supported hardware/process |
| D07 | Multiple radio integrations, advanced opportunistic routing, richer courier statistics | One real transport and physical courier workflow meet their acceptance checks |

### 12.2 Non-goals

| ID | Exclusion | Basis |
| --- | --- | --- |
| N01 | Internet-hosted deployment mode, internet bridge/gateway, automatic internet fallback, cloud-dependent identity or runtime | R04/R09 |
| N02 | Engagement-ranked feeds, paid amplification, pay-to-reach invitations, attention-maximizing product design | R01/R18/R19 |
| N03 | A centrally controlled, uniform network or compulsory global peer bans | R02/R06/R09/R14 |
| N04 | Automatic mesh/Pigeon file-library replication | R08 |
| N05 | Requiring all visitors' phones/laptops to be internet-isolated | R04 and explicit U07 correction |
| N06 | Hosting SysOps automatically reading encrypted groups or acquiring forum-wide moderation authority | R13/R16; post-reference-based local suppression remains available under R16 |
| N07 | Preventing authorized recipients from sharing plaintext or guaranteeing immediate revocation across disconnected nodes | R17 |
| N08 | Mandatory global membership coordination/posting shutdown solely to deliver instant universal exclusion | R17 |
| N09 | Nostalgia fidelity that compromises ordinary modern-device usability | R03 |
| N10 | Designing for massive anonymous parasocial groups as the target community model | R01/U09 |
| N11 | Making a host-business purchase mandatory for ordinary participation, selling social reach through game bonuses, or redeeming host-business codes at a different BBS | R22/U13; game-economy design itself remains open |
| N12 | Treating this spec as approval of every brainstormed feature, a selected technology stack, or a selected license | §1, R23 |

## 13. Development phases

**Planning proposal for this edition.** This sequence is not a release-date commitment. Settled requirements continue to govern every phase; a prototype may implement a subset if its limits are plainly stated. A phase cannot claim an unimplemented feature.

| Phase | Deliverable | Exit evidence |
| --- | --- | --- |
| 0 — Scope and design foundations | This baseline; proposed Tier 2 decisions for local architecture, offline deployment, identity boundaries, event model, and client trust | Requirement/proposal mapping; chosen prototype scope; unresolved choices named |
| 1 — A useful standalone BBS | One modest computer/router; modern UI with vibrant-on-black default; local accounts/directory/profiles, clan/cohort formation/pages/rosters, boards, mail, files, SysOp customization; simple local game and friends/events workflows | AC01–AC04, AC08–AC09, AC20, local portions of AC10/AC21/AC22; usable with WAN absent |
| 2 — Two communities exchanging public messages | Selected public forums/feeds, peer trust, signed-event prototype, offline LAN exchange, Pigeon transport, inspectable queues/outcomes, attributed Lump Drop counts/privacy | AC05–AC07, AC12, AC18; interruption/out-of-order test results |
| 3 — Encrypted groups and practical operation | Approved client/key design, invitations/membership controls, encrypted local/federated forums, post-linked complaints and optional local suppression pending moderator review; backup/recovery/update procedures | AC11, AC13–AC15, AC19; demonstrates accepted revocation limits |
| 4 — Neighborhood mesh pilot | Real offline mesh transport plus Pigeons; several independently operated nodes; nontechnical host/resident trials | Repeat applicable checks under actual disconnection; measured device/link limits |
| 5 — Game integration and optional community expansion | Game Passport/portable characters with local clan/cohort and profile integration; individually approved community modules; possible business reward/game prototypes | AC10, AC21–AC22 and conditional AC16; update scope and acceptance for each added feature |

Prototype sequencing may change through a recorded planning revision. In particular, real mesh transport experiments or encryption research may run earlier without making their unapproved implementation choices canonical. Encrypted forums must not be labeled available until the relevant privacy/authority checks pass.

No numeric latency, uptime, simultaneous-user, storage, battery-life, or radio-range promise exists yet. Phase 1/4 measurements should establish realistic targets for specified hardware and links.

## 14. Acceptance criteria

These are proposed verification scenarios derived from the settled requirements and identified protocol proposal. They define what evidence should be collected; **none has been executed by this document task**. Detailed P06 behavior needs its Tier 2 decision before becoming a final interoperability contract.

| ID | Scenario and pass condition | Trace |
| --- | --- | --- |
| AC01 | Install from prepared local media with WAN absent. Start/restart the node; load UI/assets, authenticate, post/read, use mail/files/game, and administer it. Capture network behavior showing no attempted internet service dependency. Test configured IPv4/IPv6/forwarding boundaries. | R04–R07 |
| AC02 | Operate a single node with no peers. Local services and existing content remain useful throughout disconnection. | R02/R09 |
| AC03 | A fresh installation uses dark mode, black backgrounds, and bright/vibrant/pleasant colors even when the device prefers light mode. A SysOp can customize the theme. Perform ordinary navigation/posting on phone and desktop using touch, mouse, and keyboard, with semantic/text navigation alongside art. | R03/R06 |
| AC04 | Keep a file in node A's local library. Synchronize subscribed forums with B by mesh and Pigeon. The file library is not replicated; local download still works. | R08 |
| AC05 | A and B opt into one shared forum while A has a local-only board and another unselected forum. Only selected, permitted records synchronize. Stopping participation changes future exchange according to the documented local policy. | R09 |
| AC06 | A Pigeon collects at A, travels with no network path, delivers at B, and returns permitted reply data. Tampering is detected; carrier has no private-group decryption privilege. | R10/R11/R15 |
| AC07 | Deliver overlapping packages 108 before 107 and replay them. Preserve unseen valid records, avoid duplicate effects and state rollback; stage missing dependencies; expose conflicting histories; interrupt/retry safely. Use equal history and policies to verify consistent derived state. | R11/R12; proposed P06 |
| AC08 | Deliver posts from selected friends and an invitation. They appear in chronological/social/event views under the documented ordering rule, with no payment or engagement threshold. Late arrival is understandable. | R18/R19 |
| AC09 | A resident creates an event and invites the intended audience. Available recipients can find it; disconnected recipients see it once delivered. The UI does not claim an unseen invitation was read. | R19/R12 |
| AC10 | Play an integrated local game, including a multiplayer example when that phase is delivered. Export/import a supported character/account; a configured GM approval gate is respected, and an unsupported game is not presented as portable. | R20/R21 |
| AC11 | Authorized members on A and B read an encrypted group while hosts/courier store or relay ciphertext. Nonmembers and a hosting-only SysOp cannot gain access merely through their role. Test the actual approved client trust model. | R13/R15 |
| AC12 | Submit/exchange an exposure report. Receiving SysOps can independently contact, restrict, or retain the peer. A report does not cause compulsory global banishment; a cellular-connected visitor alone is not treated as a node violation. | R04/R14 |
| AC13 | A member reports one post and then a set of posts. Each original complaint retains the target references and explanation; the SysOp forwards the complaint reference without decrypting the posts. An authorized moderator follows it to the exact targets, reviews the reason, and responds on the same complaint. The SysOp retains hosting authority without gaining group membership. | R16 |
| AC14 | Remove a member while another node/client is disconnected. Explain its locally known key state and the accepted possibility of stale-key access; reconcile authenticated updates on contact without claiming immediate universal revocation or imposing a blanket global posting halt. | R17 |
| AC15 | Exercise the selected offline backup/restore and verified update workflow. Show what identity/history/key material is preserved, what recovery requires, and how failures are reported. No online recovery service is required. | R04; proposed P04/P09 |
| AC16 | If P20 is approved: redeem an optional purchase-related game code at its issuing BBS; reject duplicate redemption under the game's rules; another BBS is not a redemption authority. Participation/social reach does not depend on buying the bonus. | R22/P20 |
| AC17 | Review a release's source, build/install materials, protocol documentation, license decision, and Tier 1 change history. No claim of a selected license precedes that decision. | R23/R24 |
| AC18 | Deliver a lump with many qualifying events: award one Lump Drop to the delivering user. Deliver one containing duplicates plus one newly accepted relevant change: award one. Deliver an older lump with a newly accepted relevant event: award one. Deliver only duplicates/no-longer-relevant/rejected changes: award zero. Stage unresolved events without premature credit; if a qualifying event is later accepted, credit once. Retry/restart/replay never adds a second credit for that receiving delivery. Verify default profile display, private-count hiding, and continued recording while private. | R25 |
| AC19 | Mark a post-linked complaint as believed illegal. The report alone does not suppress posts. The SysOp can temporarily suppress exactly the referenced posts on their BBS without seeing plaintext; authorized moderators can still retrieve them through the complaint reference and reply. The SysOp can lift suppression or make a further hosting decision. Other BBSes do not inherit a compulsory action. | R16 |
| AC20 | Create private and public accounts. Both appear in the local directory with username/common public fields. Private optional details stay hidden; a public profile displays only chosen optional contact information, bio, short message, and hosted events. Verify the profile's feed link follows feed visibility and the Lump Drop privacy choice is respected. | R25/R26 |
| AC21 | A user chooses whether to display a supported game's name and character information on their profile; changes to that choice are reflected in the display. Game Passport integration supplies the agreed account/character associations. Achievement/progress display is not required unless P24 is adopted. | R21/R26/P17 |
| AC22 | Form a local clan and cohort; find both in the home BBS's directory, open their profiles, and inspect their membership rosters under the chosen visibility rules. A participating game associates relevant local memberships through the Game Passport interface. Ordinary forum synchronization or character transfer does not automatically federate the groups or enroll members at another BBS. | R21/R27/P17 |

For the federation prototype, the proposed test setup is three nodes and two couriers on an isolated network, with missing/duplicate/reordered messages, independent trust choices, and interruption. Equivalent smaller fixtures can establish early behavior; the multi-node test is a phase exit recommendation from the protocol pitch.

## 15. Open decisions and next design documents

Tier 2 documents should answer bounded questions and cite the relevant R/P/AC IDs.

| Document area | Questions to resolve |
| --- | --- |
| Local application/deployment | New implementation versus reuse; first hardware/OS profile; offline address/access setup; installation and service management |
| UI and community interaction | R26 directory/public-field schema and privacy controls; profile/feed and game/event displays; local clan/cohort membership and roster rules; friend/follow model; chronology; event lifecycle; exact vibrant-on-black palette |
| Identity and authorization | User/device/node/forum/courier credentials, authority delegation, introductions, ownership continuity, recovery |
| Event and Pigeon protocol | Canonical encoding, signed bytes/IDs, dependencies, forks, receipts, retention, crash recovery, versioning; Lump Drop attribution, qualifying integration linkage, duplicate-safe accounting, and multi-node total aggregation |
| Encrypted client/groups | Offline secure delivery of client code, browser/native approach, keys/invitations, cryptographic state transitions, practical membership UX |
| Trust and operations | Local diagnostics/reports, operator contact; complaint-to-post references, edited/missing targets, temporary local suppression and authorized moderator retrieval; peer reinstatement, quotas, backup/restore, verified offline updates |
| Game interface | Game lifecycle, local multiplayer, Game Passport format/account transfer, GM acceptance, local clan/cohort association, chosen profile game/character display, optional issuing-node business rewards |
| Licensing | Explicit reciprocal open-source license choice and treatment of project components |

No decision in this list is reopened by default if it is already settled in §§2–10. In particular, offline-only infrastructure, owner-led encrypted moderation, the accepted revocation limitation, and the absence of engagement ranking remain constraints on every solution.

## 16. Change control

**R24 establishes the need to keep scope in this master document.** The workflow below is the initial operational process proposed to implement that requirement.

### 16.1 Document hierarchy

- **Tier 1 — This document:** mission, settled scope, proposals, non-goals, phase gates, acceptance criteria, and decision/change register.
- **Tier 2 — Design documents/ADRs:** architecture, protocols, UI flows, operational procedures, game interfaces, and selected implementation choices.
- **Tier 3 — Execution:** issues, implementation plans, code, tests, and release notes.

If a Tier 2/3 artifact conflicts with Tier 1, revise the lower-tier artifact or explicitly approve and record a Tier 1 change. A prototype is evidence, not automatic scope approval.

### 16.2 Status changes

A new idea enters as Proposed. The product owner may approve it as Settled, defer it, or reject it. A contributor or assistant may propose those transitions but must not attribute approval to silence, a fictional participant, or its own prior suggestion.

Existing IDs remain stable. When a requirement is superseded or removed, retain its ID and a link to the replacing decision; do not reuse it.

### 16.3 Change proposal record

Each material scope change records:

| Field | Required content |
| --- | --- |
| Change ID/title | Unique identifier and concise description |
| Problem/use case | Who benefits and how it serves the mission |
| Source/authority | User decision, research evidence, implementation result, or clearly labeled proposal |
| Current → proposed status | Affected R/P/D/N IDs and requested transition |
| Scope | Behavior added, removed, or changed; core versus optional/game/extension |
| Impact | Offline operation, local autonomy, UI, identity/privacy, transport, compatibility, hardware, and dependencies as relevant |
| Acceptance | Updated AC IDs/scenarios and phase implications |
| Decision | Approved/rejected/deferred, by whom, when, and why |
| Repository reference | Commit or PR containing the decision and document changes |

Use only relevant impact fields; this process is for concrete decisions, not manufacturing hypothetical objections.

### 16.4 Repository workflow

1. Capture a scoped proposal in an issue, PR, or design note.
2. Prepare the Tier 1 edit and affected Tier 2/acceptance changes together.
3. Obtain the product owner's explicit scope decision when promoting/changing requirements. Routine clarifications that preserve scope may be maintained editorially.
4. Commit the updated master document with its version and changelog.
5. Link implementation work to the requirement/proposal decision and acceptance evidence.
6. For a release, state which phase/capabilities are actually delivered and which remain pending.

Repository updates authorized by U15–U17 record the specified scope decisions; committing the document does not approve every remaining proposal inside it. Future approval authority may be delegated explicitly by the product owner.

### 16.5 Version policy

Document versions are independent of software and protocol versions.

- **Patch** (0.1.0 → 0.1.1): wording, source corrections, and clarifications with no scope change.
- **Minor** (0.1.0 → 0.2.0): approved additions, removals, status transitions, or material phase/acceptance changes within the mission.
- **Major** (1.x → 2.0, or explicit pre-1.0 rebaseline): changes to foundational product boundaries or the document's authority model.

Version 0.1.0 was the initial consolidation; version 0.2.0 was the source-audited revision; version 0.3.0 adds the explicitly requested community, delivery-credit, moderation, and theme capabilities. These are document versions, not protocol or product releases, completion claims, or declarations that all open decisions are resolved. Git history preserves earlier versions; use tags for deliberate baselines when needed.

## 17. Decision corrections and change history

### 17.1 Corrections preserved from the conversation

| Earlier idea/ambiguity | Current treatment |
| --- | --- |
| Offline rule could be interpreted to ban internet-connected visitors | Infrastructure remains internet-isolated; visitors' devices may have independent connectivity (U07) |
| Hosting SysOp could be responsible for private-group moderation | Owner and appointed moderators govern; SysOp controls hosting and contacts them (U07) |
| Revocation might require stopping disconnected groups until global agreement | Immediate universal exclusion is not promised; owners manage the accepted stale-credential risk (U09) |
| Reject an entire older Pigeon delivery after a newer stamp | Later direction preserves valid missing events without rollback (R12); the detailed reconciliation implementation remains P06 |
| Classic BBS inspiration might imply terminal-era UX | Modern device interaction is foundational; retro aesthetic and customization remain available (U12) |
| Business rewards might require cross-node coupon redemption or platform paid reach | Issuing BBS redeems; optional bonuses affect the game and support its local host (U13) |
| Assistant suggestions/roundtable conclusions might become canon automatically | Read them as brainstorming, following ideas through later adoption/correction; source and status remain explicit (U13–U16) |
| Initial spec broadened local file-library scope into an attachment restriction | File libraries remain local; a general message-attachment policy was not settled |
| Initial spec listed a universal game economy as excluded | Remove the unsupported exclusion; individual game economy/portability design remains open |

### 17.2 Changelog

| Version | Date | Change |
| --- | --- | --- |
| 0.1.0 | 2026-10-25 | Initial Tier 1 consolidation from the original discussion and specification request. Established R01–R24, P01–P22, D01–D07, N01–N12, AC01–AC17; preserved user corrections; added a proposed phased plan and versioned change-control process. |
| 0.2.0 | 2026-10-25 | Audited all 16 source turns against the supplied export. Clarified later delivery/trust conclusions and brainstorm provenance; removed unsupported attachment/game-economy restrictions; restored omitted proposal details; added dated source evidence. No technology, license, optional module, or release schedule was newly selected. |
| 0.3.0 | 2026-10-25 | Integrated U17: added R25 Lump Drops, R26 account directory/profiles, R27 local clans/cohorts; amended R03 dark default, R16 post-linked review/suppression, and R21 Game Passport integration. Updated phase mapping, AC03/AC13, added AC18–AC22, and recorded P23/P24 future possibilities. |

### 17.3 Scope decision log

| Decision | State |
| --- | --- |
| Produce and store this master specification in BBSBOX | Authorized by U14–U15 |
| Preserve settled mission/infrastructure/privacy/social/game boundaries | Consolidated in R01–R24 |
| Select a software license, technology stack, wire protocol, or encryption library | **Not decided** |
| Approve every proposed community/game feature or the full phase schedule | **Not decided** |
| Claim implemented behavior or completed acceptance testing | **Not applicable: documentation-only baseline** |
| U17 additions: Lump Drop accounting/privacy, targeted complaints and local suppression, profiles/directories, local clans/cohorts, Game Passport integration, vibrant dark default | Approved by Michael on 2026-10-25; R03/R16/R21 amended and R25–R27 added |
| Lump Drop reward tools; achievement/progress display and unspecified additional profile/group features | P23/P24 remain proposed; count-only Lump Drop scope is current |

### 17.4 Export audit — 2026-10-25

**Audit source:** `ChatGPT-Build A Local BBS-20261025-0035.md`, exported 2026-10-25 at 00:35:39 as labeled by the exporter. SHA-256: `7df8033d14e6d1073fde1364183b9bbc4b79d77ad4978c2be52a16f8cbfbfd54`.

All 16 user turns and their assistant responses were read directly. The exported timestamps below are reproduced as source labels. Line references identify this exact exported file; it is not copied into the public repository. Visual illustration payloads and website-search results are not evidence of product approval.

| Source point | Export evidence | Result in this specification |
| --- | --- | --- |
| Local hardware and permanent communities | 2026-10-23 22:44 and 22:56; lines 9–163 | R01–R05 retain modern local access and permanent-neighborhood priority |
| Customization, selected federation, local files, encryption | 2026-10-23 23:01; lines 329–333 | R06/R08/R09/R15; no extra general attachment prohibition |
| Pigeon and internet isolation | 2026-10-23 23:09–23:17; lines 486–779 | R04/R10/R11; individual implementation profiles remain labeled |
| Simulated discussion | 2026-10-23 23:46; lines 1081–2154 | Brainstorm source for proposal registers; follow ideas into later user conclusions |
| Clients, trust, moderation, sequence stamps | 2026-10-24 00:28; lines 2156–2164, response through 2540 | R04/R11/R14/R16; infrastructure/client distinction and local hosting/trust authority |
| Event/identity/reconciliation pitch | 2026-10-24 15:15; lines 2542–3046 | Later delivery principles in R12; exact format, algorithms, libraries, and namespaces remain P06/P07/P09 |
| Removed-member access limitation | 2026-10-24 15:35; lines 3048–3117 | R17/N07/N08 take precedence over earlier coordinator/posting-halt ideas |
| Later foundation recap and open choices | 2026-10-24 22:24; lines 3118–3249 | Confirms the later conversational direction; recovery, retention, client details, and extensibility remain design proposals |
| Naming discussion | 2026-10-24 23:30–23:32; lines 3250–3456 | R24; no renewed naming investigation or adoption of unrelated projects' features |
| Modern mission, social experience, games | 2026-10-24 23:54; lines 3457–3722 | R01/R03/R18–R23; tentative cross-node/game ideas retain tentative status |
| Business bonuses and objection to invented caveats | 2026-10-25 00:10; lines 3723–3772 | R22/N11; issuing-node redemption and voluntary bonuses; remove unsupported restrictions |
| Final specification request | 2026-10-25 00:34; lines 3773–3806 | R24; versioned Tier 1 scope reference |

**Audit conclusion:** the core mission, offline infrastructure, SysOp autonomy, encrypted-forum responsibility, social visibility, and game direction were already represented. This revision corrects overreach and restores missing brainstorm details without promoting them to mandatory features. Pigeon chronology now separates the resulting no-loss/no-rollback direction from the still-proposed exact protocol. Existing R/P/D/N/AC identifiers remain stable.

**Recorded decision:** the product owner's U16 audit request authorizes correcting the specification against the export. This revision does not resolve the open technology, license, optional-feature, or scheduling choices.

### 17.5 CR-001 — Community participation and delivery credit

| Field | Record |
| --- | --- |
| Decision | Approved by Michael's explicit U17 request, 2026-10-25; incorporated in version 0.3.0 |
| Purpose | Recognize useful physical deliveries, make complaints actionable without host decryption, help local members find each other and organize/play, and establish the shipped visual style |
| Scope changes | R03/R16/R21 amended; R25–R27 added; profile/directory capability promoted out of P03 and Game Passport capability adopted while P17 retains format/interface choices |
| Future ideas | P23: discretionary SysOp rewards; P24: game achievements/progress and unspecified additional profile/group features |
| Impacts | Receiver integration/receipt accounting, user attribution and profile privacy, post-reference-based host controls with authorized moderator review, local group/game associations, and default theme |
| Acceptance and phases | AC03/AC13 updated; AC18–AC22 added; proposed phase mapping updated in §13 |
| Repository record | The Git commit introducing version 0.3.0 of this file records this decision and its exact changes |
