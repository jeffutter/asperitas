# Elixir Data Validation

Elixir's primary validation tool is Ecto.Changeset, which provides a pipeline-based approach to validating and transforming data.

## Ecto.Changeset Fundamentals

Changesets track changes to data and accumulate validation errors:

```elixir
defmodule MyApp.Accounts.User do
  use Ecto.Schema
  import Ecto.Changeset

  schema "users" do
    field :name, :string
    field :email, :string
    field :age, :integer
    field :role, Ecto.Enum, values: [:user, :admin, :moderator]
    timestamps()
  end

  @required_fields [:name, :email]
  @optional_fields [:age, :role]

  def changeset(user, attrs) do
    user
    |> cast(attrs, @required_fields ++ @optional_fields)
    |> validate_required(@required_fields)
    |> validate_format(:email, ~r/^[^\s]+@[^\s]+$/, message: "must be a valid email")
    |> validate_length(:name, min: 2, max: 100)
    |> validate_number(:age, greater_than: 0, less_than: 150)
    |> validate_inclusion(:role, [:user, :admin, :moderator])
    |> unique_constraint(:email)
  end
end
```

### Core Functions

| Function | Purpose |
|----------|---------|
| `cast/3` | Filter and cast input attributes |
| `validate_required/2` | Ensure fields are present |
| `validate_format/3` | Match against regex |
| `validate_length/3` | String/list length bounds |
| `validate_number/3` | Numeric constraints |
| `validate_inclusion/3` | Value in allowed list |
| `validate_exclusion/3` | Value not in blocked list |
| `unique_constraint/2` | Database uniqueness |

### Cast: The Entry Point

`cast/3` filters and type-casts external input:

```elixir
def changeset(user, attrs) do
  user
  |> cast(attrs, [:name, :email, :age])  # Only these fields accepted
  # Ignores unknown fields like :admin, :id, etc.
  # Type-casts: "25" -> 25 for :age (integer field)
end
```

**Security:** cast acts as an allowlist - fields not listed are ignored, preventing mass assignment vulnerabilities.

## Built-in Validations

### Required Fields

```elixir
|> validate_required([:email, :password])
|> validate_required([:name], message: "can't be blank")
```

### Format Validation

```elixir
# Email format
|> validate_format(:email, ~r/^[^\s]+@[^\s]+\.[^\s]+$/)

# Phone number
|> validate_format(:phone, ~r/^\+?[1-9]\d{1,14}$/, message: "invalid phone format")

# URL
|> validate_format(:website, ~r/^https?:\/\//, message: "must start with http:// or https://")
```

### Length Validation

```elixir
# String length
|> validate_length(:name, min: 2, max: 100)
|> validate_length(:bio, max: 500)

# Exact length
|> validate_length(:postal_code, is: 5)

# List length
|> validate_length(:tags, max: 10, message: "cannot have more than 10 tags")
```

### Number Validation

```elixir
|> validate_number(:age, greater_than: 0, less_than: 150)
|> validate_number(:price, greater_than_or_equal_to: 0)
|> validate_number(:quantity, greater_than: 0, message: "must be positive")
```

### Inclusion/Exclusion

```elixir
|> validate_inclusion(:status, ["pending", "active", "closed"])
|> validate_exclusion(:username, ["admin", "root", "system"])
```

## Custom Validations

### validate_change/3

For single-field custom validation:

```elixir
def changeset(user, attrs) do
  user
  |> cast(attrs, [:password])
  |> validate_change(:password, &validate_password_strength/2)
end

defp validate_password_strength(:password, password) do
  cond do
    String.length(password) < 8 ->
      [password: "must be at least 8 characters"]
    not String.match?(password, ~r/[A-Z]/) ->
      [password: "must contain an uppercase letter"]
    not String.match?(password, ~r/[0-9]/) ->
      [password: "must contain a number"]
    true ->
      []  # No errors
  end
end
```

### Custom Validation Function

For complex validations:

```elixir
def changeset(user, attrs) do
  user
  |> cast(attrs, [:start_date, :end_date])
  |> validate_date_range()
end

defp validate_date_range(changeset) do
  start_date = get_field(changeset, :start_date)
  end_date = get_field(changeset, :end_date)

  if start_date && end_date && Date.compare(start_date, end_date) == :gt do
    add_error(changeset, :end_date, "must be after start date")
  else
    changeset
  end
end
```

### Conditional Validation

```elixir
def changeset(user, attrs) do
  user
  |> cast(attrs, [:role, :department])
  |> validate_required([:role])
  |> maybe_require_department()
end

defp maybe_require_department(changeset) do
  if get_field(changeset, :role) == :employee do
    validate_required(changeset, [:department])
  else
    changeset
  end
end
```

## Database Constraints

Constraints are validated at the database level:

```elixir
def changeset(user, attrs) do
  user
  |> cast(attrs, [:email])
  |> unique_constraint(:email)  # Requires unique index on email
  |> foreign_key_constraint(:team_id)  # Requires FK constraint
  |> check_constraint(:age, name: :age_must_be_positive)  # Requires CHECK constraint
end
```

**Important:** Constraints only trigger on `Repo.insert/update`. They catch errors that slip past application validation.

### Constraint vs Validation

| Aspect | Validation | Constraint |
|--------|------------|------------|
| When checked | Before database call | During database call |
| Error format | Changeset error | Must be mapped to changeset |
| Race conditions | Possible | Prevented by database |
| Use for | Format, business rules | Uniqueness, foreign keys |

## Action-Specific Changesets

