# Rust Reference: Distributed Systems

## Overview

Rust distributed systems leverage async runtimes (primarily tokio) with explicit concurrency patterns. Unlike Erlang/Elixir, Rust has no built-in distributed VM, so networking, serialization, and cluster management require explicit implementation through libraries.

## raft-rs Crate

TiKV's production-grade Raft implementation.

### RawNode API

```rust
use raft::{prelude::*, storage::MemStorage};

// Create Raft configuration
let config = Config {
    id: 1,
    election_tick: 10,
    heartbeat_tick: 3,
    max_size_per_msg: 1024 * 1024,
    max_inflight_msgs: 256,
    ..Default::default()
};

// Create storage and node
let storage = MemStorage::new_with_conf_state(ConfState::from((vec![1, 2, 3], vec![])));
let mut node = RawNode::new(&config, storage, &slog::Logger::root(slog::Discard, slog::o!()))?;

// Propose a command
node.propose(vec![], "my command".as_bytes().to_vec())?;

// Process ready state
if node.has_ready() {
    let mut ready = node.ready();

    // Persist entries to storage
    if !ready.entries().is_empty() {
        store.append(ready.entries())?;
    }

    // Send messages to other nodes
    for msg in ready.take_messages() {
        send_to_peer(msg).await?;
    }

    // Apply committed entries
    for entry in ready.take_committed_entries() {
        apply_entry(entry)?;
    }

    node.advance(ready);
}
```

### Custom Storage Backend

```rust
use raft::{prelude::*, Storage, StorageError};

struct MyStorage {
    entries: Vec<Entry>,
    snapshot: Snapshot,
    hard_state: HardState,
}

impl Storage for MyStorage {
    fn initial_state(&self) -> Result<RaftState> {
        Ok(RaftState {
            hard_state: self.hard_state.clone(),
            conf_state: self.snapshot.get_metadata().get_conf_state().clone(),
        })
    }

    fn entries(&self, low: u64, high: u64, max_size: impl Into<Option<u64>>) -> Result<Vec<Entry>> {
        let max_size = max_size.into().unwrap_or(u64::MAX);
        // Return entries in range [low, high)
        Ok(self.entries
            .iter()
            .filter(|e| e.index >= low && e.index < high)
            .take_while(|e| e.compute_size() as u64 <= max_size)
            .cloned()
            .collect())
    }

    fn term(&self, idx: u64) -> Result<u64> {
        self.entries
            .iter()
            .find(|e| e.index == idx)
            .map(|e| e.term)
            .ok_or(StorageError::Unavailable.into())
    }

    fn first_index(&self) -> Result<u64> {
        Ok(self.entries.first().map(|e| e.index).unwrap_or(1))
    }

    fn last_index(&self) -> Result<u64> {
        Ok(self.entries.last().map(|e| e.index).unwrap_or(0))
    }

    fn snapshot(&self, request_index: u64, to: u64) -> Result<Snapshot> {
        Ok(self.snapshot.clone())
    }
}
```

## tonic for gRPC

### Service Definition (Proto)

```protobuf
// proto/cluster.proto
syntax = "proto3";
package cluster;

service ClusterService {
    rpc Join(JoinRequest) returns (JoinResponse);
    rpc Leave(LeaveRequest) returns (LeaveResponse);
    rpc Heartbeat(HeartbeatRequest) returns (HeartbeatResponse);
    rpc StreamEvents(EventRequest) returns (stream Event);
}

message JoinRequest {
    string node_id = 1;
    string address = 2;
}

message JoinResponse {
    bool success = 1;
    repeated string members = 2;
}
```

### Server Implementation

