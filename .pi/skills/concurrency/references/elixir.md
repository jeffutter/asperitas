# Elixir Concurrency

Elixir runs on the BEAM VM, which provides lightweight processes, preemptive scheduling, and fault tolerance through supervision trees.

## Process Model

Elixir processes are not OS threads - they're lightweight, isolated units managed by the BEAM:

| Feature | Elixir Process | OS Thread |
|---------|---------------|-----------|
| Memory | ~2KB initial | ~1MB stack |
| Creation time | Microseconds | Milliseconds |
| Count | Millions possible | Thousands |
| Communication | Message passing | Shared memory |
| Isolation | Complete (crash isolation) | None |

### Spawning Processes

```elixir
# Basic spawn - fire and forget
spawn(fn -> IO.puts("Hello from process!") end)

# Spawn with link - crash propagation
spawn_link(fn -> raise "Error!" end)  # Caller also crashes

# Spawn with monitor - notification only
{pid, ref} = spawn_monitor(fn -> :work end)
receive do
  {:DOWN, ^ref, :process, ^pid, reason} ->
    IO.puts("Process exited: #{inspect(reason)}")
end
```

### Message Passing

Processes communicate by sending messages to mailboxes:

```elixir
# Send message
send(pid, {:hello, "world"})

# Receive message
receive do
  {:hello, msg} -> IO.puts("Got: #{msg}")
  {:error, reason} -> IO.puts("Error: #{reason}")
after
  5000 -> IO.puts("Timeout!")
end
```

## Task Module

`Task` provides a simple abstraction for concurrent operations:

### Async/Await

```elixir
# Start tasks concurrently
task1 = Task.async(fn -> fetch_user(user_id) end)
task2 = Task.async(fn -> fetch_orders(user_id) end)
task3 = Task.async(fn -> fetch_reviews(user_id) end)

# Wait for results
user = Task.await(task1)
orders = Task.await(task2)
reviews = Task.await(task3)

# Or await all at once
[user, orders, reviews] = Task.await_many([task1, task2, task3], 5000)
```

### Task.async_stream

Process collections concurrently with backpressure:

```elixir
urls
|> Task.async_stream(&fetch_url/1, max_concurrency: 10, timeout: 30_000)
|> Enum.map(fn {:ok, result} -> result end)
```

### Task.Supervisor

Supervised tasks with fault isolation:

```elixir
# In application supervisor
children = [
  {Task.Supervisor, name: MyApp.TaskSupervisor}
]

# Start supervised task
{:ok, result} = Task.Supervisor.async_nolink(
  MyApp.TaskSupervisor,
  fn -> expensive_operation() end
) |> Task.await()

# Fire and forget with supervision
Task.Supervisor.start_child(MyApp.TaskSupervisor, fn ->
  send_notification(user)
end)
```

## GenServer

GenServer provides stateful processes with a client-server interface:

```elixir
defmodule MyApp.Counter do
  use GenServer

  # Client API
  def start_link(initial_value) do
    GenServer.start_link(__MODULE__, initial_value, name: __MODULE__)
  end

  def increment do
    GenServer.call(__MODULE__, :increment)
  end

  def get do
    GenServer.call(__MODULE__, :get)
  end

  def reset do
    GenServer.cast(__MODULE__, :reset)
  end

  # Server Callbacks
  @impl true
  def init(initial_value) do
    {:ok, initial_value}
  end

  @impl true
  def handle_call(:increment, _from, state) do
    new_state = state + 1
    {:reply, new_state, new_state}
  end

  @impl true
  def handle_call(:get, _from, state) do
    {:reply, state, state}
  end

  @impl true
  def handle_cast(:reset, _state) do
    {:noreply, 0}
  end
end
```

### Call vs Cast

| Method | Behavior | Use When |
|--------|----------|----------|
| `call` | Synchronous, waits for reply | Need result or confirmation |
| `cast` | Asynchronous, fire-and-forget | Don't need result |
| `send` | Raw message to mailbox | Custom protocols |

### GenServer Patterns

**Periodic work:**

```elixir
def init(state) do
  schedule_work()
  {:ok, state}
end

def handle_info(:do_work, state) do
  # Do periodic work
  new_state = perform_work(state)
  schedule_work()
  {:noreply, new_state}
end

defp schedule_work do
  Process.send_after(self(), :do_work, :timer.seconds(30))
end
```

**Timeouts:**

```elixir
def handle_call(:get, _from, state) do
  {:reply, state, state, 5000}  # Timeout after 5s of inactivity
end

def handle_info(:timeout, state) do
  # Cleanup idle state
  {:noreply, cleanup(state)}
end
```

## Supervision

Supervisors monitor child processes and restart them on failure:

```elixir
defmodule MyApp.Application do
  use Application

  def start(_type, _args) do
    children = [
      # Simple child
      MyApp.Repo,

      # Child with options
      {MyApp.Cache, []},

      # Dynamic supervisor for runtime children
      {DynamicSupervisor, name: MyApp.WorkerSupervisor, strategy: :one_for_one},

      # Task supervisor
      {Task.Supervisor, name: MyApp.TaskSupervisor}
    ]

    opts = [strategy: :one_for_one, name: MyApp.Supervisor]
    Supervisor.start_link(children, opts)
  end
end
```

