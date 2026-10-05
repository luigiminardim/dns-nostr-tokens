---
description: Explore the problem space, research the codebase, and brainstorm solutions (PRD options).
---

# Discover Workflow (Double Diamond - Phase 1)

## Instructions

1. **Understand the Domain:**
   - Read the local `domain-driven-design` skill (`.agents/skills/domain-driven-design/SKILL.md`) and [`docs/domain.md`](file:///home/luigi/Desktop/dns-nostr-tokens/docs/domain.md).
   - Ground your thinking in the ubiquitous language and existing bounded contexts of the repository.

2. **Exploration & Brainstorming:**
   - Analyze the current codebase and the user's initial idea or problem statement.
   - Brainstorm potential architectural approaches, trade-offs, and alternative solutions.

3. **Draft PRD(s):**
   - Synthesize the findings into one or more Product Requirements Documents (PRDs). Multiple PRDs or solution alternatives may be presented (e.g., contrasting scope, trade-offs, or design directions).
   - Each PRD draft should clearly define:
     - **Problem Statement & Motivation**
     - **Domain Impact** (entities, events, and ubiquitous terms)
     - **Proposed Solution & Scope**
     - **Explicit Out-of-Scope Boundaries**

4. **Halt & Review:**
   - Present the PRD(s) to the user for discussion, feedback, and selection.
   - Do not proceed to defining issues until the user explicitly selects/confirms the desired PRD direction.

5. **Next Step Recommendation:**
   - Once the user approves the chosen PRD, explicitly recommend that they trigger the `2-define` workflow (`/2-define`) to survey the codebase and create the GitHub issue.
