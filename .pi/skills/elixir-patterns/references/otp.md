# OTP Patterns Reference

Comprehensive reference for OTP (Open Telecom Platform) patterns in Elixir.

## GenServer

GenServers provide stateful processes with a client/server architecture.

### Basic Structure

```elixir
defmodule MyApp.Counter do
  use GenServer

  # Client API (runs in caller's process)

  def start_link(initial_value) do
    GenServer.start_link(__MODULE__, initial_value, name: __MODULE__)
  end

  def get do
    GenServer.call(__MODULE__, :get)
  end

  def increment do
    GenServer.cast(__MODULE__, :increment)
  end

  def increment_by(amount) do
    GenServer.call(__MODULE__, {:increment, amount})
  end

  # Server Callbacks (runs in GenServer process)

  @impl true
  def init(initial_value) do
    {:ok, initial_value}
  end

  @impl true
  def handle_call(:get, _from, state) do
    {:reply, state, state}
  end

  @impl true
  def handle_call({:increment, amount}, _from, state) do
    new_state = state + amount
    {:reply, new_state, new_state}
  end

  @impl true
  def handle_cast(:increment, state) do
    {:noreply, state + 1}
  end

  @impl true
  def handle_info(:timeout, state) do
    # Handle timeout or other messages
    {:noreply, state}
  end
end
```

### Callback Return Values

| Callback | Success Returns |
|----------|-----------------|
| `init/1` | `{:ok, state}`, `{:ok, state, timeout}`, `{:ok, state, :hibernate}`, `{:ok, state, {:continue, term}}`, `:ignore`, `{:stop, reason}` |
| `handle_call/3` | `{:reply, reply, state}`, `{:reply, reply, state, timeout}`, `{:noreply, state}`, `{:stop, reason, reply, state}` |
| `handle_cast/2` | `{:noreply, state}`, `{:noreply, state, timeout}`, `{:stop, reason, state}` |
| `handle_info/2` | `{:noreply, state}`, `{:noreply, state, timeout}`, `{:stop, reason, state}` |
| `handle_continue/2` | `{:noreply, state}`, `{:stop, reason, state}` |
| `terminate/2` | Any (return value ignored) |

### handle_info for External Messages

```elixir
@impl true
def handle_info({:DOWN, ref, :process, pid, reason}, state) do
  # Monitored process died
  {:noreply, handle_process_down(state, pid, reason)}
end

@impl true
def handle_info(:refresh, state) do
  # Periodic refresh (from Process.send_after)
  new_state = refresh_data(state)
  schedule_refresh()
  {:noreply, new_state}
end

@impl true
def handle_info(msg, state) do
  Logger.warning("Unexpected message: #{inspect(msg)}")
  {:noreply, state}
end

defp schedule_refresh do
  Process.send_after(self(), :refresh, :timer.minutes(5))
end
```

### When to Use GenServer

**Use GenServer for:**
- Maintaining state across calls
- Coordinating access to shared resources
- Rate limiting or throttling
- Stateful background processing
- Wrapping external resources (connections, files)

**Avoid GenServer for:**
- Stateless operations (use plain modules)
- Simple key-value cache (use ETS)
- Pure computation (use functions)
- One-off async work (use Task)

## Supervisor

Supervisors monitor and restart child processes.

### Application Supervisor

```elixir
defmodule MyApp.Application do
  use Application

  @impl true
  def start(_type, _args) do
    children = [
      # Database connection pool
      MyApp.Repo,

      # PubSub for real-time features
      {Phoenix.PubSub, name: MyApp.PubSub},

      # Custom GenServer
      {MyApp.Cache, []},

      # GenServer with options
      {MyApp.RateLimiter, max_requests: 100, window_ms: 60_000},

      # DynamicSupervisor for runtime children
      {DynamicSupervisor, name: MyApp.WorkerSupervisor, strategy: :one_for_one},

      # Registry for process lookup
      {Registry, keys: :unique, name: MyApp.Registry},

      # Web endpoint (start last)
      MyAppWeb.Endpoint
    ]

    opts = [strategy: :one_for_one, name: MyApp.Supervisor]
    Supervisor.start_link(children, opts)
  end
end
```

