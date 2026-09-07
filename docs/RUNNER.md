# Self-hosted validation

CI selects `[self-hosted, Linux, X64, vibeshop]`, the labels supplied for `mikparchy-vibeshop`. The run evidence records the actual runner name and tested commit. Labels select runners; a machine name alone is not a routing label.

## Trust boundary

CI runs on pushes to this repository's `main` and `agent/**` branches, or a maintainer's manual dispatch. There is no `pull_request` or `pull_request_target` trigger. Fork PRs are not automatically executed by this workflow. A maintainer must review external code before explicitly putting it on a trusted branch. Never approve an external workflow just to obtain a green check.

This workflow is not a sandbox. Code from authorized writers and their dependencies executes as the runner user. Use a dedicated, unprivileged account or disposable VM without personal files, SSH agents, cloud credentials, Docker socket access or unrelated network access. Do not attach a personal interactive account to a public repository and assume a YAML condition secures it. Runner isolation and GitHub-side runner access restrictions still require owner verification (issue #11). Main-branch enforcement is documented below.

The GitHub token is read-only and checkout does not persist it. No deployment secrets are passed. Tests use generated fixtures and a separate Xvfb display. System packages are never installed automatically, and CI never uses sudo. Cargo's normal local cache is reused; no hosted cache service or alternate CI implementation is needed.

Before merging, verify the CI run's `head_sha` matches the PR head and that it completed successfully on this repository. Push-triggered checks validate the exact branch commit, not a synthetic merge commit: integrate current main and rerun when main changes. Missing or skipped CI is not approval. Independent review is still required by AGENTS.md.

## Machine prerequisites

Install Rust through rustup, a working Vulkan implementation (hardware or Mesa software), native build tools, pkg-config, xkbcommon including its X11 library, Wayland client libraries, Xvfb, xauth, xdotool and ImageMagick's import/convert commands. The pinned Rust toolchain is installed by rustup on first use.

On an Arch-based runner the corresponding system packages include `base-devel`, `pkgconf`, `libxkbcommon`, `libxkbcommon-x11`, `wayland`, `vulkan-icd-loader`, a Vulkan driver appropriate for the machine, `xorg-server-xvfb`, `xorg-xauth`, `xdotool` and `imagemagick`. Choose the GPU driver for the actual hardware; do not replace the host driver from CI.

Run `scripts/check.sh` and `scripts/smoke.sh` as the runner account to validate setup. Tests never silently skip GPU work when no adapter exists. Software Vulkan demonstrates correctness, not physical-GPU performance.

The old hello-world runner smoke is manual-only. `CI / verify` is the substantive build, GPU, lint, coordination and native-interaction check.


## Main-branch enforcement and review identity

Ruleset [22436524](https://github.com/Mik-pe/vibeshop/rules/22436524) was activated and read back on 2026-09-07. Its configuration is recorded in
[`.github/main-ruleset.json`](../.github/main-ruleset.json). It applies only to
`refs/heads/main`, requires a pull request and the `verify` check from the GitHub
Actions integration (ID 15368), requires current main in the tested branch, and
requires actionable review threads to be resolved. Force-pushes and branch
deletion are blocked. The bypass-actor list is empty, including for admins.

The ruleset does **not** enforce an independent approving GitHub account. The
current agents share a login, which cannot approve its own PR; the required
GitHub approving-review count is explicitly zero. AGENTS.md still requires an
independent agent/session to review the exact current head and record a
substantive COMMENT with its evidence before a guarded merge. Such a comment is
workflow evidence, not an independent account approval or a server-enforced
review gate. Once the owner supplies a separate authorized reviewer identity,
require at least one approval and last-push approval. Do not impersonate that
identity or add bypass actors to make a blocked merge pass.

Check actual hosted enforcement instead of assuming this file applies itself:

```sh
gh api repos/Mik-pe/vibeshop/rules/branches/main
gh api repos/Mik-pe/vibeshop/rulesets
```

An authorized administrator can create the ruleset with the documented GitHub
[repository rules API](https://docs.github.com/en/rest/repos/rules#create-a-repository-ruleset)
and this JSON. Subsequent changes must update the existing ruleset ID rather
than create duplicates. Reading back active rules is non-destructive; do not
try force-pushing or deleting real main to test enforcement. The next reviewed,
current-head green PR merge demonstrates the normal guarded path.

Issue #11 remains open for independent account enforcement, runner-isolation and
scheduler verification, and disposable remote coordination/stale-head rejection
experiments. `scripts/test-leases.sh` covers lease ownership, renewal and expiry
against local disposable Git remotes; it does not prove scheduled remote agents
or a separate review identity exist.
