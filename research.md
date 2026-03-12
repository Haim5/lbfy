# research.md

## 1. Overview of TCP Load Balancing Systems

A TCP Load Balancer operates at Layer 4 (Transport Layer) of the OSI model. Unlike Layer 7 (Application Layer) proxies which understand protocols like HTTP, a TCP load balancer acts as a packet forwarder for raw byte streams. It creates a bridge between a client and a backend server, forwarding data transparently.

### Core Responsibilities
*   **Connection Termination:** The load balancer accepts the TCP handshake from the client.
*   **Backend Selection:** It applies an algorithm to select an appropriate upstream server.
*   **Connection Initiation:** It establishes a separate TCP connection to the selected backend.
*   **Stream Forwarding:** It pipes bytes from Client $\to$ Backend and Backend $\to$ Client.
*   **Resource Management:** It tracks active connections and enforces limits to protect the system.

## 2. TCP Proxy Connection Lifecycle

Understanding the exact sequence of events is critical for managing state and handling errors.

### The Lifecycle Steps
1.  **Accept Phase:** The load balancer listens on a port. When a client initiates a connection (`SYN`), the OS completes the handshake (`SYN-ACK` $\to$ `ACK`) and places the socket in the accept queue. The Rust application accepts this `TcpStream`.
2.  **Selection Phase:** The application consults the load balancing logic. No data is typically read from the client yet (unless implementation requires reading a header for routing, which moves towards L7).
3.  **Connect Phase:** The load balancer initiates a non-blocking TCP connect to the chosen backend.
    *   *Timeout Risk:* This is a blocking operation logically. In async Rust, this must be wrapped in a timeout (e.g., `tokio::time::timeout`) to prevent hanging tasks.
4.  **Piping Phase (Bidirectional Copy):**
    *   Once both sockets are established, two concurrent tasks (or a single `select!` loop) are needed:
        *   Task A: Read Client $\to$ Write Backend.
        *   Task B: Read Backend $\to$ Write Client.
5.  **Teardown Phase:**
    *   **Half-Close:** If the client sends `FIN`, the LB should send `FIN` to the backend but keep reading from the backend (the server might still be sending data).
    *   **Full-Close:** When both sides have sent `FIN` or a connection `RST` occurs, resources are dropped.

### Backpressure and Buffering
A critical aspect of TCP proxying is **Flow Control**.
*   If the Backend is slow to read, the LB's internal write buffer fills up.
*   The LB must stop reading from the Client.
*   This causes the Client's TCP Window to shrink (TCP Backpressure).
*   *Design Implication:* Do not use unbounded internal buffers in Rust. Use bounded buffers (e.g., 8KB - 64KB) per connection to propagate backpressure naturally to the sender.

## 3. Load Balancing Algorithms and Trade-offs

### Round Robin
*   **How it works:** Iterates through the list of backends sequentially (A $\to$ B $\to$ C $\to$ A).
*   **Complexity:** (1)$ (Atomic counter).
*   **Pros:** Extremely fast, stateless, deterministic.
*   **Cons:** Does not account for server load or connection duration. One server could accumulate many long-lived connections.

### Least Connections
*   **How it works:** Selects the backend with the fewest currently active connections.
*   **Complexity:** (N)$ (scan list) or (\log N)$ (min-heap/priority queue).
*   **Pros:** Balances actual load effectively, preventing "congested" servers.
*   **Cons:** Requires shared mutable state (counters) across all async tasks, leading to potential lock contention (though atomic counters mitigate this).

### Weighted Round Robin
*   **How it works:** Assigns a "weight" to servers (e.g., Server A: 3, Server B: 1). A receives 3 connections for every 1 that B receives.
*   **Complexity:** (1)$.
*   **Use Case:** Heterogeneous hardware (e.g., one server is 2x more powerful).

### Consistent Hashing
*   **How it works:** Maps client attributes (IP address) and servers onto a hash ring.
*   **Pros:** Stickiness. The same client IP always reaches the same backend server (assuming the server set is stable). Minimizes remapping when a server is added/removed.
*   **Cons:** Computationally more expensive (hashing). Uneven distribution if the hash function is poor.

## 4. Async Networking Architecture in Rust (Tokio)

### The Async Model
Rust uses a **Poll-based** asynchronous model.
*   **Futures:** A `Future` is a state machine that can be polled. It returns `Pending` or `Ready`.
*   **Non-blocking I/O:** Sockets are set to non-blocking mode. If a `read` returns `EWOULDBLOCK`, the runtime registers interest with the OS selector (`epoll`/`kqueue`/`IOCP`).
*   **Wakers:** When the OS signals data is ready, the Waker notifies the executor to poll the task again.

### Tokio Runtime
Tokio is the industry-standard runtime for this workload.
*   **Multi-threaded Scheduler:** Uses a work-stealing scheduler. It spawns $ OS threads (usually equal to CPU cores).
*   **Tasks:** A "connection" is a lightweight unit of work (Green Thread). Tokio can handle 10,000+ tasks on a single OS thread, context switching effectively for I/O bound work.
*   **IO Driver:** Batches syscalls to the OS event queue for efficiency.

