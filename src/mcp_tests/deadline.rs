//! Deadline for one protocol call in the MCP test harness.

use std::sync::OnceLock;
use std::time::Duration;

pub(super) const DEADLINE_ENV: &str = "AZTECCRAFT_MCP_TEST_DEADLINE_MS";
const DEFAULT_MS: u64 = 2_000;
const MAX_MS: u64 = 600_000;

pub(super) fn parse_deadline(raw: Option<&str>) -> Result<Duration, String> {
    let Some(raw) = raw else {
        return Ok(Duration::from_millis(DEFAULT_MS));
    };
    let ms: u64 = raw
        .trim()
        .parse()
        .map_err(|_| format!("{DEADLINE_ENV}={raw:?} is not a whole number of milliseconds"))?;
    if ms == 0 || ms > MAX_MS {
        return Err(format!(
            "{DEADLINE_ENV}={ms} is outside 1..={MAX_MS} milliseconds"
        ));
    }
    Ok(Duration::from_millis(ms))
}

pub(super) fn protocol_call_deadline() -> Duration {
    static DEADLINE: OnceLock<Duration> = OnceLock::new();
    *DEADLINE.get_or_init(|| {
        let raw = match std::env::var(DEADLINE_ENV) {
            Ok(raw) => Some(raw),
            Err(std::env::VarError::NotPresent) => None,
            Err(error) => panic!("test setup: {DEADLINE_ENV} is unreadable: {error}"),
        };
        parse_deadline(raw.as_deref()).unwrap_or_else(|message| panic!("test setup: {message}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unset_uses_the_two_second_default() {
        assert_eq!(parse_deadline(None), Ok(Duration::from_secs(2)));
    }

    #[test]
    fn configured_value_is_honoured() {
        assert_eq!(parse_deadline(Some("15000")), Ok(Duration::from_secs(15)));
        assert_eq!(parse_deadline(Some(" 1 ")), Ok(Duration::from_millis(1)));
        assert_eq!(parse_deadline(Some("600000")), Ok(Duration::from_secs(600)));
    }

    #[test]
    fn invalid_values_are_refused_naming_the_variable() {
        for raw in [
            "0",
            "600001",
            "",
            "abc",
            "-5",
            "1.5",
            "99999999999999999999",
        ] {
            let message = parse_deadline(Some(raw)).expect_err(raw);
            assert!(message.contains(DEADLINE_ENV), "{raw}: {message}");
        }
    }
}
