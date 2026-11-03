# SPDX-License-Identifier: AGPL-3.0-or-later
"""Retention for one configured backup set with appliance-owned sealed-archive receipts."""

from dataclasses import dataclass
from datetime import datetime, timedelta, timezone
import hashlib
import os
import re
import stat

MAX_RECORDS = 512
MAX_CIPHER = 9 * 1024**4
NAME = re.compile(r"backup-[0-9a-f]{32}\.age")
HEX = re.compile(r"[0-9a-f]{64}")


@dataclass(frozen=True)
class Receipt:
    name: str
    created_at: int
    inventory: str
    sha256: str
    size: int

    def __post_init__(self):
        if not isinstance(self.name, str) or not NAME.fullmatch(self.name):
            raise ValueError("Invalid backup receipt name")
        if (
            not isinstance(self.created_at, int)
            or isinstance(self.created_at, bool)
            or not 0 <= self.created_at < 253402300800
        ):
            raise ValueError("Invalid backup receipt time")
        if (
            not isinstance(self.size, int)
            or isinstance(self.size, bool)
            or not 0 < self.size <= MAX_CIPHER
        ):
            raise ValueError("Invalid backup receipt length")
        if any(
            not isinstance(value, str) or not HEX.fullmatch(value)
            for value in [self.inventory, self.sha256]
        ):
            raise ValueError("Invalid backup receipt hash")

    @property
    def day(self):
        return (
            datetime(1970, 1, 1, tzinfo=timezone.utc)
            + timedelta(seconds=self.created_at)
        ).date()


def keep(records):
    """Keep the latest backup in each of the newest seven UTC days and four ISO weeks."""
    selected = set()
    days = set()
    weeks = set()
    for record in sorted(records, key=lambda r: (r.created_at, r.name), reverse=True):
        day = record.day
        week = day.isocalendar()[:2]
        if day not in days and len(days) < 7:
            days.add(day)
            selected.add(record.name)
        if week not in weeks and len(weeks) < 4:
            weeks.add(week)
            selected.add(record.name)
    return selected


def stamp(metadata):
    return (
        metadata.st_dev,
        metadata.st_ino,
        metadata.st_size,
        metadata.st_mtime_ns,
        metadata.st_ctime_ns,
    )


def file_identity(directory, name, owner):
    """Hash one known regular file through the anchored media directory descriptor."""
    if not NAME.fullmatch(name):
        raise ValueError("Invalid archive name")
    try:
        descriptor = os.open(
            name, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=directory
        )
    except OSError:
        return None
    try:
        metadata = os.fstat(descriptor)
        if (
            not stat.S_ISREG(metadata.st_mode)
            or metadata.st_uid != owner
            or metadata.st_mode & 0o077
            or not 0 < metadata.st_size <= MAX_CIPHER
        ):
            return None
        with os.fdopen(descriptor, "rb", closefd=False) as stream:
            digest = hashlib.sha256()
            remaining = metadata.st_size
            while remaining:
                raw = stream.read(min(remaining, 1024 * 1024))
                if not raw:
                    return None
                remaining -= len(raw)
                digest.update(raw)
            if stream.read(1) or stamp(os.fstat(descriptor)) != stamp(metadata):
                return None
        return stamp(metadata), digest.hexdigest()
    finally:
        os.close(descriptor)


def unchanged(directory, name, expected):
    try:
        return stamp(os.stat(name, dir_fd=directory, follow_symlinks=False)) == expected
    except OSError:
        return False


def intact(directory, record, owner):
    result = file_identity(directory, record.name, owner)
    return (
        result[0]
        if result and result[0][2] == record.size and result[1] == record.sha256
        else None
    )


def prune(directory, records, owner=0):
    """Remove only unchanged indexed archives, retaining at least two intact copies before pruning."""
    metadata = os.fstat(directory)
    if (
        not stat.S_ISDIR(metadata.st_mode)
        or metadata.st_uid != owner
        or metadata.st_mode & 0o077
    ):
        raise ValueError("Backup set directory must be private and operator-owned")
    if len(records) > MAX_RECORDS or len({r.name for r in records}) != len(records):
        raise ValueError("Invalid backup receipt set")
    good = {}
    unverified = []
    for record in records:
        identity = intact(directory, record, owner)
        if identity:
            good[record.name] = (record, identity)
        else:
            unverified.append(record.name)
    retained = keep([r for r, _ in good.values()])
    removed = []
    for name, (_, identity) in sorted(
        good.items(), key=lambda item: (item[1][0].created_at, item[0])
    ):
        if name in retained:
            continue
        # Known media must still have at least two rechecked anchors before each unlink.
        if (
            sum(unchanged(directory, anchor, good[anchor][1]) for anchor in retained)
            < 2
        ):
            break
        if not unchanged(directory, name, identity):
            unverified.append(name)
            continue
        os.unlink(name, dir_fd=directory)
        os.fsync(directory)
        removed.append(name)
    return dict(
        kept=sorted(set(good) - set(removed)),
        pruned=removed,
        unverified=sorted(unverified),
    )
