@AGENTS.md

# Claude Code configuration

Project-specific Claude subagents are available under `.claude/agents/`. Shared skills are exposed
through `.claude/skills`, which is a relative symlink to the canonical `.agents/skills/` tree.
Claude-only hooks and the Rust analyzer plugin are configured in `.claude/settings.json`.

## Worktree gotchas (Claude sessions)

- The Bash tool's "Shell cwd was reset to …" can leave you in the MAIN checkout (`/Users/…/plx-native`), not your worktree: a bare `git checkout -B x` there moves the owner's checkout off `main`. Start branch-switching commands with `cd <your worktree> &&` (or `git -C`), and confirm with `git branch --show-current`.
- A branch an `isolation: "worktree"` subagent created stays checked out in its worktree, so you cannot `git checkout` it elsewhere: `git checkout -B tmp origin/<branch>`, then `git push origin HEAD:<branch>`.
- Finished lanes: `tools/build-gc.sh --worktrees -n` previews. It judges a lane against `origin/main`, so a squash-merged lane reads removable once `git fetch` has brought the squash in; it still reads "unmerged" while `origin/main` predates the squash, or after `main` has changed that lane's lines again. A clean lane (`git status --short` empty) can be removed with plain `git worktree remove <path>` (no `--force`), which keeps its branch but takes the lane's IGNORED files too (`git status --short --ignored` lists them; `--worktrees -n` lists the ones that are not build output, e.g. `src/config.local.h`, and `--auto` keeps such a lane). The auto-mode classifier may refuse a batch removal until the owner says yes.
- The tv-lock guard hook gives each device command the first lane it can read, in this order: a `PLX_TV_LOCK_LANE=<worktree>` prefix (it names the lane of the WHOLE command and is read only at the very START of the Bash command, optionally after `env ` or one `cd <dir> &&`; behind `sleep;` or a second statement it is not seen), that variable in the hook's environment, the command's own absolute `tools/`/`tests/` script path or `make -C <dir>`, an absolute `cd <dir>` the shell certainly kept, then the cwd's checkout (`decide()` in `.claude/hooks/tv-lock-guard.py`). A plain subagent's cwd is the SESSION's checkout, so a relative script path or a `cd "$W"` still needs the prefix, and a lane read from a script path or a `cd` that is not the cwd's own also needs your session in the lease's `SESSIONS`. Run `tv-lock.sh acquire` as its own call, then one device command per call.

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
