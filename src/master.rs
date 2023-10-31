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

pub(crate) fn create_puppet_entities<M, P>(
    master: &Puppet<M>,
    builder: &mut PuppetBuilder<P>,
) -> (
    Puppet<P>,
    PuppetHandle<P>,
    Postman<P>,
    ServicePostman,
    Address<P>,
)
where
    M: PuppetState,
    Puppet<M>: Lifecycle,
    P: PuppetState,
    Puppet<P>: Lifecycle,
{
    let pid = Pid::new::<P>();
    let (status_tx, status_rx) = watch::channel::<LifecycleStatus>(LifecycleStatus::Inactive);
    let (tx, rx) = mpsc::channel::<Box<dyn Envelope<P>>>(builder.messages_bufer_size.into());
    let (command_tx, command_rx) =
        mpsc::channel::<ServicePacket>(builder.commands_bufer_size.into());
    let supervision_config = builder.supervision_config.take().unwrap();

    let puppet = Puppet {
        pid,
        state: builder.state.clone(),
        status_tx,
        context: master.context.clone(),
        post_office: master.post_office.clone(),
        supervision_config,
    };

    let handler = PuppetHandle {
        pid,
        status_rx: status_rx.clone(),
        message_rx: Mailbox::new(rx),
        command_rx: ServiceMailbox::new(command_rx),
    };

    let postman = Postman::new(tx);

    let address = Address {
        pid,
        status_rx,
        message_tx: postman.clone(),
    };

    let service_postman = ServicePostman::new(command_tx);
    (puppet, handler, postman, service_postman, address)
}

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
