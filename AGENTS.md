# AGENTS.md

`clakka` is a Rust macOS keyboard-sound engine that synthesizes key sounds in real time.

## Rules

- After completing your task, always run `scripts/ci.sh`.
- Don't write comments in code unless asked.
- Don't write tests unless asked.
- Latency from key press to sound must stay near instant: no allocation, lock, syscall or print on the audio thread or in the event tap path.
