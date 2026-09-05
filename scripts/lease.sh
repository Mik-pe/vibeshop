#!/usr/bin/env bash
set -euo pipefail
command=${1:?Usage: lease.sh claim|check|renew|release issue-N|pr-N [token]}
key=${2:?Missing task key}
[[ "$key" =~ ^(issue|pr)-[1-9][0-9]*$ ]] || { echo 'Invalid task key' >&2; exit 2; }
case "$command" in claim|check|renew|release) ;; *) echo 'Invalid lease command' >&2; exit 2;; esac
ref="refs/heads/agent-lease/$key"
ttl=${VIBESHOP_LEASE_TTL:-1800}
[[ "$ttl" =~ ^[0-9]+$ ]] && (( ttl >= 60 && ttl <= 3600 )) || { echo 'TTL must be 60..3600 seconds' >&2; exit 2; }
now=$(date -u +%s)
old=''
if remote=$(git ls-remote --exit-code origin "$ref"); then
    old=${remote%%[[:space:]]*}
    git fetch --quiet --no-tags origin "$ref"
    fetched=$(git rev-parse FETCH_HEAD)
    [[ "$old" == "$fetched" ]] || { echo 'Lease moved; retry' >&2; exit 1; }
    body=$(git show -s --format=%B "$old")
    owner=$(printf '%s\n' "$body" | sed -n 's/^token=//p')
    expires=$(printf '%s\n' "$body" | sed -n 's/^expires=//p')
    [[ "$expires" =~ ^[0-9]+$ && "$owner" =~ ^[a-f0-9]{32}$ ]] || { echo 'Unknown remote lease format; refusing to steal it' >&2; exit 1; }
else
    status=$?
    (( status == 2 )) || exit "$status"
fi
if [[ "$command" == claim ]]; then
    if [[ -n "$old" ]] && (( expires > now )); then echo 'Task already leased' >&2; exit 1; fi
    token=$(od -An -N16 -tx1 /dev/urandom | tr -d ' \n')
else
    token=${3:?Missing lease token}
    [[ -n "$old" && "$token" == "$owner" ]] || { echo 'Lease ownership lost' >&2; exit 1; }
    if [[ "$command" != release ]] && (( expires <= now )); then echo 'Lease expired; stop work' >&2; exit 1; fi
fi
if [[ "$command" == check ]]; then
    # Leave a publication margin rather than starting a write at the expiry boundary.
    (( expires > now + 30 )) || { echo 'Renew before publishing' >&2; exit 1; }
    exit 0
fi
if [[ "$command" == release ]]; then
    git push --quiet --force-with-lease="$ref:$old" origin ":$ref"
    exit 0
fi
new=$(printf 'Vibeshop task lease\ntoken=%s\nexpires=%s\n' "$token" "$((now + ttl))" |
    git -c user.name='Vibeshop agent' -c user.email='agent@vibeshop.invalid' commit-tree 'HEAD^{tree}' -p HEAD)
git push --quiet --force-with-lease="$ref:$old" origin "$new:$ref"
printf '%s\n' "$token"
