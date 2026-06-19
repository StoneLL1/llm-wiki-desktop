---
title: Claude Code Self-Check Feedback Loop
created: 2026-06-04
updated: 2026-06-04
type: concept
sources:
  - raw/articles/2026-06-03-claude-code-self-check-feedback-loop.md
  - raw/articles/2026-06-03-claude-code-self-check-deep-dive.md
tags: [methodology, agent, engineering, workflow]
---

# Claude Code Self-Check Feedback Loop

## Definition

The Claude Code self-check feedback loop is a methodology for **encoding manual human checks into automated verification steps** that Claude Code executes before returning work. The core insight: shift from "Claude works → you check → Claude fixes → you recheck" (open loop) to "Claude works → Claude auto-runs checks → Claude self-fixes → checks pass → delivers to you" (closed loop).

**Origin**: Claude Devs (@ClaudeDevs) X/Twitter post (2.4K likes, June 2026) with a video demonstration of encoding manual checks into Claude Code's feedback loop.

## Traditional (Open Loop) vs Closed Loop

```
❌ Traditional:
  Claude works → hands to you → you check → find issues → tell Claude → Claude fixes → you recheck → ...

✅ Closed Loop:
  Claude works → Claude auto-runs checks → finds issues → Claude self-fixes → re-runs checks → all pass → hands to you
```

The essence is **shifting verification left**: you upgrade from "manual QA" to "designer of check rules."

## Three Implementation Levels

### Level 1: Inline Self-Check Rules (Simplest)

Embed check rules directly in prompts or [[claude-md|CLAUDE.md]]:

```markdown
After completing a task, automatically run these checks before submitting:
1. Run `npm test` — ensure all pass
2. Run `eslint` on all modified files
3. Confirm no console.log remnants
4. Verify all import paths exist
5. If any check fails, fix then re-run ALL checks
```

This is the most basic form — encoding your existing manual checks as rules Claude executes in a loop.

### Level 2: Skill-Level Self-Check Rules (Intermediate)

Encapsulate check logic into a reusable [[skills|Skill]]:

```markdown
# Code Review Skill
Before submitting code, automatically:
1. Security scan: hardcoded keys, SQL injection risks
2. Boundary checks: empty input, overflow input, concurrency scenarios
3. Style check: consistency with existing project code style
4. File structure: new files in correct directories
```

Every use of this Skill auto-runs the checks — consistent enforcement across the team.

### Level 3: Subagent Parallel Review (Advanced)

Recommended by community contributor @agenticrohan: launch **dual subagents** for parallel review after the main agent completes work:

```
Main Claude (does the work)
  ├─ Builds code
  ├─ Runs basic tests
  └─ Launches two review subagents:

  Subagent A (shared context)    Subagent B (fresh context)
  ├─ Full context awareness       ├─ Zero bias
  ├─ Understands design intent    ├─ Discovers blind spots
  └─ Logic review                 └─ Fresh-perspective review

  → Merge both review results → Fix all issues → Deliver to you
```

The dual-subagent pattern combines the strengths of context-aware review (understands intent) and context-free review (catches blind spots).

## Learnings Loop: Cross-Session Persistent Improvement

Beyond single-session closed loops, the **Learnings Loop** enables cross-session persistent improvement:

```
Use Skill → You give feedback → Feedback written into Skill instructions →
  Next use (by anyone) auto-applies corrections →
    Corrections accumulate → Skill improves continuously
```

### Flywheel Effect

> Better instructions → Better output → Less correction needed → Occasional correction → Even better instructions → Better output...

This creates a **positive flywheel** where Skills become more accurate with each use, encoding organizational knowledge over time.

## Dynamic Workflow Integration

Combining self-check loops with [[claude-code-dynamic-workflow]] creates a **mini CI/CD pipeline within Claude Code's context**:

```
Main Agent completes task
  → Trigger Dynamic Workflow
    → Spawn Reviewer Agent (specialized review)
    → Spawn Tester Agent (specialized testing)
    → Spawn Fixer Agent (fixes based on review + test results)
    → Reviewer re-checks
    → Pass → Deliver
```

This runs entirely within Claude Code — no Jenkins, no external CI required.

## Limitations & Criticisms

### Self-Analysis Reliability

Community member @AGIGuardian raised an important constraint: Claude's self-analysis capability is limited by its awareness constraints and guardrail-imposed priors. The model struggles with tasks requiring genuine self-reflection (e.g., naming itself in a report).

**Mitigation**: Self-checks work best as **objective, mechanical verifications** (running test suites, lint checks, type checking) rather than subjective self-reflection tasks. For subjective review, use **fresh-context subagents** or **multi-model cross-validation** (e.g., Claude builds + GPT 5.5 reviews).

### Community Techniques

| Technique | Source | Key Insight |
|-----------|--------|-------------|
| Dual-agent parallel review | @agenticrohan | Shared context (intent) + fresh context (bias-free) merged |
| Multi-model cross-validation | @aljosa | Claude implements + GPT 5.5 reviews + other models verify |
| Dynamic Workflow automation | @Layton_Gott | Subagent cluster automates the entire check pipeline |
| Encode manual checks | Claude Devs (official) | Write test/lint/typecheck as Claude auto-exec rules |

## Connection to AI-Native Engineering Org

The self-check methodology directly complements [[fiona-fung|Fiona Fung]]'s "trust but verify" framework for [[ai-native-development]]:

- **Fung**: Humans focus on legal, security, and product taste review
- **Self-check**: Claude auto-reviews style, lint, bugs, and tests

Both reflect the same paradigm shift: **humans move from executor to supervisor, from checking code to checking rules.** See [[claude-code]] for the broader engineering organization context.

## See Also

- [[claude-code]] — The platform this methodology applies to
- [[claude-code-hooks]] — Deterministic behavior control at workflow nodes
- [[claude-code-dynamic-workflow]] — JS-scripted multi-subagent orchestration
- [[harness-engineering]] — Systematic scaffolding for guiding AI model capabilities
- [[reflection-pattern]] — Agent self-evaluation and iterative improvement pattern
- [[fiona-fung]] — AI-native engineering org practices from Claude Code team lead
- [[ai-native-development]] — Development paradigm where AI is the primary coder
