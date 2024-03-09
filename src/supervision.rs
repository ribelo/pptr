use std::{
    fmt,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use async_trait::async_trait;

use crate::{
    errors::{PuppetError, RetryError},
    message::ServiceCommand,
    pid::Pid,
    puppeter::Puppeter,
};

/// Defines supervision strategies for handling failures in a puppet system.
///
/// The module provides three strategies:
/// - [`OneToOne`]: Handles failures individually for each puppet.
/// - [`OneForAll`]: Restarts all puppets when any one fails.
/// - [`RestForOne`]: Restarts the failed puppet and all puppets started after it.
pub mod strategy {
    /// A one-to-one supervision strategy.
    ///
    /// This strategy handles failures individually for each puppet, without affecting others.
    #[derive(Debug, Clone, Copy)]
    pub struct OneToOne;
    /// A one-for-all supervision strategy.
    ///
    /// When any single puppet fails, this strategy restarts all puppets in the system.
    #[derive(Debug, Clone, Copy)]
    pub struct OneForAll;
    /// A rest-for-one supervision strategy.
    ///
    /// If a puppet fails, this strategy restarts that puppet and all puppets started after it,
    /// while leaving puppets started before it unaffected.
    #[derive(Debug, Clone, Copy)]
    pub struct RestForOne;
}

/// Configuration for retry behavior.
///
/// `RetryConfig` allows specifying the maximum number of retries, the duration within which
/// retries should occur, and the time to wait between retry attempts.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub inner: Arc<Mutex<RetryConfigInner>>,
}

/// The inner data of a `RetryConfig`.
///
/// This struct holds the actual retry configuration values and current retry state.
#[derive(Debug, Clone)]
pub struct RetryConfigInner {
    pub max_retries: Option<usize>,
    pub within_duration: Option<Duration>,
    pub with_time_between: Option<Duration>,
    pub current_retry_count: usize,
    pub last_retry: Instant,
}

/// A builder for creating `RetryConfig` instances.
///
/// `RetryConfigBuilder` provides a fluent interface for specifying retry configuration options.
#[derive(Default)]
pub struct RetryConfigBuilder {
    max_retries: Option<usize>,
    within_duration: Option<Duration>,
    with_time_between: Option<Duration>,
}

impl RetryConfigBuilder {
    /// Creates a new `RetryConfigBuilder` with default options.
    ///
    /// This is a convenience method that initializes a new builder instance.
    ///
    /// # Example
    ///
    /// ```
    /// use pptr::supervision::RetryConfigBuilder;
    ///
    /// let builder = RetryConfigBuilder::new();
    /// ```
    #[must_use]
    pub fn new() -> Self {
        RetryConfigBuilder::default()
    }

    /// Sets the maximum number of retries for the `RetryConfig`.
    ///
    /// # Example
    ///
    /// ```
    /// use pptr::supervision::RetryConfigBuilder;
    ///
    /// let builder = RetryConfigBuilder::new().with_max_retries(3);
    /// ```
    ///
    /// # Returns
    ///
    /// The updated `RetryConfigBuilder` instance.
    #[must_use]
    pub fn with_max_retries(mut self, retries: usize) -> Self {
        self.max_retries = Some(retries);
        self
    }

    /// Sets the duration within which retries are allowed for the `RetryConfig`.
    ///
    /// # Example
    ///
    /// ```
    /// use std::time::Duration;
    /// use pptr::supervision::RetryConfigBuilder;
    ///
    /// let builder = RetryConfigBuilder::new().within_duration(Duration::from_secs(30));
    /// ```
    ///
    /// # Returns
    ///
    /// The updated `RetryConfigBuilder` instance.
    #[must_use]
    pub fn within_duration(mut self, duration: Duration) -> Self {
        self.within_duration = Some(duration);
        self
    }

    /// Sets the time to wait between retries for the `RetryConfig`.
    ///
    /// # Example
    ///
    /// ```
    /// use std::time::Duration;
    /// use pptr::supervision::RetryConfigBuilder;
    ///
    /// let builder = RetryConfigBuilder::new().with_time_between(Duration::from_secs(5));
    /// ```
    ///
    /// # Returns
    ///
    /// The updated `RetryConfigBuilder` instance.
    #[must_use]
    pub fn with_time_between(mut self, time: Duration) -> Self {
        self.with_time_between = Some(time);
        self
    }

