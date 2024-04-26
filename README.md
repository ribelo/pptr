<h1 align="center">ππtρ/pptr</h1>

<p align="center">
  <a href="https://github.com/ribelo/pptr"><img alt="github" src="https://img.shields.io/badge/github-ribelo/pptr-8da0cb?style=for-the-badge&labelColor=555555&logo=github" height="20"></a>
  <a href="https://crates.io/crates/pptr"><img alt="crates.io" src="https://img.shields.io/crates/v/pptr.svg?style=for-the-badge&color=fc8d62&logo=rust" height="20"></a>
  <a href="https://docs.rs/pptr"><img alt="docs.rs" src="https://img.shields.io/badge/docs.rs-pptr-66c2a5?style=for-the-badge&labelColor=555555&logo=docs.rs" height="20"></a>
</p>

<p align="center">
  <!-- <a href="https://github.com/ribelo/pptr/actions"><img alt="Build Status" src="https://github.com/ribelo/pptr/workflows/CI/badge.svg"></a> -->
  <a href="https://codecov.io/gh/ribelo/pptr"><img alt="Coverage" src="https://codecov.io/gh/ribelo/pptr/graph/badge.svg?token=7CVCP09OQN"></a>
  <!-- <a href="https://deps.rs/repo/github/ribelo/pptr"><img alt="Dependencies" src="https://deps.rs/repo/github/ribelo/pptr/status.svg"></a> -->
</p>

# Puppeteer: A Flexible Actor-Based Framework for Composable Systems in Rust

Puppeteer (pptr) is an actor-based framework designed to simplify the
development of composable and maintainable asynchronous systems in Rust. With
its type-driven API design, Puppeteer provides a safe and convenient way to
create and manage actors that communicate through message passing.

Puppeteer is built with composability, encapsulation, and single responsibility
in mind, rather than focusing on building large-scale distributed systems. This
is reflected in its approach of allowing only one instance of an actor per
type, promoting a modular and maintainable architecture. Each puppet
encapsulates a specific responsibility and communicates with other puppets
through message passing, leading to a more composable system.

While this design decision might seem like a limitation, it excels in building
regular systems that prioritize these principles, providing a high degree of
scalability for typical applications. Puppeteer's approach to building systems
as a collection of independent, reusable components that communicate through
message passing is driven by Alan Kay's vision of object-oriented programming.

Rather than being another copy of Erlang OTP or Akka, Puppeteer has different
goals and aims to provide a fresh perspective at the intersection of the actor
model and object-oriented programming through message passing. Whether you're
building a responsive user interface or a modular system that values
composability and maintainability, Puppeteer makes it easy to write efficient
and maintainable asynchronous code in Rust.

## Key Features

### 1. Type-Driven Development

Puppeteer embraces type-driven development, leveraging Rust's powerful type
system to ensure compile-time safety and runtime reliability. The framework
takes advantage of Rust's traits and type inference to provide a seamless and
expressive API for defining actors and their message-handling behaviors.

With Puppeteer, you can define custom message types for each actor, enabling
precise communication and strong type safety. The framework automatically
derives necessary traits for your message types, reducing boilerplate and
making it easy to send messages between actors.

Puppeteer's type-driven approach extends to message handling as well. You can
define multiple handlers for a single message type, providing flexibility and
modularity in message processing. The framework's use of Rust's type system
ensures that only valid message types can be sent to and handled by actors,
catching potential errors at compile time.

By leveraging Rust's type system, Puppeteer helps you write more robust and
maintainable code. The framework's type-driven design encourages you to think
carefully about the messages your actors will send and receive, leading to
clearer and more explicit communication patterns. With Puppeteer, you can rely
on the compiler to catch type-related errors early in the development process,
saving time and effort in debugging and refactoring.

### 2. Ergonomic API

Puppeteer offers a clean and expressive API that makes actor creation and
message passing a breeze. The API is designed to be intuitive and easy to use,
allowing developers to focus on building their applications rather than
wrestling with complex syntax or boilerplate code.

Creating actors is straightforward, and Puppeteer provides a simple way to
define actor behaviors and handle incoming messages. The API encourages a clear
separation of concerns, making it easy to reason about the responsibilities of
each actor and how they interact with one another.

Puppeteer seamlessly integrates with popular Rust libraries and frameworks, such
as Tokio, enabling developers to leverage the power of asynchronous programming
and build highly concurrent applications with ease.

The API also provides helpful utilities and abstractions that simplify common
tasks, such as managing actor lifecycles, handling errors, and gracefully
shutting down the actor system when necessary.

With Puppeteer's ergonomic API, developers can quickly get started building
robust and scalable actor-based systems in Rust, without sacrificing
performance or flexibility.

### 3. Effortless Asynchronous Programming

