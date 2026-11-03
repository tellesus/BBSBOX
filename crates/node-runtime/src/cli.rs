// SPDX-License-Identifier: AGPL-3.0-or-later
//! Shared CLI for a first-party compiled module registry.
pub async fn run(
    args: Vec<std::ffi::OsString>,
    registry: Vec<bbsbox_storage::Migration>,
) -> crate::Result<()> {
    if args.len() == 1 && args[0] == "--release-profile" {
        println!(
            "{}",
            serde_json::to_string(&crate::release_profile_for_registry(&registry))?
        );
        return Ok(());
    }
    if args
        .first()
        .and_then(|s| s.to_str())
        .is_some_and(|s| s.starts_with("release-"))
    {
        let result =
            tokio::task::spawn_blocking(move || crate::release_operator::command(&args)).await??;
        println!("{}", serde_json::to_string(&result)?);
        return Ok(());
    }
    if args.first().is_some_and(|s| {
        matches!(
            s.to_str(),
            Some(
                "backup"
                    | "restore"
                    | "backup-keygen"
                    | "backup-public"
                    | "restore-and-authorize"
                    | "migrate-existing"
            )
        )
    }) {
        let result = tokio::task::spawn_blocking(move || {
            crate::operator::command_with_registry(&args, registry)
        })
        .await??;
        println!("{}", serde_json::to_string(&result)?);
        return Ok(());
    }
    let initialize = args.len() == 3 && args[2] == "--initialize";
    if !(args.len() == 2 || initialize) || args[0] != "--config" {
        return Err("Usage: bbsbox-node --config <node.json> [--initialize]".into());
    }
    let config = crate::Config::read(std::path::Path::new(&args[1]))?;
    let installation = std::path::Path::new(&args[1]).with_file_name("installation.json");
    let present = match std::fs::symlink_metadata(&installation) {
        Ok(_) => true,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => return Err(error.into()),
    };
    let expected = match present {
        true => Some(crate::release_operator::check_installed_node(
            &config.database,
            &installation,
        )?),
        false => None,
    };
    if initialize {
        let runtime = match expected {
            Some(node) => {
                crate::Runtime::start_existing_with_registry(&config, registry, node).await?
            }
            None => crate::Runtime::start_with_registry(&config, registry).await?,
        };
        let info = runtime.info();
        println!(
            "{}",
            serde_json::json!({"format":"bbsbox.node-init/v1","node_id":info.node_id,"name":info.name,"version":info.version})
        );
        runtime.shutdown().await
    } else {
        match expected {
            Some(node) => crate::serve_existing_with_registry(config, registry, node).await,
            None => crate::serve_with_registry(config, registry).await,
        }
    }
}
