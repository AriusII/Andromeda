use std::{env, error::Error, path::PathBuf};

fn main() -> Result<(), Box<dyn Error>> {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let repo_root = manifest_dir
        .parent()
        .and_then(|path| path.parent())
        .ok_or("andromeda-proto must live under crates/")?;
    let proto_root = repo_root.join("schemas").join("proto");
    let contract_proto = proto_root.join("andromeda/contract/v1/contract.proto");
    let protocol_proto = proto_root.join("andromeda/protocol/v1/protocol.proto");
    let descriptor_set_path = PathBuf::from(env::var("OUT_DIR")?).join("andromeda_descriptor.bin");

    println!("cargo:rerun-if-changed={}", contract_proto.display());
    println!("cargo:rerun-if-changed={}", protocol_proto.display());
    println!("cargo:rerun-if-changed={}", proto_root.display());

    let mut config = prost_build::Config::new();
    config.protoc_executable(protoc_bin_vendored::protoc_bin_path()?);
    config.file_descriptor_set_path(&descriptor_set_path);
    config.compile_protos(&[contract_proto, protocol_proto], &[proto_root])?;

    Ok(())
}
