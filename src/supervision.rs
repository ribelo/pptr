// use std::any::TypeId;
//
// use crate::{master::Puppeter, Id, PuppeterError};
//
// pub struct OneToOne;
// pub struct OneForAll;
// pub struct RestForOne;
//
// pub trait SupervisionStrategy {
//     async fn handle_failure(&mut self, master: Id, puppet: Id, err:
// PuppeterError); }
//
// impl SupervisionStrategy for OneToOne {
//     async fn handle_failure(
//         &mut self,
//         puppeter: Puppeter,
//         master: Id,
//         puppet: Id,
//         err: PuppeterError,
//     ) { // Send restart request only to the failed puppet
//       puppeter.send_command_by_id( puppet,
//       crate::prelude::ServiceCommand::RequestRestart { sender: master }, );
//     }
// }
//
// impl SupervisionStrategy for OneForAll {
//     async fn handle_failure(
//         &mut self,
//         puppeter: &Puppeter,
//         master: Id,
//         puppet: Id,
//         err: PuppeterError,
//     ) { async fn restart_all_children_recursively(actor_id: Id, puppeter:
//       &Puppeter, master: Id) { if let Some(service_address) =
//       puppeter.get_command_address_by_id(&actor_id) { service_address
//       .send_command(crate::prelude::ServiceCommand::RequestRestart { sender:
//       master }) .await .unwrap(); } let child_puppets =
//       puppeter.get_puppets_by_id(actor_id); for child_id in child_puppets {
//       restart_all_children_recursively(child_id, puppeter, master).await; } }
//
//         // Start the recursive restart from the master actor
//         restart_all_children_recursively(master, &puppeter, master).await;
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
//     ) { let mut restart_next = false; let puppets =
//       puppeter.get_puppets_by_id(master);
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
