---
name: develop-deliver
description: Develop and deliver a planned feature. Consumes a PRD, plans with clean-architecture, gets review, uses subagents to implement, enforces independent verification, and cleans up with clean-code. Use when the user says "develop this feature", "implement the PRD", or wants to execute a plan.
license: CC-BY-4.0
metadata:
  author: Luigi Minardi
  version: 1.1.0
---

# Develop & Deliver Skill

This skill executes the final phases of the Double Diamond process. It consumes a PRD, creates a concrete technical plan, extracts a checklist, delegates the actual implementation to subagents, proves every check with an independent verifier, and finally updates documentation.

## Critical Rules

1. **Verify independently:** The author cannot verify their own work. A fresh, independent Verifier subagent must prove every check after the build.
2. **Name the proof:** Every check in the plan must name the specific test or command that settles it. No proof, no check.
3. **Tests follow the plan:** Tests assert what the checklist says, never what the implementation happens to do. Never write a test by reading the implementation.
4. **Halt for approval:** Never start implementation before the user approves the plan.
5. **Blast radius:** An approved plan authorizes local edits and commits. `git push`, deploy, and production data changes need an explicit go-ahead.

## Instructions

### Step 1: Consume PRD & Create Plan (Clean Architecture)
- Read the PRD.
- **Apply the `clean-architecture` skill:** Evaluate layer placement and ensure adherence to the Dependency Rule before defining the tasks.
- Break the work into observable slices (one outcome per slice). For every slice, write a checklist of explicit claims.
- **Sweep:** Explicitly address unwritten requirements: validation, failure modes, idempotency/retry, authorization, concurrency/ordering, data lifecycle, observability. 

### Step 2: Review Plan
- Halt and present the plan to the user. Ask: "Does this plan need any modifications?"
- Iterate until the user approves. 

### Step 3: Implement via Subagents
- Use subagents to implement the plan.
- **Batch by slices:** Hand off work to subagents in whole slices. Never split a slice midway. Handoffs carry the checklist and the diff of what has landed.
- **Test-Driven:** The subagents must write tests first according to the plan, then implement the code to make them pass.
- **Apply the `clean-code` skill:** After the implementation is working, the subagent must enforce quality standards (meaningful naming, Boy Scout Rule, small functions) before committing.

### Step 4: Independent Verification
- After the last batch has landed, dispatch a **fresh Verifier subagent** over the whole feature.
- The Verifier's report must account for every check. The feature is not done until the Verifier proves them all.

### Step 5: Clean Up & Update Knowledge
- Verify that the full test suite (`cargo test --workspace`) passes.
- Remove temporary planning files.
- Update repository documentation (`AGENTS.md`, `README.md`, `docs/domain.md`) to reflect the new architecture or features.
- Notify the user that development is complete.

## Examples

### Example 1: Implementing a planned feature
User says: "Implement the PRD for the DNS resolver cache."
Actions:
1. Apply `clean-architecture` to plan the layers. Create `.agents/docs/plan.md` with slices and nameable proofs.
2. Present the plan to the user and wait for approval.
3. Once approved, use a subagent to implement the first slice. Write tests first, implement, then apply `clean-code`.
4. Dispatch an independent Verifier subagent to check the proofs.
5. Update `AGENTS.md` and clean up the plan.
Result: The feature is built, tested, independently verified, and documented.
