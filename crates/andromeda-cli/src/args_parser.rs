use crate::error::cli_error;
use crate::parse;
use andromeda_error::AndromedaResult;
use andromeda_wal::Lsn;
use std::path::PathBuf;

pub fn parse_vertical_v0_wal_path(args: &[String]) -> AndromedaResult<PathBuf> {
    let mut wal_path = None;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--wal" => {
                index += 1;
                let Some(path) = args.get(index) else {
                    return Err(cli_error("missing path after --wal"));
                };
                wal_path = Some(PathBuf::from(path));
            },
            unknown => {
                return Err(cli_error(format!(
                    "unknown vertical-v0 option `{unknown}`; expected `--wal <path>`"
                )));
            },
        }
        index += 1;
    }

    Ok(wal_path.unwrap_or_else(default_v0_wal_path))
}

pub struct RecoveryInspectOptions {
    pub wal_path: PathBuf,
    pub required_wal_start_lsn: Lsn,
}

pub fn parse_recovery_inspect_options(args: &[String]) -> AndromedaResult<RecoveryInspectOptions> {
    let mut wal_path = None;
    let mut required_wal_start_lsn = Lsn::new(1);
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--required-wal-start-lsn" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(cli_error("missing value after --required-wal-start-lsn"));
                };
                required_wal_start_lsn =
                    Lsn::new(parse_u64_option(value, "--required-wal-start-lsn")?);
            },
            option if option.starts_with("--") => {
                return Err(cli_error(format!(
                    "unknown recovery-inspect option `{option}`"
                )));
            },
            path => {
                if wal_path.is_some() {
                    return Err(cli_error("recovery-inspect accepts exactly one WAL path"));
                }
                wal_path = Some(PathBuf::from(path));
            },
        }
        index += 1;
    }

    let Some(wal_path) = wal_path else {
        return Err(cli_error(
            "usage: andromeda-cli recovery-inspect <wal-path> [--required-wal-start-lsn <lsn>]",
        ));
    };

    Ok(RecoveryInspectOptions {
        wal_path,
        required_wal_start_lsn,
    })
}

pub fn parse_u64_option(value: &str, option: &str) -> AndromedaResult<u64> {
    parse::parse_u64_option(value, option)
}

const DEFAULT_V0_WAL_FILE: &str = "andromeda-v0-vertical.wal";

fn default_v0_wal_path() -> PathBuf {
    std::env::temp_dir().join(DEFAULT_V0_WAL_FILE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vertical_v0_defaults_to_temp_wal() {
        let path = parse_vertical_v0_wal_path(&[]).unwrap();
        assert!(path.ends_with(DEFAULT_V0_WAL_FILE));
    }

    #[test]
    fn vertical_v0_accepts_explicit_path() {
        let explicit = parse_vertical_v0_wal_path(&[
            "--wal".to_string(),
            "target/andromeda-cli-test.wal".to_string(),
        ])
        .unwrap();

        assert_eq!(explicit, PathBuf::from("target/andromeda-cli-test.wal"));
    }

    #[test]
    fn recovery_inspect_requires_one_path() {
        let options = parse_recovery_inspect_options(&[
            "target/andromeda-cli-test.wal".to_string(),
            "--required-wal-start-lsn".to_string(),
            "2".to_string(),
        ])
        .unwrap();

        assert_eq!(
            options.wal_path,
            PathBuf::from("target/andromeda-cli-test.wal")
        );
        assert_eq!(options.required_wal_start_lsn, Lsn::new(2));
    }
}
