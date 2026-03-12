# plan.md

## 1. System Architecture Overview

The system is designed as a high-performance, asynchronous TCP proxy. It will be built on the Tokio runtime, leveraging a multi-threaded, work-stealing scheduler to handle a large number of concurrent I/O-bound tasks efficiently.

The architecture is modular, separating concerns into distinct components that communicate through well-defined, thread-safe interfaces. At its core, the system operates on a "one task per connection" model.

**High-Level Flow:**
1.  A central **TCP Listener** accepts incoming client connections.
2.  For each connection, a new async task is spawned to act as the **Connection Handler**.
3.  The handler consults the **Load Balancing Scheduler** to select an optimal backend server.
4.  The scheduler makes its decision based on data from the **Backend Pool Manager**, which maintains the state and health of all configured backends.
5.  A background **Health Checker** task continuously monitors backends and updates the pool, ensuring traffic is not sent to failed servers.
6.  A **Latency Measurement System**, integrated with the health checker, provides the data for advanced latency-aware routing.
7.  A **Load Shedding Controller** monitors global connection counts and can instruct the listener to reject new connections under extreme load, ensuring system stability.
8.  All components report to a unified **Observability System** for metrics and distributed tracing, providing deep insight into the system's behavior and performance.

This design prioritizes performance and resilience by minimizing lock contention (using atomic operations and read-mostly data structures) and isolating failures (via health checks and timeouts).

## 2. Component Design

Each component has a specific and isolated responsibility.

### TCP Listener
*   **Responsibility:** Bind to a TCP socket and listen for new client connections in a loop.
*   **Inputs:** A listen address (e.g., `0.0.0.0:8080`), a shared handle to the system's state (including the backend pool and scheduler).
*   **Outputs:** Accepted `tokio::net::TcpStream` sockets.
*   **Interactions:**
    *   Consults the **Load Shedding Controller** before accepting a connection to see if the system is overloaded.
    *   If not overloaded, it spawns a new **Connection Handler** task for each accepted stream, passing it the socket and a clone of the shared state.

### Connection Handler
*   **Responsibility:** Manage the entire lifecycle of a single client-to-backend proxy connection.
*   **Inputs:** A client `TcpStream`, a shared handle to the system state (`Arc<AppState>`).
*   **Outputs:** Logs and traces detailing the connection's lifecycle.
*   **Internal State:** The client socket and the backend socket.
*   **Interactions:**
    1.  Creates a `tracing` span to track the connection's lifecycle.
    2.  Increments the global active connection counter (for the **Load Shedding Controller**).
    3.  Requests a backend from the **Load Balancing Scheduler**.
    4.  Attempts to connect to the chosen backend with a strict timeout.
    5.  If the connection is successful, it uses `tokio::io::copy_bidirectional` to proxy data between the client and backend.
    6.  Upon completion (or error), it decrements the active connection counter and ensures all resources are released.

### Load Balancing Scheduler
*   **Responsibility:** Select the "best" backend for a new connection based on a configured algorithm.
*   **Inputs:** A request for a backend.
*   **Outputs:** The `SocketAddr` of a chosen backend.
*   **Interactions:**
    *   Reads from the **Backend Pool Manager** to get the list of currently healthy backends.
    *   For advanced algorithms, it reads from the **Latency Measurement System** to get real-time performance data.
    *   It is designed as a trait, allowing for multiple implementations (Round Robin, Least Connections, Latency-Aware).

### Backend Pool Manager
*   **Responsibility:** Act as the single source of truth for the state of all backend servers.
*   **Inputs:** Updates from the **Health Checker**.
*   **Outputs:** A list of healthy backends for the **Scheduler**.
*   **Internal State:** A thread-safe collection (e.g., `Arc<RwLock<Vec<Arc<Backend>>>>`) representing all configured backends, their health status, and their statistics (active connections, latency).

### Health Checker
*   **Responsibility:** Actively and passively monitor the health of all backend servers.
*   **Inputs:** The list of all configured backends.
*   **Outputs:** Health status updates to the **Backend Pool Manager**.
*   **Interactions:**
    *   Runs as a dedicated background task.
    *   Periodically attempts to connect to each backend (active check).
    *   Also incorporates passive feedback (e.g., a connection failure from a **Connection Handler**) to mark a backend as unhealthy immediately.
    *   Updates the `is_healthy` flag on the corresponding backend object within the **Backend Pool Manager**.

