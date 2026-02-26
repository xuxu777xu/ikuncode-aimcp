---
description: Use when encountering any bug, test failure, or unexpected behavior, before proposing fixes
---

# Systematic Debugging

## Overview

Random fixes waste time and create new bugs. Quick patches mask underlying issues.

**Core principle:** ALWAYS find root cause before attempting fixes. Symptom fixes are failure.

**Violating the letter of this process is violating the spirit of debugging.**

## The Iron Law

```
NO FIXES WITHOUT ROOT CAUSE INVESTIGATION FIRST
```

If you haven't completed Phase 1, you cannot propose fixes.

## When to Use

Use for ANY technical issue: test failures, bugs, unexpected behavior, performance problems, build failures, integration issues.

**Use this ESPECIALLY when:**
- Under time pressure (emergencies make guessing tempting)
- "Just one quick fix" seems obvious
- You've already tried multiple fixes
- Previous fix didn't work

## The Four Phases

You MUST complete each phase before proceeding to the next.

### Phase 1: Root Cause Investigation

**BEFORE attempting ANY fix:**

1. **Read Error Messages Carefully**
   - Don't skip past errors or warnings
   - Read stack traces completely
   - Note line numbers, file paths, error codes

2. **Reproduce Consistently**
   - Can you trigger it reliably?
   - What are the exact steps?
   - If not reproducible → gather more data, don't guess

3. **Check Recent Changes**
   - What changed that could cause this?
   - Git diff, recent commits
   - New dependencies, config changes

4. **Gather Evidence in Multi-Component Systems**
   - For EACH component boundary: log what enters, log what exits
   - Verify environment/config propagation
   - Run once to gather evidence showing WHERE it breaks
   - THEN analyze evidence to identify failing component

5. **Trace Data Flow (Root Cause Tracing)**
   - Where does bad value originate?
   - What called this with bad value?
   - Keep tracing up until you find the source
   - Fix at source, not at symptom

   **The Tracing Process:**
   1. Observe the symptom
   2. Find immediate cause (what code directly causes this?)
   3. Ask: What called this? What value was passed?
   4. Keep tracing up the call chain
   5. Find original trigger
   6. Fix at the source

### Phase 2: Pattern Analysis

1. **Find Working Examples** - Locate similar working code in same codebase
2. **Compare Against References** - Read reference implementation COMPLETELY
3. **Identify Differences** - List every difference, however small
4. **Understand Dependencies** - What other components/config/environment does this need?

### Phase 3: Hypothesis and Testing

1. **Form Single Hypothesis** - "I think X is the root cause because Y"
2. **Test Minimally** - Make the SMALLEST possible change to test hypothesis
3. **Verify Before Continuing** - Didn't work? Form NEW hypothesis. DON'T add more fixes on top.
4. **When You Don't Know** - Say "I don't understand X". Don't pretend to know.

### Phase 4: Implementation

1. **Create Failing Test Case** - MUST have before fixing. Use `/superpowers-tdd`
2. **Implement Single Fix** - ONE change at a time. No "while I'm here" improvements.
3. **Verify Fix** - Test passes? No other tests broken? Issue actually resolved?
4. **If Fix Doesn't Work** - Count fixes tried. If < 3: return to Phase 1. **If ≥ 3: STOP and question architecture.**
5. **If 3+ Fixes Failed: Question Architecture**
   - Each fix reveals new coupling/problem in different place?
   - Fixes require "massive refactoring"?
   - Each fix creates new symptoms elsewhere?
   - **STOP and discuss with user before attempting more fixes**

## Defense-in-Depth (After Finding Root Cause)

When you fix a bug, validate at EVERY layer data passes through:

- **Layer 1: Entry Point** - Reject obviously invalid input at API boundary
- **Layer 2: Business Logic** - Ensure data makes sense for this operation
- **Layer 3: Environment Guards** - Prevent dangerous operations in specific contexts (e.g. refuse git init outside tmpdir in tests)
- **Layer 4: Debug Instrumentation** - Capture context for forensics

**Single validation: "We fixed the bug." Multiple layers: "We made the bug impossible."**

## Condition-Based Waiting (For Flaky Tests)

Replace arbitrary timeouts with condition polling:

```typescript
// ❌ BEFORE: Guessing at timing
await new Promise(r => setTimeout(r, 50));

// ✅ AFTER: Waiting for condition
await waitFor(() => getResult() !== undefined);
```

**Use when:** tests have arbitrary delays, are flaky, or timeout in parallel.

## Red Flags - STOP and Follow Process

- "Quick fix for now, investigate later"
- "Just try changing X and see if it works"
- "Add multiple changes, run tests"
- "It's probably X, let me fix that"
- "I don't fully understand but this might work"
- Proposing solutions before tracing data flow
- **"One more fix attempt" (when already tried 2+)**

**ALL of these mean: STOP. Return to Phase 1.**

## Common Rationalizations

| Excuse | Reality |
|--------|---------|
| "Issue is simple, don't need process" | Simple issues have root causes too. Process is fast for simple bugs. |
| "Emergency, no time for process" | Systematic debugging is FASTER than guess-and-check thrashing. |
| "Just try this first, then investigate" | First fix sets the pattern. Do it right from the start. |
| "Multiple fixes at once saves time" | Can't isolate what worked. Causes new bugs. |
| "I see the problem, let me fix it" | Seeing symptoms ≠ understanding root cause. |
| "One more fix attempt" (after 2+ failures) | 3+ failures = architectural problem. Question pattern, don't fix again. |

## Quick Reference

| Phase | Key Activities | Success Criteria |
|-------|---------------|------------------|
| **1. Root Cause** | Read errors, reproduce, check changes, gather evidence | Understand WHAT and WHY |
| **2. Pattern** | Find working examples, compare | Identify differences |
| **3. Hypothesis** | Form theory, test minimally | Confirmed or new hypothesis |
| **4. Implementation** | Create test, fix, verify | Bug resolved, tests pass |
