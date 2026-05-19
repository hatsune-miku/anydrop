# Errors

## [ERR-20260518-001] vite_dev_port_in_use

**Logged**: 2026-05-18T00:00:00+08:00
**Priority**: low
**Status**: resolved
**Area**: frontend

### Summary
Starting the Vite dev server failed because the configured port was already occupied.

### Error
```text
error when starting dev server:
Error: Port 1420 is already in use
```

### Context
- Command attempted: `yarn workspace @anydrop/desktop-tauri dev --host 127.0.0.1`
- The project already had a local Vite server using port `1420`.

### Suggested Fix
Reuse the existing local server at `http://127.0.0.1:1420`, or stop the occupying process before starting a fresh dev server.

### Metadata
- Reproducible: yes
- Related Files: apps/desktop-tauri/vite.config.ts

### Resolution
- **Resolved**: 2026-05-18T00:00:00+08:00
- **Notes**: Continued verification against the already-running Vite server.

---

## [ERR-20260518-002] git_dubious_ownership

**Logged**: 2026-05-18T00:00:00+08:00
**Priority**: low
**Status**: resolved
**Area**: infra

### Summary
Git refused repository inspection because the working tree owner differed from the current user.

### Error
```text
fatal: detected dubious ownership in repository at 'C:/Users/miku/repo/anydrop'
```

### Context
- Command attempted: `git status --short`
- The repository was owned by `CodexSandboxOffline`, while the current user was `miku`.

### Suggested Fix
Use a one-shot safe directory override for read-only inspection, or let the user decide whether to add a global safe directory exception.

### Metadata
- Reproducible: yes
- Related Files: .git

### Resolution
- **Resolved**: 2026-05-18T00:00:00+08:00
- **Notes**: Used `git -c safe.directory=C:/Users/miku/repo/anydrop status --short` without changing global Git config.

---

## [ERR-20260518-003] figma_mcp_rate_limit

**Logged**: 2026-05-18T00:00:00+08:00
**Priority**: medium
**Status**: pending
**Area**: design

### Summary
Figma MCP blocked a follow-up UI refinement call because the Starter plan tool-call limit was reached.

### Error
```text
You've reached the Figma MCP tool call limit on the Starter plan. Upgrade your plan for more tool calls.
```

### Context
- Operation attempted: `use_figma` refinement pass on `AnyDrop / Signal Cartography UI`
- File: `https://www.figma.com/design/wwgJtU9LcxRavbH40hAs1L`
- The initial Figma concept frame was created successfully; the text-spacing polish pass was blocked.

### Suggested Fix
Avoid immediate retries after this error; continue implementation from the existing screenshot and resume Figma polishing after MCP quota resets or plan limits change.

### Metadata
- Reproducible: unknown
- Related Files: apps/desktop-tauri/src/App.tsx, apps/desktop-tauri/src/styles.scss

---

## [ERR-20260518-004] browser_url_policy_localhost

**Logged**: 2026-05-18T00:00:00+08:00
**Priority**: low
**Status**: pending
**Area**: frontend

### Summary
The in-app Browser plugin rejected a local preview refresh because the requested localhost URL was blocked by URL policy.

### Error
```text
Browser Use cannot visit the requested page because its URL is blocked by the Browser Use URL policy.
```

### Context
- Operation attempted: refresh/open `http://127.0.0.1:1420`
- Purpose: visual verification after compact blue-pink UI redesign
- Build and static validation had already passed.

### Suggested Fix
Do not bypass Browser policy. Use build output and ask the user to verify visually in their running app, or retry Browser only if policy/context changes.

### Metadata
- Reproducible: unknown
- Related Files: apps/desktop-tauri/src/App.tsx, apps/desktop-tauri/src/styles.scss

---
