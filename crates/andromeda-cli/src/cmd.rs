use crate::args::{parse_recovery_inspect_options, parse_vertical_v0_wal_path};
use crate::error::cli_error;
use andromeda_core::AndromedaResult;

pub use crate::cmd_protocol::run_protocol_smoke;
pub use crate::cmd_recovery::run_recovery_inspect;
pub use crate::cmd_vertical::{print_help, run_vertical_demo, run_vertical_v0_demo};

/// Dispatches a CLI command based on the first argument.
pub fn dispatch_command(args: &[String]) -> AndromedaResult<()> {
    match args.first().map(String::as_str) {
        Some("vertical") => run_vertical_demo(),
        Some("vertical-v0") => run_vertical_v0_demo(parse_vertical_v0_wal_path(&args[1..])?),
        Some("protocol-smoke") => {
            let detailed = args[1..].iter().any(|arg| arg == "--detail");
            let report = run_protocol_smoke(detailed)?;
            print!("{}", report);
            Ok(())
        }
        Some("recovery-inspect") => {
            let options = parse_recovery_inspect_options(&args[1..])?;
            run_recovery_inspect(options)
        }
        Some("-h" | "--help" | "help") | None => {
            print_help();
            Ok(())
        }
        Some(command) => Err(cli_error(format!(
            "unknown command `{command}`; run `andromeda-cli --help`"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_command_returns_error() {
        let result = dispatch_command(&["unknown".to_string()]);
        assert!(result.is_err());
    }
}
