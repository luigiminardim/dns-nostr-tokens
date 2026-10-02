---
description: Converge on the approved PRD and slice it into concrete, actionable issues and an execution plan.
---

# Define Workflow

## Instructions
1. **Architectural Alignment:** Consult the local `clean-architecture` skill (`.agents/skills/clean-architecture/SKILL.md`) to determine exactly where the new changes will live within the Dependency Rule.

2. **Issue Generation:** Use the **`mattpocock/skills@to-issues`** skill to parse the approved PRD and generate GitHub issues. 
   - **CRITICAL CONSTRAINT:** Do not create micro-tasks or overly small issues. Each issue must cover an **entire user story** or a **complete technical story** (e.g., "Implement the DNS resolution use case" rather than "Add a DNS parser function"). 
   - You may create multiple issues to cover the PRD, but each issue should be chunky, self-contained, and formatted like a mini-PRD itself.

3. **Execution Plan:** Use the local `tlc-plan` skill to create a concrete execution plan (`plan.md`) for the first issue to be tackled.

4. **Next Step Recommendation:** Once the plan is created and verified, explicitly recommend that the user trigger the `3-develop` workflow to begin implementation.