```rust
use tonic::{transport::Server, Request, Response, Status};

pub mod cluster {
    tonic::include_proto!("cluster");
}

use cluster::cluster_service_server::{ClusterService, ClusterServiceServer};
use cluster::{JoinRequest, JoinResponse, LeaveRequest, LeaveResponse};

#[derive(Default)]
pub struct MyClusterService {
    members: Arc<RwLock<Vec<String>>>,
}

#[tonic::async_trait]
impl ClusterService for MyClusterService {
    async fn join(&self, request: Request<JoinRequest>) -> Result<Response<JoinResponse>, Status> {
        let req = request.into_inner();

        let mut members = self.members.write().await;
        members.push(req.address.clone());

        Ok(Response::new(JoinResponse {
            success: true,
            members: members.clone(),
        }))
    }

    type StreamEventsStream = ReceiverStream<Result<Event, Status>>;

    async fn stream_events(
        &self,
        request: Request<EventRequest>,
    ) -> Result<Response<Self::StreamEventsStream>, Status> {
        let (tx, rx) = mpsc::channel(128);

        tokio::spawn(async move {
            // Stream events to client
            loop {
                let event = get_next_event().await;
                if tx.send(Ok(event)).await.is_err() {
                    break;
                }
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "[::1]:50051".parse()?;
    let service = MyClusterService::default();

    Server::builder()
        .add_service(ClusterServiceServer::new(service))
        .serve(addr)
        .await?;

    Ok(())
}
```

### Client Implementation

```rust
use cluster::cluster_service_client::ClusterServiceClient;

async fn join_cluster(address: &str) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut client = ClusterServiceClient::connect("http://[::1]:50051").await?;

    let request = tonic::Request::new(JoinRequest {
        node_id: "node-1".to_string(),
        address: address.to_string(),
    });

    let response = client.join(request).await?;
    Ok(response.into_inner().members)
}

// Streaming client
async fn watch_events() -> Result<(), Box<dyn std::error::Error>> {
    let mut client = ClusterServiceClient::connect("http://[::1]:50051").await?;

    let mut stream = client
        .stream_events(EventRequest {})
        .await?
        .into_inner();

    while let Some(event) = stream.message().await? {
        println!("Received event: {:?}", event);
    }

    Ok(())
}
```

## tokio Networking Patterns

### TCP Server with Connection Pooling

```rust
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Semaphore;
use std::sync::Arc;

struct Server {
    listener: TcpListener,
    max_connections: Arc<Semaphore>,
}

impl Server {
    async fn new(addr: &str, max_conn: usize) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            listener: TcpListener::bind(addr).await?,
            max_connections: Arc::new(Semaphore::new(max_conn)),
        })
    }

    async fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        loop {
            let permit = self.max_connections.clone().acquire_owned().await?;
            let (socket, addr) = self.listener.accept().await?;

            tokio::spawn(async move {
                if let Err(e) = handle_connection(socket).await {
                    eprintln!("Connection error from {}: {}", addr, e);
                }
                drop(permit); // Release semaphore permit
            });
        }
    }
}

async fn handle_connection(socket: TcpStream) -> Result<(), Box<dyn std::error::Error>> {
    let (reader, writer) = socket.into_split();
    // Process connection
    Ok(())
}
```

### Graceful Shutdown

```rust
use tokio::signal;
use tokio::sync::broadcast;

async fn run_server_with_shutdown() -> Result<(), Box<dyn std::error::Error>> {
    let (shutdown_tx, _) = broadcast::channel(1);

    // Spawn server task
    let server_shutdown = shutdown_tx.subscribe();
    let server_handle = tokio::spawn(async move {
        let listener = TcpListener::bind("127.0.0.1:8080").await.unwrap();

        loop {
            tokio::select! {
                result = listener.accept() => {
                    let (socket, _) = result.unwrap();
                    let mut rx = server_shutdown.resubscribe();
                    tokio::spawn(async move {
                        tokio::select! {
                            _ = handle_connection(socket) => {}
                            _ = rx.recv() => {
                                // Shutdown signal received
                            }
                        }
                    });
                }
                _ = server_shutdown.recv() => {
                    break;
                }
            }
        }
    });

    // Wait for shutdown signal
    signal::ctrl_c().await?;
    println!("Shutdown signal received");

    // Signal all tasks to shutdown
    let _ = shutdown_tx.send(());

    // Wait for server to finish
    server_handle.await?;

    Ok(())
}
```

## Distributed Tracing with tracing

### Basic Setup

```rust
use tracing::{info, instrument, span, Level};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

fn setup_tracing() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();
}

#[instrument(skip(request))]
async fn handle_request(request: Request) -> Response {
    info!(request_id = %request.id, "Processing request");

    let span = span!(Level::INFO, "database_query", table = "users");
    let _enter = span.enter();

    let result = query_database().await;

    info!("Request completed");
    result
}
```

