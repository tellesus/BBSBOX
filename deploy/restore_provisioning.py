# SPDX-License-Identifier: AGPL-3.0-or-later
"""Prepare restored identity, configuration and data for a fresh signed appliance."""
from dataclasses import dataclass
import os
from pathlib import Path
import re
import shutil
import stat
import time
import unicodedata

import install_development as base
import enrollment
import local_tls
import release_candidate as candidate


@dataclass(frozen=True)
class Request:
    archive: Path
    identity: Path
    node: str
    source: str
    reason: str
    minimum_sequence: int
    game_node: bool = False

    def validate(self):
        if not re.fullmatch(r'node:[0-9a-f]{64}',self.node) or self.source not in ('lost','retired'):
            raise ValueError('Provide the saved node ID and explicitly declare the source lost or retired')
        if not self.reason.strip() or len(self.reason.encode())>512 or any(unicodedata.category(c)=='Cc' for c in self.reason):
            raise ValueError('Provide a reason up to 512 UTF-8 bytes without controls')
        if type(self.minimum_sequence) is not int or not 1<=self.minimum_sequence<2**64:
            raise ValueError('Provide the independently known minimum release sequence')
        for path in [self.archive,self.identity]:
            if path!=path.resolve(strict=True) or not stat.S_ISREG(path.lstat().st_mode):raise ValueError('Use absolute unlinked regular recovery inputs')
        key=self.identity.stat()
        if key.st_uid!=0 or key.st_mode&0o077 or self.archive.samefile(self.identity):raise ValueError('Use a separate root-private backup identity')


def run(arguments):
    return candidate.materials.decoded(candidate.bounded_run(arguments,timeout=1200))


def prepare(request,executable,verified,work):
    if verified['sequence']<request.minimum_sequence:
        raise ValueError('Prepared release is below the independently supplied minimum sequence')
    restored=work/'restored'
    command='restore-game-node-and-record' if request.game_node else 'restore-compatible'
    evidence=run([executable,command,'--archive',request.archive,'--node',request.node,'--identity-file',request.identity,'--output',restored,'--staging',work,'--source',request.source,'--reason',request.reason,'--at',str(int(time.time()))])
    if evidence.get('node')!=request.node:
        raise ValueError('Unexpected isolated recovery result')
    if request.game_node:
        if evidence.get('format')!='bbsbox.game-node-recovery-record/v1' or evidence.get('state')!='recovery-required' or evidence.get('game_checkpoints')!=1:
            raise ValueError('Expected a complete fenced game restore')
        evidence['migration_required']=False
    elif evidence.get('format')!='bbsbox.compatible-continuation/v1' or evidence.get('isolated_continuation_authorized') is not True or evidence.get('installed') is not False or type(evidence.get('migration_required')) is not bool:
        raise ValueError('Unexpected isolated recovery result')
    configuration=restored/'configuration';data=restored/'data'
    decision=configuration/'maintenance/recovery.cose'
    if candidate.materials.file_digest(decision)!=evidence.get('decision_sha256') or not re.fullmatch('[0-9a-f]{64}',evidence.get('inventory_sha256','')):
        raise ValueError('Recovery evidence does not match the staged decision')
    if (configuration/'.network-prepare-incomplete').exists():raise ValueError('The snapshot contains incomplete network preparation')
    settings=enrollment.read_json(configuration/'node.json')
    previous=enrollment.read_json(configuration/'installation.json')
    ca=configuration/'tls/ca.pem';ca_key=configuration/'tls/ca-key.pem'
    public=enrollment.export(configuration/'installation.json',configuration/'node.json',ca,work/'source-enrollment')
    allowed={'format','name','bind','origin','database','certificate','certificate_key','additional_binds','additional_origins'}
    if request.game_node:allowed.add('game_origin')
    if public['node_id']!=request.node or settings.get('database')!='/var/lib/bbsbox/node.sqlite' or set(settings)-allowed:
        raise ValueError('Recovery requires matching installed appliance configuration')
    port=int(settings['origin'].rsplit(':',1)[1]);origin=f'https://localhost:{port}'
    # Reuse the original CA, and issue a local leaf for the replacement's loopback bootstrap.
    leaf=local_tls.issue(ca,ca_key,[public['hostname'],'localhost','127.0.0.1','::1'],work)
    relative=Path('tls')/('recovered-'+leaf['serial'])
    leaf_dir=work/'replacement-leaf';leaf_dir.mkdir(mode=0o700)
    (leaf_dir/'server.pem').write_bytes(leaf['certificate']);(leaf_dir/'server-key.pem').write_bytes(leaf['key'])
    settings.update(bind=f'127.0.0.1:{port}',origin=origin,certificate='/etc/bbsbox/'+(relative/'server.pem').as_posix(),certificate_key='/etc/bbsbox/'+(relative/'server-key.pem').as_posix())
    for key in ['additional_binds','additional_origins','game_origin']:settings.pop(key,None)
    # Migration preflight touches only the disposable restored copy, before any installed state exists.
    if not request.game_node:
        migrated=run([executable,'migrate-existing','--database',data/'node.sqlite','--node',request.node])
        if migrated.get('format')!='bbsbox.migration-existing/v1' or migrated.get('node')!=request.node or migrated.get('listener_opened') is not False:
            raise ValueError('Unexpected recovery migration result')
        check_closed_data(data)
    return Prepared(configuration,data,settings,previous,relative,leaf_dir,evidence)


