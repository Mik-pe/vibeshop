---
name: pr-triage
description: Independently review and merge ready Vibeshop PRs; repair failures without colliding with other agents.
---

Read AGENTS.md. Fetch current PR metadata, diff, head/base SHAs, checks, reviews and unresolved threads. Do not act from an old queue snapshot.

## Work selection

If the runner supports subagents and there is enough work, use two lanes: (A) review/merge and (B) repair/rebase/CI fixes. Without subagents, execute those lanes sequentially. Do not invent unavailable delegation. Both lanes claim the same `pr-N` key with scripts/lease.sh so they never mutate/review the same PR concurrently. Other PRs remain available. Renew within 20 minutes; check the lease before every remote write; release on exit. Stop when ownership is lost.

## Review and merge

Review every eligible PR, not just the first. Check out the exact head in an isolated worktree. An authoring session cannot be its own independent reviewer. Read actual behavior and all changed code, look for unnecessary systems and regressions, and run the relevant checks. Verify the UI/pixel/export path rather than accepting a screenshot or green compile as proof of everything.

Record a substantive review tied to the exact head SHA: findings, tests actually executed, limitations, and decision. GitHub may reject self-approval when different agents share a login; a truthful independent-session COMMENT can record review evidence, but it must not be an author-written pretend approval. Required GitHub approvals still require a separate authorized identity.

Before merging, refresh head/base, draft state, every required check and actionable review thread. Missing, pending, skipped, failed or stale checks are not success. A changed head requires review of the new diff and fresh validation. Merge only when independently reviewed current-head work meets the issue and has no known blocker. Use `gh pr merge N --squash --match-head-commit SHA` or an API merge with expected-head SHA. Do not use admin bypass, force main, or weaken repository rules.

## Repair

Read the actual failure logs and review findings. Repair the smallest cause; do not disable a test, inflate tolerances without justification, delete evidence, or approve over a blocker. Rebase onto current main only while holding the PR lease; use an explicit expected-old-head force-with-lease on that PR branch if rewriting is necessary. Run checks again, publish the exact new SHA and request fresh review. Resolve only threads whose concerns are demonstrably addressed.

Finish with PRs reviewed, repaired, merged, or blocked, plus evidence and remaining work. No future/background promises and no extra issue creation when the PR already owns the work.
