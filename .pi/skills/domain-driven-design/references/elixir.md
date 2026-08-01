# Elixir Domain-Driven Design

Phoenix contexts provide Elixir's implementation of Domain-Driven Design, organizing code by business domain rather than technical layers.

## Phoenix Context Structure

```
lib/my_app/
  accounts/           # Private implementation
    user.ex           # Schema
    user_token.ex     # Schema
    queries.ex        # Query helpers (private)
  accounts.ex         # Public API - The Context

  products/
    product.ex
    category.ex
  products.ex         # Public API
```

### The Context Module

The context module (`accounts.ex`) is the public API:

```elixir
defmodule MyApp.Accounts do
  @moduledoc """
  The Accounts context handles user management and authentication.
  """

  import Ecto.Query
  alias MyApp.Repo
  alias MyApp.Accounts.{User, UserToken}

  # Public API - Business Operations

  @doc """
  Registers a new user with email and password.

  Returns {:ok, user} on success, {:error, changeset} on validation failure.
  """
  def register_user(attrs) do
    %User{}
    |> User.registration_changeset(attrs)
    |> Repo.insert()
  end

  @doc """
  Authenticates a user by email and password.

  Returns {:ok, user} if valid, {:error, :unauthorized} otherwise.
  """
  def authenticate_user(email, password) do
    user = Repo.get_by(User, email: email)

    cond do
      user && valid_password?(user, password) ->
        {:ok, user}

      user ->
        # Prevent timing attacks
        Argon2.no_user_verify()
        {:error, :unauthorized}

      true ->
        Argon2.no_user_verify()
        {:error, :unauthorized}
    end
  end

  @doc """
  Gets a user by ID.

  Raises if not found.
  """
  def get_user!(id), do: Repo.get!(User, id)

  @doc """
  Gets a user by ID.

  Returns {:ok, user} or {:error, :not_found}.
  """
  def fetch_user(id) do
    case Repo.get(User, id) do
      nil -> {:error, :not_found}
      user -> {:ok, user}
    end
  end

  # Private Functions - Implementation Details

  defp valid_password?(%User{hashed_password: hashed}, password) do
    Argon2.verify_pass(password, hashed)
  end
end
```

### Key Patterns

**1. Business operations, not CRUD:**

```elixir
# Good - expresses business intent
def register_user(attrs)
def verify_email(token)
def reset_password(user, new_password)
def deactivate_account(user)

# Avoid - generic CRUD
def create_user(attrs)
def update_user(user, attrs)
def delete_user(user)
```

**2. Separate changesets for different operations:**

```elixir
defmodule MyApp.Accounts.User do
  use Ecto.Schema
  import Ecto.Changeset

  # Different changesets for different use cases
  def registration_changeset(user, attrs) do
    user
    |> cast(attrs, [:email, :password])
    |> validate_required([:email, :password])
    |> validate_email()
    |> validate_password()
    |> hash_password()
  end

  def profile_changeset(user, attrs) do
    user
    |> cast(attrs, [:name, :bio, :avatar_url])
    |> validate_length(:name, max: 100)
    |> validate_length(:bio, max: 500)
  end

  def password_changeset(user, attrs) do
    user
    |> cast(attrs, [:password])
    |> validate_required([:password])
    |> validate_password()
    |> hash_password()
  end
end
```

**3. Return tagged tuples:**

```elixir
# Always return {:ok, value} or {:error, reason}
def fetch_user(id) do
  case Repo.get(User, id) do
    nil -> {:error, :not_found}
    user -> {:ok, user}
  end
end

# Use ! suffix for functions that raise
def get_user!(id), do: Repo.get!(User, id)
```

## Context Boundaries

### Never Cross at Database Level

```elixir
# BAD - Direct cross-context query
defmodule MyApp.Orders do
  def list_user_orders(user_id) do
    from(o in Order,
      join: u in User, on: o.user_id == u.id,
      where: u.id == ^user_id,
      select: o
    )
    |> Repo.all()
  end
end

# GOOD - Use context public APIs
defmodule MyApp.Orders do
  alias MyApp.Accounts

  def list_user_orders(user_id) do
    with {:ok, _user} <- Accounts.fetch_user(user_id) do
      Order
      |> where(user_id: ^user_id)
      |> Repo.all()
      |> then(&{:ok, &1})
    end
  end
end
```

### Cross-Context Communication

```elixir
defmodule MyApp.Orders do
  alias MyApp.Accounts
  alias MyApp.Products

  def create_order(user_id, product_id, attrs) do
    with {:ok, user} <- Accounts.fetch_user(user_id),
         {:ok, product} <- Products.fetch_product(product_id),
         {:ok, order} <- do_create_order(user, product, attrs) do
      {:ok, order}
    end
  end

  defp do_create_order(user, product, attrs) do
    %Order{}
    |> Order.changeset(attrs)
    |> Ecto.Changeset.put_assoc(:user_id, user.id)
    |> Ecto.Changeset.put_assoc(:product_id, product.id)
    |> Repo.insert()
  end
end
```

## When to Create New Context

**Create new context when:**
- Clear business domain boundary
- Different stakeholders/teams
- Independent lifecycle
- Distinct set of related operations

**Keep in same context when:**
- Tightly coupled data
- Share many operations
- Same business rules
- Always used together

### Splitting a Growing Context

