use std::{fs, path::PathBuf};

fn read_source(relative_path: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative_path);
    match fs::read_to_string(&path) {
        Ok(source) => source,
        Err(error) => panic!("failed to read {}: {error}", path.display()),
    }
}

#[test]
fn digest_crate_rejects_native_struct_layout_assumptions() {
    let sources = [
        ("src/lib.rs", read_source("src/lib.rs")),
        ("src/digest.rs", read_source("src/digest.rs")),
    ];
    let forbidden_tokens = [
        "repr(C",
        "repr(packed",
        "std::mem::transmute",
        "core::mem::transmute",
        "transmute_copy",
        "std::mem::size_of",
        "core::mem::size_of",
        "std::mem::align_of",
        "core::mem::align_of",
        "std::slice::from_raw_parts",
        "core::slice::from_raw_parts",
        "from_raw_parts_mut",
        "std::ptr::copy",
        "core::ptr::copy",
        "read_unaligned",
        "write_unaligned",
        "align_to",
        "MaybeUninit",
        "bytemuck",
        "zerocopy",
    ];

    for (path, source) in sources {
        for token in forbidden_tokens {
            assert!(
                !source.contains(token),
                "{path} must derive digest input from explicit bytes, not native struct layout token {token:?}"
            );
        }
    }
}
