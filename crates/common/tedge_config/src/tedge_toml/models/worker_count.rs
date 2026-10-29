use std::fmt;
use std::num::NonZeroU16;
use std::num::NonZeroUsize;
use std::str::FromStr;

/// A number of workers, either given explicitly or `auto`,
/// meaning one worker per CPU core available to the process
#[derive(Debug, Copy, Clone, Eq, PartialEq, serde::Deserialize)]
#[serde(try_from = "DeserializeWorkerCount")]
pub enum WorkerCount {
    Auto,
    Count(NonZeroU16),
}

impl WorkerCount {
    /// The number of workers to use
    ///
    /// For `auto`, the number of CPU cores available to the process,
    /// which takes into account CPU affinity and cgroup quotas on Linux.
    pub fn count(&self) -> NonZeroUsize {
        match self {
            WorkerCount::Auto => std::thread::available_parallelism().unwrap_or(NonZeroUsize::MIN),
            WorkerCount::Count(count) => NonZeroUsize::from(*count),
        }
    }
}

#[derive(thiserror::Error, Debug)]
#[error("Invalid value '{input}': must be 'auto' or a positive integer between 1 and 65535")]
pub struct InvalidWorkerCount {
    input: String,
}

impl FromStr for WorkerCount {
    type Err = InvalidWorkerCount;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let input = input.trim();
        if input == "auto" {
            return Ok(WorkerCount::Auto);
        }
        input
            .parse::<NonZeroU16>()
            .map(WorkerCount::Count)
            .map_err(|_| InvalidWorkerCount {
                input: input.to_owned(),
            })
    }
}

impl fmt::Display for WorkerCount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WorkerCount::Auto => "auto".fmt(f),
            WorkerCount::Count(count) => count.fmt(f),
        }
    }
}

impl serde::Serialize for WorkerCount {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            WorkerCount::Auto => serializer.serialize_str("auto"),
            WorkerCount::Count(count) => serializer.serialize_u16(count.get()),
        }
    }
}

impl doku::Document for WorkerCount {
    fn ty() -> doku::Type {
        String::ty()
    }
}

/// A worker count is written as an integer or as the string `auto` in `tedge.toml`
#[derive(serde::Deserialize)]
#[serde(untagged)]
enum DeserializeWorkerCount {
    Count(u64),
    Text(String),
}

impl TryFrom<DeserializeWorkerCount> for WorkerCount {
    type Error = InvalidWorkerCount;

    fn try_from(value: DeserializeWorkerCount) -> Result<Self, Self::Error> {
        match value {
            DeserializeWorkerCount::Count(count) => count.to_string().parse(),
            DeserializeWorkerCount::Text(text) => text.parse(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_auto_and_positive_values() {
        assert_eq!("auto".parse::<WorkerCount>().unwrap(), WorkerCount::Auto);
        assert_eq!(
            "4".parse::<WorkerCount>().unwrap(),
            WorkerCount::Count(NonZeroU16::new(4).unwrap())
        );
    }

    #[test]
    fn rejects_invalid_values_with_a_clear_message() {
        for input in ["0", "-1", "65536", "Auto", ""] {
            let err = input.parse::<WorkerCount>().unwrap_err();
            assert_eq!(
                err.to_string(),
                format!(
                    "Invalid value '{input}': must be 'auto' or a positive integer between 1 and 65535"
                )
            );
        }
    }

    #[test]
    fn auto_uses_at_least_one_worker() {
        assert!(WorkerCount::Auto.count().get() >= 1);
    }

    #[test]
    fn round_trips_through_toml() {
        #[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq)]
        struct Config {
            workers: WorkerCount,
        }
        for (workers, text) in [
            (WorkerCount::Auto, "workers = \"auto\"\n"),
            (
                WorkerCount::Count(NonZeroU16::new(4).unwrap()),
                "workers = 4\n",
            ),
        ] {
            let config = Config { workers };
            assert_eq!(toml::to_string(&config).unwrap(), text);
            assert_eq!(toml::from_str::<Config>(text).unwrap(), config);
        }
        assert!(toml::from_str::<Config>("workers = 0").is_err());
    }
}
