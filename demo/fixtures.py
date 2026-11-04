#!/usr/bin/env python3
"""Write made-up Claude Code sessions into CLAUDE_CONFIG_DIR-style folder $1, for recording the demo."""
import json
import os
import sys
import time
import uuid
from datetime import datetime, timezone

SESSIONS = [
    (2, "acme-api", "fix/session-expiry", "Session cookie expiry bug",
     "You're fixing users being logged out after five minutes: the API sets the session cookie's Max-Age in "
     "seconds but the gateway reads it as minutes. Next: add a regression test and ship the fix.",
     ["users report being logged out after a few minutes, can you find why?",
      "check the gateway config too", "write a regression test for the cookie max-age"],
     "The gateway multiplies Max-Age by 60 before forwarding. I removed the conversion and added a test.", 412),
    (25, "acme-api", "perf/orders-index", "Orders endpoint p99 at 4s",
     "You're speeding up GET /orders: EXPLAIN ANALYZE shows a sequential scan on orders(tenant_id, created_at). "
     "Next: ship the composite index migration with CONCURRENTLY.",
     ["the orders endpoint is super slow in prod, p99 is 4 seconds", "run explain analyze on the main query",
      "add the index as a migration"],
     "Adding the composite index brings the query from 3.8 s to 9 ms on a copy of the prod data.", None),
    (70, "webapp", "feat/dark-mode", "Dark mode toggle",
     "You're adding a dark mode toggle: colours moved to CSS variables, the toggle persists in localStorage. "
     "Next: fix the chart colours that ignore the theme.",
     ["add a dark mode toggle in the header", "the charts stay white in dark mode"],
     "Charts now read their colours from the CSS variables, so they follow the theme.", 418),
    (190, "infra", "main", "Pods OOMKilled after deploy",
     "You're investigating worker pods OOMKilled since Tuesday's deploy: the new JVM ignores the container "
     "limit. Next: set -XX:MaxRAMPercentage=75 and raise the limit to 1Gi.",
     ["workers keep restarting since the last deploy", "is it the memory limit or a leak?"],
     "It's not a leak: heap is sized from the node's memory, not the container's. MaxRAMPercentage fixes it.", None),
    (310, "webapp", "main", "Flaky checkout e2e test",
     "You're fixing the checkout e2e test that fails one run in five: it clicks Pay before the price request "
     "returns. Next: wait on the network response instead of a fixed timeout.",
     ["the checkout e2e test is flaky again", "can we avoid sleeps entirely?"],
     "Replaced the 2 s sleep with a wait on the /price response. 50 runs in a row pass.", None),
    (1500, "dotfiles", "main", "Slow zsh startup",
     "You cut zsh startup from 1.2 s to 180 ms by lazy-loading nvm and caching compinit. Next: nothing, done.",
     ["my terminal takes forever to open", "profile zsh startup"],
     "nvm was 900 ms of it. It now loads on the first call to node, npm or nvm.", None),
    (1600, "clodo", "main", "Session picker in Rust",
     "You're building clodo, an fzf picker for Claude Code sessions with semantic search. Next: record a demo GIF.",
     ["build a CLI to find and resume my Claude sessions", "do it in rust just for the fun of it"],
     "The picker lists sessions with their recap and resumes the one you pick.", None),
    (2900, "acme-api", "feat/rate-limit", "Per-tenant rate limiting",
     "You're adding per-tenant rate limits with a Redis token bucket. Next: return Retry-After on 429s.",
     ["one tenant is hammering the API, add rate limiting per tenant", "use redis so it works across replicas"],
     "Token bucket in a Lua script, so refill and take happen atomically.", 405),
    (4400, "data-pipeline", "main", "Nightly ETL skips rows",
     "You found why the nightly ETL drops about 3% of rows: the date filter compares UTC timestamps with local "
     "dates. Next: backfill the last 30 days.",
     ["the nightly job is missing rows, totals don't match", "which rows are missing?"],
     "Rows created between midnight and 2 am local time fall outside the UTC window.", None),
    (5800, "infra", "chore/terraform-upgrade", "Terraform provider upgrade",
     "You're upgrading the cloud provider from v4 to v5: two renamed arguments, plan is now clean. Next: apply on staging.",
     ["upgrade the terraform provider to v5"],
     "Plan shows no changes after renaming the two deprecated arguments.", None),
    (8700, "webapp", "fix/i18n-plurals", "French plural forms",
     "You fixed French plurals: 0 takes the singular in French, unlike English. Next: check the other locales.",
     ["the french translation says 0 articles, should be 0 article"],
     "Switched to Intl.PluralRules, which knows each locale's rule.", None),
    (12000, "blog", "main", "Post: it was DNS",
     "You're drafting a blog post about the outage caused by a 300 s DNS TTL during a failover. Next: add the timeline.",
     ["help me write a post about last week's outage", "make the intro punchier"],
     "Draft ready: hook, timeline, what we changed, and the TTL lesson.", None),
    (21000, "ml-sandbox", "main", "Classify support tickets",
     "You trained a small classifier routing support tickets to teams, 91% accuracy on the held-out set. "
     "Next: look at the billing vs. account confusion.",
     ["can we auto-route support tickets to the right team?", "show me the confusion matrix"],
     "Most errors are billing tickets labelled account: the two teams share half their keywords.", None),
]


def main(root):
    now = time.time()
    for minutes, repo, branch, title, recap, prompts, reply, pr in SESSIONS:
        cwd = f"/Users/dev/code/{repo}"
        folder = os.path.join(root, "projects", cwd.replace("/", "-"))
        os.makedirs(folder, exist_ok=True)
        sid = str(uuid.uuid5(uuid.NAMESPACE_URL, title))
        t = now - minutes * 60 - 3600
        lines = []
        for i, p in enumerate(prompts):
            ts = datetime.fromtimestamp(t + i * 600, timezone.utc).isoformat().replace("+00:00", "Z")
            base = {"cwd": cwd, "gitBranch": branch, "sessionId": sid, "timestamp": ts}
            lines.append({"type": "user", "message": {"role": "user", "content": p}, **base})
            lines.append({"type": "last-prompt", "lastPrompt": p, "sessionId": sid})
            answer = reply if i == len(prompts) - 1 else "Looking into it."
            lines.append({"type": "assistant", "message": {"content": [{"type": "text", "text": answer}]}, **base})
        lines.append({"type": "ai-title", "aiTitle": title, "sessionId": sid})
        if pr:
            lines.append({"type": "pr-link", "prUrl": f"https://github.com/acme/{repo}/pull/{pr}", "sessionId": sid})
        lines.append({"type": "system", "subtype": "away_summary", "content": recap, "cwd": cwd, "gitBranch": branch})
        path = os.path.join(folder, f"{sid}.jsonl")
        with open(path, "w") as f:
            f.write("\n".join(json.dumps(line, separators=(",", ":")) for line in lines) + "\n")
        os.utime(path, (now - minutes * 60, now - minutes * 60))


if __name__ == "__main__":
    main(sys.argv[1])
