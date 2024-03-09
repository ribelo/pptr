Puppeter is a flexible actor-based framework for building asynchronous systems in Rust. With a focus on type-driven API design, Puppeter provides a safe and convenient way to create and manage actors that communicate through message passing. Whether you're building a complex distributed system or a responsive user interface, Puppeter makes it easy to write efficient and maintainable asynchronous code.
Key Features

    - Type-Driven Development: Puppeter leverages Rust's strong typing and
      trait system to ensure compile-time safety and runtime reliability. The
      Message trait is automatically implemented for any type that is Debug,
      Send, and 'static, allowing most structs to be sent as messages without
      requiring explicit trait implementation. Multiple handlers can handle a
      single message type, providing flexibility in message processing. When
      sending messages, you specify the type that implements the Puppet trait,
      enabling message sending to any actor from anywhere in the code. For
      example:
      ```rust
      let res = ctx.:ask:<OtherPuppet, _>(msg).await?; 
      ```
      Thanks
      to type inference, you don't need to explicitly specify the message type
      or the result type. With strict type constraints and the ability to
      define custom message and response types for each actor, Puppeter helps
      you catch errors early and write more robust code.

    - Ergonomic API: Puppeter provides an intuitive and user-friendly API that
      simplifies actor creation, message passing, and resource management. You
      can easily store and share resources between actors, retrieve resources
      by type, and ensure thread safety through the use of mutexes. Puppeter
      also integrates seamlessly with popular Rust libraries and architectures,
      such as Tokio and The Elm Architecture (TEA).

    - Effortless Asynchronous Programming: Puppeter makes asynchronous
      programming a breeze. Built on top of the battle-tested Tokio runtime,
      Puppeter seamlessly integrates with Rust's async/await syntax, allowing
      you to write asynchronous code that looks and feels like synchronous
      code. With Puppeter, you can easily create actors that manage their own
      state and communicate through message passing or even communicate by
      sharing state, eliminating the need for manual synchronization and
      reducing the risk of race conditions.

    - Performance-Driven: Puppeter is designed with performance in mind. By
      leveraging the power of the Tokio runtime, Puppeter enables efficient
      concurrent and parallel processing of messages. With three message
      handling modes (Sequential, Concurrent, and DedicatedConcurrent), you
      have the flexibility to optimize the execution behavior of your actors
      based on your specific requirements. Whether you need to ensure strict
      message ordering, handle high-throughput scenarios, or dedicate
      CPU-intensive tasks to separate executors, Puppeter has you covered. The
      framework also provides a flexible Executor trait, allowing you to
      implement custom executors for specialized workloads, giving you full
      control over the execution environment.

    - Flexible Supervision: Puppeter offers a flexible supervision system that
      allows you to monitor and manage actors hierarchically. Actors can be
      their own masters or have a master, and monitoring is optional but
      available when needed. With three built-in supervision strategies
      (one-for-one, one-for-all, and rest-for-one) and the ability to define
      custom strategies, Puppeter gives you fine-grained control over error
      handling and fault tolerance.

    - Versatile Message Passing: Puppeter offers a wide range of message
      passing techniques to accommodate diverse communication requirements. The
      framework provides intuitive methods for sending messages, such as `send`
      for reliable one-way communication, `ask` for request-response
      interactions, and `cast` for asynchronous fire-and-forget messaging. With
      Puppeter, you have the flexibility to choose the most appropriate
      message passing strategy based on your application's needs. Whether you
      prioritize low-latency communication, require reliable message
      delivery, or need to perform asynchronous operations without blocking,
      Puppeter empowers you to optimize your actor's communication patterns
      effortlessly. The framework's message passing abstractions are designed
      to be expressive and easy to use, enabling you to focus on building
      robust and efficient actor-based systems.

    - Hierarchical Actor Structure: Puppeter introduces the concept of
      "puppets" and "masters," enabling you to create hierarchical
      relationships between actors. Puppets can be managed and controlled by
      masters, providing a clear separation of concerns and facilitating the
      creation of complex, scalable systems. While hierarchies are supported,
      Puppeter also allows for flat actor structures when desired.

    - Robust Error Handling: Puppeter prioritizes error handling and provides
      mechanisms for supervising actors, automatically restarting them in case
      of failures, and configuring supervision strategies. It also offers a
      dedicated mechanism for reporting and handling critical errors that
      cannot be resolved by individual actors or their supervisors.

    - Lifecycle Management: Puppeter includes built-in methods for managing the
      lifecycle of actors, including initialization, startup, shutdown, and
      state reset. With pre-implemented lifecycle management, you can avoid
      boilerplate code and focus on implementing your actor's core logic.
      Actors can be controlled through lifecycle-related commands, which take
      precedence over regular messages.

    - Resource Management: Puppeter enables flexible resource management by
      allowing you to add and share resources among actors. Resources can be
      safely and efficiently retrieved, modified, and removed by actors,
      promoting encapsulation and modularity.

    - Separation of Concerns: Puppeter promotes a clean separation between
      business logic and actor management infrastructure. This allows
      developers to focus on implementing the core functionality of their
      actors while the framework takes care of the underlying plumbing.

