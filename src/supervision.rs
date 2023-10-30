use std::{
    cell::RefCell,
    fmt,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use async_trait::async_trait;

use crate::{
    errors::{PuppetError, PuppetSendCommandError, RetryError},
    message::ServiceCommand,
    pid::Pid,
    post_office::PostOffice,
    puppet::{Lifecycle, Puppet, PuppetState},
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
    inner: Arc<Mutex<RetryConfigInner>>,
}

#[derive(Debug)]
pub struct RetryConfigInner {
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
            inner: Arc::new(Mutex::new(RetryConfigInner {
                max_retries: self.max_retries,
                within_duration: self.within_duration,
                with_time_between: self.with_time_between,
                current_retry_count: 0,
                last_retry: Instant::now(),
            })),
        }
    }
}

impl Default for RetryConfig {
    fn default() -> Self {
        RetryConfig {
            inner: Arc::new(Mutex::new(RetryConfigInner {
                max_retries: None,
                within_duration: None,
                with_time_between: None,
                current_retry_count: 0,
                last_retry: Instant::now(),
            })),
        }
    }
}

impl RetryConfig {
    fn reset_count(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.current_retry_count = 0;
        inner.last_retry = Instant::now();
    }

    pub fn increment_retry(&self) -> Result<(), RetryError> {
        let mut inner = self.inner.lock().unwrap();
        let elapsed = inner.last_retry.elapsed();

        if let Some(duration) = inner.within_duration {
            if elapsed > duration {
                self.reset_count();
            }
        }

        if inner.current_retry_count < inner.max_retries.unwrap_or(0) {
            inner.current_retry_count += 1;
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

#[derive(Debug)]
pub struct SupervisionConfig {
    pub strategy: Box<dyn SupervisionStrategy + Send + Sync + 'static>,
    pub retry: RetryConfig,
}

impl Default for SupervisionConfig {
    fn default() -> Self {
        SupervisionConfig {
            strategy: Box::new(strategy::OneToOne),
            retry: RetryConfig::default(),
        }
    }
}

#[async_trait]
pub trait SupervisionStrategy: fmt::Debug {
    async fn handle_failure(
        &self,
        post_office: &PostOffice,
        master: Pid,
        puppet: Pid,
    ) -> Result<(), PuppetSendCommandError>;
}

#[async_trait]
impl SupervisionStrategy for strategy::OneToOne {
    async fn handle_failure(
        &self,
        post_office: &PostOffice,
        master: Pid,
        puppet: Pid,
    ) -> Result<(), PuppetSendCommandError> {
        post_office
            .send_command_by_pid(master, puppet, ServiceCommand::Restart { stage: None })
            .await
    }
}

// impl SupervisionStrategy for OneForAll {
//     async fn handle_failure(
//         &self,
//         post_office: &PostOffice,
//         master: Pid,
//         puppet: Pid,
//     ) -> Result<(), PuppetSendCommandError> {
//         async fn restart_all_children_recursively(
//             post_office: &PostOffice,
//             master: Pid,
//             puppet: Pid,
//         ) -> Result<(), PuppetSendCommandError> {
//             post_office
//                 .send_command_by_pid(master, puppet, ServiceCommand::Restart)
//                 .await?;
//             let child_puppets = post_office.get_puppets_by_pid(puppet);
//             for child_id in child_puppets {
//                 restart_all_children_recursively(child_id, puppeter,
// master).await;             }
//             Ok(())
//         }
//
//         // Start the recursive restart from the master actor
//         restart_all_children_recursively(post_office, master, puppet).await;
//     }
// }
//
// impl SupervisionStrategy for RestForOne {
//     async fn handle_failure(
//         &mut self,
//         puppeter: Puppeter,
//         master: Id,
//         puppet: Id,
//         err: PuppeterError,
//     ) {
//         let mut restart_next = false;
//         let puppets = puppeter.get_puppets_by_id(master);
//
//         for id in puppets.into_iter().rev() {
//             if restart_next {
//                 if let Some(service_address) =
// puppeter.get_command_address_by_id(&id) {                     service_address
//
// .send_command(crate::prelude::ServiceCommand::RequestRestart {
// sender: master,                         })
//                         .await
//                         .unwrap();
//                 }
//             }
//
//             if id == puppet {
//                 restart_next = true;
//                 if let Some(service_address) =
// puppeter.get_command_address_by_id(&id) {                     service_address
//
// .send_command(crate::prelude::ServiceCommand::RequestRestart {
// sender: master,                         })
//                         .await
//                         .unwrap();
//                 }
//             }
//         }
//     }
// }
