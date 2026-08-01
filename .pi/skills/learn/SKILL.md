---
name: learn
description: Use when documenting project-specific patterns, capturing gotchas, recording architectural decisions, or building a project knowledge base in project-learnings.md
---

# Learn Skill

Interactive knowledge capture in `.claude/project-learnings.md` for building project-specific knowledge.

## When to Use

Invoke with `/learn` when:
- Documenting a project-specific pattern after discovering it
- Capturing knowledge that should persist across sessions
- Recording architectural decisions and their rationale
- Building the project knowledge base after completing a feature
- Finding a gotcha, bug pattern, or performance insight

Skip this skill when:
- The pattern is universal (belongs in language/framework docs)
- Creating temporary notes (use spike-debt.md instead)
- Tracking TODO items (use TodoWrite tool)
- Writing general documentation (use module docs)

## Usage

```bash
/learn "All HTTP calls need explicit timeout"
/learn "JSON columns require indexes for query performance"
/learn "Optimistic UI updates improve perceived performance"
```

Expects a pattern description as the argument.

## Tools Used

This skill typically uses:
- **Read** - Load existing project-learnings.md
- **Write/Edit** - Update the knowledge base file


## What It Does

Captures and organizes project knowledge systematically:

1. **Read Current Knowledge** - Load existing project-learnings.md
2. **Parse Pattern** - Understand what to document
3. **Categorize** - Place in appropriate section
4. **Format** - Structure for easy searching
5. **Update File** - Add to project knowledge base
6. **Confirm** - Show what was added

## Implementation

### Step 1: Initialize or Read project-learnings.md

Check if `.claude/project-learnings.md` exists:

```bash
# Check for file
if [ -f ".claude/project-learnings.md" ]; then
  # Read existing
  cat .claude/project-learnings.md
else
  # Create from template
  mkdir -p .claude
  # Create initial template
fi
```

### Step 2: Template Structure

If creating new file, use this template:

```markdown
# Project Learnings

Project-specific patterns, conventions, and gotchas for [Project Name].

Last updated: [Date]

## Domain Conventions

Project-specific domain rules and patterns.

### Naming Conventions

<!-- Example:
- User-facing features use `user_facing_*` prefix
- Background jobs suffix with `_worker`
- Constants use SCREAMING_SNAKE_CASE
-->

### Module Organization

<!-- Example:
- Business logic in src/services/
- API layer in src/api/
- Shared utilities in src/utils/
-->

## Common Patterns

Established patterns used throughout the project.

### Data Access

<!-- Example:
- All database queries wrapped in repository pattern
- Use eager loading for associations to avoid N+1
- Pagination uses cursor-based approach
-->

### Error Handling

<!-- Example:
- Public APIs return Result types or exceptions
- Use early returns for validation errors
- Log errors with correlation IDs
-->

### Testing

<!-- Example:
- Use describe/test blocks for organization
- Mark critical tests with priority tags
- Use factories for test data generation
-->

## Library-Specific Patterns

How we use third-party libraries.

### HTTP Client

<!-- Example:
- Always set explicit timeouts
- Use connection pooling for high throughput
- Implement retry with exponential backoff
-->

### Background Jobs

<!-- Example:
- Max 3 retry attempts for jobs
- Use unique constraints for idempotent jobs
- Queue priority: critical > default > low
-->

## Performance Patterns

Performance optimizations and guidelines.

### Database

<!-- Example:
- Always add indexes on foreign keys
- Use JSON columns for flexible schemas
- Batch inserts for bulk operations
-->

### Caching

<!-- Example:
- Cache computation results in memory/Redis
- TTL: 5 minutes for user data, 1 hour for config
- Cache keys: "resource:id:field"
-->

## Common Gotchas

Mistakes to avoid, with examples.

### Security

<!-- Example:
- Always parameterize SQL (no string interpolation)
- Validate user input before processing
- Use signed tokens, not plain JWTs
-->

### Data Integrity

<!-- Example:
- Wrap multi-step operations in transactions
- Use database constraints, not just validations
- Handle optimistic locking conflicts
-->

### Concurrency

<!-- Example:
- Background workers should handle timeouts
- Use supervised task pools for async work
- Avoid long-running operations blocking main thread
-->

## Architectural Decisions

Why things are the way they are.

### Technology Choices

<!-- Example:
Decision: Use PostgreSQL JSONB over separate tables for user preferences
Reason: Flexibility for variable preference schemas
Trade-off: Complex queries require GIN indexes
Date: 2025-01-16
-->

### Design Patterns

<!-- Example:
Pattern: Command pattern for all mutations
Reason: Consistent audit logging and rollback capability
Alternative considered: Direct ORM updates
Date: 2025-01-18
-->

## Development Workflow

Team-specific development practices.

### Code Review

<!-- Example:
- All PRs require 1 approval
- Run /precommit before requesting review
- Security changes require 2 approvals
-->

### Testing Strategy

<!-- Example:
- Unit tests for business logic (criticality 9-10)
- Integration tests for API endpoints (criticality 8-9)
- E2E tests for critical user flows (criticality 10)
-->

## Production Considerations

Deployment and operations knowledge.

### Monitoring

<!-- Example:
- Alert on error rate >1% in 5-minute window
- Track P95 response time <200ms
- Monitor job queue depth
-->

### Feature Flags

<!-- Example:
- Use feature flag service for gradual rollouts
- Always add flag for risky features
- Clean up flags after 2 weeks
-->

## Learning Resources

Helpful references for the team.

### Internal Docs

- Architecture overview: docs/architecture.md
- API documentation: docs/api.md
- Deployment guide: docs/deployment.md

### External Resources

- Framework documentation: [link]
- Performance optimization guides: [link]
```