Puppeteer simplifies asynchronous programming in Rust. It uses the Tokio runtime
and works well with Rust's `async`/`await` syntax. This allows you to write
asynchronous code that is easy to read and understand, almost like synchronous
code.

With Puppeteer, you can create actors that manage their own state and
communicate with each other by sending messages. You can also choose to have
actors share state directly. Puppeteer handles the synchronization for you,
reducing the chances of race conditions and making your code safer.

### 4. Performance-Driven
Puppeteer is designed with performance in mind. It uses the Tokio runtime to
efficiently handle messages concurrently and in parallel. The framework offers
three message handling modes:

1. Sequential: Messages are processed one after another, ensuring strict ordering.
2. Concurrent: Messages are handled concurrently, allowing for higher throughput.
3. DedicatedConcurrent: CPU-intensive tasks are assigned to separate executors, preventing them from blocking other tasks.

These modes give you the flexibility to choose the best approach for your
specific needs. Puppeteer also provides an `Executor` trait, which allows you to
create custom executors for specialized workloads. This gives you full control
over how your actors' tasks are executed.
### 5. Flexible Supervision
Puppeteer provides a versatile oversight system that enables you to oversee and
organize actors in a hierarchical manner. Actors have the freedom to operate
independently or under the guidance of a supervisor. Monitoring is not
mandatory but can be implemented as needed. Puppeteer offers three predefined
supervision strategies: one-for-one, one-for-all, and rest-for-one. Additionally,
you can create your own custom strategies to handle errors and maintain system
stability in a way that best suits your specific requirements.

### 6. Versatile Message Passing
Puppeteer provides several ways for actors to send messages to each other. You
can choose the best method based on what your application needs. If you want
messages to be sent quickly, Puppeteer has options for that. If you need to make
sure messages are delivered reliably, Puppeteer supports that too. You can also
send messages asynchronously, which means the sender doesn't have to wait for a
response before continuing with other tasks. Puppeteer's message passing
features are designed to be simple and easy to understand, so you can focus on
building your application without getting bogged down in complicated details.

### 7. Hierarchical Actor Structure
Puppeteer allows you to organize actors in a hierarchical structure using
"puppets" and "masters." This means that some actors (called masters) can
control and manage other actors (called puppets). Organizing actors in this way
can make it easier to build and understand complex systems.

Using puppets and masters, you can:
- Clearly define the roles and responsibilities of different actors
- Create relationships between actors, making it clear how they work together
- Simplify the management of large numbers of actors

While Puppeteer supports hierarchical structures, you can also choose to use a
flat structure, where all actors are at the same level, if that better suits
your needs.

### 8. Robust Error Handling
When working with multiple actors in a system, it's important to have a robust
way of dealing with errors. Puppeteer includes built-in features to monitor
actors and automatically restart them if something goes wrong. It also allows
you to define different strategies for handling errors based on your specific
needs.

In case of critical errors that can't be fixed by the actors themselves or
their supervisors, Puppeteer provides a separate mechanism to report and manage
these issues. This helps ensure that your system remains stable and reliable
even in the face of unexpected problems.

### 9. Lifecycle Management
Puppeteer comes with built-in methods that help manage the lifecycle of actors.
These methods handle tasks like initializing, starting up, shutting down, and
resetting the state of an actor. By using these pre-implemented lifecycle
management features, you can save time and effort by not having to write the
same basic code for each actor. Instead, you can focus on writing the important
parts of your actor's logic.

Using Puppeteer's lifecycle management features makes it easier to create and
manage actors in your application. You don't need to worry about the details of
how to initialize or clean up an actor's resources, because Puppeteer takes care
of that for you. This helps keep your code cleaner and more focused on the
specific tasks your actors need to perform.

### 10. Resource Management

Puppeteer provides a way to manage resources that can be shared among actors.
This can be useful in certain situations, such as when using external libraries
or when multiple actors need access to the same database handle or user
interface.

In Rust, sharing resources can be challenging due to its ownership system.
However, Puppeteer offers a solution that allows actors to safely access and
modify shared resources.

While sharing state among actors is not recommended, Puppeteer's
resource management system provides a practical approach to handle scenarios
where it becomes necessary. It gives you the flexibility needed in certain use
cases.

By leveraging Puppeteer's resource management capabilities, you can effectively
manage shared resources while still maintaining the safety and efficiency of
your actor-based system.

## Getting Started

To start using Puppeteer in your Rust project, add the following dependency to
your `Cargo.toml` file:

```toml
[dependencies]
Puppeteer = "0.2.0"
```

