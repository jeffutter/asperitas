---
name: concurrency
description: Use when implementing concurrent operations, choosing between concurrency models, designing async workflows, or working with parallel data processing
---

# Concurrency Patterns

Production patterns for concurrent and parallel code that is correct, performant, and maintainable.

## When to Use

- Designing concurrent systems
- Choosing between async/await, threads, or actors
- Implementing parallel data processing
- Managing shared state safely
- Building responsive applications
- Handling multiple I/O operations efficiently

## Core Concepts

### Concurrency vs Parallelism

| Concept | Definition | Example |
|---------|------------|---------|
| Concurrency | Managing multiple tasks, not necessarily simultaneous | Single-core handling HTTP requests |
| Parallelism | Executing multiple tasks simultaneously | Multi-core processing data |

Concurrency is about structure; parallelism is about execution.

### Concurrency Models Comparison

| Model | Best For | Complexity | Overhead | Shared State |
|-------|----------|------------|----------|--------------|
| Async/Await | I/O-bound tasks | Low | Lowest | Explicit |
| Threads | CPU-bound tasks | Medium | Medium | Requires sync |
| Actors | Stateful services | Medium | Low | None (message passing) |
| CSP (channels) | Pipelines | Medium | Low | None (message passing) |

### Async/Await Pattern

Non-blocking I/O without callback complexity:

```python
import asyncio

async def fetch_url(url: str) -> str:
    """Non-blocking HTTP request."""
    async with aiohttp.ClientSession() as session:
        async with session.get(url) as response:
            return await response.text()

async def fetch_all(urls: list[str]) -> list[str]:
    """Fetch multiple URLs concurrently."""
    tasks = [asyncio.create_task(fetch_url(url)) for url in urls]
    return await asyncio.gather(*tasks)

# Run the async function
results = asyncio.run(fetch_all([
    "https://api.example.com/users",
    "https://api.example.com/orders",
    "https://api.example.com/products",
]))
```

**Key concepts:**

- **Coroutines**: Functions defined with `async def`
- **Tasks**: Scheduled coroutines that run concurrently
- **Event loop**: Scheduler that runs tasks
- **await**: Yields control back to event loop

### Message Passing vs Shared State

| Approach | Pros | Cons |
|----------|------|------|
| Message passing | No locks, no races, clear ownership | Overhead for small data |
| Shared state | Efficient for read-heavy workloads | Requires careful synchronization |

**Message passing** (preferred for most cases):

```python
import asyncio
from asyncio import Queue

async def producer(queue: Queue, items: list):
    for item in items:
        await queue.put(item)
    await queue.put(None)  # Sentinel to signal completion

async def consumer(queue: Queue, worker_id: int):
    while True:
        item = await queue.get()
        if item is None:
            await queue.put(None)  # Pass sentinel to next consumer
            break
        print(f"Worker {worker_id} processing {item}")
        await process(item)

async def main():
    queue = Queue()
    items = [1, 2, 3, 4, 5]

    # Start consumers first
    consumers = [
        asyncio.create_task(consumer(queue, i))
        for i in range(3)
    ]

    # Then producer
    await producer(queue, items)
    await asyncio.gather(*consumers)
```

**Shared state** (when necessary):

```python
import asyncio

class Counter:
    def __init__(self):
        self._value = 0
        self._lock = asyncio.Lock()

    async def increment(self):
        async with self._lock:
            self._value += 1

    async def get(self) -> int:
        async with self._lock:
            return self._value
```

### Supervision and Fault Tolerance

Isolate failures so one task's crash doesn't bring down the system:

```python
async def supervised_worker(task_fn, *args, max_retries=3):
    """Run a task with automatic retry on failure."""
    for attempt in range(max_retries):
        try:
            return await task_fn(*args)
        except Exception as e:
            if attempt == max_retries - 1:
                raise
            await asyncio.sleep(2 ** attempt)  # Exponential backoff

async def run_workers(tasks: list):
    """Run workers independently - one failure doesn't affect others."""
    results = await asyncio.gather(
        *[supervised_worker(task) for task in tasks],
        return_exceptions=True  # Don't fail all if one fails
    )

    for i, result in enumerate(results):
        if isinstance(result, Exception):
            log.error(f"Task {i} failed: {result}")
```

