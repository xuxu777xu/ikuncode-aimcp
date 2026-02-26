---
description: Superpowers overview - use when starting any conversation to determine which workflow skills to apply before taking action
---

# Superpowers - Agentic Skills Framework

## How to Access Skills

Skills are available as Windsurf workflows. Use `/superpowers-<skill-name>` to invoke them.

## The Rule

**Check for relevant skills BEFORE any response or action.** Even a 1% chance a skill might apply means you should check.

## Red Flags

These thoughts mean STOP — you're rationalizing:

| Thought | Reality |
|---------|---------|
| "This is just a simple question" | Questions are tasks. Check for skills. |
| "I need more context first" | Skill check comes BEFORE clarifying questions. |
| "Let me explore the codebase first" | Skills tell you HOW to explore. Check first. |
| "This doesn't need a formal skill" | If a skill exists, use it. |
| "The skill is overkill" | Simple things become complex. Use it. |
| "I'll just do this one thing first" | Check BEFORE doing anything. |

## Skill Priority

When multiple skills could apply, use this order:

1. **Process skills first** (brainstorming, debugging) - these determine HOW to approach the task
2. **Implementation skills second** - these guide execution

"Let's build X" → brainstorming first, then implementation skills.
"Fix this bug" → debugging first, then domain-specific skills.

## The Basic Workflow

1. **brainstorming** `/superpowers-brainstorming` - Refines ideas through questions, explores alternatives, presents design for validation
2. **git-worktrees** `/superpowers-git-worktrees` - Creates isolated workspace on new branch
3. **writing-plans** `/superpowers-writing-plans` - Breaks work into bite-sized tasks (2-5 min each)
4. **executing-plans** `/superpowers-executing-plans` - Executes tasks in batches with human checkpoints
5. **test-driven-development** `/superpowers-tdd` - Enforces RED-GREEN-REFACTOR cycle
6. **requesting-code-review** `/superpowers-code-review` - Reviews against plan, reports issues by severity
7. **finishing-branch** `/superpowers-finishing-branch` - Verifies tests, presents merge/PR/keep/discard options

## Available Skills

**Core Workflow:**
- `/superpowers-brainstorming` - Socratic design refinement
- `/superpowers-writing-plans` - Detailed implementation plans
- `/superpowers-executing-plans` - Batch execution with checkpoints

**Testing & Debugging:**
- `/superpowers-tdd` - RED-GREEN-REFACTOR cycle
- `/superpowers-debugging` - 4-phase root cause process
- `/superpowers-verification` - Ensure it's actually fixed

**Code Review:**
- `/superpowers-code-review` - Pre-review checklist
- `/superpowers-receiving-review` - Responding to feedback

**Development Flow:**
- `/superpowers-git-worktrees` - Parallel development branches
- `/superpowers-finishing-branch` - Merge/PR decision workflow
- `/superpowers-parallel-agents` - Concurrent task workflows

## Skill Types

**Rigid** (TDD, debugging): Follow exactly. Don't adapt away discipline.
**Flexible** (patterns): Adapt principles to context.

## User Instructions

Instructions say WHAT, not HOW. "Add X" or "Fix Y" doesn't mean skip workflows.