    /// Builds a new `RetryConfig` instance with the specified options.
    ///
    /// This method consumes the builder and creates a new `RetryConfig` based on the configured options.
    ///
    /// # Example
    ///
    /// ```
    /// use std::time::Duration;
    /// use pptr::supervision::RetryConfigBuilder;
    ///
    /// let config = RetryConfigBuilder::new()
    ///     .with_max_retries(3)
    ///     .within_duration(Duration::from_secs(30))
    ///     .with_time_between(Duration::from_secs(5))
    ///     .build();
    /// ```
    ///
    /// # Returns
    ///
    /// A new `RetryConfig` instance.
    #[must_use]
    pub fn build(self) -> RetryConfig {
        let inner = RetryConfigInner {
            max_retries: self.max_retries,
            within_duration: self.within_duration,
            with_time_between: self.with_time_between,
            current_retry_count: 0,
            last_retry: Instant::now(),
        };
        RetryConfig {
            inner: Arc::new(Mutex::new(inner)),
        }
    }
}

impl RetryConfig {
    /// Resets the current retry count and last retry timestamp.
    fn reset_count(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.current_retry_count = 0;
        inner.last_retry = Instant::now();
    }

    /// Increments the retry count and updates the last retry timestamp.
    ///
    /// Returns an error if the maximum number of retries has been reached.
    pub fn increment_retry(&self) -> Result<(), RetryError> {
        let mut inner = self.inner.lock().unwrap();

        if let Some(duration) = inner.within_duration {
            if inner.last_retry.elapsed() > duration {
                inner.current_retry_count = 0;
            }
        }

        if inner.current_retry_count < inner.max_retries.unwrap_or(0) {
            inner.current_retry_count += 1;
            inner.last_retry = Instant::now();
            Ok(())
        } else {
            Err(RetryError::new("Max retry reached"))
        }
    }

    /// Waits for the configured time between retries, if specified.
    pub async fn maybe_wait(&self) {
        let duration = self.inner.lock().unwrap().with_time_between;
        if let Some(duration) = duration {
            tokio::time::sleep(duration).await;
        }
    }
}

/// A trait for implementing supervision strategies.
///
/// Supervision strategies define how to handle failures in a puppet system.
#[async_trait]
pub trait SupervisionStrategy: fmt::Debug {
    /// Handles a failure in the puppet system.
    ///
    /// This method is called when a puppet fails, allowing the supervision strategy to decide
    /// how to handle the failure and potentially restart puppets.
    ///
    /// # Errors
    ///
    /// Returns a `PuppetError` if the failure cannot be handled.
    async fn handle_failure(
        post_office: &Puppeter,
        master: Pid,
        puppet: Pid,
    ) -> Result<(), PuppetError>;
}

#[async_trait]
impl SupervisionStrategy for strategy::OneToOne {
    async fn handle_failure(
        post_office: &Puppeter,
        master: Pid,
        puppet: Pid,
    ) -> Result<(), PuppetError> {
        Ok(post_office
            .send_command_by_pid(master, puppet, ServiceCommand::Restart { stage: None })
            .await?)
    }
}

#[async_trait]
impl SupervisionStrategy for strategy::OneForAll {
    async fn handle_failure(
        post_office: &Puppeter,
        master: Pid,
        _puppet: Pid,
    ) -> Result<(), PuppetError> {
        if let Some(puppets) = post_office.get_puppets_by_pid(master) {
            for pid in puppets.into_iter().rev() {
                post_office
                    .send_command_by_pid(master, pid, ServiceCommand::Restart { stage: None })
                    .await?;
            }
        }
        Ok(())
    }
}

#[async_trait]
impl SupervisionStrategy for strategy::RestForOne {
    async fn handle_failure(
        post_office: &Puppeter,
        master: Pid,
        puppet: Pid,
    ) -> Result<(), PuppetError> {
        if let Some(puppets) = post_office.get_puppets_by_pid(master) {
            let mut restart_next = false;
            for pid in puppets.into_iter().rev() {
                if pid == puppet {
                    restart_next = true;
                }
                if restart_next {
                    post_office
                        .send_command_by_pid(master, pid, ServiceCommand::Restart { stage: None })
                        .await?;
                }
            }
        }
        Ok(())
    }
}
