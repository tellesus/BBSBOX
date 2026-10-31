// SPDX-License-Identifier: AGPL-3.0-or-later
#[tokio::main]
async fn main() -> bbsbox_node_runtime::Result<()> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.len() != 2 || args[0] != "--config" {
        return Err("Usage: bbsbox-node --config <node.json>".into());
    }
    let config = bbsbox_node_runtime::Config::read(std::path::Path::new(&args[1]))?;
    bbsbox_node_runtime::serve(config).await
}