### Latency Measurement System
*   **Responsibility:** Collect and maintain latency statistics for each backend.
*   **Inputs:** Round-trip time measurements.
*   **Outputs:** Smoothed latency data (e.g., EWMA) for the **Scheduler**.
*   **Interactions:**
    *   Integrates with the **Health Checker**. The health check "ping" is used to measure round-trip time.
    *   Updates the latency metric on the `Backend` data structure.

### Load Shedding Controller
*   **Responsibility:** Protect the system from overload by rejecting connections when necessary.
*   **Inputs:** The current active connection count and a configured high-water mark.
*   **Outputs:** A "shed load" signal to the **TCP Listener**.
*   **Internal State:** A global `AtomicUsize` for the active connection count and a configuration threshold.

### Observability System (Metrics & Tracing)
*   **Responsibility:** Collect, aggregate, and expose telemetry data.
*   **Inputs:** Events and measurements from all other components.
*   **Outputs:** A Prometheus metrics endpoint and structured logs/traces.
*   **Interactions:** All components use this system to report their status. For example, the **Connection Handler** creates traces, and the **Load Shedding Controller** increments a "connections_shed" counter.

## 3. Rust Module Structure

The project will be organized to reflect the component architecture, promoting separation of concerns.

```
src/
├── main.rs           // Entry point, configuration loading, component wiring.
├── config.rs         // Defines configuration structs and parsing logic.
├── app_state.rs      // Defines the shared `AppState` struct.
├── listener.rs       // The main TCP listener loop.
├── proxy.rs          // Connection handling logic, including bidirectional copy.
├── backend/
│   ├── mod.rs        // Defines the `Backend` struct and the `BackendPool`.
│   └── health.rs     // The health checking and latency measurement background task.
├── scheduler/
│   ├── mod.rs        // Defines the `Scheduler` trait.
│   ├── round_robin.rs// Round Robin implementation.
│   └── latency.rs    // Latency-aware (P2C) implementation.
├── shed.rs           // Load shedding logic and state.
└── observability/
    ├── mod.rs        // Common observability setup.
    ├── metrics.rs    // Metrics definitions and Prometheus exporter setup.
    └── tracing.rs    // Tracing/logging setup and span definitions.
```

## 4. Data Structures

Core data structures will be designed for safe and efficient concurrent access.

*   **`Backend`**: Represents a single backend server.
    ```rust
    use std::net::SocketAddr;
    use std::sync::atomic::{AtomicUsize, AtomicBool};
    use std::time::Duration;
    use parking_lot::Mutex; // More performant than std::sync::Mutex

    pub struct Backend {
        addr: SocketAddr,
        active_connections: AtomicUsize,
        is_healthy: AtomicBool,
        // EWMA latency, stored as microseconds in an atomic for lock-free reads.
        latency_ewma_us: AtomicUsize,
    }
    ```

*   **`BackendPool`**: The shared collection of backends.
    ```rust
    use std::sync::Arc;
    use tokio::sync::RwLock;

    // A type alias for the shared, mutable pool of backends.
    // RwLock is suitable because reads (by schedulers) are frequent,
    // while writes (by the health checker) are infrequent.
    pub type BackendPool = Arc<RwLock<Vec<Arc<Backend>>>>;
    ```

*   **`AppState`**: A container for all shared state, cloned for each task.
    ```rust
    use std::sync::Arc;

    pub struct AppState {
        pub pool: BackendPool,
        pub scheduler: Arc<dyn scheduler::Scheduler + Send + Sync>,
        pub metrics: Arc<observability::metrics::Metrics>,
        pub shed_controller: Arc<shed::Controller>,
    }

    impl Clone for AppState { // cheap clone since all fields are Arcs
        // ...
    }
    ```

*   **`Scheduler`**: A trait to allow for dynamic dispatch of balancing strategies.
    ```rust
    pub trait Scheduler {
        fn select_backend(&self, pool: &Vec<Arc<Backend>>) -> Option<Arc<Backend>>;
    }
    ```

## 5. Async Execution Model

The system's concurrency model is built entirely on Tokio's async ecosystem.