```rust
use pptr::prelude::*;

#[derive(Default)]
struct PingActor;

#[async_trait]
impl Puppet for PingActor {
    // This actor uses the 'OneForAll' supervision strategy.
    // If any child actor fails, all child actors will be restarted.
    type Supervision = OneForAll;

    // The 'reset' method is called when the actor needs to reset its state.
    // In this example, we simply return a default instance of 'PingActor'.
    async fn reset(&self, _ctx: &Context) -> Result<Self, CriticalError> {
        Ok(Self::default())
    }
}

// We define a 'Ping' message that contains a counter value.
#[derive(Debug)]
struct Ping(u32);

// The 'Handler' trait defines how the actor should handle incoming messages.
// It is a generic trait, which allows defining message handling for specific message types,
// rather than using a single large enum for all possible messages.
// This provides better type safety and easier maintainability.
// By implementing the 'Handler' trait for a particular message type and actor,
// you can define the specific behavior for handling that message within the actor.
// Additionally, the 'Handler' trait can be implemented multiple times for the same message type,
// allowing different actors to handle the same message type in their own unique way.
// This flexibility enables better separation of concerns and modular design in the actor system.
#[async_trait]
impl Handler<Ping> for PingActor {

    // The 'Response' associated type specifies the type of the response returned by the handler.
    // In this case, the response type is '()', which means the handler doesn't return any meaningful value.
    // It is common to use '()' as the response type when the handler only performs side effects and doesn't need to return a specific value.
    type Response = ();

    // The 'Executor' associated type specifies the execution strategy for handling messages.
    // It determines how the actor processes incoming messages concurrently.
    // The 'SequentialExecutor' processes messages sequentially, one at a time, in the order they are received.
    // This ensures that the handler for each message is executed to completion before processing the next message.
    // The 'SequentialExecutor' is suitable when the order of message processing is important and the handler doesn't perform any blocking operations.
    type Executor = SequentialExecutor;

    // The 'handle_message' method is called when the actor receives a 'Ping' message.
    // It prints the received counter value and sends a 'Pong' message to 'PongActor'
    // with an incremented counter value, until the counter reaches 10.
    async fn handle_message(&mut self, msg: Ping, ctx: &Context) -> Result<Self::Response, PuppetError> {
        // The 'ctx' parameter is a reference to the 'Context' struct, which encapsulates
        // the actor's execution context and provides access to the same methods as the 'pptr' instance.
        // It allows the actor to send messages, spawn new actors, and perform other actions.
        // If an actor is spawned using 'ctx', it automatically assigns the spawning actor as its supervisor.
        // The 'ctx' parameter enables safe and consistent interaction with the actor system,
        // abstracting away the underlying complexity.

        println!("Ping received: {}", msg.0);
        if msg.0 < 10 {
            // By using 'ctx.send', the actor can send messages to other actors directly from the message handler,
            // ensuring proper error propagation and potential supervision.
            ctx.send::<PongActor, _>(Pong(msg.0 + 1)).await?;
        } else {
            println!("Ping-Pong finished!");
        }

        Ok(())
    }
}

#[derive(Clone, Default)]
struct PongActor;

// By default, similar to 'PingActor', the 'reset' method returns a default instance of 'PongActor'.
#[async_trait]
impl Puppet for PongActor {
    type Supervision = OneForAll;
}

// We define a 'Pong' message that contains a counter value.
#[derive(Debug)]
struct Pong(u32);

#[async_trait]
impl Handler<Pong> for PongActor {
    type Response = ();
    type Executor = SequentialExecutor;

    // The 'handle_message' method for 'PongActor' is similar to 'PingActor'.
    // It prints the received counter value and sends a 'Ping' message back to 'PingActor'
    // with an incremented counter value, until the counter reaches 10.
    async fn handle_message(&mut self, msg: Pong, ctx: &Context) -> Result<Self::Response, PuppetError> {
        println!("Pong received: {}", msg.0);

        if msg.0 < 10 {
            ctx.send::<PingActor, _>(Ping(msg.0 + 1)).await?;
        } else {
            println!("Ping-Pong finished!");
        }
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), PuppetError> {

    // Create a new instance of the Puppeteer.
    let pptr = Puppeteer::new();

    // Spawn a 'PingActor' and specify 'PingActor' as its own supervisor.
    // This means that 'PingActor' will manage itself.
    pptr.spawn::<PingActor, PingActor>(PuppetBuilder::new(PingActor::default())).await?;

    // Spawn a 'PongActor' using the shorter 'spawn_self' method.
    // This is equivalent to specifying 'PongActor' as its own supervisor.
    pptr.spawn_self(PuppetBuilder::new(PongActor::default())).await?;

    // Send an initial 'Ping' message to 'PingActor' with a counter value of 0.
    // This starts the ping-pong game between 'PingActor' and 'PongActor'.
    pptr.send::<PingActor, _>(Ping(0)).await?;

    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    Ok(())
}
```

For detailed usage examples and API documentation, please refer to the
[Puppeteer Documentation](https://docs.rs/pptr).

## License

Puppeteer is open-source software licensed under the [MIT License](https://opensource.org/licenses/MIT).
