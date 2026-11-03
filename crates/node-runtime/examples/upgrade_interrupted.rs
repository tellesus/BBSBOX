// SPDX-License-Identifier: AGPL-3.0-or-later
#[path = "support/upgrade.rs"]
mod upgrade;
#[tokio::main]
async fn main() -> bbsbox_node_runtime::Result<()> {
    upgrade::run(false, true).await
}
