#!/usr/bin/env bash
set -euo pipefail
script=$(cd "$(dirname "$0")" && pwd)/lease.sh
root=$(mktemp -d)
trap 'rm -rf "$root"' EXIT
git init --bare -q "$root/origin.git"
git init -q "$root/a"
cd "$root/a"
git config user.name test; git config user.email test@example.invalid
git commit --allow-empty -qm initial
git remote add origin "$root/origin.git"
git push -q origin HEAD:main
git -C "$root/origin.git" symbolic-ref HEAD refs/heads/main
git clone -q "$root/origin.git" "$root/b"
first=$("$script" claim issue-2)
"$script" check issue-2 "$first"
cd "$root/b"
if "$script" claim issue-2; then echo 'Duplicate claim succeeded' >&2; exit 1; fi
if "$script" release issue-2 deadbeef; then echo 'Non-owner released lease' >&2; exit 1; fi
second=$("$script" claim pr-3)
"$script" renew pr-3 "$second" >/dev/null
"$script" release pr-3 "$second"
cd "$root/a"
"$script" release issue-2 "$first"
if "$script" check issue-2 "$first"; then echo 'Deleted lease remained valid' >&2; exit 1; fi
# Simulate a crashed worker's expired lease without a timing-dependent sleep.
expired=$(printf 'Vibeshop task lease\ntoken=0123456789abcdef0123456789abcdef\nexpires=1\n' | git commit-tree 'HEAD^{tree}' -p HEAD)
git push -q origin "$expired:refs/heads/agent-lease/issue-2"
cd "$root/b"
replacement=$("$script" claim issue-2)
if "$script" release issue-2 0123456789abcdef0123456789abcdef; then echo 'Expired owner released replacement' >&2; exit 1; fi
"$script" release issue-2 "$replacement"
printf 'Lease ownership, separate tasks, renewal, expiry and safe release passed.\n'