### Supervision Strategies

| Strategy | Behavior | Use When |
|----------|----------|----------|
| `:one_for_one` | Restart only the failed child | Children are independent |
| `:one_for_all` | Restart all children when one fails | Children are interdependent |
| `:rest_for_one` | Restart failed child and all started after it | Sequential dependencies |

### Child Specification

```elixir
# Full specification
%{
  id: MyApp.Worker,
  start: {MyApp.Worker, :start_link, [arg1, arg2]},
  restart: :permanent,
  shutdown: 5000,
  type: :worker
}

# Module with child_spec/1
defmodule MyApp.Worker do
  use GenServer

  def child_spec(opts) do
    %{
      id: opts[:id] || __MODULE__,
      start: {__MODULE__, :start_link, [opts]},
      restart: :permanent,
      shutdown: 5000
    }
  end
end

# In supervisor
children = [
  {MyApp.Worker, id: :worker_1, name: :worker_1},
  {MyApp.Worker, id: :worker_2, name: :worker_2}
]
```

### Restart Values

| Value | Behavior |
|-------|----------|
| `:permanent` | Always restart (default) |
| `:temporary` | Never restart |
| `:transient` | Restart only on abnormal exit |

### Shutdown Values

| Value | Behavior |
|-------|----------|
| `:brutal_kill` | Kill immediately |
| `timeout` (ms) | Wait for graceful shutdown |
| `:infinity` | Wait forever |

## DynamicSupervisor

DynamicSupervisor starts children at runtime.

### Setup

```elixir
# In Application supervisor
children = [
  {DynamicSupervisor, name: MyApp.WorkerSupervisor, strategy: :one_for_one}
]

# Or with max_children
{DynamicSupervisor,
  name: MyApp.WorkerSupervisor,
  strategy: :one_for_one,
  max_children: 1000}
```

### Starting Children

```elixir
def start_worker(user_id) do
  spec = {MyApp.UserWorker, user_id: user_id}
  DynamicSupervisor.start_child(MyApp.WorkerSupervisor, spec)
end

# Returns {:ok, pid} or {:error, reason}
```

### Stopping Children

```elixir
def stop_worker(pid) do
  DynamicSupervisor.terminate_child(MyApp.WorkerSupervisor, pid)
end
```

### Listing Children

```elixir
def list_workers do
  DynamicSupervisor.which_children(MyApp.WorkerSupervisor)
end

# Returns [{:undefined, pid, :worker, [module]}, ...]

def count_workers do
  DynamicSupervisor.count_children(MyApp.WorkerSupervisor)
end

# Returns %{active: n, specs: n, supervisors: n, workers: n}
```

### Pattern: Worker per Entity

```elixir
defmodule MyApp.UserWorkers do
  def ensure_started(user_id) do
    case Registry.lookup(MyApp.Registry, {:user, user_id}) do
      [{pid, _}] ->
        {:ok, pid}

      [] ->
        DynamicSupervisor.start_child(
          MyApp.WorkerSupervisor,
          {MyApp.UserWorker, user_id: user_id}
        )
    end
  end

  def stop(user_id) do
    case Registry.lookup(MyApp.Registry, {:user, user_id}) do
      [{pid, _}] ->
        DynamicSupervisor.terminate_child(MyApp.WorkerSupervisor, pid)

      [] ->
        :ok
    end
  end
end
```

## Registry

Registry provides local process registration and lookup.

### Basic Setup

```elixir
# In Application supervisor
{Registry, keys: :unique, name: MyApp.Registry}

# Or with partitions for high concurrency
{Registry, keys: :unique, name: MyApp.Registry, partitions: System.schedulers_online()}
```

