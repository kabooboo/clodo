<p align="center"><img src="assets/logo.svg" width="128" alt=""></p>
<h1 align="center">clodo</h1>

Find and resume [Claude Code](https://claude.com/claude-code) sessions.

`clodo` lists your recent sessions in [fzf](https://github.com/junegunn/fzf): when each was last active, its
path, git branch and title, and Claude's own recap of what was going on. Describe what you remember and it
finds the session by meaning, even if you use none of its words. Press enter to `cd` into the session's
folder and `claude --resume` it.

![clodo demo: semantic search finds the session, ctrl-f filters by word](demo/demo.gif)

## Install

Needs `fzf` and `claude` on your `PATH`. Prebuilt binaries for macOS and Linux (x86_64, arm64) are on the
[releases page](https://github.com/kabooboo/clodo/releases):

```sh
target=aarch64-apple-darwin   # or x86_64-apple-darwin, x86_64-unknown-linux-musl, aarch64-unknown-linux-musl
curl -fsSL "https://github.com/kabooboo/clodo/releases/latest/download/clodo-$target.tar.gz" | tar -xz -C ~/.local/bin
```

Or from source:

```sh
cargo install --path .
```

## Use

```text
clodo [-n N] [QUERY...]
  -n N    how many recent sessions to list (0 = all, default: as many as fit the terminal)
  QUERY   start with a semantic search for QUERY
```

| Key    | Action                                                                  |
| ------ | ----------------------------------------------------------------------- |
| type   | semantic search over all sessions, runs when you pause (~4 s)           |
| ctrl-f | switch to fuzzy filtering of the recent list, and back                  |
| enter  | `cd` to the session's folder and run `claude --resume <id>`             |
| esc    | quit                                                                    |

The preview shows the resume command, every path and branch the session touched, linked PRs, the recap, your
last prompts and Claude's last reply.

## How it works

Sessions are the JSONL files under `~/.claude/projects/` (or `$CLAUDE_CONFIG_DIR/projects/`). The title is the `/rename` name or Claude's
generated title; the recap is the latest "while you were away" summary, or your last prompt when there is none.

Semantic search sends the title, path, branch, recap and your recent prompts of every session to
`claude -p --model haiku` and asks it to rank them. It runs with `--no-session-persistence`, so searches don't
show up as sessions.
