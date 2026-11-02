# SPDX-License-Identifier: AGPL-3.0-or-later
"""Material/provenance refusal tests; dependency text is explicitly synthetic fixture data."""
import hashlib
import io
import json
from pathlib import Path
import subprocess
import tarfile
import tempfile
import unittest
import release_materials as materials


class Materials(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name).resolve()
        self.repo = self.root/'repo'
        self.repo.mkdir()
        self.vendor = self.root/'vendor'
        package = self.vendor/'fixture-1.0.0'
        package.mkdir(parents=True)
        files = {
            'Cargo.toml': b'[package]\nname="fixture"\nversion="1.0.0"\nlicense="MIT"\n',
            'src/lib.rs': b'// synthetic fixture\npub fn test() {}\n',
            'LICENSE': b'Synthetic license test data, not a real dependency grant.\n',
        }
        for name, raw in files.items():
            target = package/name
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(raw)
        (package/'.cargo-checksum.json').write_bytes(materials.encoded({'$comment': 'Cargo metadata only; release signatures supply authenticity.', 'package': '11'*32, 'files': {name: materials.digest(raw) for name, raw in files.items()}}))
        (self.repo/'Cargo.lock').write_text('version=4\n[[package]]\nname="fixture"\nversion="1.0.0"\nsource="'+materials.REGISTRY+'"\nchecksum="'+'11'*32+'"\n', encoding='utf-8')
        support = self.repo/'deploy/materials'
        support.mkdir(parents=True)
        (support/'supplements.json').write_bytes(materials.encoded({'format': 'bbsbox.license-supplements/v1', 'files': {}, 'packages': {}}))
        (self.repo/'hello.txt').write_bytes(b'committed source\n')
        self.git('init', '--quiet')
        self.git('add', '.')
        self.git('-c', 'user.name=BBSBOX fixture', '-c', 'user.email=fixture@example.invalid', 'commit', '--quiet', '-m', 'fixture')
        self.commit = self.git('rev-parse', 'HEAD').decode().strip()
        self.output = self.root/'materials'

    def git(self, *arguments):
        return subprocess.check_output(['git', '-C', str(self.repo), *arguments], stderr=subprocess.PIPE)

    def build(self):
        return materials.build(self.repo, self.commit, self.vendor, self.output)

    def rewrite(self, archive, transform):
        limits = (materials.SOURCE_FILES+1, materials.SOURCE_MEMBER, materials.SOURCE_TOTAL) if archive.name == 'source.tar' else (materials.DEPENDENCY_FILES, materials.DEPENDENCY_MEMBER, materials.DEPENDENCY_TOTAL)
        entries = list(materials.tar_entries(archive, *limits))
        with tarfile.open(archive, 'w', format=tarfile.USTAR_FORMAT) as target:
            for name, raw, mode in transform(entries):
                materials.add(target, name, raw, mode)

    def test_committed_source_full_vendor_and_notices_round_trip_without_worktree_leaks(self):
        (self.repo/'hello.txt').write_bytes(b'uncommitted secret must not travel\n')
        (self.repo/'private.key').write_bytes(b'untracked secret must not travel\n')
        result = self.build()
        self.assertEqual(result['dependencies'], 1)
        self.assertFalse(result['installation_authority'])
        extracted = self.root/'extracted'
        materials.extract(self.output, self.commit, extracted)
        self.assertEqual((extracted/'source/hello.txt').read_bytes(), b'committed source\n')
        self.assertFalse((extracted/'source/private.key').exists())
        self.assertEqual((extracted/'vendor/fixture-1.0.0/src/lib.rs').read_bytes(), (self.vendor/'fixture-1.0.0/src/lib.rs').read_bytes())
        with self.assertRaises(FileExistsError): self.build()
        with self.assertRaises(FileExistsError): materials.extract(self.output, self.commit, extracted)

    def test_source_commit_tree_and_blob_mismatches_fail(self):
        self.build()
        with self.assertRaises(ValueError): materials.verify(self.output, '00'*20)
        self.rewrite(self.output/'source.tar', lambda items: [(name, b'altered source' if name=='source/hello.txt' else raw, mode) for name, raw, mode in items])
        with self.assertRaises(ValueError): materials.verify(self.output, self.commit)
        with self.assertRaises(ValueError): materials.extract(self.output, self.commit, self.root/'bad-extract')
        self.assertFalse((self.root/'bad-extract').exists())

    def test_missing_source_member_and_forged_git_object_are_rejected(self):
        self.build()
        self.rewrite(self.output/'source.tar', lambda items: [entry for entry in items if entry[0]!='source/hello.txt'])
        with self.assertRaises(ValueError): materials.verify(self.output, self.commit)
        proof = materials.source_proof(self.repo, self.commit)[1]
        sha = next(iter(proof['trees']))
        proof['trees'][sha] += '00'
        with self.assertRaises(ValueError): materials.proof_files(proof, self.commit)

    def test_altered_vendor_and_forged_notices_are_rejected(self):
        self.build()
        notices = self.output/'notices.json'
        original = notices.read_bytes()
        notices.write_bytes(original.replace(b'"MIT"', b'"GPL-3.0"'))
        with self.assertRaises(ValueError): materials.verify(self.output, self.commit)
        notices.write_bytes(original)
        self.rewrite(self.output/'dependencies.tar', lambda items: [(name, b'tampered dependency' if name.endswith('/src/lib.rs') else raw, mode) for name, raw, mode in items])
        with self.assertRaises(ValueError): materials.verify(self.output, self.commit)

    def test_vendor_missing_package_checksum_or_reviewed_license_refuses_preparation(self):
        (self.vendor/'fixture-1.0.0/LICENSE').unlink()
        with self.assertRaises(ValueError): self.build()
        self.assertFalse((self.output/'notices.json').exists())

    def test_reviewed_supplement_is_bound_to_committed_text_and_grant(self):
        package = self.vendor/'fixture-1.0.0'
        (package/'LICENSE').unlink()
        checksum = json.loads((package/'.cargo-checksum.json').read_bytes())
        del checksum['files']['LICENSE']
        (package/'.cargo-checksum.json').write_bytes(materials.encoded(checksum))
        support = self.repo/'deploy/materials'
        (support/'licenses').mkdir()
        text = b'Synthetic supplementary test license\n'
        (support/'licenses/MIT.txt').write_bytes(text)
        (support/'supplements.json').write_bytes(materials.encoded({'format':'bbsbox.license-supplements/v1', 'files':{'MIT.txt':{'sha256':materials.digest(text),'source':'https://example.invalid/fixture'}},'packages':{'fixture-1.0.0':{'choice':'MIT','files':['MIT.txt']}}}))
        self.git('add', '.')
        self.git('-c','user.name=BBSBOX fixture','-c','user.email=fixture@example.invalid','commit','--quiet','-m','supplement fixture')
        self.commit = self.git('rev-parse','HEAD').decode().strip()
        self.build()
        self.rewrite(self.output/'dependencies.tar', lambda items: [(name,b'wrong grant text' if name.startswith('supplemental-licenses/') else raw,mode) for name,raw,mode in items])
        with self.assertRaises(ValueError): materials.verify(self.output,self.commit)

    def test_tar_extensions_links_traversal_duplicates_truncation_and_oversized_headers_fail(self):
        archive = self.root/'bad.tar'
        for name, kind in [('../escape',tarfile.REGTYPE),('safe',tarfile.SYMTYPE),('safe',tarfile.XHDTYPE)]:
            info=tarfile.TarInfo(name);info.type=kind;info.size=0
            archive.write_bytes(info.tobuf(format=tarfile.USTAR_FORMAT)+bytes(1024))
            with self.assertRaises(ValueError): list(materials.tar_entries(archive,10,1000,1000))
        with tarfile.open(archive,'w',format=tarfile.USTAR_FORMAT) as target:
            materials.add(target,'same',b'a');materials.add(target,'same',b'b')
        with self.assertRaises(ValueError): list(materials.tar_entries(archive,10,1000,1000))
        info=tarfile.TarInfo('large');info.size=1001
        archive.write_bytes(info.tobuf(format=tarfile.USTAR_FORMAT))
        with self.assertRaises(ValueError): list(materials.tar_entries(archive,10,1000,10000))
        archive.write_bytes(bytes(512)+bytes(20))
        with self.assertRaises(ValueError): list(materials.tar_entries(archive,10,1000,1000))
        self.assertFalse((self.root/'escape').exists())


if __name__ == '__main__': unittest.main()
