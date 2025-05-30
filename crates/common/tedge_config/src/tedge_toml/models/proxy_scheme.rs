use std::fmt::Display;
use std::fmt::Formatter;
use std::str::FromStr;

/// A flag that can be HTTP or HTTPS
#[derive(
    Debug, Clone, Copy, serde::Serialize, serde::Deserialize, Eq, PartialEq, doku::Document,
)]
pub enum ProxyScheme {
    Http,
    Https,
}

#[derive(thiserror::Error, Debug)]
#[error("Failed to parse flag: {input}. Supported values are: HTTP, HTTPS")]
pub struct InvalidScheme {
    input: String,
}

impl FromStr for ProxyScheme {
    type Err = InvalidScheme;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input.to_lowercase().as_str() {
            "http" => Ok(Self::Http),
            "https" => Ok(Self::Https),
            _ => Err(Self::Err {
                input: input.to_string(),
            }),
        }
    }
}

impl Display for ProxyScheme {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let output = match self {
            Self::Http => "http",
            Self::Https => "https",
        };
        output.fmt(f)
    }
}
