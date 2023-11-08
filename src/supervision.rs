use std::{
    fmt,
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
    pub max_retries: Option<usize>,
    pub within_duration: Option<Duration>,
    pub with_time_between: Option<Duration>,
    current_retry_count: usize,
    last_retry: Instant,
}

#[derive(Default)]
pub struct RetryConfigBuilder {
    max_retries: Option<usize>,
    within_duration: Option<Duration>,
    with_time_between: Option<Duration>,
}

impl RetryConfigBuilder {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn with_max_retries(mut self, retries: usize) -> Self {
        self.max_retries = Some(retries);
        self
    }

    pub fn within_duration(mut self, duration: Duration) -> Self {
        self.within_duration = Some(duration);
        self
    }

    pub fn with_time_between(mut self, time: Duration) -> Self {
        self.with_time_between = Some(time);
        self
    }

    pub fn build(self) -> RetryConfig {
        RetryConfig {
            max_retries: self.max_retries,
            within_duration: self.within_duration,
            with_time_between: self.with_time_between,
            current_retry_count: 0,
            last_retry: Instant::now(),
        }
    }
}

impl Default for RetryConfig {
    fn default() -> Self {
        RetryConfig {
            max_retries: None,
            within_duration: None,
            with_time_between: None,
            current_retry_count: 0,
            last_retry: Instant::now(),
        }
    }
}

impl RetryConfig {
    fn reset_count(&mut self) {
        self.current_retry_count = 0;
        self.last_retry = Instant::now();
    }

    pub fn increment_retry(&mut self) -> Result<(), RetryError> {
        let elapsed = self.last_retry.elapsed();

        if let Some(duration) = self.within_duration {
            if elapsed > duration {
                self.reset_count();
            }
        }

        if self.current_retry_count < self.max_retries.unwrap_or(0) {
            self.current_retry_count += 1;
            Ok(())
        } else {
            Err(RetryError::new("Max retry reached"))
        }
    }

    pub async fn maybe_wait(&self) {
        let duration = self.with_time_between;
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
                    .await?
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
                        .await?
                }

                if pid == puppet {
                    restart_next = true;
                    post_office
                        .send_command_by_pid(master, pid, ServiceCommand::Restart { stage: None })
                        .await?
                }
            }
        }
        Ok(())
    }
}