### Common Patterns

#### Worker Pool

Process items with bounded concurrency:

```python
from asyncio import Semaphore

async def process_with_limit(items: list, max_concurrent: int = 10):
    """Process items with bounded concurrency."""
    semaphore = Semaphore(max_concurrent)

    async def bounded_process(item):
        async with semaphore:
            return await process_item(item)

    return await asyncio.gather(*[bounded_process(i) for i in items])
```

#### Producer-Consumer with Backpressure

```python
async def producer(queue: Queue, source):
    """Backpressure: if queue is full, producer waits."""
    async for item in source:
        await queue.put(item)  # Blocks if queue at max size

async def consumer(queue: Queue):
    while True:
        item = await queue.get()
        await process(item)
        queue.task_done()

async def main():
    # Bounded queue provides backpressure
    queue = Queue(maxsize=100)

    producer_task = asyncio.create_task(producer(queue, data_source))
    consumers = [asyncio.create_task(consumer(queue)) for _ in range(5)]

    await producer_task
    await queue.join()  # Wait for all items to be processed
```

#### Fan-Out/Fan-In

```python
async def fan_out_fan_in(items: list):
    """Process items in parallel, collect results."""
    # Fan out: start all tasks
    tasks = [asyncio.create_task(process(item)) for item in items]

    # Fan in: collect all results
    results = await asyncio.gather(*tasks)

    # Aggregate
    return aggregate(results)
```

#### Pipeline Stages

```python
async def pipeline(data_source):
    """Multi-stage processing pipeline."""
    stage1_queue = Queue()
    stage2_queue = Queue()
    results = []

    async def stage1():
        async for item in data_source:
            transformed = await transform(item)
            await stage1_queue.put(transformed)
        await stage1_queue.put(None)

    async def stage2():
        while True:
            item = await stage1_queue.get()
            if item is None:
                await stage2_queue.put(None)
                break
            enriched = await enrich(item)
            await stage2_queue.put(enriched)

    async def collector():
        while True:
            item = await stage2_queue.get()
            if item is None:
                break
            results.append(item)

    await asyncio.gather(stage1(), stage2(), collector())
    return results
```

### Synchronization Primitives

#### Lock

Mutual exclusion for critical sections:

```python
lock = asyncio.Lock()

async def critical_section():
    async with lock:
        # Only one task can be here at a time
        await modify_shared_resource()
```

#### Semaphore

Limit concurrent access:

```python
semaphore = asyncio.Semaphore(5)  # Max 5 concurrent

async def limited_operation():
    async with semaphore:
        await perform_operation()
```

#### Event

Signal between tasks:

```python
event = asyncio.Event()

async def waiter():
    await event.wait()  # Blocks until event is set
    print("Event received!")

async def setter():
    await asyncio.sleep(1)
    event.set()  # Unblocks all waiters
```

#### Condition

Complex coordination:

```python
condition = asyncio.Condition()
data_ready = False

async def consumer():
    async with condition:
        await condition.wait_for(lambda: data_ready)
        process_data()

async def producer():
    global data_ready
    prepare_data()
    async with condition:
        data_ready = True
        condition.notify_all()
```

## Trade-offs

### Async vs Threads

| Consideration | Async/Await | Threads |
|---------------|-------------|---------|
| I/O-bound work | Excellent | Good |
| CPU-bound work | Poor (blocks event loop) | Good |
| Memory per task | ~KB | ~MB |
| Scaling | Thousands of tasks | Hundreds of threads |
| Debugging | Stack traces can be confusing | More intuitive |
| Libraries | Must be async-compatible | Any library works |

### When to Use What

**Use async/await when:**
- Many concurrent I/O operations (HTTP, database, files)
- Need thousands of concurrent tasks
- Already in async ecosystem

**Use threads when:**
- CPU-bound computation
- Blocking libraries without async support
- Need true parallelism (with GIL-free operations)

**Use actors/processes when:**
- Need fault isolation
- State must be encapsulated
- Distributed systems

## Anti-Patterns

### 1. Blocking in Async Context

