// SPDX-License-Identifier: AGPL-3.0-or-later
use bbsbox_storage::{Database, Migration, open_reader};
use std::{
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
#[test]
fn killing_a_migration_with_spilled_wal_frames_rolls_back_schema_and_rows() {
    const ENV: &str = "BBSBOX_MIGRATION_CRASH_PATH";
    if let Some(path) = std::env::var_os(ENV) {
        Database::open(path,&[Migration{module:"interrupted",version:1,sql:"CREATE TABLE interrupted_rows(value BLOB); WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<1000000) INSERT INTO interrupted_rows SELECT zeroblob(128) FROM n;"}]).unwrap();
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("node.sqlite");
    drop(Database::open(&path, &[]).unwrap());
    let child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "killing_a_migration_with_spilled_wal_frames_rolls_back_schema_and_rows",
        ])
        .env(ENV, &path)
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    let mut child = ChildGuard(child);
    let deadline = Instant::now() + Duration::from_secs(20);
    let wal = dir.path().join("node.sqlite-wal");
    loop {
        if std::fs::metadata(&wal).is_ok_and(|m| m.len() > 1024 * 1024) {
            break;
        }
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "migration finished before the crash point"
        );
        assert!(
            Instant::now() < deadline,
            "migration never spilled uncommitted WAL frames"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    child.0.kill().unwrap();
    child.0.wait().unwrap();
    let reader = open_reader(&path).unwrap();
    assert_eq!(
        reader
            .query_row(
                "SELECT count(*) FROM sqlite_schema WHERE name='interrupted_rows'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    assert_eq!(
        reader
            .query_row(
                "SELECT count(*) FROM core_migrations WHERE module='interrupted'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    drop(reader);
    // No migration was committed, so recovery can apply the intended small fixture cleanly.
    drop(Database::open(&path,&[Migration{module:"interrupted",version:1,sql:"CREATE TABLE interrupted_rows(value BLOB); INSERT INTO interrupted_rows VALUES(x'01');"}]).unwrap());
    assert_eq!(
        open_reader(&path)
            .unwrap()
            .query_row("SELECT count(*) FROM interrupted_rows", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
}
