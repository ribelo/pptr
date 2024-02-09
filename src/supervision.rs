use std::{
    fmt,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use async_trait::async_trait;

use crate::{
    errors::{PuppetError, RetryError},
    master_of_puppets::MasterOfPuppets,
    message::ServiceCommand,
    pid::Pid,
};

pub mod strategy {
    #[derive(Debug, Clone, Copy)]
    pub struct OneToOne;
    #[derive(Debug, Clone, Copy)]
    pub struct OneForAll;
    #[derive(Debug, Clone, Copy)]
    pub struct RestForOne;
}

#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub inner: Arc<Mutex<RetryConfigInner>>,
}

#[derive(Debug, Clone)]
pub struct RetryConfigInner {
    pub max_retries: Option<usize>,
    pub within_duration: Option<Duration>,
    pub with_time_between: Option<Duration>,
    pub current_retry_count: usize,
    pub last_retry: Instant,
}

#[derive(Default)]
pub struct RetryConfigBuilder {
    max_retries: Option<usize>,
    within_duration: Option<Duration>,
    with_time_between: Option<Duration>,
}

impl RetryConfigBuilder {
    #[must_use]
    pub fn new() -> Self {
        RetryConfigBuilder::default()
    }

    #[must_use]
    pub const fn with_max_retries(mut self, retries: usize) -> Self {
        self.max_retries = Some(retries);
        self
    }

    #[must_use]
    pub const fn within_duration(mut self, duration: Duration) -> Self {
        self.within_duration = Some(duration);
        self
    }

    #[must_use]
    pub const fn with_time_between(mut self, time: Duration) -> Self {
        self.with_time_between = Some(time);
        self
    }

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

impl Default for RetryConfig {
    fn default() -> Self {
        let inner = RetryConfigInner {
            max_retries: None,
            within_duration: None,
            with_time_between: None,
            current_retry_count: 0,
            last_retry: Instant::now(),
        };
        Self {
            inner: Arc::new(Mutex::new(inner)),
        }
    }
}

impl RetryConfig {
    fn reset_count(&self) {
        self.inner.lock().unwrap().current_retry_count = 0;
        self.inner.lock().unwrap().last_retry = Instant::now();
    }

    pub fn increment_retry(&self) -> Result<(), RetryError> {
        let elapsed = self.inner.lock().unwrap().last_retry.elapsed();

        if let Some(duration) = self.inner.lock().unwrap().within_duration {
            if elapsed > duration {
                self.reset_count();
            }
        }

        if self.inner.lock().unwrap().current_retry_count
            < self.inner.lock().unwrap().max_retries.unwrap_or(0)
        {
            self.inner.lock().unwrap().current_retry_count += 1;
            Ok(())
        } else {
            Err(RetryError::new("Max retry reached"))
        }
    }

    pub async fn maybe_wait(&self) {
        let duration = self.inner.lock().unwrap().with_time_between;
        if let Some(duration) = duration {
            tokio::time::sleep(duration).await;
        }
    }
}

#[async_trait]
pub trait SupervisionStrategy: fmt::Debug {
    async fn handle_failure(
        post_office: &MasterOfPuppets,
        master: Pid,
        puppet: Pid,
    ) -> Result<(), PuppetError>;
}

#[async_trait]
impl SupervisionStrategy for strategy::OneToOne {
    async fn handle_failure(
        post_office: &MasterOfPuppets,
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
        post_office: &MasterOfPuppets,
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
        post_office: &MasterOfPuppets,
        master: Pid,
        puppet: Pid,
    ) -> Result<(), PuppetError> {
        let mut restart_next = false;
        if let Some(puppets) = post_office.get_puppets_by_pid(master) {
            for pid in puppets.into_iter().rev() {
                if restart_next {
                    post_office
                        .send_command_by_pid(master, pid, ServiceCommand::Restart { stage: None })
                        .await?;
                }

                if pid == puppet {
                    restart_next = true;
                    post_office
                        .send_command_by_pid(master, pid, ServiceCommand::Restart { stage: None })
                        .await?;
                }
            }
        }
        Ok(())
    }
}
