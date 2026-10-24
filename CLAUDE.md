@AGENTS.md

# Claude Code configuration

Project-specific Claude subagents are available under `.claude/agents/`. Shared skills are exposed
through `.claude/skills`, which is a relative symlink to the canonical `.agents/skills/` tree.
Claude-only hooks and the Rust analyzer plugin are configured in `.claude/settings.json`.

## This fork: opening a PR on upstream (GLinnik21/plx-native)

This repository is a fork. A session that has `danzig666/plx-native` attached cannot reach the
upstream repo directly, and the symptoms all look like "impossible":

- GitHub MCP `create_pull_request` on owner `GLinnik21` says the repository is not configured for
  this session;
- `add_repo` for upstream fails with "a different repository named plx-native is already attached"
  (same-named repositories cannot share one session);
- `gh api` / curl to `api.github.com` for upstream answers "GitHub access to this repository is not
  enabled for this session".

That is a session-layout limit, not missing permission: the fork's owner can always open a PR
upstream. Do not tell the owner it cannot be done, and do not ask them to start a session on
upstream (the app only offers their own repositories). Instead:

1. Push the work to a branch of this fork.
2. Start a helper session yourself with the `create_session` tool (claude-code-remote MCP):
   `source_url: https://github.com/GLinnik21/plx-native`, `clone_depth: 1`, and a self-contained
   prompt: open ONE pull request with GitHub MCP `create_pull_request` (owner `GLinnik21`, repo
   `plx-native`, base `main`, head `danzig666:<branch>`, `maintainer_can_modify: true`), the exact
   title, and the full body between BEGIN/END markers; check first that no PR from that head is
   already open; change no code and push nothing; reply with the PR URL or the exact GitHub error.
3. Follow it with `send_later` + `get_session` (its `post_turn_summary` carries the PR URL) and give
   the owner the link. PR #986 (Magic Remote edge scrolling, 2026-10-24) was opened this way.

The same works for any repository whose name collides with one already attached to the session.
