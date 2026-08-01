# Elixir Reference: Distributed Systems

## Overview

Distributed systems in Elixir/Erlang leverage the BEAM VM's built-in support for distributed computing, including transparent location, process isolation, and automatic failure detection. The runtime provides primitives that most languages must implement through external libraries.

## Distributed Erlang Architecture

### Node Connections and Naming

```elixir
# Start a named node
iex --sname myapp@localhost
# or with fully qualified names
iex --name myapp@192.168.1.100

# Connect to another node
Node.connect(:"other_app@localhost")

# List connected nodes
Node.list()  # [:other_app@localhost]
```

### Location Transparency

```elixir
# Send message to local process
send(pid, {:hello, "world"})

# Send message to remote process - identical API
send({:my_process, :"other@node"}, {:hello, "world"})

# GenServer calls work transparently
GenServer.call({MyServer, :"other@node"}, :get_state)
```

### Global Name Registration

```elixir
# Register globally (available on all connected nodes)
:global.register_name(:my_singleton, self())

# Lookup globally registered process
case :global.whereis_name(:my_singleton) do
  :undefined -> {:error, :not_found}
  pid -> {:ok, pid}
end

# GenServer with global registration
GenServer.start_link(MyServer, [], name: {:global, :my_singleton})
```

### Limitations

- **Full-mesh topology**: Every node connects to every other node
- **O(n^2) network overhead**: Scales poorly beyond ~50-100 nodes
- **All-or-nothing security**: Nodes either fully trust each other or cannot connect
- **Global namespace conflicts**: Name collisions possible across nodes

## libcluster for Service Discovery

### Kubernetes DNS Strategy

```elixir
# config/runtime.exs
config :libcluster,
  topologies: [
    k8s: [
      strategy: Cluster.Strategy.Kubernetes,
      config: [
        mode: :dns,
        kubernetes_node_basename: "myapp",
        kubernetes_selector: "app=myapp",
        kubernetes_namespace: "default",
        polling_interval: 10_000
      ]
    ]
  ]
```

### Kubernetes API Strategy

```elixir
config :libcluster,
  topologies: [
    k8s_api: [
      strategy: Cluster.Strategy.Kubernetes,
      config: [
        mode: :ip,
        kubernetes_ip_lookup_mode: :pods,
        kubernetes_node_basename: "myapp",
        kubernetes_selector: "app=myapp"
      ]
    ]
  ]
```

### Gossip Strategy (UDP Multicast)

```elixir
config :libcluster,
  topologies: [
    gossip: [
      strategy: Cluster.Strategy.Gossip,
      config: [
        port: 45892,
        multicast_addr: "230.1.1.251",
        multicast_ttl: 1,
        secret: "my-secret-token"
      ]
    ]
  ]
```

### Consul Strategy

```elixir
config :libcluster,
  topologies: [
    consul: [
      strategy: Cluster.Strategy.Consul,
      config: [
        host: "consul.service.consul",
        port: 8500,
        service_name: "myapp"
      ]
    ]
  ]
```

## :ra (Raft Implementation)

RabbitMQ's production-grade Raft library for strong consistency.

### State Machine Behaviour

```elixir
defmodule MyApp.StateMachine do
  @behaviour :ra_machine

  @impl true
  def init(_config) do
    %{data: %{}}
  end

  @impl true
  def apply(_meta, {:set, key, value}, state) do
    new_state = put_in(state, [:data, key], value)
    {new_state, :ok}
  end

  @impl true
  def apply(_meta, {:get, key}, state) do
    value = get_in(state, [:data, key])
    {state, {:ok, value}}
  end

  @impl true
  def apply(_meta, {:delete, key}, state) do
    {_, new_state} = pop_in(state, [:data, key])
    {new_state, :ok}
  end
end
```

### Cluster Lifecycle