### Step 3: Parse Pattern

Analyze what the user wants to document:

```markdown
Analyzing pattern: "All Req HTTP calls need explicit timeout"

Categorization:
- Type: Common Gotcha (avoiding mistakes)
- Category: Library-Specific Pattern (Req usage)
- Reason: Performance / reliability

Best fit: Common Gotchas → Performance / External Services
```

### Step 4: Determine Section

Map pattern types to sections:

**Domain Conventions**:
- Naming rules
- Module organization
- Business logic patterns

**Common Patterns**:
- Data access patterns
- Error handling approaches
- Testing practices

**Library-Specific Patterns**:
- How to use third-party libraries
- Configuration patterns
- Best practices for specific tools

**Performance Patterns**:
- Database optimizations
- Caching strategies
- Memory management

**Common Gotchas**:
- Mistakes to avoid
- Edge cases
- Known issues

**Architectural Decisions**:
- Why we chose X over Y
- Trade-offs documented
- Historical context

### Step 5: Format Pattern

Structure the knowledge entry:

```markdown
### [Pattern Name]

**Pattern**: [Clear description]

**Example**:
```python
# Good - explicit timeout
response = requests.get(url, timeout=5.0)

# Bad - no timeout, can hang indefinitely
response = requests.get(url)
```

**Reason**: [Why this matters]

**Related**: [Cross-references to other patterns]
```

Or for gotchas:

```markdown
### [Gotcha Name]

**Issue**: [What goes wrong]

**Why**: [Root cause]

**Solution**:
```python
# Example of correct approach
```

**Related**: [Similar issues or patterns]
```

### Step 6: Update File

Add pattern to appropriate section:

```markdown
## Common Gotchas

### HTTP Client Timeouts

**Issue**: HTTP calls can hang indefinitely if remote service is slow or unresponsive

**Why**: Default timeout may be infinite or very long, causing threads/processes to block

**Solution**: Always specify explicit timeout

```python
# Good - explicit timeout
response = requests.get(url, timeout=5.0)  # 5 second timeout

# Better - handle timeout errors
try:
    response = requests.get(url, timeout=5.0)
    response.raise_for_status()
    return {"ok": True, "data": response.json()}
except requests.Timeout:
    return {"ok": False, "error": "timeout"}
except requests.HTTPError as e:
    return {"ok": False, "error": f"http_error_{e.response.status_code}"}
except requests.RequestException as e:
    return {"ok": False, "error": str(e)}
```

**Related**: Error Handling → Network Failures

**Added**: 2025-01-16 (during email preferences feature)
```

### Step 7: Confirm Update

Show what was added:

```markdown
✅ Knowledge Captured

Updated: .claude/project-learnings.md

Section: Common Gotchas → HTTP Client Timeouts

Added:
- Pattern description
- Code examples (good vs bad)
- Solution with error handling
- Related references

This pattern is now searchable and will help prevent similar issues.
```

## Pattern Categories

### Domain Conventions

Project-specific business rules:

```bash
/learn "User roles: admin, manager, user - never 'superuser'"
/learn "Money amounts stored in cents (integer) to avoid float precision"
/learn "All timestamps use UTC, convert to user timezone in UI"
```

### Common Patterns

Established coding patterns:

```bash
/learn "Use early returns for validation to reduce nesting"
/learn "Prefix private functions with underscore to distinguish from public API"
/learn "Pagination uses keyset (cursor) not offset for performance"
```

### Library-Specific Patterns

Third-party library usage:

```bash
/learn "HTTP client: always set timeout parameter explicitly"
/learn "Background jobs: use unique constraint to prevent duplicate work"
/learn "ORM: use eager loading not lazy loading to prevent N+1"
```

### Performance Patterns

Optimization knowledge:

```bash
/learn "Use read-optimized caches for lookup-heavy workloads"
/learn "JSON columns need GIN indexes for queries: CREATE INDEX USING GIN"
/learn "Batch database inserts with bulk_create for 10x speedup"
```

### Common Gotchas

Mistakes to avoid:

```bash
/learn "WebSocket handlers must handle reconnection gracefully"
/learn "ORM lazy loading in loop causes N+1 - always eager load"
/learn "JWT decode returns claims dict, not user object"
```

### Architectural Decisions

Historical context:

```bash
/learn "Decision: PostgreSQL over MySQL - better JSON support, needed for preferences"
/learn "Pattern: CQRS for orders - read model separate from write for performance"
```

