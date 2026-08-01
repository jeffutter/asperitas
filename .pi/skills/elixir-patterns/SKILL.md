---
name: elixir-patterns
description: Elixir-specific language patterns including pattern matching, immutability, protocols, behaviours, and macros
---

# Elixir Patterns

This skill covers patterns specific to the Elixir language. For universal concepts, see the cross-references below.

## Cross-References

For cross-cutting concerns implemented in Elixir, see these universal skills:

- **Error handling**: the `error-handling` skill - Railway-oriented programming, tagged tuples
- **Concurrency**: the `concurrency` skill and the `elixir-patterns` skill\'s otp reference
- **Domain-Driven Design**: the `domain-driven-design` skill and the `elixir-patterns` skill\'s contexts reference
- **Data validation**: the `data-validation` skill

## 1. Pattern Matching

Pattern matching is Elixir's fundamental control flow mechanism.

### Function Head Pattern Matching

```elixir
# Match on data structure shape
def process_result({:ok, value}), do: {:success, value}
def process_result({:error, reason}), do: {:failure, reason}

# Match and destructure structs
def greet(%User{name: name, admin: true}), do: "Hello Admin #{name}"
def greet(%User{name: name}), do: "Hello #{name}"

# Match with guards
def calculate(x) when is_integer(x) and x > 0, do: x * 2
def calculate(x) when is_integer(x), do: 0
def calculate(_), do: {:error, :invalid_input}
```

**When to use:**
- Multiple code paths based on input shape
- Extracting values from complex structures
- Validating input at function boundary

**Prefer pattern matching over conditionals:**
```elixir
# Prefer: match in function head
def process({:ok, value}), do: handle_success(value)
def process({:error, reason}), do: handle_error(reason)

# Avoid: case inside function
def process(result) do
  case result do
    {:ok, value} -> handle_success(value)
    {:error, reason} -> handle_error(reason)
  end
end
```

### Pin Operator

The pin operator (`^`) matches against existing variable values:

```elixir
expected_id = 123

case user do
  %User{id: ^expected_id} -> :found
  %User{} -> :different_user
end

# Common in with expressions
with {:ok, ^expected} <- validate(input) do
  :validated
end
```

### Case and Cond

```elixir
# case: match on structure
case HTTP.get(url) do
  {:ok, %{status: 200, body: body}} -> {:ok, body}
  {:ok, %{status: 404}} -> {:error, :not_found}
  {:ok, %{status: status}} -> {:error, {:http_error, status}}
  {:error, reason} -> {:error, reason}
end

# cond: match on boolean conditions
cond do
  age < 13 -> :child
  age < 20 -> :teenager
  age < 65 -> :adult
  true -> :senior
end
```

### Destructuring

```elixir
# Maps
%{name: name, email: email} = user

# Nested
%{user: %{profile: %{bio: bio}}} = data

# Lists
[head | tail] = list
[first, second | rest] = list

# Tuples
{:ok, result} = operation()

# Function arguments
def process(%{items: items, total: total}) do
  # items and total bound
end
```

## 2. Immutable Data Transformations

All data in Elixir is immutable. Transformations create new data structures.

### Pipe Operator

Chain transformations with the pipe operator:

```elixir
user
|> Map.put(:name, "New Name")
|> Map.put(:updated_at, DateTime.utc_now())
|> Map.update!(:login_count, &(&1 + 1))
```

**Guidelines:**
- First argument flows through the pipe
- Keep transformations small and focused
- Each step should be meaningful

### Enum Module

Transform collections:

```elixir
users
|> Enum.filter(&(&1.active))
|> Enum.map(&format_user/1)
|> Enum.sort_by(& &1.name)
|> Enum.take(10)

# Reduce for aggregation
Enum.reduce(items, 0, fn item, acc -> acc + item.price end)

# Group by attribute
Enum.group_by(users, & &1.role)
```

### Map Operations

