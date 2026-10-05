---
description: Plan implementation, resolve doubts, write tests, implement changes, verify story conformance, and update documentation.
---

# Develop & Deliver Workflow (Double Diamond - Phases 3 & 4)

## Instructions

### Step 1: Story Intake
- Retrieve and review the target GitHub issue created during the `2-define` phase (e.g., via `gh issue view <issue-number>`).
- Carefully examine the user/technical story, acceptance criteria, and codebase survey notes.

### Step 2: Technical Planning & Doubt Resolution
- Structure a concrete implementation plan breaking down the technical tasks needed to satisfy the story.
- Deep-dive into the affected codebase and **identify and resolve all doubts, ambiguities, or architectural uncertainties** before writing any production code.
- If any question regarding domain rules, public APIs, or unexpected constraints arises, clarify it upfront with the user.
- **Halt & Await Plan Approval (MANDATORY):** Present the technical plan to the user. **You must wait for the user to explicitly approve the plan before proceeding to Step 3 (writing tests) or Step 4 (implementing code). Never start implementation without user approval of the plan.**

### Step 3: Write Tests First (Specification)
- Before implementing production logic, write or expand co-located tests (`#[cfg(test)]`) reproducing the requirements and acceptance criteria defined in the story.
- Confirm that tests capture expected edge cases, error conditions, and success scenarios.

### Step 4: Implement Changes (Clean Code)
- Methodically implement the code to satisfy the tests and execute the plan.
- Apply the local `clean-code` skill:
  - Adhere to the Single Responsibility Principle (SRP).
  - Use meaningful naming and small, focused functions.
  - Apply the Boy Scout Rule: leave every file cleaner than you found it.
- Run the full test suite (`cargo test --workspace`) and ensure all tests pass cleanly.

### Step 5: Verify Story Conformance
- **Acceptance Criteria Verification:** Systematically cross-reference every acceptance criterion from the GitHub issue against the implemented behavior and automated tests. Prove that the story has been completely fulfilled.
- **Code Quality Audit:** Invoke the local `the-judge` skill to perform an evidence-based review of the diff/PR. Address and fix any quality or architecture issues detected.

### Step 6: Documentation Integrity & Ship
- Check if the changes alter architecture, domain concepts, setup commands, or public interfaces. Update [`AGENTS.md`](file:///home/luigi/Desktop/dns-nostr-tokens/AGENTS.md), [`README.md`](file:///home/luigi/Desktop/dns-nostr-tokens/README.md), or [`docs/domain.md`](file:///home/luigi/Desktop/dns-nostr-tokens/docs/domain.md) accordingly. Code and documentation must stay in sync.
- Create a clean commit and pull request summarizing the changes, explicitly referencing the issue (e.g., `Closes #<issue-number>`).
- Announce to the user that development, verification, and documentation are complete and the feature is ready to merge!