```elixir
# Start Raft cluster
nodes = [
  {:my_cluster, :"node1@host"},
  {:my_cluster, :"node2@host"},
  {:my_cluster, :"node3@host"}
]

:ra.start_cluster(
  :default,
  :my_cluster,
  {MyApp.StateMachine, %{}},
  nodes
)

# Add node to existing cluster
:ra.add_member({:my_cluster, node()}, {:my_cluster, :"node4@host"})

# Remove node from cluster
:ra.remove_member({:my_cluster, node()}, {:my_cluster, :"node4@host"})
```

### Command Processing

```elixir
# Write command (goes through consensus)
case :ra.process_command({:my_cluster, node()}, {:set, :key, :value}) do
  {:ok, :ok, _leader} -> :ok
  {:timeout, _} -> {:error, :timeout}
  {:error, reason} -> {:error, reason}
end

# Consistent read (goes through leader)
{:ok, {:ok, value}, _} = :ra.consistent_query(
  {:my_cluster, node()},
  fn state -> get_in(state, [:data, :key]) end
)

# Local query (may read stale data)
{:ok, {:ok, value}, _} = :ra.local_query(
  {:my_cluster, node()},
  fn state -> get_in(state, [:data, :key]) end
)
```

## Partisan for Large Clusters

When Distributed Erlang's full-mesh topology becomes limiting (>50 nodes), Partisan provides overlay networks with configurable topologies.

### HyParView Configuration

```elixir
# config/config.exs
config :partisan,
  peer_service: Partisan.PeerService.HyParView,
  peer_port: 10200,
  active_max_size: 6,    # Active view size
  passive_max_size: 30   # Passive view size

# Application supervisor
defmodule MyApp.Application do
  use Application

  def start(_type, _args) do
    children = [
      {Partisan.Supervisor, []}
    ]

    Supervisor.start_link(children, strategy: :one_for_one)
  end
end
```

### Partisan Messaging

```elixir
# Join cluster
Partisan.PeerService.join(:"node@other_host")

# Send message through overlay network
Partisan.forward_message(
  :"target@node",
  :my_process,
  {:hello, "world"}
)

# Get current membership view
Partisan.PeerService.members()
```

### When to Use Partisan

- Cluster size > 50 nodes
- Need custom network topology
- Building multi-datacenter systems
- Require gossip protocols
- Implementing peer-to-peer systems

## CRDTs with delta_crdt

### GCounter (Grow-only Counter)

```elixir
{:ok, counter} = DeltaCrdt.start_link(DeltaCrdt.GCounter, sync_interval: 100)

DeltaCrdt.mutate(counter, :increment)
DeltaCrdt.mutate(counter, :increment)

DeltaCrdt.read(counter)  # 2
```

### PNCounter (Increment/Decrement Counter)

```elixir
{:ok, counter} = DeltaCrdt.start_link(DeltaCrdt.PNCounter)

DeltaCrdt.mutate(counter, :increment)
DeltaCrdt.mutate(counter, :decrement)
DeltaCrdt.mutate(counter, :increment)

DeltaCrdt.read(counter)  # 1
```

### AWLWWMap (Add-wins Last-Write-Wins Map)

```elixir
{:ok, map} = DeltaCrdt.start_link(DeltaCrdt.AWLWWMap)

DeltaCrdt.mutate(map, :add, ["user:1", %{name: "Alice"}])
DeltaCrdt.mutate(map, :add, ["user:2", %{name: "Bob"}])
DeltaCrdt.mutate(map, :remove, ["user:1"])

DeltaCrdt.read(map)  # %{"user:2" => %{name: "Bob"}}
```

### ORSet (Observed-Remove Set)

```elixir
{:ok, set} = DeltaCrdt.start_link(DeltaCrdt.ORSet)

DeltaCrdt.mutate(set, :add, ["item1"])
DeltaCrdt.mutate(set, :add, ["item2"])
DeltaCrdt.mutate(set, :remove, ["item1"])

DeltaCrdt.read(set)  # #MapSet<["item2"]>
```

### Synchronization Between Nodes