1.  **Initialization**: The `main` function, marked with `#[tokio::main]`, initializes the Tokio runtime. It parses configuration, builds the `AppState` (including the `BackendPool`, `Scheduler`, etc.), and spawns the `Health Checker` as a long-running background task.

2.  **Connection Acceptance**: The `listener` runs in its own primary task, looping on `TcpListener::accept()`. This call is non-blocking; the task yields to the Tokio scheduler until the OS notifies it of a new connection.

3.  **Task Spawning**: For each accepted connection, `tokio::spawn` is called. This places a new `Connection Handler` future onto the runtime's task queue. The Tokio work-stealing scheduler will assign this task to one of its worker threads for execution. This model allows the listener to immediately go back to accepting new connections without waiting for the previous one to be processed.

4.  **Bidirectional Proxying**: Inside the handler task, once a backend connection is established, `tokio::io::copy_bidirectional` is used. This is a highly optimized utility that manages the two-way copying of data. It handles cases like half-closed connections correctly and suspends the task efficiently when there's no data to read or write.

5.  **Shared State Management**:
    *   **Ownership**: All shared resources (`BackendPool`, `Scheduler`, etc.) are wrapped in `Arc`. This allows multiple tasks to share ownership of the same data safely.
    *   **Concurrency**:
        *   `Arc<T>` is passed to each task, which is a cheap pointer clone.
        *   For read-heavy data that is occasionally modified (the list of healthy backends), `tokio::sync::RwLock` is used.
        *   For simple, high-traffic counters (active connections), `std::sync::atomic::AtomicUsize` is used to avoid locking entirely.

This relationship ensures that thousands of tasks can run concurrently, sharing state without data races, and spending most of their time idly suspended (without consuming OS thread resources) while waiting for I/O.

## 6. Implementation Roadmap

The project will be developed in iterative phases, ensuring a working, testable system at each step.

### Phase A — Minimal Viable TCP Proxy
*   **Goal:** Establish a single, functioning proxy path.
*   **Components:**
    *   `config.rs`: Hardcode a single listen address and a single backend address.
    *   `listener.rs`: Basic loop to accept one connection.
    *   `proxy.rs`: Connect to the hardcoded backend and use `copy_bidirectional`.
*   **Outcome:** [COMPLETED] A program that can proxy a single TCP connection from a client (e.g., `netcat`) to a backend server.

### Phase B — Core Load Balancing
*   **Goal:** Distribute connections across multiple backends.
*   **Components:**
    *   `config.rs`: Allow configuring multiple backends.
    *   `backend/mod.rs`: Implement the `Backend` and `BackendPool` data structures.
    *   `scheduler/mod.rs` & `round_robin.rs`: Implement the `Scheduler` trait and a simple Round Robin strategy using an `AtomicUsize` counter.
    *   Wire the scheduler into the `Connection Handler`.
*   **Outcome:** [COMPLETED] A load balancer that distributes incoming connections evenly across all configured backends.

### Phase C — Health Checking and Resilience
*   **Goal:** Make the load balancer resilient to backend failures.
*   **Components:**
    *   `backend/health.rs`: Implement the active health checking background task. It should periodically try to connect to each backend and update the `is_healthy` flag in the `BackendPool` using the `RwLock`.
    *   Modify the `Scheduler` to only select from backends where `is_healthy` is true.
*   **Outcome:** [COMPLETED] The load balancer automatically stops sending traffic to unresponsive backends and resumes when they recover.

### Phase D — Foundational Observability
*   **Goal:** Gain visibility into the system's operation.
*   **Components:**
    *   `observability/tracing.rs`: Integrate the `tracing` and `tracing-subscriber` crates for structured logging.
    *   `observability/metrics.rs`: Integrate the `prometheus` crate.
    *   Add metrics: a counter for total connections (`connections_total`), a gauge for active connections (`connections_active`).
*   **Outcome:** [COMPLETED] A running `/metrics` endpoint and structured logs that provide basic operational insight.

### Phase E — Advanced Experimental Features
*   **Goal:** Implement and evaluate the project's differentiating features.
*   **Components:** This phase will implement the features detailed in the next section.
*   **Outcome:** [COMPLETED] A sophisticated load balancer with advanced routing and self-protection capabilities.

## 7. Feature Experiment Plan