### Handling Concurrency
*   **Spawn:** For every incoming TCP connection, we use `tokio::spawn`. This moves the connection handling logic into a new, independent task.
*   **Send + Sync:** Data shared between tasks (like the list of backends) must be thread-safe (`Arc`, `Mutex`, `RwLock`, or Atomic types).

## 5. Architecture Patterns in Production Load Balancers

### NGINX / HAProxy (Event Loop Model)
*   **Architecture:** Typically single-threaded (or one process per core) event loops.
*   **Memory:** Zero-copy where possible.
*   **State:** Each process might have its own state, or use shared memory.
*   **Rust Equivalent:** A `LocalSet` in Tokio can mimic a single-threaded event loop, but the standard Tokio threaded runtime is more idiomatic and safer for avoiding blocking mistakes.

### Work-Stealing Model (Rust/Tokio Standard)
*   **Architecture:** A pool of worker threads processes a global queue (and local queues) of tasks.
*   **Advantage:** Automatically balances CPU load if request processing involves CPU work (e.g., TLS termination).
*   **Disadvantage:** Accessing global state (e.g., connection counters) requires synchronization primitives, unlike a single-threaded loop where state is local.

## 6. Performance Considerations and Bottlenecks

### 1. Lock Contention
*   **Problem:** If using "Least Connections", every connection start/end updates a shared counter. With 50k RPS, a single `Mutex` becomes a bottleneck.
*   **Solution:** Use `std::sync::atomic` types (AtomicUsize) for counters. Use `dashmap` or sharded locks for backend maps. Use `ArcSwap` for configuration data that is read often but written rarely.

### 2. Memory Allocation
*   **Problem:** Allocating a new 8KB buffer for every connection creates heap churn.
*   **Solution:**
    *   **Buffer Pooling:** Reuse byte buffers. (Note: In Rust/Tokio, simple allocation is often fast enough, but pooling prevents fragmentation under high load).
    *   **Stack Buffers:** If buffers are small, they can live on the task stack (though async stack sizes are dynamic).

### 3. Copying Overhead
*   **Problem:** Reading from kernel to userspace and writing back to kernel costs CPU.
*   **Solution:**
    *   **Userspace Copy:** `read()` into buffer $\to$ `write()`. Standard and portable.
    *   **Splice (Linux):** Zero-copy movement of data between file descriptors. Rust supports this via platform-specific calls (`nix` crate), significantly reducing CPU usage for heavy throughput.

### 4. File Descriptor Limits
*   **Constraint:** By default, Linux limits open FDs (often 1024). A proxy needs 2 FDs per active connection.
*   **Mitigation:** `ulimit -n` must be increased. Code should handle "Too many open files" errors gracefully.

## 7. Failure Handling Strategies

### Health Checks
*   **Passive:** If a `connect()` to a backend fails, mark it as temporarily down.
*   **Active:** A background task runs periodically (e.g., every 5s) and attempts to connect/ping backends.

### Circuit Breaking
*   If a backend fails $ times in $ seconds, remove it from the rotation for $ seconds. This prevents the LB from sending traffic to a "black hole" and allows the backend time to recover.

### Timeouts
*   **Connect Timeout:** Essential. If a backend IP is unreachable, the TCP SYN might hang for minutes. Hard cap this (e.g., 200ms).
*   **Idle Timeout:** If no data flows for $ minutes, drop the connection to free up RAM/FDs.

## 8. Observability and Metrics

### Structured Logging
*   Use the `tracing` crate. It allows contextual logging (e.g., attaching a `request_id` or `client_ip` to every log generated by a specific async task).

### Metrics
*   **Counters:** Total connections accepted, Total bytes transferred.
*   **Gauges:** Current active connections.
*   **Histograms:** Connection establishment latency.
*   **Implementation:** Use a metrics exporter (Prometheus). In high-perf paths, verify that updating metrics does not introduce lock contention.

## 9. Recommended Architecture for this Project

Based on the research, the following architecture is proposed:

1.  **Runtime:** Tokio (Multi-threaded scheduler).
2.  **Concurrency:** One async task per client connection.
3.  **Core Object:** `ProxyService` struct held in an `Arc`.
    *   Contains `Arc<Vec<Backend>>`.
    *   Contains `Arc<LoadBalancerStrategy>`.
4.  **Data Transfer:** Use `tokio::io::copy_bidirectional` initially for simplicity and correctness. It handles the `read` $\to$ `write` loop and half-closing logic efficiently.
5.  **State Management:**
    *   Use `AtomicUsize` for connection counting.
    *   Use `tokio::sync::RwLock` for the list of healthy backends (read often, write only on health check failure).
6.  **Health Check:** A separate background task (`tokio::spawn`) that loops forever, pinging backends and updating the shared state.

## 10. Possible Advanced Extensions
*   **Proxy Protocol v2:** Implementing the header parsing to preserve client IP addresses to the backend.
*   **TLS Termination:** Decrypting client traffic before forwarding (requires `rustls` or `openssl`).
*   **Hot Reloading:** updating config file without dropping active connections.