```elixir
# On each node, start with same name and sync neighbors
{:ok, crdt} = DeltaCrdt.start_link(
  DeltaCrdt.AWLWWMap,
  name: :my_crdt,
  sync_interval: 100
)

# Set neighbors for synchronization
DeltaCrdt.set_neighbours(crdt, [{:my_crdt, :"node2@host"}, {:my_crdt, :"node3@host"}])
```

## Phoenix.PubSub for Distributed Messaging

### Cluster-wide Broadcasts

```elixir
# Broadcast to all subscribers across all nodes
Phoenix.PubSub.broadcast(MyApp.PubSub, "cache:invalidate", {:invalidate, :user_cache})

# Subscribe to topic
Phoenix.PubSub.subscribe(MyApp.PubSub, "cache:invalidate")

# Handle in GenServer
def handle_info({:invalidate, cache_name}, state) do
  :ets.delete_all_objects(cache_name)
  {:noreply, state}
end
```

### PubSub Configuration

```elixir
# config/config.exs
config :my_app, MyApp.PubSub,
  adapter: Phoenix.PubSub.PG2,
  name: MyApp.PubSub

# Application supervisor
children = [
  {Phoenix.PubSub, name: MyApp.PubSub}
]
```

## Phoenix.Tracker for Distributed Presence

```elixir
defmodule MyApp.Presence do
  use Phoenix.Presence,
    otp_app: :my_app,
    pubsub_server: MyApp.PubSub
end

# Track presence
MyApp.Presence.track(
  self(),
  "users:online",
  user.id,
  %{name: user.name, joined_at: DateTime.utc_now()}
)

# List all presences across cluster
MyApp.Presence.list("users:online")
# %{"user:1" => %{metas: [%{name: "Alice", joined_at: ~U[...]}]}}

# Untrack (happens automatically on process termination)
MyApp.Presence.untrack(self(), "users:online", user.id)
```

## Horde for Distributed Supervisors and Registries

### Distributed Supervisor

```elixir
defmodule MyApp.DistributedSupervisor do
  use Horde.DynamicSupervisor

  def start_link(init_arg) do
    Horde.DynamicSupervisor.start_link(__MODULE__, init_arg, name: __MODULE__)
  end

  def init(_init_arg) do
    Horde.DynamicSupervisor.init(
      strategy: :one_for_one,
      members: :auto
    )
  end
end

# Start child (runs on one node in cluster)
Horde.DynamicSupervisor.start_child(MyApp.DistributedSupervisor, {MyWorker, []})
```

### Distributed Registry

```elixir
defmodule MyApp.Registry do
  use Horde.Registry

  def start_link(_) do
    Horde.Registry.start_link(__MODULE__, [keys: :unique], name: __MODULE__)
  end

  def init(init_arg) do
    Horde.Registry.init(init_arg, members: :auto)
  end
end

# Register process
{:ok, pid} = GenServer.start_link(MyWorker, [], name: {:via, Horde.Registry, {MyApp.Registry, "worker:1"}})

# Lookup
[{pid, _}] = Horde.Registry.lookup(MyApp.Registry, "worker:1")
```

## Library Recommendations

| Library | Use Case | Notes |
|---------|----------|-------|
| `:ra` | Raft consensus | Production-grade, from RabbitMQ |
| `partisan` | Large clusters (>50 nodes) | HyParView topology |
| `libcluster` | Auto-discovery | K8s, Gossip, DNS, Consul strategies |
| `delta_crdt` | Eventually consistent state | G-Counter, PN-Counter, AWLWWMap, ORSet |
| `horde` | Distributed supervisor/registry | Built on CRDTs |
| `lasp` | Advanced CRDTs | Research-grade |
| `swarm` | Process distribution | Auto-rebalancing |

## Additional Resources

- [libcluster GitHub](https://github.com/bitwalker/libcluster)
- [:ra documentation](https://github.com/rabbitmq/ra)
- [Partisan paper](https://arxiv.org/abs/1802.02652)
- [delta_crdt documentation](https://hexdocs.pm/delta_crdt)
- [Horde documentation](https://hexdocs.pm/horde)
- [Distributed Erlang documentation](https://www.erlang.org/doc/reference_manual/distributed.html)
