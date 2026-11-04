// SPDX-License-Identifier: AGPL-3.0-or-later
//! Shared CLI for a first-party compiled module registry.
use std::io::{BufRead, IsTerminal, Read};
use zeroize::Zeroizing;

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
                    | "backup-game-node"
                    | "restore"
                    | "restore-game-node"
                    | "backup-keygen"
                    | "backup-public"
                    | "backup-check-recipient"
                    | "restore-and-authorize"
                    | "restore-compatible"
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
    let bootstrap = args.len() == 4 && args[2] == "--bootstrap-sysop";
    if !(args.len() == 2 || initialize || bootstrap) || args[0] != "--config" {
        return Err(
            "Usage: bbsbox-node --config <node.json> [--initialize | --bootstrap-sysop <username>]"
                .into(),
        );
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
    if bootstrap {
        let username = bbsbox_identity::normalize_username(args[3].to_str().unwrap_or_default())?;
        let account = tokio::task::spawn_blocking(move || {
            use bbsbox_identity::{bootstrap_sysop, node::NodeSigner, password::Engine};
            use bbsbox_storage::{
                disk::Class,
                writer::{Config, Priority, Writer},
            };

            let node = NodeSigner::inspect_existing(&config.database)?;
            if expected.is_some_and(|expected| expected != node) {
                return Err("existing runtime identity differs from its expected node".into());
            }
            bbsbox_storage::verify_exact_registry(&config.database, &registry)?;
            let stdin = std::io::stdin();
            if stdin.is_terminal() {
                return Err("Supply the password through a private stdin pipe".into());
            }
            let mut password = Zeroizing::new(String::new());
            stdin.lock().take(1027).read_line(&mut password)?;
            if password.ends_with('\n') {
                password.pop();
                if password.ends_with('\r') {
                    password.pop();
                }
            }
            let prepared = Engine::default().prepare(password)?;
            let writer = Writer::start(&config.database, registry, Config::default())?;
            let result = writer
                .handle()
                .submit_service(Priority::Interactive, move |database| {
                    // Recheck inside the exclusive writer before changing account state.
                    if NodeSigner::inspect_existing(&config.database)? != node {
                        return Err(bbsbox_identity::Error::InvalidNodeState);
                    }
                    database.write(Class::Control, 64 * 1024, |tx| {
                        bootstrap_sysop(tx, &username, prepared)
                    })
                })?
                .blocking_wait();
            let shutdown = writer.shutdown();
            let account = result?;
            shutdown?;
            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(account)
        })
        .await??;
        println!(
            "{}",
            serde_json::json!({"format":"bbsbox.sysop-bootstrap/v1", "username":account.username})
        );
        Ok(())
    } else if initialize {
        let mut config = config;
        config.game_origin = None;
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
