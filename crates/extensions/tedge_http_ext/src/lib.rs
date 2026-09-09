mod actor;
mod messages;

#[cfg(test)]
mod tests;

#[cfg(feature = "test_helpers")]
pub mod test_helpers;

use std::time::Duration;

use backoff::ExponentialBackoff;
pub use messages::*;

use actor::*;
use tedge_actors::Concurrent;
use tedge_actors::ServerActorBuilder;
use tedge_actors::ServerConfig;

pub use backoff;

/// How long a request may wait for the response headers before it is given up
///
/// Without a bound, a server that accepts the connection but never answers holds the caller
/// forever, and the actors awaiting the response inline stop processing anything else.
pub const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug)]
pub struct HttpActor {
    config: ServerConfig,
    tls_client_config: rustls::ClientConfig,
    backoff: ExponentialBackoff,
    request_timeout: Duration,
}

impl HttpActor {
    pub fn new(tls_client_config: rustls::ClientConfig) -> Self {
        Self {
            config: <_>::default(),
            tls_client_config,
            backoff: ExponentialBackoff {
                initial_interval: Duration::from_secs(2),
                max_elapsed_time: Some(Duration::from_secs(30)),
                randomization_factor: 0.1,
                ..Default::default()
            },
            request_timeout: DEFAULT_REQUEST_TIMEOUT,
        }
    }

    pub fn builder(&self) -> ServerActorBuilder<HttpService, Concurrent> {
        ServerActorBuilder::new(
            HttpService::new(
                self.tls_client_config.clone(),
                self.backoff.clone(),
                self.request_timeout,
            ),
            &self.config,
            Concurrent,
        )
    }

    /// Bounds how long a request waits for the response headers
    pub fn with_request_timeout(self, request_timeout: Duration) -> Self {
        Self {
            request_timeout,
            ..self
        }
    }

    pub fn request_timeout(&self) -> Duration {
        self.request_timeout
    }

    pub fn with_capacity(self, capacity: usize) -> Self {
        Self {
            config: self.config.with_capacity(capacity),
            ..self
        }
    }

    pub fn with_max_concurrency(self, max_concurrency: usize) -> Self {
        Self {
            config: self.config.with_max_concurrency(max_concurrency),
            ..self
        }
    }

    pub fn with_backoff(self, backoff: ExponentialBackoff) -> Self {
        Self { backoff, ..self }
    }
}
