# lbfy

**lbfy** is a high-performance, asynchronous TCP load balancer written in Rust using the [Tokio](https://tokio.rs/) runtime.

This project serves as an experimental implementation of a Layer 4 proxy, designed to handle thousands of concurrent connections efficiently while exploring advanced infrastructure behaviors like latency-aware routing and load shedding.

## Features

- **Async Core**: Built on Tokio for non-blocking I/O and high concurrency.
- **Load Balancing Strategies**:
  - **Round Robin**: Distributes connections sequentially.
  - **Latency Aware**: Uses "Power of Two Choices" (P2C) to select backends with lower latency (EWMA).
- **Resilience**:
  - **Active Health Checks**: Background task periodically pings backends.
  - **Passive Health Checks**: Detects connection failures during proxying.
  - **Load Shedding**: Rejects new connections when the active count exceeds a configured threshold to protect the system.
- **Observability**:
  - **Prometheus Metrics**: Exposes connection counts and system state at `http://0.0.0.0:9090/metrics`.
  - **Structured Logging**: Uses `tracing` for detailed operational logs.

## Getting Started

### Prerequisites

- Rust (latest stable)
- `netcat` (optional, for manual testing)

### Installation

Clone the repository:

```bash
git clone https://github.com/<YOUR_GITHUB_USERNAME>/lbfy.git
cd lbfy
```

### Running the Load Balancer

1.  **Start Backend Servers**:
    To test the load balancer, you need upstream servers running. You can use `netcat` to simulate them on ports 9000 and 9001.

    Terminal 1 (Backend A):
    ```bash
    nc -l -k 9000
    ```

    Terminal 2 (Backend B):
    ```bash
    nc -l -k 9001
    ```

2.  **Run lbfy**:
    Terminal 3:
    ```bash
    cargo run --release
    ```

    You should see logs indicating the server is listening on `127.0.0.1:8080`.

3.  **Connect as a Client**:
    Terminal 4:
    ```bash
    nc 127.0.0.1 8080
    ```
    Type a message and press Enter. It will be forwarded to one of the backends. Connect again to see the load balancing in action.

## Configuration

Currently, configuration is defined in `src/config.rs` and `src/main.rs`:
- **Listen Address**: `127.0.0.1:8080`
- **Backends**: `127.0.0.1:9000`, `127.0.0.1:9001`
- **Metrics Server**: `0.0.0.0:9090`
- **Max Connections**: 1000 (Load Shedding threshold)

## Observability

To view metrics while the server is running:

```bash
curl http://localhost:9090/metrics
```

Metrics include:
- `lbfy_connections_total`: Total number of accepted connections.
- `lbfy_connections_active`: Current number of active connections.