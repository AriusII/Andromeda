#![forbid(unsafe_code)]

use std::{
    env,
    error::Error,
    fs,
    path::{Path, PathBuf},
};

fn main() -> Result<(), Box<dyn Error>> {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let proto_root = manifest_dir.join("proto");
    let proto_sources = collect_proto_sources(&proto_root)?;
    let descriptor_set_path = PathBuf::from(env::var("OUT_DIR")?).join("andromeda_descriptor.bin");

    reject_forbidden_proto_boundary_identifiers(&proto_sources)?;

    for proto_source in &proto_sources {
        println!("cargo:rerun-if-changed={}", proto_source.display());
    }

    let mut config = prost_build::Config::new();
    config.protoc_executable(protoc_bin_vendored::protoc_bin_path()?);
    config.file_descriptor_set_path(&descriptor_set_path);
    config.compile_protos(&proto_sources, &[proto_root])?;

    Ok(())
}

fn collect_proto_sources(proto_root: &Path) -> Result<Vec<PathBuf>, Box<dyn Error>> {
    if !proto_root.is_dir() {
        return Err(format!(
            "crate-local proto root does not exist: {}",
            proto_root.display()
        )
        .into());
    }

    let mut sources = Vec::new();
    collect_proto_sources_from(proto_root, &mut sources)?;
    sources.sort_by_key(|path| stable_path_key(path));

    if sources.is_empty() {
        return Err(format!(
            "crate-local proto root contains no .proto sources: {}",
            proto_root.display()
        )
        .into());
    }

    Ok(sources)
}

fn collect_proto_sources_from(
    directory: &Path,
    sources: &mut Vec<PathBuf>,
) -> Result<(), Box<dyn Error>> {
    let mut entries = fs::read_dir(directory)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| stable_path_key(&entry.path()));

    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            collect_proto_sources_from(&path, sources)?;
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("proto") {
            sources.push(path);
        }
    }

    Ok(())
}

fn reject_forbidden_proto_boundary_identifiers(
    proto_sources: &[PathBuf],
) -> Result<(), Box<dyn Error>> {
    for proto_source in proto_sources {
        let source = fs::read_to_string(proto_source)?;
        let active_source = active_proto_source(&source);
        for (line_index, line) in active_source.lines().enumerate() {
            if let Some(forbidden) = line
                .split(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
                .find_map(forbidden_proto_identifier)
            {
                return Err(format!(
                    "forbidden protobuf boundary identifier `{forbidden}` in crate-local proto source {}:{}",
                    proto_source.display(),
                    line_index + 1
                )
                .into());
            }
        }
    }

    Ok(())
}

fn forbidden_proto_identifier(token: &str) -> Option<&'static str> {
    let lower = token.to_ascii_lowercase();
    if lower == "service" {
        return Some("service");
    }
    if lower == "rpc" {
        return Some("rpc");
    }
    if lower.contains("grpc") {
        return Some("grpc");
    }
    if lower.contains("tonic") {
        return Some("tonic");
    }

    None
}

fn active_proto_source(source: &str) -> String {
    let mut active = String::with_capacity(source.len());
    let mut chars = source.chars().peekable();
    let mut in_block_comment = false;
    let mut in_string = false;

    while let Some(character) = chars.next() {
        if in_block_comment {
            if character == '*' && chars.peek() == Some(&'/') {
                chars.next();
                in_block_comment = false;
            }
            if character == '\n' {
                active.push('\n');
            }
            continue;
        }

        if in_string {
            if character == '\\' {
                chars.next();
                continue;
            }
            if character == '"' {
                in_string = false;
            }
            if character == '\n' {
                active.push('\n');
            }
            continue;
        }

        if character == '/' && chars.peek() == Some(&'/') {
            for comment_character in chars.by_ref() {
                if comment_character == '\n' {
                    active.push('\n');
                    break;
                }
            }
            continue;
        }

        if character == '/' && chars.peek() == Some(&'*') {
            chars.next();
            in_block_comment = true;
            continue;
        }

        if character == '"' {
            in_string = true;
            continue;
        }

        active.push(character);
    }

    active
}

fn stable_path_key(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}
