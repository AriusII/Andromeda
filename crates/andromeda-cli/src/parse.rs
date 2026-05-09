use crate::error::cli_error;
use andromeda_error::AndromedaResult;

pub(crate) fn parse_u64(value: &str, error_message: &'static str) -> AndromedaResult<u64> {
    value.parse::<u64>().map_err(|_| cli_error(error_message))
}

pub(crate) fn parse_usize(value: &str, error_message: &'static str) -> AndromedaResult<usize> {
    value.parse::<usize>().map_err(|_| cli_error(error_message))
}

pub(crate) fn parse_u64_option(value: &str, option: &str) -> AndromedaResult<u64> {
    value
        .parse::<u64>()
        .map_err(|_| cli_error(format!("{option} expects an unsigned integer")))
}

pub(crate) fn parse_u32_option(value: &str, option: &str) -> AndromedaResult<u32> {
    value
        .parse::<u32>()
        .map_err(|_| cli_error(format!("{option} expects an unsigned integer")))
}

pub(crate) fn next_option_value<'a>(
    args: &'a [String],
    index: &mut usize,
    missing_message: &'static str,
) -> AndromedaResult<&'a str> {
    *index += 1;
    args.get(*index)
        .map(String::as_str)
        .ok_or_else(|| cli_error(missing_message))
}

pub(crate) fn next_option_value_rejecting_flag<'a>(
    args: &'a [String],
    index: &mut usize,
    missing_message: &'static str,
) -> AndromedaResult<&'a str> {
    let value = next_option_value(args, index, missing_message)?;
    if value.starts_with("--") {
        Err(cli_error(missing_message))
    } else {
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_u64_preserves_caller_error_message() {
        let err = parse_u64("not-a-number", "custom unsigned integer error").unwrap_err();
        assert_eq!(err.message(), "custom unsigned integer error");
    }

    #[test]
    fn parse_usize_preserves_caller_error_message() {
        let err = parse_usize("not-a-number", "custom usize error").unwrap_err();
        assert_eq!(err.message(), "custom usize error");
    }

    #[test]
    fn named_integer_parsers_include_option_name() {
        let err = parse_u64_option("not-a-number", "--lsn").unwrap_err();
        assert_eq!(err.message(), "--lsn expects an unsigned integer");

        let err = parse_u32_option("not-a-number", "--samples").unwrap_err();
        assert_eq!(err.message(), "--samples expects an unsigned integer");
    }

    #[test]
    fn next_option_value_advances_to_value_or_reports_missing() {
        let args = ["--id".to_string(), "42".to_string()];
        let mut index = 0;

        assert_eq!(
            next_option_value(&args, &mut index, "--id requires a value").unwrap(),
            "42"
        );
        assert_eq!(index, 1);

        let mut missing_index = 0;
        let err =
            next_option_value(&args[..1], &mut missing_index, "--id requires a value").unwrap_err();
        assert_eq!(err.message(), "--id requires a value");
    }

    #[test]
    fn next_option_value_rejecting_flag_rejects_following_flag() {
        let args = ["--namespace".to_string(), "--json".to_string()];
        let mut index = 0;

        let err = next_option_value_rejecting_flag(
            &args,
            &mut index,
            "--namespace requires a namespace argument",
        )
        .unwrap_err();

        assert_eq!(err.message(), "--namespace requires a namespace argument");
    }
}