### Via Tuple Pattern

```elixir
defmodule MyApp.UserWorker do
  use GenServer

  def start_link(opts) do
    user_id = Keyword.fetch!(opts, :user_id)
    GenServer.start_link(__MODULE__, opts, name: via_tuple(user_id))
  end

  def get_state(user_id) do
    GenServer.call(via_tuple(user_id), :get_state)
  end

  defp via_tuple(user_id) do
    {:via, Registry, {MyApp.Registry, {:user, user_id}}}
  end

  # Callbacks...
end
```

### Lookup Patterns

```elixir
# Find by key
case Registry.lookup(MyApp.Registry, {:user, user_id}) do
  [{pid, _value}] -> {:ok, pid}
  [] -> {:error, :not_found}
end

# Dispatch to all matching keys (duplicate keys mode)
Registry.dispatch(MyApp.PubSubRegistry, topic, fn entries ->
  for {pid, _} <- entries do
    send(pid, {:broadcast, message})
  end
end)

# Count registrations
Registry.count(MyApp.Registry)

# List all keys
Registry.keys(MyApp.Registry, self())
```

### With Metadata

```elixir
# Register with value
Registry.register(MyApp.Registry, key, %{started_at: DateTime.utc_now()})

# Update value
Registry.update_value(MyApp.Registry, key, fn old -> Map.put(old, :updated_at, DateTime.utc_now()) end)

# Lookup returns value
[{pid, %{started_at: started_at}}] = Registry.lookup(MyApp.Registry, key)
```

## Agent

Agents provide simple state management without the GenServer boilerplate.

### Basic Usage

```elixir
# Start agent
{:ok, pid} = Agent.start_link(fn -> %{} end)

# Get state
Agent.get(pid, fn state -> state end)

# Update state
Agent.update(pid, fn state -> Map.put(state, :key, "value") end)

# Get and update atomically
Agent.get_and_update(pid, fn state ->
  {Map.get(state, :key), Map.put(state, :key, "new_value")}
end)
```

### Named Agent

```elixir
defmodule MyApp.Config do
  use Agent

  def start_link(initial_config) do
    Agent.start_link(fn -> initial_config end, name: __MODULE__)
  end

  def get(key) do
    Agent.get(__MODULE__, fn config -> Map.get(config, key) end)
  end

  def put(key, value) do
    Agent.update(__MODULE__, fn config -> Map.put(config, key, value) end)
  end
end
```

### When to Use Agent

**Use Agent for:**
- Simple state that doesn't need complex logic
- Configuration or feature flags
- Caching simple values

**Use GenServer instead when:**
- Complex business logic in state updates
- Need handle_info for external messages
- Need timeouts or hibernation
- State updates have side effects

## Task

Tasks provide simple concurrency for one-off work.

### Async/Await

```elixir
# Start async task
task = Task.async(fn -> expensive_operation() end)

# Do other work...

# Wait for result (blocks, raises on error)
result = Task.await(task)

# With timeout
result = Task.await(task, 5000)
```

### Multiple Tasks

```elixir
# Start multiple tasks
tasks = [
  Task.async(fn -> fetch_users() end),
  Task.async(fn -> fetch_orders() end),
  Task.async(fn -> fetch_products() end)
]

# Wait for all (raises if any fail)
[users, orders, products] = Task.await_many(tasks, 10_000)

# Yield results (doesn't raise)
results = Task.yield_many(tasks, 5000)

for {task, result} <- results do
  case result do
    {:ok, value} -> handle_success(value)
    {:exit, reason} -> handle_failure(reason)
    nil -> Task.shutdown(task)  # Timed out
  end
end
```

### Task.Supervisor

For fault tolerance, start tasks under a supervisor:

