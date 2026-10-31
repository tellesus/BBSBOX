// SPDX-License-Identifier: AGPL-3.0-or-later
#[tokio::main]
async fn main() -> bbsbox_node_runtime::Result<()> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let initialize = args.len() == 3 && args[2] == "--initialize";
    if !(args.len() == 2 || initialize) || args[0] != "--config" {
        return Err("Usage: bbsbox-node --config <node.json> [--initialize]".into());
    }
    let config = bbsbox_node_runtime::Config::read(std::path::Path::new(&args[1]))?;
    if initialize {
        let runtime = bbsbox_node_runtime::Runtime::start(&config).await?;
        let info = runtime.info();
        println!(
            "{}",
            serde_json::json!({"format":"bbsbox.node-init/v1","node_id":info.node_id,"name":info.name,"version":info.version})
        );
        runtime.shutdown().await
    } else {
        bbsbox_node_runtime::serve(config).await
    }
}
