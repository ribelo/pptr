use std::any::TypeId;

use async_trait::async_trait;

use crate::{Id, PuppeterError};

use super::Puppet;

#[derive(Debug, Clone, strum::Display, PartialEq, Eq)]
pub enum LifecycleStatus {
    Activating,
    Active,
    Deactivating,
    Inactive,
    Restarting,
    Failed,
}

impl Copy for LifecycleStatus {}

impl LifecycleStatus {
    pub fn should_handle_message(&self) -> bool {
        matches!(self, LifecycleStatus::Active)
    }
    pub fn should_drop_message(&self) -> bool {
        matches!(
            self,
            LifecycleStatus::Inactive | LifecycleStatus::Deactivating | LifecycleStatus::Failed
        )
    }
    pub fn should_wait_for_activation(&self) -> bool {
        matches!(
            self,
            LifecycleStatus::Activating | LifecycleStatus::Restarting
        )
    }
}

#[async_trait]
pub trait PuppetLifecycle {
    fn get_status(&mut self) -> Result<(), PuppeterError>;
    fn set_status(&mut self, status: LifecycleStatus) -> Result<(), PuppeterError>;

    async fn _start(&mut self) -> Result<(), PuppeterError> {
        self.set_status(LifecycleStatus::Activating);
        self.pre_start().await?;
        self.start().await?;
        self.set_status(LifecycleStatus::Active);
        self.post_start().await?;
        Ok(())
    }

    async fn _stop(&mut self) -> Result<(), PuppeterError> {
        self.set_status(LifecycleStatus::Deactivating);
        self.pre_stop().await?;
        self.stop().await?;
        self.set_status(LifecycleStatus::Inactive);
        self.post_stop().await?;
        Ok(())
    }

    async fn _restart(&mut self) -> Result<(), PuppeterError> {
        self.set_status(LifecycleStatus::Restarting);
        self.pre_stop().await?;
        self.stop().await?;
        self.post_stop().await?;
        *self = Default::default();
        self.pre_start().await?;
        self.start().await?;
        self.set_status(LifecycleStatus::Active);
        self.post_start().await?;
        Ok(())
    }

    async fn _suicide(&mut self) -> Result<(), PuppeterError> {
        self._stop().await?;
        // TODO:
        Ok(())
    }

    async fn pre_start(&mut self) -> Result<(), PuppeterError> {
        Ok(())
    }

    async fn start(&mut self) -> Result<(), PuppeterError> {
        tracing::debug!("Starting puppet: {}", self.name());
        Ok(())
    }

    async fn post_start(&mut self) -> Result<(), PuppeterError> {
        Ok(())
    }

    async fn pre_stop(&mut self) -> Result<(), PuppeterError> {
        Ok(())
    }

    async fn stop(&mut self) -> Result<(), PuppeterError> {
        tracing::debug!("Stopping puppet {}", self.name());
        Ok(())
    }

    async fn post_stop(&mut self) -> Result<(), PuppeterError> {
        Ok(())
    }

    async fn restart(&mut self) -> Result<(), PuppeterError> {
        tracing::debug!("Restarting puppet {}", self.name());
        Ok(())
    }

    async fn suicide(&mut self) -> Result<(), PuppeterError> {
        tracing::debug!("Suicide puppet {}", self.name());
        Ok(())
    }
}
