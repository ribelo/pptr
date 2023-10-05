use async_trait::async_trait;

use crate::{
    message::{MessageError, Messageable},
    minion::Minion,
};

#[async_trait]
pub trait Dispatcher {
    async fn send<A>(&self, message: A::Msg) -> Result<(), MessageError>
    where
        A: Minion + Clone,
        <A as Minion>::Msg: Clone;

    // async fn ask<A>(
    //     &self,
    //     message: A::Msg,
    // ) -> Result<<<A as Minion>::Msg as Messageable>::Response, MessageError>
    // where
    //     A: Minion,
    //     <A as Minion>::Msg: std::clone::Clone;
    //
    // async fn ask_with_timeout<A>(
    //     &self,
    //     message: A::Msg,
    //     timeout: std::time::Duration,
    // ) -> Result<<<A as Minion>::Msg as Messageable>::Response, MessageError>
    // where
    //     A: Minion,
    //     <A as Minion>::Msg: std::clone::Clone;
    //
    // async fn ask_service<A>(&self, command: ServiceCommand) -> Result<(), MessageError>
    // where
    //     A: Minion,
    //     <A as Minion>::Msg: std::clone::Clone;
}
