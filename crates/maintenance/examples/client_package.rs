// SPDX-License-Identifier: AGPL-3.0-or-later
//! M00 trusted-bootstrap fixture. Its trust key must be enrolled independently of package delivery.
use bbsbox_maintenance::{
    client_install::Store,
    client_payload::{self, Result},
    client_release::{self, Manifest},
    inventory::FileRecord,
    release::ReleaseSigner,
};
use sha2::{Digest, Sha256};
use std::{
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::Path,
};

fn array<const N: usize>(value: &std::ffi::OsStr) -> Result<[u8; N]> {
    Ok(
        hex::decode(value.to_str().ok_or("hex argument is not UTF-8")?)?
            .try_into()
            .map_err(|_| "hex argument has the wrong length")?,
    )
}
fn hash(path: &Path) -> Result<(u64, String)> {
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    let mut total = 0u64;
    let mut buffer = [0; 65536];
    loop {
        let length = file.read(&mut buffer)?;
        if length == 0 {
            break;
        }
        total += length as u64;
        if total > 2 * 1024 * 1024 * 1024 {
            return Err("client package member exceeds its limit".into());
        }
        hash.update(&buffer[..length]);
    }
    Ok((total, hex::encode(hash.finalize())))
}
fn fresh(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}
/// Synthetic signer only. Real issuer key storage and production bootstrap delivery are separate.
fn fixture(
    binary: &Path,
    repository: &Path,
    materials: &Path,
    commit: &str,
    output: &Path,
    public: &Path,
) -> Result<serde_json::Value> {
    if public.try_exists()? {
        return Err("fixture public-key output already exists".into());
    }
    let signer = ReleaseSigner::generate()?;
    fs::create_dir(output)?;
    let mut files = Vec::new();
    for name in client_release::REQUIRED {
        let source = if name == client_release::ENTRY {
            binary.to_path_buf()
        } else if let Some(name) = name.strip_prefix("materials/") {
            materials.join(name)
        } else {
            repository.join(name)
        };
        let metadata = fs::symlink_metadata(&source)?;
        if !metadata.is_file()
            || metadata.file_type().is_symlink()
            || metadata.len() > 2 * 1024 * 1024 * 1024
        {
            return Err("invalid fixture source file".into());
        }
        let destination = output.join(name);
        fs::create_dir_all(destination.parent().ok_or("missing package parent")?)?;
        let mut input = File::open(source)?.take(metadata.len() + 1);
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&destination)?;
        if std::io::copy(&mut input, &mut file)? != metadata.len() {
            return Err("fixture source changed".into());
        }
        file.sync_all()?;
        let (size, sha256) = hash(&destination)?;
        files.push(FileRecord {
            path: name.into(),
            size,
            sha256,
        });
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let manifest = Manifest {
        format: client_release::DOMAIN.into(),
        issuer: signer.id().to_string(),
        sequence: 1,
        version: "0.1.0-native-fixture".into(),
        target: client_release::TARGET.into(),
        source_commit: commit.into(),
        vault_profiles: vec![1],
        files,
        next_key: None,
    };
    let signed = client_release::sign(manifest, &signer)?;
    fresh(&output.join(client_payload::MANIFEST), signed.encoded())?;
    client_payload::verify(output, &signer.public())?;
    fresh(public, hex::encode(signer.public()).as_bytes())?;
    Ok(
        serde_json::json!({"format":"bbsbox.client-package-fixture/v1","public_key":hex::encode(signer.public()),"release_id":hex::encode(signed.id()),"production_signing":false}),
    )
}
fn execute(arguments: Vec<OsString>) -> Result<serde_json::Value> {
    match arguments.as_slice(){
        [command,binary,repository,materials,commit,output,public] if command=="fixture-package"=>
            fixture(Path::new(binary),Path::new(repository),Path::new(materials),commit.to_str().ok_or("invalid source commit")?,Path::new(output),Path::new(public)),
        [command,root,public] if command=="init"=>Ok(serde_json::to_value(Store::initialize(Path::new(root),array(public)?)?.state()?)?),
        [command,package,public] if command=="verify"=>{
            let signed=client_payload::verify(Path::new(package),&array(public)?)?;
            Ok(serde_json::json!({"release_id":hex::encode(signed.id()),"sequence":signed.manifest().sequence,"target":signed.manifest().target}))
        },
        [command,root] if command=="status"=>Ok(serde_json::to_value(Store::open(Path::new(root))?.state()?)?),
        [command,root] if command=="path"=>Ok(serde_json::json!({"executable":Store::open(Path::new(root))?.executable()?})),
        [command,root,request,package] if command=="prepare"=>{
            let store=Store::open(Path::new(root))?;let expected=store.state()?.revision;
            Ok(serde_json::to_value(store.prepare(expected,array(request)?,Path::new(package))?)?)
        },
        [command,root,request] if command=="activate"=>{
            let store=Store::open(Path::new(root))?;let expected=store.state()?.revision;
            Ok(serde_json::to_value(store.activate(expected,array(request)?)?)?)
        },
        [command,root,separator,child_args @ ..] if command=="run"&&separator=="--"=>{
            #[cfg(windows)] {
                let store=Store::open(Path::new(root))?;let executable=store.executable()?;
                let mut child=std::process::Command::new(&executable).current_dir(executable.parent().ok_or("missing installed directory")?).args(child_args).spawn()?;
                drop(store);let status=child.wait()?;
                if !status.success(){return Err(format!("installed client exited with {status}").into());}
                Ok(serde_json::json!({"client_exit_success":true}))
            }
            #[cfg(not(windows))] {let _=(root,child_args);Err("this client profile requires Windows to run".into())}
        },
        _=>Err("Usage: client_package fixture-package BINARY REPOSITORY MATERIALS COMMIT FRESH_PACKAGE FRESH_PUBLIC_FILE | init STORE PUBLIC_HEX | verify PACKAGE PUBLIC_HEX | status STORE | prepare STORE REQUEST_HEX PACKAGE | activate STORE REQUEST_HEX | run STORE -- CLIENT_ARGS".into()),
    }
}
fn main() {
    match execute(std::env::args_os().skip(1).collect()) {
        Ok(report) => println!("{}", serde_json::to_string(&report).expect("JSON report")),
        Err(error) => {
            eprintln!("Client package: {error}");
            std::process::exit(1);
        }
    }
}