```elixir
# Put/update
Map.put(map, :key, value)
Map.update(map, :key, default, &update_fn/1)
Map.update!(map, :key, &update_fn/1)  # Raises if key missing

# Get with default
Map.get(map, :key, default)

# Merge
Map.merge(map1, map2)
Map.merge(map1, map2, fn _k, v1, v2 -> v1 + v2 end)

# Struct update syntax
%{user | name: "Updated", email: "new@example.com"}
```

### Nested Updates

Access behaviour for deep updates:

```elixir
# get_in
get_in(data, [:user, :profile, :bio])

# put_in
put_in(data, [:user, :profile, :bio], "New bio")

# update_in
update_in(data, [:user, :stats, :views], &(&1 + 1))

# With structs (use Access.key)
update_in(user, [Access.key(:profile), Access.key(:bio)], &String.upcase/1)

# Dynamic path
path = [:user, :settings, setting_key]
get_in(data, path)
```

### Performance Considerations

```elixir
# List prepend is O(1)
[new_item | existing_list]

# List append is O(n) - avoid
existing_list ++ [new_item]

# Build in reverse, then reverse once
items
|> Enum.reduce([], fn item, acc -> [transform(item) | acc] end)
|> Enum.reverse()

# Or use Enum.map (handles this internally)
Enum.map(items, &transform/1)
```

## 3. Protocols

Protocols enable polymorphic behavior across types.

### Defining Protocols

```elixir
defprotocol Serializable do
  @doc "Serialize data to JSON-compatible format"
  @spec serialize(t) :: map()
  def serialize(data)
end
```

### Implementing Protocols

```elixir
defimpl Serializable, for: User do
  def serialize(%User{name: name, email: email, id: id}) do
    %{id: id, name: name, email: email}
  end
end

defimpl Serializable, for: Product do
  def serialize(%Product{id: id, name: name, price: price}) do
    %{id: id, name: name, price: Decimal.to_float(price)}
  end
end

# For any struct (fallback)
defimpl Serializable, for: Any do
  def serialize(struct) do
    struct
    |> Map.from_struct()
    |> Map.drop([:__meta__])
  end
end
```

### Using @derive

```elixir
defmodule User do
  @derive {Serializable, only: [:id, :name, :email]}
  defstruct [:id, :name, :email, :password_hash]
end
```

### Built-in Protocols

| Protocol | Purpose | Common Implementations |
|----------|---------|----------------------|
| `Enumerable` | Enable Enum functions | List, Map, Range |
| `Collectable` | Enable into: option | List, Map, MapSet |
| `Inspect` | Custom inspect output | All types |
| `String.Chars` | to_string/1 | Atom, Integer, etc. |

### Protocol vs Behaviour

| Aspect | Protocol | Behaviour |
|--------|----------|-----------|
| Dispatch | On data type | On module |
| Use case | Polymorphism over data | Contract for modules |
| Definition | `defprotocol` | `@callback` |
| Implementation | `defimpl` | `@behaviour` + `@impl` |

## 4. Behaviours

Behaviours define module contracts with callback specifications.

### Defining Behaviours

```elixir
defmodule MyApp.PaymentGateway do
  @doc "Process a payment charge"
  @callback charge(amount :: Money.t(), card :: Card.t()) ::
    {:ok, Transaction.t()} | {:error, term()}

  @doc "Refund a previous charge"
  @callback refund(transaction_id :: String.t(), amount :: Money.t()) ::
    {:ok, Refund.t()} | {:error, term()}

  @doc "Check gateway health"
  @callback health_check() :: :ok | {:error, term()}

  # Optional callback with default
  @optional_callbacks [health_check: 0]
end
```

### Implementing Behaviours

```elixir
defmodule MyApp.PaymentGateway.Stripe do
  @behaviour MyApp.PaymentGateway

  @impl true
  def charge(amount, card) do
    # Stripe-specific implementation
    {:ok, %Transaction{}}
  end

  @impl true
  def refund(transaction_id, amount) do
    # Stripe-specific implementation
    {:ok, %Refund{}}
  end

  # health_check/0 not implemented (optional)
end
```

### @impl Annotation

Always use `@impl true` for callback implementations:

