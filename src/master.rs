use std::num::NonZeroUsize;

use tokio::sync::{mpsc, watch};

use crate::{
    address::Address,
    errors::{PuppetCannotHandleMessage, PuppetError},
    message::{Envelope, Mailbox, Message, Postman, ServiceMailbox, ServicePacket, ServicePostman},
    pid::Pid,
    puppet::{
        self, Handler, Lifecycle, LifecycleStatus, Puppet, PuppetBuilder, PuppetHandle, PuppetState,
    },
};

pub(crate) async fn run_puppet_loop<P>(mut puppet: Puppet<P>, mut handle: PuppetHandle<P>)
where
    P: PuppetState,
    Puppet<P>: Lifecycle,
{
    let mut puppet_status = puppet
        .post_office
        .subscribe_status_by_pid(puppet.pid)
        .unwrap();

    loop {
        tokio::select! {
            res = puppet_status.changed() => {
                match res {
                    Ok(_) => {
                        if matches!(*puppet_status.borrow(), LifecycleStatus::Inactive
                            | LifecycleStatus::Failed) {
                            println!("Stopping loop due to puppet status change");
                            tracing::info!(puppet = puppet.pid.to_string(),  "Stopping loop due to puppet status change");
                            break;
                        }
                    }
                    Err(_) => {
                        println!("Stopping loop due to closed puppet status channel");
                        tracing::debug!(puppet = puppet.pid.to_string(),  "Stopping loop due to closed puppet status channel");
                        break;
                    }
                }
            }
            res = handle.command_rx.recv() => {
                match res {
                    Some(mut service_packet) => {
                        if matches!(*puppet_status.borrow(), LifecycleStatus::Active) {
                            service_packet.handle_command(&mut puppet).await;
                        } else {
                            tracing::debug!(puppet = puppet.pid.to_string(),  "Ignoring command due to non-Active puppet status");
                            let error_response = PuppetCannotHandleMessage::new(puppet.pid, *puppet_status.borrow()).into();
                            service_packet.reply_error(error_response).await;
                        }
                    }
                    None => {
                        tracing::debug!(puppet = puppet.pid.to_string(),  "Stopping loop due to closed command channel");
                        break;
                    }
                }
            }
            res = handle.message_rx.recv() => {
                match res {
                    Some(mut envelope) => {
                        println!("Got message");
                        if matches!(*puppet_status.borrow(), LifecycleStatus::Active) {
                            // TODO: Handle error
                            envelope.handle_message(&mut puppet).await;
                        } else {
                            let status = *puppet_status.borrow();
                            tracing::debug!(puppet = puppet.pid.to_string(),  "Ignoring message due to non-Active puppet status");
                            envelope.reply_error(PuppetCannotHandleMessage::new(puppet.pid, status).into()).await;
                        }
                    }
                    None => {
                        tracing::debug!(puppet = puppet.pid.to_string(),  "Stopping loop due to closed message channel");
                        break;
                    }
                }
            }
        }
    }
}