### Latency-Aware Routing
*   **Implementation Approach:**
    1.  The `Health Checker` will be enhanced. Instead of just a `TcpStream::connect`, it will perform a minimal transaction (e.g., connect, send a "ping" byte, wait for an "pong" echo) and measure the round-trip time.
    2.  The `Backend` struct's `latency_ewma_us` field will be updated after each successful health check using an Exponentially Weighted Moving Average formula to smooth out measurements.
    3.  A new `scheduler::latency::LatencyAwareScheduler` will be implemented. It will use the "Power of Two Choices" (P2C) algorithm: randomly select two distinct, healthy backends and forward the connection to the one with the lower `latency_ewma_us`.
*   **Metrics:**
    *   Gauge: `backend_latency_ewma_us{backend="..."}` for each backend.
    *   Histogram: `scheduler_chosen_backend_latency_seconds` to track the latency of the backends being chosen.
*   **Evaluation:** In a test environment, artificially introduce latency to one backend (e.g., using `tc netem`) and observe via metrics that the scheduler shifts traffic away from it.

### Load Shedding
*   **Implementation Approach:**
    1.  The `shed::Controller` will hold the `active_connections` `AtomicUsize` and a configurable `max_connections` threshold.
    2.  The `Connection Handler` will increment the counter on creation and decrement it on drop (using a RAII guard).
    3.  In the `listener` loop, before spawning a new task, it will check: `if controller.is_overloaded()`.
    4.  If true, the listener will immediately close the newly accepted socket without spawning a task for it, and a `connections_shed_total` metric will be incremented.
*   **Metrics:**
    *   Counter: `connections_shed_total`.
    *   Gauge: `connections_active`.
*   **Evaluation:** Use a load generator (like `oha` or a custom tool) to create a connection storm that exceeds the `max_connections` limit. Verify that the server remains stable, CPU/memory usage stays bounded, and the `connections_shed_total` counter increases.

### Connection Lifecycle Tracing
*   **Implementation Approach:**
    1.  Use `tracing::instrument` or manual `tracing::Span` creation in the `Connection Handler`.
    2.  The root span will be created upon connection acceptance, with fields like `client.ip`.
    3.  Child spans will be created for key phases: `get_backend`, `connect_to_backend`, `proxy_data`.
    4.  Events will be recorded within the spans for significant lifecycle moments: `backend.selected`, `backend.connect.success`, `backend.connect.error`, `client.half_close`, `connection.teardown`.
    5.  Integrate `tracing-opentelemetry` to export these traces to a collector like Jaeger or Honeycomb for visualization.
*   **Metrics:** This is not about numeric metrics but about rich, queryable event logs.
*   **Evaluation:** Manually inspect traces from a running system. Trigger failure modes (e.g., shut down a backend while a connection is active) and verify that the trace accurately reflects the error propagation and teardown sequence.

## 8. Testing and Benchmarking Strategy

### Correctness Testing
*   **Unit Tests:** Each scheduler implementation will have unit tests to verify its logic.
*   **Integration Tests:** A test suite will programmatically start the load balancer, mock backend servers (that can be configured to fail or respond slowly), and mock clients. These tests will verify end-to-end behavior, such as:
    *   Traffic is correctly proxied.
    *   The health checker correctly removes and re-adds a backend.
    *   Connections are balanced according to the selected strategy.

### Performance Benchmarking
*   **Tooling:** A dedicated Rust benchmarking application will be built using Tokio to act as a high-performance client, capable of generating thousands of concurrent connections and measuring latency and throughput accurately.
*   **Metrics Collection:** The benchmarking tool will measure client-side connection setup time and throughput. Simultaneously, the load balancer will be run with Prometheus exporting enabled. A test run will consist of running the load generator for a fixed duration and then scraping the Prometheus data.
*   **Scenarios:**
    *   **Throughput:** Measure maximum data transfer rate (Gbps) with a small number of long-lived connections.
    *   **Concurrency:** Measure the maximum number of stable, concurrent connections the system can hold.
    *   **Connection Churn:** Measure requests-per-second with short-lived connections to stress the connection setup/teardown path.
    *   **Latency:** Measure P50, P99, and P99.9 latencies for connection setup under various load levels.

This comprehensive plan provides a clear blueprint for building a robust, high-performance, and feature-rich TCP load balancer.