```elixir
# Good: explicit implementation
@impl true
def handle_call(:get, _from, state) do
  {:reply, state, state}
end

# Compiler warns if @impl true on non-callback function
@impl true  # Warning: got @impl true for my_helper/1 but...
def my_helper(arg) do
  # This is not a behaviour callback, so @impl causes a warning
  {:processed, arg}
end
```

### Dynamic Dispatch

```elixir
defmodule MyApp.Payments do
  def process(gateway_module, amount, card) do
    gateway_module.charge(amount, card)
  end
end

# Usage
MyApp.Payments.process(MyApp.PaymentGateway.Stripe, amount, card)

# Or from config
gateway = Application.get_env(:my_app, :payment_gateway)
MyApp.Payments.process(gateway, amount, card)
```

## 5. Macros

Macros transform code at compile time. Use sparingly.

### When to Use Macros

**Good use cases:**
- DSLs (router definitions, test frameworks)
- Compile-time optimization
- Code generation from external sources
- Eliminating boilerplate that can't be solved with functions

**Avoid macros when:**
- A function would work
- The abstraction is unclear
- Debugging would be difficult

### Basic Macro Structure

```elixir
defmodule MyApp.Macros do
  defmacro debug(expression) do
    quote do
      value = unquote(expression)
      IO.puts("#{unquote(Macro.to_string(expression))} = #{inspect(value)}")
      value
    end
  end
end

# Usage
require MyApp.Macros
MyApp.Macros.debug(1 + 2)
# Prints: 1 + 2 = 3
# Returns: 3
```

### Quote and Unquote

```elixir
# quote: capture AST
quote do
  1 + 2
end
# => {:+, [], [1, 2]}

# unquote: inject value into AST
x = 5
quote do
  1 + unquote(x)
end
# => {:+, [], [1, 5]}

# unquote_splicing: inject list as arguments
args = [1, 2, 3]
quote do
  sum(unquote_splicing(args))
end
# => {:sum, [], [1, 2, 3]}
```

### Hygiene

Macros are hygienic by default (variables don't leak):

```elixir
defmacro double(x) do
  quote do
    result = unquote(x) * 2  # 'result' won't conflict with caller
    result
  end
end

# Explicitly unhygienic (use with caution)
defmacro assign_x(value) do
  quote do
    var!(x) = unquote(value)  # Assigns to caller's 'x'
  end
end
```

## 6. Module Directives

### alias

Create short names for modules:

```elixir
# Single alias
alias MyApp.Accounts.User

# Multiple from same namespace
alias MyApp.Accounts.{User, Token, Session}

# With custom name
alias MyApp.Accounts.User, as: AccountUser
```

### import

Bring functions into current scope:

```elixir
# Import all
import Ecto.Query

# Import specific functions
import Ecto.Query, only: [from: 2, where: 3]

# Import except
import Ecto.Query, except: [preload: 3]

# Import macros only
import Ecto.Query, only: :macros
```

### require

Load module and make macros available:

```elixir
require Logger
Logger.debug("message")

# Or with alias
require Logger, as: Log
Log.info("message")
```

### use

Invoke __using__/1 macro:

```elixir
# In your module
use GenServer

# Equivalent to
require GenServer
GenServer.__using__([])

# With options
use Phoenix.Controller, namespace: MyAppWeb
```

### Conventions

```elixir
defmodule MyApp.Accounts.UserController do
  # 1. use directives
  use MyAppWeb, :controller

  # 2. alias directives (sorted alphabetically)
  alias MyApp.Accounts
  alias MyApp.Accounts.User

  # 3. import directives
  import MyApp.Helpers

  # 4. require directives
  require Logger

  # Then the rest of the module
end
```

## References

- **OTP patterns**: the `elixir-patterns` skill\'s otp reference
- **Contexts**: the `elixir-patterns` skill\'s contexts reference
- **Algorithms**: the `elixir-patterns` skill\'s algorithms reference
- **Phoenix**: the elixir frameworks/phoenix/liveview.md reference
- **Language overview**: the elixir LANGUAGE.md reference