```python
# BAD - blocks entire event loop
async def process():
    time.sleep(5)  # Blocks everything!
    data = requests.get(url)  # Also blocks!

# GOOD - use async versions
async def process():
    await asyncio.sleep(5)
    async with aiohttp.ClientSession() as session:
        data = await session.get(url)

# ACCEPTABLE - run blocking code in thread pool
async def process():
    loop = asyncio.get_event_loop()
    data = await loop.run_in_executor(None, blocking_function)
```

### 2. Shared Mutable State Without Synchronization

```python
# BAD - race condition
counter = 0

async def increment():
    global counter
    counter += 1  # NOT atomic!

# GOOD - use lock
counter = 0
lock = asyncio.Lock()

async def increment():
    global counter
    async with lock:
        counter += 1
```

### 3. Creating Tasks Without Awaiting

```python
# BAD - task may be garbage collected
def fire_and_forget():
    asyncio.create_task(do_something())  # Task reference lost

# GOOD - keep reference and await eventually
class TaskManager:
    def __init__(self):
        self.tasks = set()

    def schedule(self, coro):
        task = asyncio.create_task(coro)
        self.tasks.add(task)
        task.add_done_callback(self.tasks.discard)

    async def wait_all(self):
        await asyncio.gather(*self.tasks)
```

### 4. Unbounded Concurrency

```python
# BAD - may overwhelm resources
async def process_all(items):
    tasks = [process(item) for item in items]  # All at once!
    await asyncio.gather(*tasks)

# GOOD - bound concurrency
async def process_all(items, max_concurrent=100):
    semaphore = asyncio.Semaphore(max_concurrent)

    async def bounded_process(item):
        async with semaphore:
            return await process(item)

    tasks = [bounded_process(item) for item in items]
    await asyncio.gather(*tasks)
```

### 5. Ignoring Cancellation

```python
# BAD - doesn't clean up on cancel
async def long_operation():
    resource = acquire_resource()
    await do_work()  # If cancelled here, resource leaked

# GOOD - handle cancellation
async def long_operation():
    resource = acquire_resource()
    try:
        await do_work()
    except asyncio.CancelledError:
        release_resource(resource)
        raise
    finally:
        release_resource(resource)
```

### 6. Deadlock

```python
# BAD - potential deadlock
lock_a = asyncio.Lock()
lock_b = asyncio.Lock()

async def task1():
    async with lock_a:
        await asyncio.sleep(0)
        async with lock_b:  # Waits for task2 to release
            pass

async def task2():
    async with lock_b:
        await asyncio.sleep(0)
        async with lock_a:  # Waits for task1 to release
            pass

# GOOD - consistent lock ordering
async def task1():
    async with lock_a:
        async with lock_b:
            pass

async def task2():
    async with lock_a:  # Same order as task1
        async with lock_b:
            pass
```

## Testing Concurrent Code

```python
import pytest

@pytest.mark.asyncio
async def test_concurrent_operations():
    """Test that concurrent operations don't interfere."""
    results = []

    async def append(value):
        await asyncio.sleep(0.1)
        results.append(value)

    await asyncio.gather(
        append(1),
        append(2),
        append(3),
    )

    assert sorted(results) == [1, 2, 3]

@pytest.mark.asyncio
async def test_timeout():
    """Test operation respects timeout."""
    with pytest.raises(asyncio.TimeoutError):
        await asyncio.wait_for(
            asyncio.sleep(10),
            timeout=0.1
        )

@pytest.mark.asyncio
async def test_cancellation():
    """Test graceful cancellation."""
    cancelled = False

    async def cancellable():
        nonlocal cancelled
        try:
            await asyncio.sleep(10)
        except asyncio.CancelledError:
            cancelled = True
            raise

    task = asyncio.create_task(cancellable())
    await asyncio.sleep(0.1)
    task.cancel()

    with pytest.raises(asyncio.CancelledError):
        await task

    assert cancelled
```

## Additional References

- the `concurrency` skill\'s elixir reference - Task, processes, OTP basics, supervision
- the `concurrency` skill\'s rust reference - tokio, channels, Arc<Mutex<T>>, async runtime
