# Elixir Error Handling

Elixir's error handling philosophy combines explicit result types with the "let it crash" supervision model.

## Tagged Tuples Convention

Elixir uses tagged tuples as the standard result type:

```elixir
# Standard return types
@type result :: {:ok, value} | {:error, reason}

# Success with value
{:ok, user} = Accounts.get_user(id)

# Failure with reason
{:error, :not_found} = Accounts.get_user(999)
```

### Multiple Error Types

Use specific atoms or structured tuples for different error cases:

```elixir
@spec create_user(map()) ::
  {:ok, User.t()}
  | {:error, :validation_failed, Ecto.Changeset.t()}
  | {:error, :email_exists}
  | {:error, :service_unavailable}

def create_user(params) do
  with {:ok, changeset} <- validate(params),
       false <- email_exists?(changeset.changes.email),
       {:ok, user} <- insert(changeset) do
    {:ok, user}
  else
    true -> {:error, :email_exists}
    {:error, changeset} -> {:error, :validation_failed, changeset}
    {:error, :db_error} -> {:error, :service_unavailable}
  end
end
```

### Guidelines

- First element is `:ok` or `:error` (the tag)
- Second element is the value or reason
- Use atoms for error types (`:not_found`, `:unauthorized`, `:invalid`)
- Complex errors can include additional context as third+ elements

### Fetch vs Get Convention

Elixir libraries follow a naming convention:

```elixir
# Returns {:ok, value} or {:error, reason}
def fetch_user(id), do: ...

# Returns value or nil
def get_user(id), do: ...

# Returns value or raises
def get_user!(id), do: ...
```

## Railway-Oriented Programming with `with`

The `with` expression chains operations that return `{:ok, value}`:

```elixir
def create_user(params) do
  with {:ok, validated} <- validate_params(params),
       {:ok, user} <- insert_user(validated),
       {:ok, _token} <- generate_token(user),
       {:ok, _email} <- send_welcome_email(user) do
    {:ok, user}
  else
    {:error, changeset = %Ecto.Changeset{}} ->
      {:error, {:validation, changeset}}

    {:error, :email_failed} = error ->
      # User created but email failed - different handling
      Logger.warning("Welcome email failed", user_id: user.id)
      error

    error ->
      error
  end
end
```

### How `with` Works

1. Each clause matches against the left side of `<-`
2. If match succeeds, continues to next clause
3. If match fails, jumps to `else` block
4. The `do` block executes only if all clauses succeed

### Pattern Matching in Else

The `else` block pattern matches against failed results:

```elixir
with {:ok, user} <- fetch_user(id),
     {:ok, order} <- fetch_order(order_id),
     true <- user.id == order.user_id do
  process_order(order)
else
  {:error, :user_not_found} -> {:error, :unauthorized}
  {:error, :order_not_found} -> {:error, :not_found}
  false -> {:error, :forbidden}  # User doesn't own order
end
```

### When to Use `with`

- Sequential operations where each depends on previous success
- Multiple potential failure points
- Need different handling for different error types

### When NOT to Use `with`

```elixir
# Too simple - just use case
with {:ok, user} <- get_user(id) do
  {:ok, user.name}
end

# Better:
case get_user(id) do
  {:ok, user} -> {:ok, user.name}
  error -> error
end
```

## Pattern Matching for Error Handling

Use pattern matching in function heads for clean error handling:

```elixir
def process_result({:ok, value}), do: {:success, transform(value)}
def process_result({:error, reason}), do: {:failure, reason}

# With guards for more specific matching
def handle_error({:error, :not_found}), do: {:error, 404}
def handle_error({:error, :unauthorized}), do: {:error, 401}
def handle_error({:error, _}), do: {:error, 500}
```

## Exception Handling

Elixir has exceptions but they're used sparingly:

### Raising Exceptions

```elixir
# Raise with message
raise "Something went wrong"

# Raise specific error type
raise ArgumentError, message: "Invalid argument"

# Bang functions raise on failure
user = Repo.get!(User, id)  # Raises Ecto.NoResultsError if not found
```

### Rescuing Exceptions

```elixir
try do
  dangerous_operation()
rescue
  e in ArgumentError ->
    Logger.error("Invalid argument: #{e.message}")
    {:error, :invalid_argument}
  e in RuntimeError ->
    {:error, {:runtime_error, e.message}}
end
```

### When to Use Exceptions

- Programming bugs (should never happen in correct code)
- Interfacing with libraries that raise
- When you want to crash and let supervisor restart

### When to Use Result Tuples

