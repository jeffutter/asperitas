---
name: language-detection
description: Canonical language detection logic and reference paths. Load this skill when you need to detect project language or find language-specific resources.
---

# Language Detection

This skill provides canonical language detection logic for the plugin. **All agents and skills should reference this skill** instead of implementing their own detection.

## Detection Logic

Detect the project's primary language by checking for configuration files in priority order:

| Priority | File | Language |
|----------|------|----------|
| 1 | `mix.exs` | Elixir |
| 2 | `Cargo.toml` | Rust |
| 3 | `pyproject.toml` or `setup.py` | Python |
| 4 | `package.json` | TypeScript/JavaScript |
| 5 | `go.mod` | Go |
| 6 | (none found) | Unknown |

**Detection command** (also available as ):

```bash
if [[ -f "mix.exs" ]]; then LANG="elixir"
elif [[ -f "Cargo.toml" ]]; then LANG="rust"
elif [[ -f "pyproject.toml" ]] || [[ -f "setup.py" ]]; then LANG="python"
elif [[ -f "package.json" ]]; then LANG="typescript"
elif [[ -f "go.mod" ]]; then LANG="go"
else LANG="unknown"
fi
```

## Language Resources

Once detected, load the appropriate language-specific resources.

### Supported Languages

| Language | Has Full Support | Package Registry |
|----------|------------------|------------------|
| Elixir | Yes | hex.pm |
| Rust | Yes | crates.io |
| Python | Partial | pypi.org |
| TypeScript | Partial | npmjs.com |
| Go | Partial | pkg.go.dev |

### Resource Paths

**Core language resources** (for fully supported languages):

| Resource | Path |
|----------|------|
| Language overview |  |
| Patterns | the language-specific patterns skill (`elixir-patterns` or `rust-patterns`) |
| Tooling | the language tooling guide (see `language-detection` skill) |
| Testing | the language testing guide (see `language-detection` skill) |
| Project setup | the language project-setup guide (see `language-detection` skill) |

**Skill-specific language references**:

| Skill | Path |
|-------|------|
| Error handling |  |
| Concurrency |  |
| Data validation |  |
| DDD |  |
| Production quality |  |
| Performance |  |
| Algorithms |  |

### Quick Reference by Language

#### Elixir

```
Core:
  the elixir LANGUAGE.md reference
  the `elixir-patterns` skill
  the elixir tooling.md reference
  the elixir testing.md reference

Pattern references:
  elixir algorithm references (otp)
  elixir algorithm references (contexts)
  elixir algorithm references (algorithms)

Frameworks:
  the elixir frameworks/phoenix/liveview.md reference
```

#### Rust

```
Core:
  the rust LANGUAGE.md reference
  the `rust-patterns` skill
  the rust tooling.md reference
  the rust testing.md reference

Pattern references:
  rust algorithm references (error-handling)
  rust algorithm references (concurrency)
  rust algorithm references (algorithms)
```

## Tooling by Language

### Build & Run

| Language | Build | Run | REPL |
|----------|-------|-----|------|
| Elixir | `mix compile` | `mix run` | `iex -S mix` |
| Rust | `cargo build` | `cargo run` | - |
| Python | - | `python main.py` | `python` |
| TypeScript | `tsc` | `node dist/main.js` | `npx ts-node` |
| Go | `go build` | `go run .` | - |

### Test

| Language | Unit Tests | Watch Mode |
|----------|------------|------------|
| Elixir | `mix test` | `mix test --stale --listen-on-stdin` |
| Rust | `cargo test` | `cargo watch -x test` |
| Python | `pytest` | `pytest-watch` |
| TypeScript | `npm test` | `npm test -- --watch` |
| Go | `go test ./...` | `gotestsum --watch` |

### Lint & Format

| Language | Format | Lint |
|----------|--------|------|
| Elixir | `mix format` | `mix credo` |
| Rust | `cargo fmt` | `cargo clippy` |
| Python | `ruff format` | `ruff check` |
| TypeScript | `prettier --write .` | `eslint .` |
| Go | `gofmt -w .` | `golangci-lint run` |

**Note:** For Python, `ruff` handles both formatting (`ruff format`) and linting (`ruff check`). Do not use `black` — this project uses `ruff` as the single Python formatter/linter.

### Quality Gate (All-in-One)

| Language | Command |
|----------|---------|
| Elixir | `mix format --check-formatted && mix credo --strict && mix dialyzer && mix test` |
| Rust | `cargo fmt --check && cargo clippy -- -D warnings && cargo test` |
| Python | `ruff format --check && ruff check && mypy src/ && pytest` |
| TypeScript | `prettier --check . && eslint . && tsc --noEmit && npm test` |
| Go | `gofmt -l . && golangci-lint run && go test ./...` |

## Usage Pattern

When an agent or skill needs language awareness:

```markdown
## Language Detection

**Before [action]**, detect the project language:

1. Load language detection: the `language-detection` skill
2. Detect language using the detection logic above
3. Load appropriate language-specific resources

**For language-specific [topic]**, see:
- Elixir: the elixir ... reference
- Rust: the rust ... reference
- Other: Provide general guidance; note when language-specific resources are unavailable
```

## Adding New Language Support

To add support for a new language:

1. **Update detection logic** in this file and `hooks/scripts/detect-language.sh`
2. **Create language directory**: `languages/{lang}/`
3. **Create core files**:
   - `LANGUAGE.md` - Philosophy, idioms, conventions
   - `patterns/SKILL.md` - Language-specific patterns
   - `tooling.md` - Build tools, package manager, linters
   - `testing.md` - Test frameworks, patterns
   - `project-setup.md` - New project configuration
4. **Add skill references**: Create `{lang}.md` in each skill's `references/` directory
5. **Update this skill**: Add to quick reference and tooling tables

## Handling Unknown Languages

When the detected language is "unknown" or unsupported:

1. **Provide general guidance** based on universal programming principles
2. **Note the limitation**: "Language-specific guidance unavailable for this project"
3. **Suggest detection override** if the user knows the language:
   - "If this is a [Language] project, please confirm and I'll load the appropriate resources"
4. **Fall back to universal skills** which work across languages:
   - Error handling patterns (railway-oriented, Result types)
   - DDD principles (bounded contexts, aggregates)
   - Cognitive complexity guidelines