Different operations need different validations:

```elixir
defmodule MyApp.Accounts.User do
  # Registration - strict validation
  def registration_changeset(user, attrs) do
    user
    |> cast(attrs, [:email, :password, :name])
    |> validate_required([:email, :password, :name])
    |> validate_email()
    |> validate_password()
    |> hash_password()
  end

  # Profile update - no password required
  def profile_changeset(user, attrs) do
    user
    |> cast(attrs, [:name, :bio, :avatar_url])
    |> validate_length(:name, min: 2, max: 100)
    |> validate_length(:bio, max: 500)
  end

  # Password change - requires current password
  def password_changeset(user, attrs) do
    user
    |> cast(attrs, [:password, :current_password])
    |> validate_required([:password, :current_password])
    |> validate_current_password()
    |> validate_password()
    |> hash_password()
  end

  # Admin update - can change role
  def admin_changeset(user, attrs) do
    user
    |> cast(attrs, [:role, :active])
    |> validate_inclusion(:role, [:user, :moderator, :admin])
  end
end
```

## Embedded Schemas

For validating non-database data:

```elixir
defmodule MyApp.ContactForm do
  use Ecto.Schema
  import Ecto.Changeset

  # No database table
  embedded_schema do
    field :name, :string
    field :email, :string
    field :message, :string
    field :category, :string
  end

  def changeset(form, attrs) do
    form
    |> cast(attrs, [:name, :email, :message, :category])
    |> validate_required([:name, :email, :message])
    |> validate_format(:email, ~r/@/)
    |> validate_length(:message, min: 10, max: 1000)
    |> validate_inclusion(:category, ["support", "sales", "feedback"])
  end
end

# Usage
def handle_contact(params) do
  changeset = ContactForm.changeset(%ContactForm{}, params)

  if changeset.valid? do
    data = apply_changes(changeset)
    send_contact_email(data)
    {:ok, data}
  else
    {:error, changeset}
  end
end
```

## Nested Validation

### Embedded Associations

```elixir
defmodule MyApp.Order do
  use Ecto.Schema
  import Ecto.Changeset

  schema "orders" do
    field :status, :string
    embeds_many :items, OrderItem, on_replace: :delete
  end

  def changeset(order, attrs) do
    order
    |> cast(attrs, [:status])
    |> cast_embed(:items, required: true)
    |> validate_items_not_empty()
  end

  defp validate_items_not_empty(changeset) do
    items = get_field(changeset, :items) || []
    if Enum.empty?(items) do
      add_error(changeset, :items, "must have at least one item")
    else
      changeset
    end
  end
end

defmodule MyApp.OrderItem do
  use Ecto.Schema
  import Ecto.Changeset

  embedded_schema do
    field :product_id, :integer
    field :quantity, :integer
    field :price, :decimal
  end

  def changeset(item, attrs) do
    item
    |> cast(attrs, [:product_id, :quantity, :price])
    |> validate_required([:product_id, :quantity, :price])
    |> validate_number(:quantity, greater_than: 0)
    |> validate_number(:price, greater_than_or_equal_to: 0)
  end
end
```

## Error Handling

### Extracting Errors

```elixir
def format_changeset_errors(changeset) do
  Ecto.Changeset.traverse_errors(changeset, fn {msg, opts} ->
    Enum.reduce(opts, msg, fn {key, value}, acc ->
      String.replace(acc, "%{#{key}}", to_string(value))
    end)
  end)
end

# Usage
case Accounts.create_user(params) do
  {:ok, user} ->
    {:ok, user}

  {:error, changeset} ->
    errors = format_changeset_errors(changeset)
    # %{email: ["has already been taken"], name: ["can't be blank"]}
    {:error, errors}
end
```

### Error Messages

```elixir
|> validate_required(:email, message: "Email is required")
|> validate_format(:email, ~r/@/, message: "Must be a valid email address")
|> validate_length(:password, min: 8, message: "Must be at least %{count} characters")
```

## Best Practices

### 1. Cast Before Validate

```elixir
# Always cast first
def changeset(user, attrs) do
  user
  |> cast(attrs, [:email, :name])  # First
  |> validate_required([:email])   # Then validate
  |> validate_format(:email, ~r/@/)
end
```

### 2. Use Specific Changesets

```elixir
# Good - action-specific
User.registration_changeset(user, params)
User.profile_changeset(user, params)

# Avoid - one changeset for everything
User.changeset(user, params)
```

### 3. Keep Changesets Pure

```elixir
# Good - just data transformation
def changeset(user, attrs) do
  user
  |> cast(attrs, [:email])
  |> validate_required([:email])
end

# Avoid - side effects in changeset
def changeset(user, attrs) do
  Logger.info("Creating user")  # Side effect
  send_email(attrs[:email])     # Side effect
  user
  |> cast(attrs, [:email])
end
```

### 4. Validate at the Boundary

```elixir
# Controller - the boundary
def create(conn, params) do
  case Accounts.create_user(params) do
    {:ok, user} -> render(conn, :show, user: user)
    {:error, changeset} -> render(conn, :error, changeset: changeset)
  end
end

# Context - uses changeset
def create_user(attrs) do
  %User{}
  |> User.registration_changeset(attrs)
  |> Repo.insert()
end
```

### 5. Database Constraints as Safety Net

```elixir
# Application validation catches most errors
|> validate_format(:email, ~r/@/)
|> validate_required([:email])

# Database constraint catches race conditions
|> unique_constraint(:email)  # In case two requests pass validation simultaneously
```
