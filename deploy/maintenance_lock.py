# SPDX-License-Identifier: AGPL-3.0-or-later
"""One appliance-wide lock, acquired before configuration or node-storage locks."""
from contextlib import contextmanager
import os
from pathlib import Path
import stat


@contextmanager
def acquire(root=Path('/')):
    import fcntl
    root=Path(root).absolute()
    path=root/'run/bbsbox-maintenance.lock'
    if root!=root.resolve() or path.parent!=path.parent.resolve():raise ValueError('Maintenance lock must not traverse links')
    descriptor=os.open(path,os.O_RDWR|os.O_CREAT|os.O_NOFOLLOW,0o600)
    try:
        metadata=os.fstat(descriptor)
        if not stat.S_ISREG(metadata.st_mode) or metadata.st_uid!=0 or metadata.st_mode&0o077:
            raise ValueError('Expected a private root-owned maintenance lock')
        fcntl.flock(descriptor,fcntl.LOCK_EX|fcntl.LOCK_NB)
        yield
    finally:os.close(descriptor)
