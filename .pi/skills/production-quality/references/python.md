# Python Production Quality Reference

Language-specific guidance for Python projects. Supplements `SKILL.md`.

## Toolchain

| Step | Tool | Command |
|------|------|---------|
| Type check | mypy | `mypy src/ tests/` |
| Format | ruff | `ruff format src/ tests/` |
| Lint | ruff | `ruff check src/ tests/` |
| Test | pytest | `uv run pytest` |

Run in that order. Lint before tests; a lint failure doesn't tell you whether tests pass.

## Type Annotations

- Use `X | Y` union syntax (PEP 604), not `Union[X, Y]` — requires Python 3.10+ or `from __future__ import annotations`.
- Use `from __future__ import annotations` at the top of every file to enable PEP 604 unions on Python 3.9 and to defer annotation evaluation.
- Prefer `typing.Protocol` over `abc.ABC` for structural typing of third-party or optional dependencies.
- Never use `Any` in public interfaces. Reserve it for suppressing noise from genuinely untyped third-party code (via mypy overrides in `pyproject.toml`, not inline `# type: ignore`).

## Dataclasses and Immutability

### Use `@dataclass(frozen=True)` for value objects

```python
# Wrong: looks frozen, is not — object.__setattr__ in __init__ bypasses
# nothing because there is no __setattr__ guard installed
class NoteEvent:
    __slots__ = ("pitch", "confidence")
    pitch: int
    confidence: float
    def __init__(self, pitch: int, velocity: int) -> None:
        object.__setattr__(self, "pitch", pitch)
        object.__setattr__(self, "confidence", velocity / 127.0)

# Right: frozen=True installs __setattr__ / __delattr__ guards
from dataclasses import dataclass, field

@dataclass(frozen=True)
class NoteEvent:
    pitch: int
    velocity: int
    confidence: float = field(init=False)

    def __post_init__(self) -> None:
        object.__setattr__(self, "confidence", round(self.velocity / 127.0, 4))
```

### Never accept derived values as constructor arguments

If a field is fully determined by other fields, exclude it from `__init__` (`field(init=False)`) and compute it in `__post_init__`. Accepting it as a parameter lets callers construct objects where `confidence != velocity / 127.0`, breaking invariants silently.

## Module-level Singletons

Any module-level object written lazily after first access is a race condition without a lock.

```python
# Wrong: two threads can both observe _cache is None and both load
_cache: Model | None = None

def get_model() -> Model:
    global _cache
    if _cache is None:
        _cache = load_expensive_model()
    return _cache

# Right: double-checked locking
import threading
_cache: Model | None = None
_lock = threading.Lock()

def get_model() -> Model:
    global _cache
    if _cache is not None:
        return _cache
    with _lock:
        result: Model | None = _cache   # re-read under lock
        if result is None:
            result = load_expensive_model()
            _cache = result
        return result
```

Use a local variable (`result`) inside the lock to avoid mypy type-narrowing conflicts with the global.

## Shared Resources in a Callgraph

When two public functions both need an expensive resource, extract a private helper that accepts the already-loaded resource. Never let a high-level function call a lower-level function that reloads the resource.

```python
# Wrong: split_audio calls detect_segments which loads the file,
# then split_audio loads it again
def detect_segments(path: Path) -> list[Segment]:
    audio = AudioSegment.from_file(path)   # load #1
    return _find(audio)

def split_audio(path: Path) -> list[Path]:
    segments = detect_segments(path)       # triggers load #1
    audio = AudioSegment.from_file(path)   # load #2 — unnecessary
    ...

# Right: private helper operates on already-loaded data
def _find_segments(audio: AudioSegment) -> list[Segment]:
    ...

def detect_segments(path: Path) -> list[Segment]:
    audio = AudioSegment.from_file(path)
    return _find_segments(audio)

def split_audio(path: Path) -> list[Path]:
    audio = AudioSegment.from_file(path)
    segments = _find_segments(audio)       # no double load
    ...
```

## CLI Design

### Batch output must identify its source

When a command processes multiple files and emits rows to stdout, always include a `file` column as the first field. Without it, rows from different sources are indistinguishable.

```python
# Wrong
print("start_s\tend_s\tpitch")
for path, events in results.items():
    for ev in events:
        print(f"{ev.start_s}\t{ev.end_s}\t{ev.pitch}")

# Right
print("file\tstart_s\tend_s\tpitch")
for path, events in results.items():
    for ev in events:
        print(f"{path}\t{ev.start_s}\t{ev.end_s}\t{ev.pitch}")
```

### Use `sys.stderr` for diagnostics, `sys.stdout` for data

Counts, warnings, and progress messages go to stderr. Tab-separated data rows go to stdout. This lets callers pipe stdout without contamination.

## Error Handling

- Catch specific exceptions at boundaries (user input, external I/O). Don't catch `Exception` unless you log the error and return a typed fallback — never swallow silently.
- Wrap calls to untyped external libraries in a `try/except` with a logged warning and typed empty return, rather than letting arbitrary exceptions propagate upward.
- For file-not-found at a public boundary, prefer `FileNotFoundError` (raised explicitly after `path.exists()`) over letting the OS raise an obscure `OSError`.

## Ruff Configuration

- Enable `ALL` rules and selectively ignore with reasons in `pyproject.toml` `[tool.ruff.lint] ignore`.
- Suppress rules per-file via `[tool.ruff.lint.per-file-ignores]`, not inline `# noqa` sprinkled through the source.
- `EM101`/`EM102`: assign exception messages to a variable before raising (`msg = "..."; raise RuntimeError(msg)`).
- `S101`: avoid `assert` in production code — use explicit `if ... raise` instead. Asserts are stripped with `-O`.