### Restart Strategies

| Strategy | Behavior |
|----------|----------|
| `:one_for_one` | Only restart failed child |
| `:one_for_all` | Restart all children if one fails |
| `:rest_for_one` | Restart failed child and all started after it |

### Child Specifications

```elixir
def child_spec(opts) do
  %{
    id: __MODULE__,
    start: {__MODULE__, :start_link, [opts]},
    restart: :permanent,  # always restart
    # restart: :temporary,  # never restart
    # restart: :transient,  # restart only on abnormal exit
    shutdown: 5000,  # graceful shutdown timeout
    type: :worker
  }
end
```

### DynamicSupervisor

For runtime-created children:

```elixir
# Start a dynamic supervisor
{:ok, sup} = DynamicSupervisor.start_link(strategy: :one_for_one)

# Add children at runtime
{:ok, pid} = DynamicSupervisor.start_child(sup, {MyWorker, user_id})

# Remove child
DynamicSupervisor.terminate_child(sup, pid)
```

## Registry

Process discovery and naming:

```elixir
# Start registry
{:ok, _} = Registry.start_link(keys: :unique, name: MyApp.Registry)

# Use via tuple for named GenServers
defmodule MyApp.UserWorker do
  use GenServer

  def start_link(user_id) do
    GenServer.start_link(__MODULE__, user_id, name: via_tuple(user_id))
  end

  def get(user_id) do
    GenServer.call(via_tuple(user_id), :get)
  end

  defp via_tuple(user_id) do
    {:via, Registry, {MyApp.Registry, {:user, user_id}}}
  end
end

# Lookup process
case Registry.lookup(MyApp.Registry, {:user, 123}) do
  [{pid, _value}] -> send(pid, :message)
  [] -> {:error, :not_found}
end
```

## "Let It Crash"

Focus on expected errors, let supervisors handle unexpected ones:

```elixir
# Expected error - handle explicitly
def get_user(id) do
  case Repo.get(User, id) do
    nil -> {:error, :not_found}
    user -> {:ok, user}
  end
end

# Unexpected error - let it crash
def process_order!(order) do
  # If any of these fail, something is seriously wrong
  # Let supervisor restart with clean state
  user = Repo.get!(User, order.user_id)
  product = Repo.get!(Product, order.product_id)
  create_invoice!(user, product, order)
end
```

### Crash Isolation

```elixir
# Child crashes don't affect siblings (one_for_one)
children = [
  {Worker1, []},  # If this crashes...
  {Worker2, []},  # ...this keeps running
  {Worker3, []}
]

# Linked processes crash together (by design)
spawn_link(fn -> raise "error" end)  # Caller also crashes
```

## ETS for Shared State

When you need shared state without message passing overhead:

```elixir
# Create table
:ets.new(:cache, [:named_table, :public, read_concurrency: true])

# Write (atomic)
:ets.insert(:cache, {:key, value})

# Read (concurrent)
case :ets.lookup(:cache, :key) do
  [{:key, value}] -> value
  [] -> nil
end

# Atomic update
:ets.update_counter(:cache, :counter, 1)
```

## Concurrency Patterns

### Fan-Out/Fan-In

```elixir
def process_all(items) do
  items
  |> Enum.map(&Task.async(fn -> process(&1) end))
  |> Task.await_many(30_000)
end
```

### Producer-Consumer with GenStage

```elixir
# For complex pipelines, use GenStage
defmodule Producer do
  use GenStage

  def init(counter) do
    {:producer, counter}
  end

  def handle_demand(demand, counter) do
    events = Enum.to_list(counter..counter + demand - 1)
    {:noreply, events, counter + demand}
  end
end
```

### Rate Limiting

```elixir
defmodule RateLimiter do
  use GenServer

  def init(opts) do
    {:ok, %{
      requests: 0,
      max: opts[:max] || 100,
      window: opts[:window] || 60_000
    }}
  end

  def handle_call(:check, _from, state) do
    if state.requests < state.max do
      {:reply, :ok, %{state | requests: state.requests + 1}}
    else
      {:reply, {:error, :rate_limited}, state}
    end
  end

  def handle_info(:reset, state) do
    schedule_reset(state.window)
    {:noreply, %{state | requests: 0}}
  end
end
```

## Best Practices

### 1. Use Tasks for Simple Concurrency

```elixir
# Good for fire-and-forget or simple async/await
Task.async(fn -> send_email(user) end)
```

### 2. Use GenServer for Stateful Processes

```elixir
# Good for caches, rate limiters, connection pools
defmodule Cache do
  use GenServer
  # ...
end
```

### 3. Supervise Everything

```elixir
# All long-running processes should be supervised
children = [
  MyApp.Repo,
  MyApp.Cache,
  {Task.Supervisor, name: MyApp.TaskSupervisor}
]
```

### 4. Avoid Shared Mutable State

```elixir
# Prefer message passing over ETS when possible
GenServer.call(Cache, {:get, key})

# Use ETS only for high-performance read-heavy workloads
:ets.lookup(:cache, key)
```

### 5. Handle Timeouts

```elixir
# Always specify timeouts
GenServer.call(pid, :request, 5000)
Task.await(task, 10_000)

# Or use :infinity only when you're certain
GenServer.call(pid, :request, :infinity)
```
