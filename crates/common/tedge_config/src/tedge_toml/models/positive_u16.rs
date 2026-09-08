use std::num::NonZeroU16;
use std::str::FromStr;

/// A parse helper for `NonZeroU16` configuration values with a user-facing error.
///
/// `NonZeroU16::from_str` reports low-level parse failures such as
/// "number would be zero for non-zero type" or "invalid digit found in string",
/// which are unhelpful to someone running `tedge config set`.
/// Use it with `#[tedge_config(from = "PositiveU16")]` to keep the field typed as
/// `NonZeroU16` while reporting a single, clear error for every invalid input.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct PositiveU16(NonZeroU16);

#[derive(thiserror::Error, Debug)]
#[error("Invalid value '{input}': must be a positive integer between 1 and 65535")]
pub struct NotPositiveU16 {
    input: String,
}

impl FromStr for PositiveU16 {
    type Err = NotPositiveU16;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        input
            .trim()
            .parse::<NonZeroU16>()
            .map(PositiveU16)
            .map_err(|_| NotPositiveU16 {
                input: input.to_owned(),
            })
    }
}

impl From<PositiveU16> for NonZeroU16 {
    fn from(value: PositiveU16) -> Self {
        value.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_positive_values() {
        assert_eq!(
            NonZeroU16::from("1".parse::<PositiveU16>().unwrap()),
            NonZeroU16::new(1).unwrap()
        );
        assert_eq!(
            NonZeroU16::from("65535".parse::<PositiveU16>().unwrap()),
            NonZeroU16::new(65535).unwrap()
        );
    }

    #[test]
    fn rejects_invalid_values_with_a_clear_message() {
        for input in ["0", "-1", "65536", "abc", ""] {
            let err = input.parse::<PositiveU16>().unwrap_err();
            assert_eq!(
                err.to_string(),
                format!("Invalid value '{input}': must be a positive integer between 1 and 65535")
            );
        }
    }
}