```elixir
# Before: Everything in Accounts
defmodule MyApp.Accounts do
  def register_user(attrs)
  def authenticate_user(email, password)
  def create_api_key(user)
  def revoke_api_key(key)
  def create_team(user, attrs)
  def add_team_member(team, user)
  # ... growing endlessly
end

# After: Split by capability
defmodule MyApp.Accounts do
  # User registration and authentication
  def register_user(attrs)
  def authenticate_user(email, password)
end

defmodule MyApp.ApiKeys do
  # API key management
  def create_key(user)
  def revoke_key(key)
  def verify_key(key)
end

defmodule MyApp.Teams do
  # Team management
  def create_team(owner, attrs)
  def add_member(team, user)
  def remove_member(team, user)
end
```

## Domain Events with PubSub

Decouple contexts with events:

```elixir
# In Accounts context
defmodule MyApp.Accounts do
  def register_user(attrs) do
    case do_register_user(attrs) do
      {:ok, user} ->
        # Publish event for other contexts
        Phoenix.PubSub.broadcast(
          MyApp.PubSub,
          "accounts:events",
          {:user_registered, user}
        )
        {:ok, user}

      error ->
        error
    end
  end
end

# In Notifications context - subscriber
defmodule MyApp.Notifications.EventHandler do
  use GenServer

  def start_link(_) do
    GenServer.start_link(__MODULE__, nil, name: __MODULE__)
  end

  def init(_) do
    Phoenix.PubSub.subscribe(MyApp.PubSub, "accounts:events")
    {:ok, nil}
  end

  def handle_info({:user_registered, user}, state) do
    MyApp.Notifications.send_welcome_email(user)
    {:noreply, state}
  end
end
```

## Anti-Patterns

### 1. Anemic Context

```elixir
# BAD - just CRUD, no business logic
defmodule MyApp.Users do
  def create(attrs), do: %User{} |> User.changeset(attrs) |> Repo.insert()
  def update(user, attrs), do: user |> User.changeset(attrs) |> Repo.update()
  def delete(user), do: Repo.delete(user)
  def get(id), do: Repo.get(User, id)
  def list, do: Repo.all(User)
end

# GOOD - business operations
defmodule MyApp.Accounts do
  def register_user(attrs)       # Includes validation, password hashing
  def verify_email(token)        # Includes token validation, status update
  def reset_password(user, new)  # Includes validation, old password clearing
  def deactivate_account(user)   # Includes cascading updates
end
```

### 2. God Context

```elixir
# BAD - everything in one giant context
defmodule MyApp.Core do
  # Users
  def create_user(attrs)
  def update_user(user, attrs)
  # Products
  def create_product(attrs)
  def update_inventory(product, quantity)
  # Orders
  def create_order(user, products)
  def cancel_order(order)
  # Payments
  def process_payment(order)
  # Notifications
  def send_email(user, template)
  # ... 5000+ lines
end
```

**Fix:** Split into focused contexts.

### 3. Technical Layering

```
# BAD
lib/my_app/
  schemas/
    user.ex
    product.ex
    order.ex
  queries/
    user_queries.ex
    product_queries.ex
  services/
    user_service.ex
    order_service.ex

# GOOD
lib/my_app/
  accounts/
    user.ex
  accounts.ex
  products/
    product.ex
  products.ex
  orders/
    order.ex
  orders.ex
```

## Testing Contexts

```elixir
defmodule MyApp.AccountsTest do
  use MyApp.DataCase, async: true

  alias MyApp.Accounts

  describe "register_user/1" do
    test "with valid data creates a user" do
      attrs = %{email: "test@example.com", password: "validpassword123"}

      assert {:ok, user} = Accounts.register_user(attrs)
      assert user.email == "test@example.com"
      assert user.hashed_password != "validpassword123"
    end

    test "with invalid email returns error changeset" do
      attrs = %{email: "invalid", password: "validpassword123"}

      assert {:error, changeset} = Accounts.register_user(attrs)
      assert "is invalid" in errors_on(changeset).email
    end

    test "with duplicate email returns error changeset" do
      attrs = %{email: "test@example.com", password: "validpassword123"}
      {:ok, _} = Accounts.register_user(attrs)

      assert {:error, changeset} = Accounts.register_user(attrs)
      assert "has already been taken" in errors_on(changeset).email
    end
  end

  describe "authenticate_user/2" do
    test "with valid credentials returns user" do
      {:ok, user} = Accounts.register_user(%{
        email: "test@example.com",
        password: "validpassword123"
      })

      assert {:ok, ^user} = Accounts.authenticate_user(
        "test@example.com",
        "validpassword123"
      )
    end

    test "with invalid password returns error" do
      Accounts.register_user(%{
        email: "test@example.com",
        password: "validpassword123"
      })

      assert {:error, :unauthorized} = Accounts.authenticate_user(
        "test@example.com",
        "wrongpassword"
      )
    end
  end
end
```

## Best Practices

### 1. Context as the API Boundary

```elixir
# Controllers only call context functions
defmodule MyAppWeb.UserController do
  def create(conn, params) do
    case Accounts.register_user(params) do
      {:ok, user} -> render(conn, :show, user: user)
      {:error, changeset} -> render(conn, :error, changeset: changeset)
    end
  end
end
```

### 2. Keep Schemas Private

```elixir
# Don't expose schemas directly
# BAD
alias MyApp.Accounts.User
user = %User{}  # Direct schema access

# GOOD - go through context
user = Accounts.get_user!(id)
```

### 3. Document the Public API

```elixir
defmodule MyApp.Accounts do
  @moduledoc """
  The Accounts context manages user registration, authentication,
  and profile management.

  ## Public Functions

  * `register_user/1` - Create new user account
  * `authenticate_user/2` - Verify credentials
  * `get_user!/1` - Fetch user by ID (raises)
  * `fetch_user/1` - Fetch user by ID (returns result tuple)
  * `update_profile/2` - Update user profile
  """
end
```
