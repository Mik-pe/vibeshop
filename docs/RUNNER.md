# Self-hosted validation

CI selects `[self-hosted, Linux, X64, vibeshop]`, the labels supplied for `mikparchy-vibeshop`. The run evidence records the actual runner name and tested commit. Labels select runners; a machine name alone is not a routing label.

## Trust boundary

CI runs on pushes to this repository's `main` and `agent/**` branches, or a maintainer's manual dispatch. There is no `pull_request` or `pull_request_target` trigger. Fork PRs are not automatically executed by this workflow. A maintainer must review external code before explicitly putting it on a trusted branch. Never approve an external workflow just to obtain a green check.

This workflow is not a sandbox. Code from authorized writers and their dependencies executes as the runner user. Use a dedicated, unprivileged account or disposable VM without personal files, SSH agents, cloud credentials, Docker socket access or unrelated network access. Do not attach a personal interactive account to a public repository and assume a YAML condition secures it. GitHub-side runner access restrictions, approval settings and branch protection still require owner configuration (issue #11).

The GitHub token is read-only and checkout does not persist it. No deployment secrets are passed. Tests use generated fixtures and a separate Xvfb display. System packages are never installed automatically, and CI never uses sudo. Cargo's normal local cache is reused; no hosted cache service or alternate CI implementation is needed.

Before merging, verify the CI run's `head_sha` matches the PR head and that it completed successfully on this repository. Push-triggered checks validate the exact branch commit, not a synthetic merge commit: integrate current main and rerun when main changes. Missing or skipped CI is not approval. Independent review is still required by AGENTS.md.

## Machine prerequisites

Install Rust through rustup, a working Vulkan implementation (hardware or Mesa software), native build tools, pkg-config, xkbcommon including its X11 library, Wayland client libraries, Xvfb, xauth, xdotool and ImageMagick's import/convert commands. The pinned Rust toolchain is installed by rustup on first use.

On an Arch-based runner the corresponding system packages include `base-devel`, `pkgconf`, `libxkbcommon`, `libxkbcommon-x11`, `wayland`, `vulkan-icd-loader`, a Vulkan driver appropriate for the machine, `xorg-server-xvfb`, `xorg-xauth`, `xdotool` and `imagemagick`. Choose the GPU driver for the actual hardware; do not replace the host driver from CI.

Run `scripts/check.sh` and `scripts/smoke.sh` as the runner account to validate setup. Tests never silently skip GPU work when no adapter exists. Software Vulkan demonstrates correctness, not physical-GPU performance.

The old hello-world runner smoke is manual-only. `CI / verify` is the substantive build, GPU, lint, coordination and native-interaction check.
