---
trigger: always_on
description: Reusable agent orchestration rules — workflow, principles, and behavioral guardrails.
---

# Agent Orchestration

## Workflow Orchestration

These rules govern how agents should approach any task in this repository.

### 1. Plan Mode Default

- Enter plan mode for **any** non-trivial task (3+ steps or architectural decisions).
- If something goes sideways, **stop and re-plan immediately** — don't keep pushing.
- Use plan mode for verification steps, not just building.
- Write detailed specs upfront to reduce ambiguity.

### 2. Subagent Strategy

- Use subagents liberally to keep the main context window clean.
- Offload research, exploration, and parallel analysis to subagents.
- For complex problems, throw more compute at it via subagents.
- One task per subagent for focused execution.

### 3. Verification Before Done

- **Never** mark a task complete without proving it works.
- Run the full test suite (`cargo test --workspace`) before considering work done.
- Verify your changes against the existing behavior.
- Ask yourself: _"Would a staff engineer approve this?"_

### 4. Demand Elegance (Balanced)

- For nontrivial changes: pause and ask _"Is there a more elegant way?"_
- If a fix feels hacky: _"Knowing everything I know now, implement the elegant solution."_
- Skip this for simple, obvious fixes — don't over-engineer.
- Challenge your own work before presenting it.

### 5. Autonomous Bug Fixing

- When given a bug report: just fix it. Don't ask for hand-holding.
- Run tests to identify the root cause.
- Zero context switching required from the user.
- Go fix failing tests without being told how.

### 6. Documentation Integrity

The code is the source of truth. Documentation must stay in sync.

- After any change that alters the **architecture**, **repository structure**, **setup commands**, **environment config**, or **key concepts**, update `AGENTS.md` (and `README.md` if applicable) in the same PR.
- If you discover that existing documentation has **drifted** from the code (wrong commands, outdated file paths, stale architecture descriptions), fix it immediately — don't leave it for later.
- Treat documentation drift the same as a bug: it misleads both humans and agents.
- When adding new modules, crates, or significant public APIs, add them to the relevant sections in `AGENTS.md`.

---

## Core Principles

| Principle        | What it means                                                               |
| ---------------- | --------------------------------------------------------------------------- |
| Simplicity First | Make every change as simple as possible. Minimal code impact.               |
| No Laziness      | Find root causes. No temporary workarounds. Senior developer standards.     |
| Minimal Impact   | Changes should only touch what's necessary. Avoid introducing bugs.         |