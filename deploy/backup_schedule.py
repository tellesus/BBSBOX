# SPDX-License-Identifier: AGPL-3.0-or-later
"""Configured-media daily backups and receipt-bound retention for a signed Debian node."""

import argparse
from contextlib import contextmanager
from dataclasses import asdict
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import tempfile
import time

import install_release as install
import backup_retention as retention

ROOT = Path(__file__).resolve().parent
STATE = Path("/var/lib/bbsbox-maintenance/backup-schedule")
REGISTRATION = Path("/etc/bbsbox/installation.json")
OPERATOR = Path("/opt/bbsbox/maintenance/bbsbox-operator")
RUNTIME = Path("/opt/bbsbox/current/bbsbox-node")
UNIT_ROOT = Path("/etc/systemd/system")
CONTROL_RESERVE = 32 * 1024 * 1024
LOCAL_FILESYSTEMS = {"ext4", "xfs", "btrfs", "f2fs"}
HEX_ID = re.compile("[0-9a-f]{32}")
NODE = re.compile("node:[0-9a-f]{64}")


def private(path, directory=False):
    meta = path.lstat()
    kind = stat.S_ISDIR if directory else stat.S_ISREG
    if (
        path != path.resolve()
        or not kind(meta.st_mode)
        or meta.st_uid != 0
        or meta.st_mode & 0o077
    ):
        raise ValueError("Expected a private root-owned backup path")


def read(path):
    private(path)
    return install.candidate.materials.decoded(
        install.candidate.materials.read_file(path, 4096)
    )


def publish(path, raw, mode):
    descriptor, temporary = tempfile.mkstemp(prefix=".publish-", dir=path.parent)
    try:
        with os.fdopen(descriptor, "wb") as stream:
            stream.write(raw)
            stream.flush()
            os.fchmod(stream.fileno(), mode)
            os.fsync(stream.fileno())
        os.link(temporary, path)
        install.sync_path(path.parent, True)
    finally:
        os.unlink(temporary)


def record(path, body):
    publish(path, install.candidate.materials.encoded(body), 0o600)


def mount_entries(table, location):
    result = []
    for line in table.splitlines():
        parts = line.split()
        if len(parts) < 10 or "-" not in parts[6:]:
            continue
        point = re.sub(r"\\([0-7]{3})", lambda match: chr(int(match[1], 8)), parts[4])
        if point != location:
            continue
        divider = parts.index("-", 6)
        if divider + 1 >= len(parts):
            continue
        result.append((parts[0], parts[2], parts[divider + 1], parts[5].split(",")))
    return result


def mount_token(path):
    if path == Path("/") or not path.is_absolute() or path != path.resolve():
        return None
    entries = mount_entries(Path("/proc/self/mountinfo").read_text(), path.as_posix())
    if len(entries) != 1:
        return None
    mount, device, filesystem, flags = entries[0]
    try:
        meta = path.lstat()
    except FileNotFoundError:
        return None
    if (
        not stat.S_ISDIR(meta.st_mode)
        or filesystem not in LOCAL_FILESYSTEMS
        or "rw" not in flags
        or device != f"{os.major(meta.st_dev)}:{os.minor(meta.st_dev)}"
    ):
        return None
    return mount, meta.st_dev, meta.st_ino


def node_id():
    installed = install.enrollment.read_json(REGISTRATION)
    node = installed.get("node_id", "")
    if installed.get("format") != "bbsbox.install/v1" or not NODE.fullmatch(node):
        raise ValueError("Configure backups on an installed signed node")
    return node


def recipient_valid(recipient):
    return isinstance(recipient, str) and re.fullmatch(
        r"age1[023456789acdefghjklmnpqrstuvwxyz]{58}", recipient
    )


@contextmanager
def schedule_lock():
    import fcntl

    descriptor = os.open(STATE / "lock", os.O_RDWR | os.O_CREAT | os.O_NOFOLLOW, 0o600)
    try:
        meta = os.fstat(descriptor)
        if not stat.S_ISREG(meta.st_mode) or meta.st_uid != 0 or meta.st_mode & 0o077:
            raise ValueError("Invalid backup schedule lock")
        fcntl.flock(descriptor, fcntl.LOCK_EX | fcntl.LOCK_NB)
        yield
    finally:
        os.close(descriptor)


