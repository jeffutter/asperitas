# Elixir Reference: Production Quality

## Overview

Elixir's production quality workflow centers on mix tasks with strict error handling. The standard toolchain provides compilation checks, formatting, static analysis, and testing in a unified workflow.

## Compilation: mix compile --warnings-as-errors

### What It Catches

- Undefined function calls
- Unused variables and imports
- Pattern matching issues
- Type mismatches (basic)
- Module naming inconsistencies

### Common Warnings and Fixes

```elixir
# Warning: variable "user" is unused
def process(user) do
  :ok
end

# Fix: prefix with underscore
def process(_user) do
  :ok
end

# Warning: function fetch/1 is undefined
def get_data(id) do
  fetch(id)  # Missing function
end

# Fix: define function or correct the call
def get_data(id) do
  Repo.get(User, id)
end
```

### CI Configuration

```yaml
# .github/workflows/ci.yml
- name: Compile with warnings as errors
  run: mix compile --warnings-as-errors
  env:
    MIX_ENV: test
```

## Formatting: mix format

### Default Configuration

```elixir
# .formatter.exs
[
  inputs: ["*.{ex,exs}", "{config,lib,test}/**/*.{ex,exs}"],
  line_length: 98
]
```

### Styler Plugin Integration

```elixir
# .formatter.exs
[
  plugins: [Styler],
  inputs: ["*.{ex,exs}", "{config,lib,test}/**/*.{ex,exs}"],
  line_length: 98
]

# mix.exs
defp deps do
  [
    {:styler, "~> 0.11", only: [:dev, :test], runtime: false}
  ]
end
```

### Styler Improvements

Styler automatically applies these transformations:

```elixir
# Before: Unsorted aliases
alias MyApp.Users
alias MyApp.Accounts
alias MyApp.Products

# After: Sorted aliases
alias MyApp.Accounts
alias MyApp.Products
alias MyApp.Users

# Before: Unnecessary parentheses
if(condition) do
  result
end

# After: Clean if
if condition do
  result
end

# Before: Single-pipe
list |> Enum.map(&transform/1)

# After: Direct call
Enum.map(list, &transform/1)
```

### Check Formatting in CI

```bash
mix format --check-formatted
```

## Static Analysis: mix credo --strict

### Priority Categories

| Category | Description | Example |
|----------|-------------|---------|
| Consistency | Naming, style | Module name doesn't match filename |
| Readability | Complexity, length | Function >50 lines |
| Refactoring | Duplicate code | Same logic in multiple places |
| Warnings | Potential bugs | Unused return value |
| Design | Anti-patterns | God modules |

### Configuration (.credo.exs)

```elixir
%{
  configs: [
    %{
      name: "default",
      files: %{
        included: ["lib/", "test/"],
        excluded: [~r"/_build/", ~r"/deps/"]
      },
      plugins: [],
      requires: [],
      strict: true,
      parse_timeout: 5000,
      color: true,
      checks: %{
        enabled: [
          # Consistency
          {Credo.Check.Consistency.TabsOrSpaces, []},
          {Credo.Check.Consistency.LineEndings, []},

          # Readability
          {Credo.Check.Readability.MaxLineLength, [max_length: 120]},
          {Credo.Check.Readability.FunctionNames, []},
          {Credo.Check.Readability.ModuleDoc, []},

          # Refactoring
          {Credo.Check.Refactor.CyclomaticComplexity, [max_complexity: 10]},
          {Credo.Check.Refactor.Nesting, [max_nesting: 3]},

          # Warnings
          {Credo.Check.Warning.IoInspect, []},
          {Credo.Check.Warning.IExPry, []},

          # Design
          {Credo.Check.Design.AliasUsage, [if_nested_deeper_than: 2]}
        ],
        disabled: [
          {Credo.Check.Readability.Specs, []}  # Use dialyzer instead
        ]
      }
    }
  ]
}
```

### Common Issues and Fixes

```elixir
# Issue: Function too complex (cyclomatic complexity)
def process(order) do
  if order.status == :pending do
    if order.total > 100 do
      if order.user.premium? do
        # ...
      end
    end
  end
end

# Fix: Extract smaller functions with clear names
def process(order) do
  with :ok <- validate_pending(order),
       :ok <- validate_total(order),
       :ok <- apply_premium_benefits(order) do
    complete_order(order)
  end
end

# Issue: IO.inspect left in code
def get_user(id) do
  user = Repo.get(User, id)
  IO.inspect(user, label: "debug")  # Warning!
  user
end

# Fix: Remove or use dbg() which is stripped in prod
def get_user(id) do
  Repo.get(User, id)
end
```

## Type Checking: Dialyzer

### dialyxir Integration

```elixir
# mix.exs
defp deps do
  [
    {:dialyxir, "~> 1.4", only: [:dev, :test], runtime: false}
  ]
end
```

### PLT Management

```bash
# Build PLT (first time, takes several minutes)
mix dialyzer --plt

# Run dialyzer
mix dialyzer

# With explanations
mix dialyzer --format dialyxir
```

### Common Dialyzer Warnings

```elixir
# Warning: Function has no local return
def fail_always do
  raise "This always fails"
end

# Warning: The pattern can never match
def process({:ok, result}) do
  case result do
    {:error, _} ->  # Can never match - result is not error
      :handled
  end
end

# Warning: Invalid type specification
@spec get_user(integer()) :: User.t()
def get_user(id) do
  case Repo.get(User, id) do
    nil -> nil  # Returns nil but spec says User.t()
    user -> user
  end
end

# Fix: Accurate spec
@spec get_user(integer()) :: User.t() | nil
```

