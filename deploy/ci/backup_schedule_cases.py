# SPDX-License-Identifier: AGPL-3.0-or-later
"""Exercise a real timer/service and a detached backup mount in the private Debian guest."""

import json
import os
from pathlib import Path
import subprocess
import sys

import boot_service


ROOT = Path(__file__).resolve().parents[2]
MEDIA = "/media/bbsbox-backup-fixture"
STATE = "/var/lib/bbsbox-maintenance/backup-schedule"


def exercise(pid, root, node):
    if not boot_service.private_process(pid, root) or os.readlink(
        f"/proc/{pid}/ns/mnt"
    ) == os.readlink("/proc/self/ns/mnt"):
        raise ValueError("The backup fixture requires a private guest mount namespace")

    def enter(*arguments, timeout=20):
        return boot_service.enter(pid, root, list(arguments), timeout=timeout).stdout

    source = root / "root/schedule-source"
    source.mkdir(mode=0o700)
    files = [
        "deploy/backup_schedule.py",
        "deploy/backup_retention.py",
        "deploy/backup_development.py",
        "deploy/install_release.py",
        "deploy/install_development.py",
        "deploy/enrollment.py",
        "deploy/systemd/bbsbox-backup.service",
        "deploy/systemd/bbsbox-backup.timer",
        "tools/release_candidate.py",
        "tools/release_materials.py",
        "tools/runtime_bundle.py",
    ]
    for name in files:
        target = source / name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes((ROOT / name).read_bytes())

    media = root / "root/schedule-media"
    media.mkdir(mode=0o700)
    mountpoint = root / MEDIA.lstrip("/")
    if not mountpoint.parent.resolve().is_relative_to(root):
        raise ValueError("Fixture mountpoint escapes the guest")
    mountpoint.mkdir(mode=0o700, parents=True)
    enter("/usr/bin/mount", "--bind", "/root/schedule-media", MEDIA)
    binary = "/opt/bbsbox/current/bbsbox-node"
    key = "/root/schedule-key"
    recipient = json.loads(enter(binary, "backup-keygen", "--output", key))["recipient"]
    helper = ("/usr/bin/python3", "/root/schedule-source/deploy/backup_schedule.py")
    wrong_checksum = recipient[:-1] + ("q" if recipient[-1] != "q" else "p")
    invalid = boot_service.enter(
        pid,
        root,
        [*helper, "configure", "--mount", MEDIA, "--recipient", wrong_checksum],
        check=False,
    )
    assert (
        invalid.returncode and not (root / STATE.lstrip("/") / "config.json").exists()
    )
    assert not any(media.iterdir())
    configured = json.loads(
        enter(*helper, "configure", "--mount", MEDIA, "--recipient", recipient)
    )
    assert configured["node"] == node and configured["status"] == "configured"
    assert json.loads(enter(*helper, "install-timer"))["status"] == "timer-installed"
    assert json.loads(enter(*helper, "install-timer"))["status"] == "timer-installed"
    enter("/usr/bin/systemctl", "daemon-reload")
    try:
        enter("/usr/bin/systemctl", "start", "bbsbox-backup.service", timeout=120)
    except (subprocess.CalledProcessError, subprocess.TimeoutExpired):
        journal = enter(
            "/usr/bin/journalctl",
            "--no-pager",
            "-u",
            "bbsbox-backup.service",
            "-n",
            "40",
        )
        print(journal.decode(errors="replace"), file=sys.stderr)
        raise
    paths = list((root / STATE.lstrip("/") / "receipts").glob("*.json"))
    assert len(paths) == 1
    receipt = json.loads(paths[0].read_text())
    archive = f"{MEDIA}/bbsbox-{configured['media_id']}/{receipt['archive']['name']}"
    restored = json.loads(
        enter(
            binary,
            "restore",
            "--archive",
            archive,
            "--node",
            node,
            "--identity-file",
            key,
            "--output",
            "/root/schedule-restore",
            "--staging",
            "/var/lib/bbsbox-maintenance",
            timeout=120,
        )
    )
    assert (
        restored["inventory_sha256"] == receipt["archive"]["inventory"]
        and restored["state"] == "recovery-required"
    )
    stable = ("/usr/bin/python3", f"{STATE}/tools/deploy/backup_schedule.py", "run")
    assert json.loads(enter(*stable))["status"] == "already-backed-up"
    assert len(list((root / STATE.lstrip("/") / "receipts").glob("*.json"))) == 1
    enter("/usr/bin/systemctl", "start", "bbsbox-backup.timer")
    assert (
        enter("/usr/bin/systemctl", "is-active", "bbsbox-backup.timer").strip()
        == b"active"
    )
    enter("/usr/bin/systemctl", "stop", "bbsbox-backup.timer")
    enter("/usr/bin/umount", MEDIA)
    assert json.loads(enter(*stable))["status"] == "media-unavailable"
    assert not any((root / MEDIA.lstrip("/")).iterdir())
    assert (
        media / f"bbsbox-{configured['media_id']}" / receipt["archive"]["name"]
    ).is_file()
    boot_service.service_identity(pid, root)
    return dict(
        real_service_backup=True,
        timer_loaded=True,
        decrypt_verified=True,
        same_day_deduplicated=True,
        detached_media_skipped=True,
    )
