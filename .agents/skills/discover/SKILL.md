---
name: discover
description: Explore ideas in the research phase of double diamond. Brainstorm ideas, have a conversation, and propose a questionnaire or solution for the next phase. Use when the user wants to brainstorm, explore a problem, or start the research phase.
license: CC-BY-4.0
metadata:
  author: Luigi Minardi
  version: 1.1.0
---

# Discover Skill

This skill guides the user through the research phase of the Double Diamond process. It involves a conversation to brainstorm ideas, explore the problem space, and concludes by proposing a solution or a questionnaire to further define the best approach. Focus on the *problem space* and avoid jumping straight to solutions.

## Instructions

### Step 1: Understand the Domain
- Ask the user to describe their initial idea or problem statement.
- Read relevant documentation (e.g., `docs/domain.md` or existing architecture guidelines) to ground your understanding.
- **Rule:** Do not proceed to brainstorming until you clearly understand the context.

### Step 2: Explore the Problem Space
- Have an interactive conversation with the user to uncover the root of the problem.
- Ask open-ended questions focused on past behavior and pain points. For example:
  - "Can you walk me through how you currently handle this?"
  - "What is the most frustrating part of your current workflow?"
  - "What workarounds do you currently use?"
- **Rule:** Resist the temptation to jump to solutions. The goal is to understand the *why* behind the struggles.

### Step 3: Assess Impact and Value
- Ask questions to determine the priority and impact of solving this problem:
  - "How much time or effort does this manual process currently take?"
  - "What is the impact of this problem on your day-to-day work?"
- Gather both qualitative insights and technical constraints from the user.

### Step 4: Propose a Solution or Questionnaire
- Once the problem space is thoroughly explored, synthesize the findings.
- Either:
  - **Propose a Solution:** A high-level direction that solves the validated problem.
  - **Propose a Questionnaire:** A set of targeted questions for the user to research offline if more data is needed (e.g., "We need to know the specific throughput requirements before deciding on the architecture").
- The output should be a clear summary that can be fed directly into the `define` skill.

### Step 5: Next Steps
- Recommend the user to run the `define` skill to create a formal PRD once they are satisfied with the brainstormed direction.

## Examples

### Example 1: Exploring a new caching feature
User says: "I want to add Redis caching to the DNS server."
Actions:
1. Ground the understanding: Check `docs/domain.md` for current DNS resolution architecture.
2. Explore problem space: "Can you walk me through the current bottlenecks? What is the most frustrating part of the current SQLite caching?"
3. Assess impact: "How much latency are we seeing right now, and what is the target?"
4. Propose solution/questionnaire: Based on the chat, propose the Redis architecture or ask for specific latency numbers.
Result: A summarized problem statement and high-level solution direction.

### Example 2: Broad problem statement
User says: "We need a better way to handle token revocation."
Actions:
1. Explore problem space: "How do you currently handle token revocation? Tell me about the last time it failed or caused issues."
2. Assess impact: "What is the impact of a failed revocation on the users?"
3. Propose questionnaire: Ask the user to clarify whether revocation should be instantaneous or if eventual consistency is acceptable.
Result: A questionnaire for the user to research before moving to the Define phase.