### Typespec Best Practices

```elixir
# Define custom types
@type user_id :: pos_integer()
@type email :: String.t()

@type create_attrs :: %{
  required(:name) => String.t(),
  required(:email) => email(),
  optional(:age) => pos_integer()
}

@type create_result :: {:ok, User.t()} | {:error, Ecto.Changeset.t()}

# Use in specs
@spec create_user(create_attrs()) :: create_result()
def create_user(attrs) do
  %User{}
  |> User.changeset(attrs)
  |> Repo.insert()
end
```

## Testing: mix test

### ExUnit Configuration

```elixir
# test/test_helper.exs
ExUnit.start(
  capture_log: true,
  exclude: [:skip, :pending],
  formatters: [ExUnit.CLIFormatter]
)

Ecto.Adapters.SQL.Sandbox.mode(MyApp.Repo, :manual)
```

### Test Organization

```elixir
defmodule MyApp.AccountsTest do
  use MyApp.DataCase, async: true

  alias MyApp.Accounts

  describe "create_user/1" do
    test "creates user with valid attributes" do
      attrs = %{name: "Alice", email: "alice@example.com"}

      assert {:ok, user} = Accounts.create_user(attrs)
      assert user.name == "Alice"
      assert user.email == "alice@example.com"
    end

    test "returns error with invalid email" do
      attrs = %{name: "Alice", email: "invalid"}

      assert {:error, changeset} = Accounts.create_user(attrs)
      assert %{email: ["has invalid format"]} = errors_on(changeset)
    end

    @tag :skip
    test "pending test" do
      # Not yet implemented
    end
  end
end
```

### Coverage with excoveralls

```elixir
# mix.exs
defp deps do
  [
    {:excoveralls, "~> 0.18", only: :test}
  ]
end

def project do
  [
    test_coverage: [tool: ExCoveralls],
    preferred_cli_env: [
      coveralls: :test,
      "coveralls.detail": :test,
      "coveralls.html": :test
    ]
  ]
end
```

```bash
# Run with coverage
mix coveralls

# Generate HTML report
mix coveralls.html
```

### Mox for Behaviour Mocking

```elixir
# Define behaviour
defmodule MyApp.Mailer do
  @callback send_email(String.t(), String.t(), String.t()) :: {:ok, term()} | {:error, term()}
end

# Create mock in test_helper.exs
Mox.defmock(MyApp.MailerMock, for: MyApp.Mailer)

# Use in test
defmodule MyApp.NotificationTest do
  use MyApp.DataCase
  import Mox

  setup :verify_on_exit!

  test "sends welcome email on registration" do
    expect(MyApp.MailerMock, :send_email, fn to, subject, body ->
      assert to == "alice@example.com"
      assert subject == "Welcome!"
      {:ok, %{id: "email-123"}}
    end)

    assert {:ok, _user} = Accounts.register(%{email: "alice@example.com"})
  end
end
```

## Precommit Workflow Script

```bash
#!/usr/bin/env bash
set -euo pipefail

echo "Running precommit checks..."

echo "1. Compiling with warnings as errors..."
mix compile --warnings-as-errors

echo "2. Checking formatting..."
mix format --check-formatted

echo "3. Running Credo..."
mix credo --strict

echo "4. Running tests..."
mix test

echo "All checks passed!"
```

## CI Configuration (GitHub Actions)

```yaml
# .github/workflows/ci.yml
name: CI

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

jobs:
  test:
    runs-on: ubuntu-latest

    services:
      postgres:
        image: postgres:15
        env:
          POSTGRES_PASSWORD: postgres
        options: >-
          --health-cmd pg_isready
          --health-interval 10s
          --health-timeout 5s
          --health-retries 5
        ports:
          - 5432:5432

    env:
      MIX_ENV: test
      DATABASE_URL: postgres://postgres:postgres@localhost/myapp_test

    steps:
      - uses: actions/checkout@v4

      - name: Set up Elixir
        uses: erlef/setup-beam@v1
        with:
          elixir-version: '1.16'
          otp-version: '26'

      - name: Restore dependencies cache
        uses: actions/cache@v3
        with:
          path: deps
          key: ${{ runner.os }}-mix-${{ hashFiles('**/mix.lock') }}
          restore-keys: ${{ runner.os }}-mix-

      - name: Install dependencies
        run: mix deps.get

      - name: Compile with warnings as errors
        run: mix compile --warnings-as-errors

      - name: Check formatting
        run: mix format --check-formatted

      - name: Run Credo
        run: mix credo --strict

      - name: Setup database
        run: mix ecto.setup

      - name: Run tests
        run: mix test

      - name: Run Dialyzer
        run: mix dialyzer
```

## Tool Summary

| Tool | Purpose | Command | Notes |
|------|---------|---------|-------|
| mix compile | Type/reference checking | `--warnings-as-errors` | Catches undefined functions |
| mix format | Code formatting | `--check-formatted` | Use with Styler plugin |
| mix credo | Static analysis | `--strict` | Style and consistency |
| mix dialyzer | Type checking | | PLT required first |
| mix test | Testing | | ExUnit framework |
| mix coveralls | Coverage | `.html` for report | excoveralls package |
