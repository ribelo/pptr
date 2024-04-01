//! # Puppeter
//!
//! Puppeter is a flexible and powerful actor-based framework for building asynchronous systems in Rust.
//! With its type-driven API design, Puppeter provides a safe and convenient way to create and manage
//! actors that communicate through message passing. Whether you're building a complex distributed
//! system or a responsive user interface, Puppeter makes it easy to write efficient and maintainable
//! asynchronous code.
//!
//! ## Key Features
//!
//! - **Type-Driven Development**: Puppeter leverages Rust's strong type system to ensure
//!   compile-time safety and runtime reliability. It provides a seamless and expressive API for
//!   defining actors and their message-handling behaviors.
//!
//! - **Ergonomic API**: Puppeter offers a clean and intuitive API for creating actors and handling
//!   messages. It integrates seamlessly with popular Rust libraries and frameworks, enabling
//!   developers to build highly concurrent applications with ease.
//!
//! - **Effortless Asynchronous Programming**: Puppeter simplifies asynchronous programming in Rust
//!   by utilizing the Tokio runtime and working well with Rust's `async`/`await`. It allows you to
//!   write asynchronous code that is easy to read and understand.
//!
//! - **Performance-Driven**: Puppeter is designed with performance in mind. It efficiently handles
//!   messages concurrently and in parallel, offering different execution modes to suit your specific
//!   needs.
//!
//! - **Flexible Supervision**: Puppeter provides a versatile oversight system that allows you to
//!   supervise and organize actors hierarchically. It offers predefined supervision strategies and
//!   supports custom strategies to handle errors and maintain system stability.
//!
//! - **Robust Error Handling**: Puppeter includes built-in features for monitoring actors and
//!   handling errors. It provides mechanisms to report and manage critical and non critical errors,
//!   ensuring the stability and reliability of your system.
//!
//! - **Lifecycle Management**: Puppeter offers built-in preimplemented methods for managing the
//!   lifecycle of actors, including initialization, startup, shutdown, and state reset.
//!
//! - **Resource Management**: Puppeter provides a way to manage resources that can be shared among
//!   actors. In Rust, some libraries provide structures that are not easy and idiomatic to send
//!   between actor, making it challenging to pass them through messages or include them as part of
//!   an actor's state due to Rust's ownership rules. Examples include UI libraries where context and
//!   ui state cannot be easily shared. Puppeter's resource management system offers a solution to
//!   handle such cases by allowing actors to safely access and modify shared resources. This enables
//!   scenarios like implementing The Elm Architecture (TEA) or Redux-like state management, where
//!   actor or actors can update a common shared state. While sharing state among actors is
//!   generally not recommended, Puppeter's resource management provides a practical approach when
//!   it becomes necessary, ensuring safety and efficiency in the actor-based system.
//!
//! ## Getting Started
//!
//! ```rust
//! use pptr::prelude::*;
//! use async_trait::async_trait;
//!
//! #[derive(Debug, Clone, Default)]
//! struct PingActor;
//!
//! #[async_trait]
//! impl Lifecycle for PingActor {
//!     // This actor uses the 'OneForAll' supervision strategy.
//!     // If any child actor fails, all child actors will be restarted.
//!     type Supervision = OneForAll;
//!
//!     // The 'reset' method is called when the actor needs to reset its state.
//!     // In this example, we simply return a default instance of 'PingActor'.
//!     async fn reset(&self, _ctx: &Context) -> Result<Self, CriticalError> {
//!         Ok(Self::default())
//!     }
//! }
//!
//! // We define a 'Ping' message that contains a counter value.
//! #[derive(Debug)]
//! struct Ping(u32);
//!
//! // The 'Handler' trait defines how the actor should handle incoming messages.
//! // It is a generic trait, which allows defining message handling for specific message types,
//! // rather than using a single large enum for all possible messages.
//! // This provides better type safety and easier maintainability.
//! // By implementing the 'Handler' trait for a particular message type and actor,
//! // you can define the specific behavior for handling that message within the actor.
//! // Additionally, the 'Handler' trait can be implemented multiple times for the same message type,
//! // allowing different actors to handle the same message type in their own unique way.
//! // This flexibility enables better separation of concerns and modular design in the actor system.
//! #[async_trait]
//! impl Handler<Ping> for PingActor {
//!
//!     // The 'Response' associated type specifies the type of the response returned by the handler.
//!     // In this case, the response type is '()', which means the handler doesn't return any meaningful value.
//!     // It is common to use '()' as the response type when the handler only performs side effects and doesn't need to return a specific value.
//!     type Response = ();
//!
//!     // The 'Executor' associated type specifies the execution strategy for handling messages.
//!     // It determines how the actor processes incoming messages concurrently.
//!     // The 'SequentialExecutor' processes messages sequentially, one at a time, in the order they are received.
//!     // This ensures that the handler for each message is executed to completion before processing the next message.
//!     // The 'SequentialExecutor' is suitable when the order of message processing is important and the handler doesn't perform any blocking operations.
//!     type Executor = SequentialExecutor;
//!
//!     // The 'handle_message' method is called when the actor receives a 'Ping' message.
//!     // It prints the received counter value and sends a 'Pong' message to 'PongActor'
//!     // with an incremented counter value, until the counter reaches 10.
//!     async fn handle_message(&mut self, msg: Ping, ctx: &Context) -> Result<Self::Response, PuppetError> {
//!         // The 'ctx' parameter is a reference to the 'Context' struct, which encapsulates
//!         // the actor's execution context and provides access to the same methods as the 'pptr' instance.
//!         // It allows the actor to send messages, spawn new actors, and perform other actions.
//!         // If an actor is spawned using 'ctx', it automatically assigns the spawning actor as its supervisor.
//!         // The 'ctx' parameter enables safe and consistent interaction with the actor system,
//!         // abstracting away the underlying complexity.
//!
//!         println!("Ping received: {}", msg.0);
//!         if msg.0 < 10 {
//!             // By using 'ctx.send', the actor can send messages to other actors directly from the message handler,
//!             // ensuring proper error propagation and potential supervision.
//!             ctx.send::<PongActor, _>(Pong(msg.0 + 1)).await?;
//!         } else {
//!             println!("Ping-Pong finished!");
//!         }
//!
//!         Ok(())
//!     }
//! }
//!
//! #[derive(Debug, Clone, Default)]
//! struct PongActor;
//!
//! // By default, similar to 'PingActor', the 'reset' method returns a default instance of 'PongActor'.
//! #[async_trait]
//! impl Lifecycle for PongActor {
//!     type Supervision = OneForAll;
//! }
//!
//! // We define a 'Pong' message that contains a counter value.
//! #[derive(Debug)]
//! struct Pong(u32);
//!
//! #[async_trait]
//! impl Handler<Pong> for PongActor {
//!     type Response = ();
//!     type Executor = SequentialExecutor;
//!
//!     // The 'handle_message' method for 'PongActor' is similar to 'PingActor'.
//!     // It prints the received counter value and sends a 'Ping' message back to 'PingActor'
//!     // with an incremented counter value, until the counter reaches 10.
//!     async fn handle_message(&mut self, msg: Pong, ctx: &Context) -> Result<Self::Response, PuppetError> {
//!         println!("Pong received: {}", msg.0);
//!
//!         if msg.0 < 10 {
//!             ctx.send::<PingActor, _>(Ping(msg.0 + 1)).await?;
//!         } else {
//!             println!("Ping-Pong finished!");
//!         }
//!         Ok(())
//!     }
//! }
//!
//! #[tokio::main]
//! async fn main() -> Result<(), PuppetError> {
//!
//!     // Create a new instance of the Puppeter.
//!     let pptr = Puppeter::new();
//!
//!     // Spawn a 'PingActor' and specify 'PingActor' as its own supervisor.
//!     // This means that 'PingActor' will manage itself.
//!     pptr.spawn::<PingActor, PingActor>().await?;
//!
//!     // Spawn a 'PongActor' using the shorter 'spawn_self' method.
//!     // This is equivalent to specifying 'PongActor' as its own supervisor.
//!     pptr.spawn_self::<PongActor>().await?;
//!
//!     // Send an initial 'Ping' message to 'PingActor' with a counter value of 0.
//!     // This starts the ping-pong game between 'PingActor' and 'PongActor'.
//!     pptr.send::<PingActor, _>(Ping(0)).await?;
//!
//!     tokio::time::sleep(std::time::Duration::from_secs(1)).await;
//!
//!     Ok(())
//! }
//! ```

pub mod address;
pub mod errors;
pub mod executor;
pub mod message;
pub mod pid;
pub mod puppet;
pub mod puppeter;
pub mod supervision;

pub mod prelude {
    pub use crate::address::Address;
    pub use crate::errors::CriticalError;
    pub use crate::errors::NonCriticalError;
    pub use crate::errors::PuppetError;
    pub use crate::executor::ConcurrentExecutor;
    pub use crate::executor::DedicatedConcurrentExecutor;
    pub use crate::executor::SequentialExecutor;
    pub use crate::message::Message;
    pub use crate::pid::Pid;
    pub use crate::puppet::Context;
    pub use crate::puppet::Handler;
    pub use crate::puppet::Lifecycle;
    pub use crate::puppet::Puppet;
    pub use crate::puppeter::Puppeter;
    pub use crate::supervision::strategy::*;
}
