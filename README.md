# Puppeter: A Flexible Actor-Based Framework for Asynchronous Systems in Rust

Puppeter is a powerful and flexible actor-based framework designed to simplify
the development of asynchronous systems in Rust. With its type-driven API
design, Puppeter provides a safe and convenient way to create and manage actors
that communicate through message passing. Whether you're building a complex
distributed system or a responsive user interface, Puppeter makes it easy to
write efficient and maintainable asynchronous code.

## Key Features

### 1. Type-Driven Development

Puppeter embraces type-driven development, leveraging Rust's powerful type
system to ensure compile-time safety and runtime reliability. The framework
takes advantage of Rust's traits and type inference to provide a seamless and
expressive API for defining actors and their message-handling behaviors.

With Puppeter, you can define custom message types for each actor, enabling
precise communication and strong type safety. The framework automatically
derives necessary traits for your message types, reducing boilerplate and
making it easy to send messages between actors.

Puppeter's type-driven approach extends to message handling as well. You can
define multiple handlers for a single message type, providing flexibility and
modularity in message processing. The framework's use of Rust's type system
ensures that only valid message types can be sent to and handled by actors,
catching potential errors at compile time.

By leveraging Rust's type system, Puppeter helps you write more robust and
maintainable code. The framework's type-driven design encourages you to think
carefully about the messages your actors will send and receive, leading to
clearer and more explicit communication patterns. With Puppeter, you can rely
on the compiler to catch type-related errors early in the development process,
saving time and effort in debugging and refactoring.

### 2. Ergonomic API

Puppeter offers a clean and expressive API that makes actor creation and
message passing a breeze. The API is designed to be intuitive and easy to use,
allowing developers to focus on building their applications rather than
wrestling with complex syntax or boilerplate code.

Creating actors is straightforward, and Puppeter provides a simple way to
define actor behaviors and handle incoming messages. The API encourages a clear
separation of concerns, making it easy to reason about the responsibilities of
each actor and how they interact with one another.

Puppeter seamlessly integrates with popular Rust libraries and frameworks, such
as Tokio, enabling developers to leverage the power of asynchronous programming
and build highly concurrent applications with ease.

The API also provides helpful utilities and abstractions that simplify common
tasks, such as managing actor lifecycles, handling errors, and gracefully
shutting down the actor system when necessary.

With Puppeter's ergonomic API, developers can quickly get started building
robust and scalable actor-based systems in Rust, without sacrificing
performance or flexibility.

### 3. Effortless Asynchronous Programming

Puppeter simplifies asynchronous programming in Rust. It uses the Tokio runtime
and works well with Rust's `async`/`await` syntax. This allows you to write
asynchronous code that is easy to read and understand, almost like synchronous
code.

With Puppeter, you can create actors that manage their own state and
communicate with each other by sending messages. You can also choose to have
actors share state directly. Puppeter handles the synchronization for you,
reducing the chances of race conditions and making your code safer.

### 4. Performance-Driven
Puppeter is designed with performance in mind. It uses the Tokio runtime to
efficiently handle messages concurrently and in parallel. The framework offers
three message handling modes:

1. Sequential: Messages are processed one after another, ensuring strict ordering.
2. Concurrent: Messages are handled concurrently, allowing for higher throughput.
3. DedicatedConcurrent: CPU-intensive tasks are assigned to separate executors, preventing them from blocking other tasks.

These modes give you the flexibility to choose the best approach for your
specific needs. Puppeter also provides an `Executor` trait, which allows you to
create custom executors for specialized workloads. This gives you full control
over how your actors' tasks are executed.
### 5. Flexible Supervision
Puppeter provides a versatile oversight system that enables you to oversee and
organize actors in a hierarchical manner. Actors have the freedom to operate
independently or under the guidance of a supervisor. Monitoring is not
mandatory but can be implemented as needed. Puppeter offers three predefined
supervision strategies: one-for-one, one-for-all, and rest-for-one. Additionally,
you can create your own custom strategies to handle errors and maintain system
stability in a way that best suits your specific requirements.

### 6. Versatile Message Passing
Puppeter provides several ways for actors to send messages to each other. You
can choose the best method based on what your application needs. If you want
messages to be sent quickly, Puppeter has options for that. If you need to make
sure messages are delivered reliably, Puppeter supports that too. You can also
send messages asynchronously, which means the sender doesn't have to wait for a
response before continuing with other tasks. Puppeter's message passing
features are designed to be simple and easy to understand, so you can focus on
building your application without getting bogged down in complicated details.

### 7. Hierarchical Actor Structure
Puppeter allows you to organize actors in a hierarchical structure using
"puppets" and "masters." This means that some actors (called masters) can
control and manage other actors (called puppets). Organizing actors in this way
can make it easier to build and understand complex systems.

Using puppets and masters, you can:
- Clearly define the roles and responsibilities of different actors
- Create relationships between actors, making it clear how they work together
- Simplify the management of large numbers of actors

While Puppeter supports hierarchical structures, you can also choose to use a
flat structure, where all actors are at the same level, if that better suits
your needs.

### 8. Robust Error Handling
When working with multiple actors in a system, it's important to have a robust
way of dealing with errors. Puppeter includes built-in features to monitor
actors and automatically restart them if something goes wrong. It also allows
you to define different strategies for handling errors based on your specific
needs.

In case of critical errors that can't be fixed by the actors themselves or
their supervisors, Puppeter provides a separate mechanism to report and manage
these issues. This helps ensure that your system remains stable and reliable
even in the face of unexpected problems.

### 9. Lifecycle Management
Puppeter comes with built-in methods that help manage the lifecycle of actors.
These methods handle tasks like initializing, starting up, shutting down, and
resetting the state of an actor. By using these pre-implemented lifecycle
management features, you can save time and effort by not having to write the
same basic code for each actor. Instead, you can focus on writing the important
parts of your actor's logic.

Using Puppeter's lifecycle management features makes it easier to create and
manage actors in your application. You don't need to worry about the details of
how to initialize or clean up an actor's resources, because Puppeter takes care
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

To start using Puppeter in your Rust project, add the following dependency to
your `Cargo.toml` file:

```toml
[dependencies]
puppeter = "0.1.68"
```

For detailed usage examples and API documentation, please refer to the
[Puppeter Documentation](https://docs.puppeter.rs).

## License

Puppeter is open-source software licensed under the [MIT License](https://opensource.org/licenses/MIT).