- Expected failure cases (user not found, validation failed)
- API boundaries
- When caller must handle the error

## "Let It Crash" Philosophy

Elixir's supervision model changes how we think about errors:

### Unexpected vs Expected Errors

```elixir
# EXPECTED: User might not exist - handle with result tuple
def get_user(id) do
  case Repo.get(User, id) do
    nil -> {:error, :not_found}
    user -> {:ok, user}
  end
end

# UNEXPECTED: Database connection failed - let it crash
def get_user!(id) do
  Repo.get!(User, id)  # Raises, process crashes, supervisor restarts
end
```

### Supervisor Restart Strategies

```elixir
children = [
  # Restart this child if it crashes
  {MyApp.Worker, restart: :permanent},

  # Don't restart (for one-time tasks)
  {MyApp.Setup, restart: :temporary},

  # Restart only if it crashes abnormally
  {MyApp.Service, restart: :transient}
]

Supervisor.start_link(children, strategy: :one_for_one)
```

### Benefits

- Clean slate on restart (no corrupted state)
- Simpler code (don't defend against every edge case)
- Focus error handling on expected cases
- Supervisors handle unexpected failures

## Error Conversion at Boundaries

Convert errors at system boundaries:

```elixir
# External API boundary - convert to domain errors
def fetch_weather(city) do
  case HTTPoison.get("https://api.weather.com/#{city}") do
    {:ok, %{status_code: 200, body: body}} ->
      {:ok, Jason.decode!(body)}

    {:ok, %{status_code: 404}} ->
      {:error, :city_not_found}

    {:ok, %{status_code: code}} when code >= 500 ->
      {:error, :service_unavailable}

    {:error, %HTTPoison.Error{reason: reason}} ->
      Logger.error("Weather API error", reason: reason)
      {:error, :service_unavailable}
  end
end
```

## Changeset Errors

Ecto changesets accumulate validation errors:

```elixir
def create_user(attrs) do
  %User{}
  |> User.changeset(attrs)
  |> Repo.insert()
  |> case do
    {:ok, user} ->
      {:ok, user}

    {:error, changeset} ->
      errors = format_changeset_errors(changeset)
      {:error, {:validation_failed, errors}}
  end
end

defp format_changeset_errors(changeset) do
  Ecto.Changeset.traverse_errors(changeset, fn {msg, opts} ->
    Enum.reduce(opts, msg, fn {key, value}, acc ->
      String.replace(acc, "%{#{key}}", to_string(value))
    end)
  end)
end
```

## Custom Error Types

Define custom errors for domain-specific failures:

```elixir
defmodule MyApp.Errors do
  defmodule NotFoundError do
    defexception [:resource, :id, :message]

    @impl true
    def exception(opts) do
      resource = Keyword.fetch!(opts, :resource)
      id = Keyword.fetch!(opts, :id)
      %__MODULE__{
        resource: resource,
        id: id,
        message: "#{resource} with id #{id} not found"
      }
    end
  end

  defmodule ValidationError do
    defexception [:field, :message]
  end
end
```

## Error Handling Best Practices

### 1. Be Explicit About What Can Fail

```elixir
@spec create_order(map()) ::
  {:ok, Order.t()}
  | {:error, :invalid_items}
  | {:error, :insufficient_stock}
  | {:error, :payment_failed}
```

### 2. Use Meaningful Error Atoms

```elixir
# Good - specific and actionable
{:error, :email_already_registered}
{:error, :password_too_short}
{:error, :rate_limit_exceeded}

# Bad - vague
{:error, :invalid}
{:error, :failed}
{:error, :error}
```

### 3. Include Context When Helpful

```elixir
{:error, {:validation_failed, changeset}}
{:error, {:rate_limited, retry_after: 60}}
{:error, {:payment_failed, reason: "Card declined", code: "card_declined"}}
```

### 4. Convert at Boundaries

```elixir
# Don't leak internal errors to API consumers
def handle_create_user(conn, params) do
  case Accounts.create_user(params) do
    {:ok, user} ->
      json(conn, %{id: user.id})

    {:error, {:validation_failed, changeset}} ->
      conn
      |> put_status(422)
      |> json(%{errors: format_errors(changeset)})

    {:error, :email_exists} ->
      conn
      |> put_status(409)
      |> json(%{error: "Email already registered"})
  end
end
```

### 5. Log at the Right Level

```elixir
# Expected failures - debug or info
{:error, :not_found} -> Logger.debug("User not found", user_id: id)

# Unexpected failures - warn or error
{:error, :db_connection_failed} -> Logger.error("Database unavailable")
```
