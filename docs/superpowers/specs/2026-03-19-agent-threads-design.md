# Agent-Initiated Threads Design

## Goal

Add `origin` and `severity` fields to threads so the system distinguishes developer-created threads from agent-created ones, with appropriate UI treatment and approval logic.

## Data Model

### New enums in `lgtm-session`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    #[default]
    Developer,
    Agent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Critical,
    Warning,
    Info,
}
```

### Thread struct changes

```rust
pub struct Thread {
    pub id: String,
    #[serde(default)]
    pub origin: Origin,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub severity: Option<Severity>,
    pub status: ThreadStatus,
    // ... rest unchanged
}
```

`serde(default)` ensures backward compatibility with existing session.json files that lack these fields.

## Server API Changes

### `POST /api/threads`

Request body gains optional fields:

```json
{
  "file": "auth.py",
  "line_start": 6,
  "line_end": 6,
  "diff_side": "right",
  "anchor_context": "...",
  "body": "...",
  "origin": "agent",
  "severity": "warning"
}
```

Both default to `"developer"` / `null` if omitted.

### `PATCH /api/threads/{id}`

- `dismissed` status is only valid for agent-origin threads. Return 422 if attempted on a developer thread.

### `PATCH /api/session` (approve validation)

Approval requires:
- All developer-origin threads: resolved or wontfix
- All agent-origin threads: resolved or dismissed
- All files reviewed

## Frontend Changes

### TypeScript types

Add to `Thread` interface:
```typescript
origin: 'developer' | 'agent';
severity?: 'critical' | 'warning' | 'info' | null;
```

### Thread.svelte

- Agent threads get a distinct left-border color based on severity
- Severity badge: red dot for critical, yellow for warning, blue for info
- "Dismiss" button appears only on agent-origin threads (alongside Resolve/Won't fix)
- Collapsed header shows origin indicator

### StatusBar.svelte

- Dismissed count shown when > 0
- Approve button gate updated: developer threads must be resolved/wontfix, agent threads must be resolved/dismissed

### App.svelte header

- Thread summary updated to reflect dismissed count in "done" total

## Backward Compatibility

- Existing session.json files without `origin`/`severity` fields deserialize correctly via serde defaults
- Existing developer-created threads continue to work identically
- No migration needed
