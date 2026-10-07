---
name: define
description: Define a Product Requirements Document (PRD) for a problem. Asks open questions about the product and generates a prd.md in .agents/docs based on a best-in-class PRD template. Use when the user wants to write a PRD or define the requirements for a solution.
license: CC-BY-4.0
metadata:
  author: Luigi Minardi
  version: 1.0.0
---

# Define Skill

This skill guides the user through the definition phase of the Double Diamond process. It asks open questions to clarify the product vision and generates a formal Product Requirements Document (PRD) in `.agents/docs/prd.md`.

## Instructions

### Step 1: Gather Context
- Ask the user for the output of the `discover` phase or their current problem statement and proposed solution.
- Ask open questions about the product to clarify scope, target audience, success metrics, and constraints.

### Step 2: Draft the PRD
- Generate a comprehensive PRD using a standard, high-quality template. 
- The PRD must include:
  - **Title & Status**
  - **Problem Statement & Motivation:** What are we solving and why?
  - **Target Audience:** Who is this for?
  - **Scope & Out of Scope:** Clear boundaries of what will and will not be built.
  - **User Stories & Requirements:** Functional and non-functional requirements.
  - **Success Metrics (KPIs):** How do we measure success?
  - **Technical Impact & Codebase Survey:** General impact on the existing system.

### Step 3: Save to File
- Automatically write the drafted PRD to `.agents/docs/prd.md`. (Create the directory if it doesn't exist).
- Do not proceed until the file is written.

### Step 4: Review and Next Steps
- Ask the user to review `.agents/docs/prd.md` and provide feedback. Iterate if necessary.
- Once approved, recommend the user to run the `develop-deliver` skill to plan and implement the PRD.
