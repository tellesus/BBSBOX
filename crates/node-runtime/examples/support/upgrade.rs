// SPDX-License-Identifier: AGPL-3.0-or-later
//! Disposable upgrade executables. These are not production modules or releases.
use bbsbox_storage::Migration;

pub fn registry(rejecting: bool, interruptible: bool) -> Vec<Migration> {
    let mut registry = bbsbox_node_runtime::migrations();
    registry.push(Migration {
        module: "events", version: 4,
        sql: if rejecting {
            // Incorrect legacy assumption: a receipt ID is globally unique across namespaces.
            "CREATE TABLE events_upgrade_receipts(namespace TEXT NOT NULL, command_id TEXT PRIMARY KEY) STRICT;"
        } else {
            "CREATE TABLE events_upgrade_receipts(namespace TEXT NOT NULL, command_id TEXT NOT NULL, PRIMARY KEY(namespace,command_id)) STRICT;"
        },
    });
    registry.push(Migration {
        module: "events", version: 5,
        sql: if interruptible {
            // Expensive valid migration work lets CI kill the actual operation mid-SQL.
            "WITH RECURSIVE counter(n) AS (VALUES(0) UNION ALL SELECT n+1 FROM counter WHERE n<100000000) SELECT sum(n) FROM counter; INSERT INTO events_upgrade_receipts SELECT namespace,command_id FROM core_command_receipts;"
        } else { "INSERT INTO events_upgrade_receipts SELECT namespace,command_id FROM core_command_receipts;" },
    });
    registry
}

pub async fn run(rejecting: bool, interruptible: bool) -> bbsbox_node_runtime::Result<()> {
    bbsbox_node_runtime::cli::run(
        std::env::args_os().skip(1).collect(),
        registry(rejecting, interruptible),
    )
    .await
}
