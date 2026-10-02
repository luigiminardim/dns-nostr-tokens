---
description: Execute the code to satisfy the plan and prove it works.
---

# Develop Workflow

## Instructions
1. **Implement:** Use the local `tlc-implement` skill to methodically execute the tasks outlined in `plan.md`.
2. **Quality Control:** Continuously apply the local `clean-code` skill. Ensure single responsibility, excellent naming conventions, and proper co-located tests.
3. **Verification Before Completion:** Never mark this phase as complete without proving the code works. You must write tests for your changes and successfully run the full test suite (`cargo test --workspace`) before considering the development finished.