def configure(mount, recipient):
    install.base.platform_preflight()
    mount = Path(mount).absolute()
    if any(
        mount.is_relative_to(Path(path))
        for path in [
            "/var/lib/bbsbox",
            "/var/lib/bbsbox-maintenance",
            "/etc/bbsbox",
            "/opt/bbsbox",
        ]
    ):
        raise ValueError("Keep configured backup media outside installed node storage")
    if not recipient_valid(recipient):
        raise ValueError("Use the operator’s age X25519 public recipient")
    token = mount_token(mount)
    if not token:
        raise ValueError(
            "Attach writable local POSIX backup media at the specified mount"
        )
    install.candidate.command([OPERATOR, "release-control-boot"])
    checked = install.candidate.command(
        [RUNTIME, "backup-check-recipient", "--recipient", recipient]
    )
    if checked != dict(format="bbsbox.backup-recipient/v1", recipient=recipient):
        raise ValueError("Use the operator’s canonical age X25519 public recipient")
    install.backup.prepare_stage(STATE)
    with schedule_lock():
        config = STATE / "config.json"
        if config.exists() or config.is_symlink():
            raise ValueError(
                "A backup set is already configured; preserve its receipts"
            )
        node = node_id()
        media_id = os.urandom(16).hex()
        name = "bbsbox-" + media_id
        marker = dict(format="bbsbox.backup-media/v1", media_id=media_id, node=node)
        initialize_media(mount, token, name, marker)
        install.backup.prepare_stage(STATE / "receipts")
        record(
            config,
            dict(
                format="bbsbox.backup-schedule/v1",
                mount=str(mount),
                media_id=media_id,
                node=node,
                recipient=recipient,
            ),
        )
    return dict(
        format="bbsbox.scheduled-backup/v1",
        status="configured",
        node=node,
        media_id=media_id,
    )


def initialize_media(mount, token, name, marker):
    flags = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW
    descriptor = os.open(mount, flags)
    try:
        meta = os.fstat(descriptor)
        if (meta.st_dev, meta.st_ino) != token[1:] or mount_token(mount) != token:
            raise ValueError("Backup media changed during configuration")
        os.mkdir(name, mode=0o700, dir_fd=descriptor)
        media = os.open(name, flags, dir_fd=descriptor)
        try:
            flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW
            output = os.open("media.json", flags, 0o600, dir_fd=media)
            with os.fdopen(output, "wb") as stream:
                stream.write(install.candidate.materials.encoded(marker))
                stream.flush()
                os.fsync(stream.fileno())
            os.fsync(media)
        finally:
            os.close(media)
        os.fsync(descriptor)
    finally:
        os.close(descriptor)
    if mount_token(mount) != token:
        raise ValueError("Backup media changed during configuration")


def load():
    settings = read(STATE / "config.json")
    if (
        set(settings) != {"format", "mount", "media_id", "node", "recipient"}
        or settings["format"] != "bbsbox.backup-schedule/v1"
        or not HEX_ID.fullmatch(settings["media_id"])
        or not NODE.fullmatch(settings["node"])
        or not recipient_valid(settings["recipient"])
    ):
        raise ValueError("Invalid backup schedule configuration")
    return settings


def receipts(settings):
    directory = STATE / "receipts"
    private(directory, True)
    result = []
    for path in directory.glob("*.json"):
        if len(result) >= retention.MAX_RECORDS:
            raise ValueError("Backup receipt catalogue is full")
        saved = read(path)
        if (
            set(saved) != {"format", "node", "media_id", "recipient", "archive"}
            or saved["format"] != "bbsbox.backup-receipt/v1"
            or any(
                saved[name] != settings[name]
                for name in ["node", "media_id", "recipient"]
            )
        ):
            raise ValueError("Backup receipt belongs to a different set")
        item = retention.Receipt(**saved["archive"])
        if path.name != item.name + ".json":
            raise ValueError("Backup receipt name mismatch")
        result.append(item)
    return result