def copy_new(source,target,mode,uid,gid):
    if not stat.S_ISREG(source.lstat().st_mode):raise ValueError('Expected a regular restored file')
    with source.open('rb') as incoming,target.open('xb') as outgoing:
        shutil.copyfileobj(incoming,outgoing,length=1024*1024)
        outgoing.flush();os.fchown(outgoing.fileno(),uid,gid);os.fchmod(outgoing.fileno(),mode);os.fsync(outgoing.fileno())


def check_closed_data(data):
    names={p.name for p in data.iterdir()}
    if not {'node.sqlite','node.sqlite.blobs'}<=names or names-{'node.sqlite','node.sqlite.writer.lock','node.sqlite.blobs'}:
        raise ValueError('Expected closed, checkpointed restored storage without SQLite sidecars')
    if not stat.S_ISREG((data/'node.sqlite').lstat().st_mode) or not stat.S_ISDIR((data/'node.sqlite.blobs').lstat().st_mode):
        raise ValueError('Unexpected restored core storage type')


@dataclass(frozen=True)
class Prepared:
    configuration: Path
    data: Path
    settings: dict
    previous: dict
    leaf: Path
    leaf_source: Path
    evidence: dict

    def place_game(self,layout,uid,gid,release,request,work):
        source=self.data.parent/'games/dungeon-demo'
        if set(path.name for path in source.iterdir())!={'state.sqlite'}:
            raise ValueError('Unexpected bundled game checkpoint layout')
        target=layout.root/'var/lib/bbsbox-game'
        target.mkdir(mode=0o700)
        save=target/'state.sqlite'
        copy_new(source/'state.sqlite',save,0o600,uid,gid)
        os.chown(target,uid,gid)
        base.run(['runuser','-u','bbsbox-game','--',str(release/candidate.GAME_WORKER),'--verify-state',str(save)],timeout=30)
        for directory in (target,target.parent):
            descriptor=os.open(directory,os.O_RDONLY|os.O_DIRECTORY|os.O_NOFOLLOW)
            try:os.fsync(descriptor)
            finally:os.close(descriptor)
        result=run([release/'bbsbox-node','finish-game-placement','--archive',request.archive,'--node',request.node,
                    '--identity-file',request.identity,'--staging',work,'--staged',self.data.parent,'--placed',target,
                    '--decision-sha256',self.evidence['decision_sha256']])
        if (result.get('format')!='bbsbox.game-node-placement/v1' or result.get('node')!=request.node
                or result.get('inventory_sha256')!=self.evidence['inventory_sha256']
                or result.get('decision_sha256')!=self.evidence['decision_sha256']
                or result.get('isolated_writer_ready') is not True or result.get('installed') is not False):
            raise ValueError('Game state placement did not release the staged core')
        check_closed_data(self.data)

    def populate(self,layout,uid,gid):
        check_closed_data(self.data)
        for source in sorted(self.data.rglob('*')):
            relative=source.relative_to(self.data);target=layout.data/relative
            if relative==Path('node.sqlite.writer.lock'):continue
            if stat.S_ISDIR(source.lstat().st_mode):
                target.mkdir(mode=0o700);os.chown(target,uid,gid)
            else:copy_new(source,target,0o600,uid,gid)
        for source in sorted(self.configuration.rglob('*')):
            relative=source.relative_to(self.configuration)
            # The encrypted source archive retains its prior generated network files. The new OS provisions them afresh.
            if relative.parts[0]=='network' or relative.as_posix() in ('installation.json','node.json','node.network.json'):continue
            target=layout.config/relative
            if stat.S_ISDIR(source.lstat().st_mode):
                accessible=relative in self.leaf.parents or relative==self.leaf
                target.mkdir(mode=0o700)
                if accessible:
                    os.chown(target, 0, gid)
                    target.chmod(0o750)
            else:
                mode,group=(0o644,gid) if relative==Path('tls/ca.pem') else (0o600,0)
                copy_new(source,target,mode,0,group)
        leaf=layout.config/self.leaf
        leaf.mkdir(mode=0o750);os.chown(leaf,0,gid);leaf.chmod(0o750)
        copy_new(self.leaf_source/'server.pem',leaf/'server.pem',0o644,0,gid)
        copy_new(self.leaf_source/'server-key.pem',leaf/'server-key.pem',0o640,0,gid)
        base.write_new(layout.config/'node.json',base.payload.encoded(self.settings),0o640,0,gid)
        return self.evidence['node'],self.previous['hostname'],self.previous['ca_sha256'],self.settings
