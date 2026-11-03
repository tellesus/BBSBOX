// SPDX-License-Identifier: AGPL-3.0-or-later
#[tokio::main]
async fn main() -> bbsbox_node_runtime::Result<()> {
    bbsbox_node_runtime::cli::run(
        std::env::args_os().skip(1).collect(),
        bbsbox_node_runtime::migrations(),
    )
    .await
}