```elixir
# In Application supervisor
{Task.Supervisor, name: MyApp.TaskSupervisor}

# Start supervised task (linked)
Task.Supervisor.start_child(MyApp.TaskSupervisor, fn ->
  process_job(job)
end)

# Async/await with supervisor
task = Task.Supervisor.async(MyApp.TaskSupervisor, fn ->
  expensive_operation()
end)
result = Task.await(task)

# Fire and forget (no link, crash won't affect caller)
{:ok, _pid} = Task.Supervisor.start_child(MyApp.TaskSupervisor, fn ->
  fire_and_forget()
end)
```

### Error Handling with Yield

```elixir
task = Task.async(fn -> risky_operation() end)

case Task.yield(task, 5000) || Task.shutdown(task) do
  {:ok, result} ->
    {:ok, result}

  {:exit, reason} ->
    Logger.error("Task failed: #{inspect(reason)}")
    {:error, :task_failed}

  nil ->
    Logger.warning("Task timed out")
    {:error, :timeout}
end
```

## GenStage

GenStage is for building producer-consumer pipelines with backpressure.

### Basic Structure

```elixir
# Producer
defmodule MyApp.Producer do
  use GenStage

  def start_link(opts) do
    GenStage.start_link(__MODULE__, opts, name: __MODULE__)
  end

  def init(_opts) do
    {:producer, %{}}
  end

  def handle_demand(demand, state) when demand > 0 do
    events = generate_events(demand)
    {:noreply, events, state}
  end
end

# Consumer
defmodule MyApp.Consumer do
  use GenStage

  def start_link(opts) do
    GenStage.start_link(__MODULE__, opts)
  end

  def init(opts) do
    {:consumer, %{}, subscribe_to: [{MyApp.Producer, max_demand: 100}]}
  end

  def handle_events(events, _from, state) do
    Enum.each(events, &process_event/1)
    {:noreply, [], state}
  end
end
```

### When to Use GenStage

**Use GenStage for:**
- Processing streams with backpressure
- Rate-limited processing
- ETL pipelines
- Real-time data processing

**Consider alternatives:**
- Broadway for out-of-box producers (SQS, RabbitMQ, Kafka)
- Flow for parallel data processing
- Simple Task.async_stream for bounded parallelism

## "Let It Crash" Philosophy

The supervision tree enables a different approach to error handling.

### Expected vs Unexpected Errors

**Handle expected errors in business logic:**
```elixir
def create_user(params) do
  case validate(params) do
    {:ok, validated} ->
      case Repo.insert(User.changeset(%User{}, validated)) do
        {:ok, user} -> {:ok, user}
        {:error, changeset} -> {:error, :validation_failed, changeset}
      end

    {:error, reason} ->
      {:error, :invalid_params, reason}
  end
end
```

**Let unexpected errors crash:**
```elixir
# Don't wrap everything in try/rescue
def handle_call(:process, _from, state) do
  # If this fails, supervisor restarts with clean state
  result = risky_but_essential_operation(state)
  {:reply, result, state}
end
```

### Supervisor Restart Strategy

```elixir
# Configure restart intensity
opts = [
  strategy: :one_for_one,
  max_restarts: 3,      # Max restarts within window
  max_seconds: 5        # Window size
]
Supervisor.start_link(children, opts)
```

If a child restarts more than `max_restarts` times in `max_seconds`, the supervisor itself terminates (escalating to its parent).

### Clean State on Restart

```elixir
defmodule MyApp.ConnectionManager do
  use GenServer

  # On restart, init/1 reconnects
  def init(config) do
    case connect(config) do
      {:ok, conn} -> {:ok, %{conn: conn, config: config}}
      {:error, _} -> {:stop, :connection_failed}
    end
  end

  # Crash on connection errors - supervisor restarts
  def handle_info({:tcp_closed, _}, state) do
    {:stop, :connection_closed, state}
  end
end
```

## References

- **Elixir patterns**: the `elixir-patterns` skill
- **Contexts**: the `elixir-patterns` skill\'s contexts reference
- **Universal concurrency**: the `concurrency` skill