### OpenTelemetry Integration

```rust
use opentelemetry::global;
use opentelemetry_otlp::WithExportConfig;
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::prelude::*;

fn setup_telemetry() -> Result<(), Box<dyn std::error::Error>> {
    let tracer = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(
            opentelemetry_otlp::new_exporter()
                .tonic()
                .with_endpoint("http://localhost:4317"),
        )
        .install_batch(opentelemetry::runtime::Tokio)?;

    tracing_subscriber::registry()
        .with(OpenTelemetryLayer::new(tracer))
        .with(tracing_subscriber::fmt::layer())
        .init();

    Ok(())
}

// Propagate trace context across services
use opentelemetry::propagation::TextMapPropagator;
use opentelemetry::sdk::propagation::TraceContextPropagator;

async fn call_downstream_service(headers: &mut HeaderMap) {
    let propagator = TraceContextPropagator::new();
    let cx = tracing::Span::current().context();

    propagator.inject_context(&cx, &mut HeaderInjector(headers));
}
```

## Service Discovery

### Consul Integration

```rust
use consul::Client;

async fn register_service() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new("http://consul:8500")?;

    client.agent().service_register(
        Service::new()
            .with_id("my-service-1")
            .with_name("my-service")
            .with_address("10.0.0.1")
            .with_port(8080)
            .with_check(Check::http("http://10.0.0.1:8080/health", "10s"))
    ).await?;

    Ok(())
}

async fn discover_services(service_name: &str) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let client = Client::new("http://consul:8500")?;

    let services = client.catalog().service(service_name).await?;

    Ok(services
        .iter()
        .map(|s| format!("{}:{}", s.service_address, s.service_port))
        .collect())
}
```

### Kubernetes Discovery

```rust
use k8s_openapi::api::core::v1::Pod;
use kube::{Api, Client};

async fn discover_pods(namespace: &str, selector: &str) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let client = Client::try_default().await?;
    let pods: Api<Pod> = Api::namespaced(client, namespace);

    let lp = ListParams::default().labels(selector);
    let pod_list = pods.list(&lp).await?;

    Ok(pod_list.items
        .iter()
        .filter_map(|p| p.status.as_ref()?.pod_ip.clone())
        .collect())
}
```

## Consensus Library Comparison

| Crate | Use Case | Maturity | Notes |
|-------|----------|----------|-------|
| `raft-rs` | Raft consensus (TiKV) | Production | Well-tested, feature-complete |
| `async-raft` | Async-native Raft | Active | Tokio-native, easier API |
| `etcd-client` | External consensus | Mature | Uses etcd for coordination |
| `zookeeper-client` | External consensus | Mature | Uses ZooKeeper for coordination |

## CRDTs in Rust

### Using crdts Crate

```rust
use crdts::{GCounter, CmRDT, CvRDT};

// G-Counter (Grow-only)
let mut counter = GCounter::new();
let actor = "node-1";

counter.apply(counter.inc(actor));
counter.apply(counter.inc(actor));

println!("Count: {}", counter.read());  // 2

// Merge with counter from another node
let other_counter = GCounter::new();
counter.merge(other_counter);
```

### LWW-Register (Last-Write-Wins)

```rust
use crdts::{LWWReg, CmRDT};
use std::time::SystemTime;

let mut reg = LWWReg::default();

let timestamp = SystemTime::now()
    .duration_since(SystemTime::UNIX_EPOCH)
    .unwrap()
    .as_nanos() as u64;

reg.apply(reg.write("value", timestamp));
println!("Value: {:?}", reg.read());
```

## Additional Resources

- [raft-rs documentation](https://docs.rs/raft)
- [tonic examples](https://github.com/hyperium/tonic/tree/master/examples)
- [tokio tutorial](https://tokio.rs/tokio/tutorial)
- [tracing documentation](https://docs.rs/tracing)
- [OpenTelemetry Rust](https://github.com/open-telemetry/opentelemetry-rust)
- [TiKV design documents](https://github.com/tikv/tikv/tree/master/docs)