def run_locked(settings):
    mount = Path(settings["mount"])
    token = mount_token(mount)
    if not token:
        return dict(status="media-unavailable")
    media = mount / ("bbsbox-" + settings["media_id"])
    try:
        private(media, True)
        marker = read(media / "media.json")
    except FileNotFoundError:
        return dict(status="media-unavailable")
    if marker != dict(
        format="bbsbox.backup-media/v1",
        media_id=settings["media_id"],
        node=settings["node"],
    ):
        return dict(status="wrong-media")
    if node_id() != settings["node"]:
        raise ValueError("Scheduled backup registration differs from the current node")
    directory = os.open(media, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    try:
        meta = os.fstat(directory)
        if (
            meta.st_dev != token[1]
            or meta.st_ino != media.lstat().st_ino
            or mount_token(mount) != token
        ):
            return dict(status="media-changed")
        saved = receipts(settings)
        now = int(time.time())
        today = datetime.fromtimestamp(now, timezone.utc).date()
        if any(item.created_at > now for item in saved):
            return dict(status="clock-before-latest-backup")
        for item in saved:
            if item.day == today and retention.intact(directory, item, 0):
                return dict(status="already-backed-up", archive=item.name)
        if len(saved) >= retention.MAX_RECORDS:
            return dict(status="receipt-catalogue-full")
        disk = os.fstatvfs(directory)
        available = disk.f_frsize * disk.f_bavail
        if available < CONTROL_RESERVE:
            return dict(status="media-capacity-low", available_bytes=available)
        name = "backup-" + os.urandom(16).hex() + ".age"
        if mount_token(mount) != token:
            return dict(status="media-changed")
        published = install.backup.backup(
            media / name, settings["recipient"], release=True
        )
        if mount_token(mount) != token:
            return dict(status="media-changed")
        if (
            published.get("format") != "bbsbox.node-backup/v1"
            or published.get("node") != settings["node"]
        ):
            raise ValueError("Unexpected scheduled snapshot result")
        verified = retention.file_identity(directory, name, 0)
        if not verified:
            raise ValueError("Published archive is not intact on the registered media")
        item = retention.Receipt(
            name,
            published["snapshot_time"],
            published["inventory_sha256"],
            verified[1],
            verified[0][2],
        )
        catalogue = STATE / "receipts"
        record(
            catalogue / (name + ".json"),
            dict(
                format="bbsbox.backup-receipt/v1",
                node=settings["node"],
                media_id=settings["media_id"],
                recipient=settings["recipient"],
                archive=asdict(item),
            ),
        )
        result = dict(status="created", archive=name, inventory_sha256=item.inventory)
        if mount_token(mount) != token:
            return dict(result, retention="media-changed")
        if item.created_at < max((r.created_at for r in saved), default=0):
            return dict(result, retention="clock-changed")
        pruned = retention.prune(directory, [*saved, item])
        for removed in pruned["pruned"]:
            (catalogue / (removed + ".json")).unlink()
        if pruned["pruned"]:
            install.sync_path(catalogue, True)
        result.update(
            pruned=len(pruned["pruned"]), unverified=len(pruned["unverified"])
        )
        return result
    finally:
        os.close(directory)


def run():
    install.base.platform_preflight()
    config = STATE / "config.json"
    if not config.exists() and not config.is_symlink():
        return dict(format="bbsbox.scheduled-backup/v1", status="unconfigured")
    private(STATE, True)
    try:
        with schedule_lock():
            result = run_locked(load())
    except BlockingIOError:
        result = dict(status="maintenance-busy")
    return dict(format="bbsbox.scheduled-backup/v1", **result)


def check_install_target(path, raw, mode):
    try:
        meta = path.lstat()
    except FileNotFoundError:
        return False
    if (
        not stat.S_ISREG(meta.st_mode)
        or meta.st_uid != 0
        or stat.S_IMODE(meta.st_mode) != mode
        or meta.st_size != len(raw)
        or path.read_bytes() != raw
    ):
        raise ValueError("Existing backup tool or unit differs and has been preserved")
    return True


def install_timer():
    install.base.platform_preflight()
    private(STATE, True)
    with schedule_lock():
        load()
        distribution = STATE / "tools"
        helper = distribution / "deploy/backup_schedule.py"
        units = ["bbsbox-backup.service", "bbsbox-backup.timer"]
        sources = {
            "deploy": [
                "backup_schedule.py",
                "backup_retention.py",
                "backup_development.py",
                "install_release.py",
                "install_development.py",
                "enrollment.py",
            ],
            "tools": [
                "release_candidate.py",
                "release_materials.py",
                "runtime_bundle.py",
            ],
        }
        files = []
        for directory, names in sources.items():
            for name in names:
                source = ROOT.parent / directory / name
                raw = install.candidate.materials.read_file(source, 1024 * 1024)
                files.append((distribution / directory / name, raw, 0o600))
        for name in units:
            raw = (
                (ROOT / "systemd" / name)
                .read_text(encoding="utf-8")
                .replace("@HELPER@", str(helper))
                .encode()
            )
            files.append((UNIT_ROOT / name, raw, 0o644))
        for directory in [
            distribution,
            distribution / "deploy",
            distribution / "tools",
        ]:
            install.backup.prepare_stage(directory)
        missing = [
            (path, raw, mode)
            for path, raw, mode in files
            if not check_install_target(path, raw, mode)
        ]
        for path, raw, mode in missing:
            if path.parent != UNIT_ROOT:
                publish(path, raw, mode)
        with tempfile.TemporaryDirectory(
            prefix="backup-unit-check-", dir=STATE
        ) as temporary:
            staging = Path(temporary)
            for path, raw, _ in files:
                if path.parent == UNIT_ROOT:
                    (staging / path.name).write_bytes(raw)
            subprocess.run(
                [
                    "systemd-analyze",
                    "verify",
                    "--man=no",
                    *[str(staging / name) for name in units],
                ],
                check=True,
                capture_output=True,
                timeout=40,
            )
        for path, raw, mode in missing:
            if path.parent == UNIT_ROOT:
                publish(path, raw, mode)
        for directory in [
            distribution / "deploy",
            distribution / "tools",
            distribution,
            STATE,
        ]:
            install.sync_path(directory, True)
    return dict(format="bbsbox.scheduled-backup/v1", status="timer-installed")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    setup = commands.add_parser("configure")
    setup.add_argument("--mount", type=Path, required=True)
    setup.add_argument("--recipient", required=True)
    commands.add_parser("run")
    commands.add_parser("install-timer")
    a = parser.parse_args()
    print(
        json.dumps(
            configure(a.mount, a.recipient)
            if a.command == "configure"
            else run()
            if a.command == "run"
            else install_timer(),
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
