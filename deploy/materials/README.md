# Offline release materials

Implementation reference for [M12](../../docs/development/modules/M12-maintenance-recovery.md); scope authority remains the maintained roadmap and current user instructions.

[The material tool](../../tools/release_materials.py) produces `source.tar`, `dependencies.tar` and `notices.json` for a prepared offline release. It performs no network request, signing, installation, service change or node-data access. Release signatures and the installed trust/sequence policy remain separate prerequisites. This provider is not a production release.

Prepare dependencies on a development/build machine using [Cargo vendor](https://doc.rust-lang.org/cargo/commands/cargo-vendor.html), with its locked versioned directories. Add `--offline` when all sources are already cached. Fetching build dependencies belongs to preparation, never to the appliance.

```sh
cargo vendor --locked --versioned-dirs work/release-vendor
python3 tools/release_materials.py build --commit HEAD --vendor work/release-vendor --output work/release-materials
python3 tools/release_materials.py verify --materials work/release-materials --source-commit COMMIT_SHA
python3 tools/release_materials.py extract --materials work/release-materials --source-commit COMMIT_SHA --output work/release-extracted
```

Use the exact commit ID reported by `build`. Outputs must be fresh. The builder reads committed Git objects rather than the working tree; untracked and uncommitted files are excluded. `source-proof.json` contains the raw commit/tree objects. Verification matches their Git IDs, walks their exact file/mode set and verifies every archived blob. Git SHA-1 IDs identify this repository's existing commits; release file commitments use SHA-256. Linked files/submodules and unsupported paths are refused. No `.git` directory or private working files travel with the source.

Dependencies are all registry packages in the committed root `Cargo.lock`, including target-specific and disabled-feature sources. Their versioned source trees, Cargo metadata/checksum files and every supplied license/notice stay intact. Verification requires the exact package/file set, package checksum identity and file hashes recorded by Cargo. Cargo's checksum catalogue alone supplies no authenticity against malicious replacement: the trusted preparation step and outer signed release commitments provide that boundary. Git dependency support requires a future explicit adapter rather than silently omitting sources.

`notices.json` records exact package identities/checksums, declared license expressions, authors/repositories, retained license/notice paths and hashes, and reviewed supplements. Nine currently locked packages omit standalone license texts. [The supplement registry](supplements.json) associates their existing license grants with recorded upstream texts: the standard Apache-2.0 text or the matching upstream MIT text. The copied text hashes must match committed source. Other packages retain their own supplied texts; embedded/native third-party sources and notices are included in full. Upgrading/removing supplemented packages requires reviewing this registry; stale/missing entries fail.

Both archives use bounded regular-file USTAR members, without links, special files, extension headers or generic unpacking. Source limits are 2,048 files, 8 MiB per member and 64 MiB total; dependencies allow 100,000 files, 128 MiB per member and 2 GiB total. Metadata is bounded to 8 MiB. Paths fit 255 UTF-8 bytes and 32 components. These are initial preparation limits, not appliance capacity claims. Current vendor sources contain 14,540 files, approximately 347 MB across 259 packages. Reading small files uses bounded chunks rather than allocating the maximum member size for each read.

Extraction requires operator-controlled material/destination ancestors throughout. It verifies before creating output, revalidates the exact bytes it copies, preserves a `.materials-incomplete` marker on failure and completes only after all sources, dependency checks and notices match. The copied materials are public source/build inputs, not server/client secrets. Their extraction does not activate an executable or recovered node.

The included `offline-build.txt` describes rebuilding with the pinned Rust toolchain and prepared Debian/native compiler dependencies. The toolchain/base OS are prepared separately. [The Linux CI fixture](../ci/rebuild_release.py) runs a full build in its own loopback-only network namespace, with fresh target/cache directories and direct pinned compiler binaries, then compares the rebuilt executable's compiled profile with the candidate's. This checks a working source/dependency rebuild and registry agreement; it does not claim byte-identical binaries. CI saves package/material hashes, notices and rebuild evidence. The actual packaged rebuild passes [Linux CI](https://github.com/tellesus/BBSBOX/actions/runs/41386542082), with empty cache/target directories, isolated loopback-only networking and matching candidate/rebuilt profiles.

Seven portable tests cover committed-source exclusion, complete round trips, altered/missing source objects/blobs, dependency/checksum/notices mismatch, supplementary license binding and malicious/truncated/oversized tar headers. Actual local vendor content/license checks pass for all 259 packages; the full packaged Linux rebuild also passes the linked CI. Signed candidate preparation and the real update lifecycle remain M12 work.
