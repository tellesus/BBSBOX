# Independent Windows client release v1

Implementation reference for M00 B/M13 under C02 v1.4.1, C05 v1.1.4 and T07/T08. Product authority remains in the [roadmap](../docs/development/README.md). [Manifest](../crates/maintenance/src/client_release.rs), [payload](../crates/maintenance/src/client_payload.rs) and [installation state](../crates/maintenance/src/client_install.rs) reuse the maintenance library. The Debian node release profile is unchanged.

## Signed package

The canonical COSE_Sign1 uses protected Ed25519 algorithm `-19` and the 32-byte issuer public key, an empty unprotected map, embedded canonical JSON and a 64-byte signature. External authenticated data is the UTF-8 domain `bbsbox.client-release/v1`. The existing bounded COSE preflight and strict signature verifier apply. The envelope is at most 65,664 bytes; its SHA-256 is the release ID.

JSON fields occur in this order: `format`, `issuer`, `sequence`, `version`, `target`, `source_commit`, `vault_profiles`, `files`, `next_key`. Unknown fields and noncanonical encodings fail. `format` equals the domain; issuer uses the existing release-key fingerprint. Sequence is positive and at most `i64::MAX`; version is 1–64 ASCII letters, digits or `._-+`. Target is `windows-x86_64-msvc`, source commit is 40 lowercase hexadecimal characters and this initial profile supports exactly vault profile `[1]`.

Each sorted file record contains `path`, `size`, `sha256`. Names use the existing short ASCII inventory rules, reject traversal/reserved Windows names, and cannot alias files or directories through case changes. `manifest.cose` cannot inventory itself. The `materials/` directory is reserved for the three named rebuild-only files; executable/runtime additions belong outside it. A package has at most 128 files, at most 2 GiB per file and at most 4 GiB total. Required nonempty files are:

- `bbsbox-client.exe`
- `LICENSE.md`
- `LICENSES/AGPL-3.0-or-later.txt`
- `LICENSES/CC-BY-SA-4.0.txt`
- `materials/source.tar`
- `materials/dependencies.tar`
- `materials/notices.json`

Inspection rejects linked/reparse entries and ancestors, missing/unlisted files and changed bytes. The entry must have an executable, non-DLL x86-64 PE32+ header. Exact signature/hash verification authenticates the issuer's bytes; the PE check alone is a compatibility preflight.

## Trust and installation

The bootstrap and initial release key must arrive through an independently trusted path. A node login, a node key or a public key supplied inside the delivered package cannot enroll trust automatically. This implementation proves the package-verification mechanism using a locally built bootstrap and synthetic fixture authority. It does not establish OS signing, production issuer-key custody, first-install distribution or supported mobile platforms.

An explicit local store keeps public release control separate from person keys and node backups. It uses the M02 writer lock, transactions, conditional revision, audit digest and persistent installation receipts. Setup refuses replacement of initialized trust. The store retains the highest accepted sequence across restart and signing-key rotation.

Preparation verifies the source package, copies it to a fresh private stage, re-verifies the complete copy and moves it to its release-ID directory. It then atomically records the pending request and advances the sequence floor. Failed writes leave an inactive copy; retries can reuse a matching verified copy. Interrupted preparation is resumed from its persisted request. A pending installation blocks launch.

Activation re-verifies the staged release and atomically publishes its selection and any authorized successor key. Historical request retries preserve the current selection. Older/equal sequences, wrong authorities and reused request IDs fail. Launch verifies the signed manifest, complete inventory and all runtime bytes before returning the fixed executable path. The three reserved material files retain name/type/size checks at launch; their byte hashes are verified during preparation, activation, full audit and material extraction/build. They are never runtime code inputs. This keeps a large offline source bundle out of the startup hashing path. Old releases remain retained; automatic pruning, repair and client-vault migration/recovery coordination are later work. This local trust store does not provide protection against a local attacker able to replace the bootstrap or restore the entire trust directory.

## Offline source and dependency materials

[`client_materials.py`](../tools/client_materials.py) verifies the complete committed Git source using the existing object/tree/blob proof. It includes both root and native Cargo lockfiles and validates their combined vendored package set, per-file checksums, licence files and notices. Client-specific supplements live under `deploy/client-materials/`; appliance supplements remain separate. Their upstream revisions and hashes are recorded. The native UI is embedded directly, so its build needs no npm package bundle. The compiler, SDK, OS and WebView2 are prepared separately.

Prepared material archives and notices have their own `bbsbox.client-materials/v1` profile. Changing their target, source proof, package set, checksums or notices fails. A source archive is not installation authority; only a release signed by the already enrolled issuer can advance client selection. SQL source files use LF checkout to keep compiled migration digests consistent with the verified source archive.

## Fixtures and remaining gate evidence

The [public byte vector](fixtures/client-release-v1.json) is checked independently in Rust and [Node crypto](../tools/verify_client_release_vectors.mjs), including exact canonical bytes, signer fingerprint, the signed 64-bit sequence limit and cross-domain rejection. Its seed and file commitments are explicitly synthetic.

Rust fixtures cover authority/domain separation, canonical bytes, paths, payload changes, PE targets, single-writer exclusion, restart, request retries, rollback fencing, key rotation and transactional failure. Windows checks include a real directory junction. The `client_package` example is a developer bootstrap/fixture, not a production issuer UI. Its `fixture-package` command generates an ephemeral test signer without saving its private key.

The [actual distribution check](../spikes/m00/native/check_distribution.py) verifies prepared materials, packages the real bundled executable, enrolls the independent fixture authority, resumes an interrupted install, and runs the existing six-process identity/recovery check through the installed bootstrap. It also tests changed installed bytes and independence from later transport changes. Passing this fixture is narrower than G2: production trust enrollment, independent delivery of the bootstrap, native update UI, pinned node TLS/pairing, actual MLS-state integration and supported-device acceptance remain open.
