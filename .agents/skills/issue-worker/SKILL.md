---
name: issue-worker
description: Implement one unblocked Vibeshop issue as a tested, reviewable vertical slice.
---

Read AGENTS.md and the current repository before acting. Work in this run; do not promise future work.

1. Fetch main, open issues, open PRs, and current review requests. If there are more than ten open PRs, run the pr-triage skill instead. Choose the highest-priority, oldest unblocked issue without an existing implementation PR. Follow explicit dependencies; do not scaffold blocked features.
2. Claim `issue-N` with `scripts/lease.sh claim issue-N`. Keep the returned token, renew within 20 minutes, and release with a shell EXIT trap. If busy, choose another issue. The claim is shared across machines, not a local cache file.
3. Create `agent/issue-N-short-topic` from current main in a clean worktree. Read the owning code, existing tests, related issues, and recent commits. State the smallest complete user-visible change and identify what can be deleted. Do not grow scope into a framework.
4. Implement the action through UI, document/history, GPU rendering, and export as applicable. Add behavioral tests for meaningful failure paths. Never write a CPU substitute for the production GPU pipeline merely to make tests easy.
5. Run scripts/check.sh. For UI work run scripts/smoke.sh, inspect the actual image and exercise the controls; report missing interaction coverage. For a performance claim measure the affected workload with hardware, build, sample count, latency percentiles and memory. Do not invent numbers.
6. Renew/check ownership before pushing. Re-read origin main and open PRs; integrate relevant changes and retest. Push only your branch, never force-push main. Open a PR linked to the issue with exact tested SHA, commands/results, screenshots when relevant, and known limits. Do not mark incomplete work ready or self-approve it.
7. Release the issue lease after publication. Leave follow-up ownership to PR triage; do not start a second implementation for the same issue. If blocked, post the concrete blocker and evidence rather than fabricating success.

A draft PR is a checkpoint, not a completed issue. Close the issue through the merged PR only after acceptance criteria are met.