## Integration with Other Commands

### Automatically triggered by

**After /feature**:
```markdown
Feature complete. Would you like to capture any patterns learned?

Suggested learnings:
- JSON column approach for preferences (Performance Pattern)
- Token-based auth usage (Library-Specific Pattern)
- Optimistic UI updates (Common Pattern)

Run: /learn "[pattern]" for any of these
```

**After /spike-migrate**:
```markdown
SPIKE migration complete. Patterns discovered:

1. JSON column indexes needed for query performance
2. Debouncing UI updates improves UX
3. Schema versioning prevents migration issues

Capture with: /learn "[pattern]"
```

**After /benchmark**:
```markdown
Benchmark complete. Performance findings:

- Generators better than lists for large datasets (>10K items)
- Preprocessing into dict/set reduces O(n²) to O(n)

Document with: /learn "[finding]"
```

## Searching project-learnings.md

Project knowledge is searchable:

```bash
# Find patterns about a topic
grep -i "database" .claude/project-learnings.md

# Find gotchas
grep -A 5 "Issue:" .claude/project-learnings.md

# Find architectural decisions
grep -A 10 "Decision:" .claude/project-learnings.md
```

## Best Practices

1. **Be specific**: "Use X for Y" not "X is good"
2. **Include examples**: Code speaks louder than words
3. **Explain why**: Context helps future developers
4. **Cross-reference**: Link related patterns
5. **Date entries**: Track when pattern was added
6. **Keep updated**: Remove obsolete patterns
7. **One pattern per entry**: Focused and searchable

## Error Handling

### No project-learnings.md

```
Creating .claude/project-learnings.md...

# Project Learnings

[Initial template created]

✅ Knowledge base initialized
Ready to capture first pattern.
```

### Pattern Already Documented

```
⚠️  Similar Pattern Found

Existing entry in project-learnings.md:

## Common Gotchas → HTTP Timeouts
"All HTTP calls should have explicit timeouts"

Options:
1. Update existing entry with new information
2. Add as separate pattern
3. Skip (already documented)

Choose: [1/2/3]
```

### Unclear Pattern

```
⚠️  Pattern Unclear

Pattern: "use streams"

This is too vague. Please provide more context:
- What specific use case?
- Why use streams vs alternatives?
- Example code?

Try: /learn "Use Stream for large datasets (>10K items) when limiting results"
```

## Example Entries

### Domain Convention

```markdown
## Domain Conventions

### Money Handling

**Pattern**: All monetary amounts stored as integers in cents

**Reason**: Avoid floating-point precision errors in financial calculations

**Example**:
```python
# Model
class Order(Base):
    amount_cents: int  # $19.99 stored as 1999

# Display
def format_price(cents: int) -> str:
    dollars = cents / 100
    return f"${dollars:.2f}"

# Calculations
def apply_discount(amount_cents: int, percent: int) -> int:
    discount = (amount_cents * percent) // 100
    return amount_cents - discount
```

**Related**: Performance Patterns → Database Precision
```

### Common Gotcha

```markdown
## Common Gotchas

### WebSocket Message Handling

**Issue**: WebSocket messages require explicit type checking

**Why**: Message handlers need to match exact message structure

**Example**:
```python
# Bad - too generic, no type checking
async def handle_message(message):
    await self.process(message)

# Good - explicit type checking
async def handle_message(message: dict):
    msg_type = message.get("type")

    if msg_type == "user_updated":
        user_id = message.get("user_id")
        await self.reload_user(user_id)
    elif msg_type == "ping":
        await self.send_pong()
    else:
        # Log unknown message types
        logger.warning(f"Unknown message type: {msg_type}")
```

**Solution**: Always validate message structure before processing

**Related**: WebSocket Patterns
```

### Performance Pattern

```markdown
## Performance Patterns

### Batch Database Inserts

**Pattern**: Use bulk_create/insert_all for inserting multiple records

**Performance**: 10-20x faster than individual inserts

**Example**:
```python
# Bad - N database round trips
for user_attrs in users:
    user = User(**user_attrs)
    session.add(user)
    session.commit()

# Good - single database round trip
now = datetime.utcnow()
users_list = [
    User(**attrs, created_at=now, updated_at=now)
    for attrs in users
]
session.bulk_save_objects(users_list)
session.commit()

# Or with Django
User.objects.bulk_create([User(**attrs) for attrs in users])
```

**Benchmark**: 1000 records - 15s → 1.2s (12.5x improvement)

**Trade-off**: No individual validation callbacks, so validate before bulk insert

**Related**: Database → Bulk Operations
```

## Success Criteria

Learn command succeeds when:
- Pattern clearly described
- Code examples provided
- Placed in appropriate section
- Searchable keywords included
- Related patterns cross-referenced
- Date/context added
- File updated successfully

## Related Skills

- `/feature` - Suggests patterns to document after completion
- `/spike-migrate` - Captures learnings from migration
- `/review` - May reference project-learnings.md for standards
- `/benchmark` - Performance findings worth documenting
- `/cognitive-audit` - Architectural patterns discovered
