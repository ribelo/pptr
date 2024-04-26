//! Supervision module for handling failures in a puppet system.
//!
//! This module provides supervision strategies and retry configuration for managing failures
//! in a puppet system. The available supervision strategies are:
//!
//! - [`OneToOne`]: Handles failures individually for each puppet.
//! - [`OneForAll`]: Restarts all puppets when any one fails.
//! - [`RestForOne`]: Restarts the failed puppet and all puppets started after it.
//!
//! The module also includes a [`RetryConfig`] struct and [`RetryConfigBuilder`] for configuring
//! retry behavior, such as the maximum number of retries, the duration within which retries
//! should occur, and the time to wait between retry attempts.
//!
//! The [`SupervisionStrategy`] trait defines the interface for implementing custom supervision
//! strategies.
//! ```

use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use crate::{
    errors::{PuppetError, RetryError},
    message::ServiceCommand,
    pid::Pid,
    puppeteer::Puppeteer,
};
use async_trait::async_trait;

/// Defines supervision strategies for handling failures in a puppet system.
///
/// The module provides three strategies:
/// - [`OneToOne`]: Handles failures individually for each puppet.
/// - [`OneForAll`]: Restarts all puppets when any one fails.
/// - [`RestForOne`]: Restarts the failed puppet and all puppets started after it.
pub mod strategy {
    /// A no-supervision strategy.
    ///
    /// This strategy does not handle failures and allows puppets to fail without intervention.
    #[derive(Debug, Clone, Copy)]
    pub struct NoSupervision;

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

/// A trait for implementing supervision strategies.
///
/// Supervision strategies define how to handle failures in a puppet system.
#[async_trait]
pub trait SupervisionStrategy: Send + Sync {
    /// Handles a failure in the puppet system.
    ///
    /// This method is called when a puppet fails, allowing the supervision strategy to decide
    /// how to handle the failure and potentially restart puppets.
    ///
    /// # Errors
    ///
    /// Returns a `PuppetError` if the failure cannot be handled.
    async fn handle_failure(pptr: &Puppeteer, master: Pid, puppet: Pid) -> Result<(), PuppetError>;
}

#[async_trait]
impl SupervisionStrategy for strategy::NoSupervision {
    async fn handle_failure(
        _pptr: &Puppeteer,
        _master: Pid,
        _puppet: Pid,
    ) -> Result<(), PuppetError> {
        Ok(())
    }
}

#[async_trait]
impl SupervisionStrategy for strategy::OneToOne {
    async fn handle_failure(pptr: &Puppeteer, master: Pid, puppet: Pid) -> Result<(), PuppetError> {
        Ok(pptr
            .send_command_by_pid(master, puppet, ServiceCommand::Restart { stage: None })
            .await?)
    }
}

#[async_trait]
impl SupervisionStrategy for strategy::OneForAll {
    async fn handle_failure(
        pptr: &Puppeteer,
        master: Pid,
        _puppet: Pid,
    ) -> Result<(), PuppetError> {
        if let Some(puppets) = pptr.get_puppets_by_pid(master) {
            for pid in puppets.into_iter().rev() {
                pptr.send_command_by_pid(master, pid, ServiceCommand::Restart { stage: None })
                    .await?;
            }
        }
        Ok(())
    }
}

#[async_trait]
impl SupervisionStrategy for strategy::RestForOne {
    async fn handle_failure(pptr: &Puppeteer, master: Pid, puppet: Pid) -> Result<(), PuppetError> {
        if let Some(puppets) = pptr.get_puppets_by_pid(master) {
            let mut restart_next = false;
            for pid in puppets.into_iter().rev() {
                if pid == puppet {
                    restart_next = true;
                }
                if restart_next {
                    pptr.send_command_by_pid(master, pid, ServiceCommand::Restart { stage: None })
                        .await?;
                }
            }
        }
        Ok(())
    }
}
