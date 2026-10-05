---
description: Survey the codebase for general impact, converge on the approved PRD, and create the GitHub story/issue.
---

# Define Workflow (Double Diamond - Phase 2)

## Instructions

1. **Architectural Alignment:**
   - Consult the local `clean-architecture` skill (`.agents/skills/clean-architecture/SKILL.md`) to evaluate layer placement and ensure adherence to the Dependency Rule.

2. **Codebase Impact Survey:**
   - Thoroughly inspect the existing codebase to map out the general impact of the approved PRD:
     - Identify impacted crates, modules, structs, traits, and functions.
     - Trace dependencies, callers, and existing test suites that will be touched or affected.
     - Document potential side effects, breaking changes, or architectural touchpoints.

3. **Create GitHub Story / Issue:**
   - Converge the approved PRD and the codebase survey findings into a GitHub issue using `gh issue create`.
   - **CRITICAL CONSTRAINT:** Each issue must represent an **entire user story** or a **complete technical story** (do not create micro-tasks or fragmented tickets).
   - The issue body must include:
     - **Story Context & Goal:** What problem is being solved and why.
     - **Acceptance Criteria:** Concrete, observable outcomes (Definition of Done).
     - **Scope & Out-of-Scope:** Explicit boundaries.
     - **Codebase Impact Summary:** High-level findings from the codebase survey (modules, structs, and interfaces to be changed).

4. **Next Step Recommendation:**
   - Once the issue is successfully created on GitHub, present the issue link/number and explicitly recommend that the user trigger the `3-develop-deliver` workflow (`/3-develop-deliver`) to begin implementation and delivery.
