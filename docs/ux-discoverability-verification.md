# UX discoverability verification

Resumed the existing Claude session's unfinished work on
`feature/ux-discoverability`.

## Verified

- `cargo test --workspace`: 225 passing tests; two display-dependent tests
  excluded from the default run.
- Both display-dependent regression tests passed separately on an isolated
  GTK Broadway display: chapter reload clears stale hover state, and a follow
  control can return from the guest header to its navigation bar.
- `cargo build -p bible-app`, `cargo fmt -p bible-app -- --check`, and
  `git diff --check` pass.
- Reviewed the previous session's dictionary-picker screenshots, including
  Easton's suggestions for “grace”.
- Inspected the native app log: GTK allocation/snapshot and GDK frame-timing
  warnings exist; no crash was shown. The only Docker container is an unrelated
  study web app, whose recent logs contain successful health checks.

Run each GTK regression in its own process because GTK requires initialization
on a single thread. Start `gtk4-broadwayd :9`, then run:

```sh
GDK_BACKEND=broadway BROADWAY_DISPLAY=:9 cargo test -p bible-app loading_a_chapter_clears_the_previous_hover_even_on_error -- --ignored --test-threads=1
GDK_BACKEND=broadway BROADWAY_DISPLAY=:9 cargo test -p bible-app promoted_follow_control_returns_to_its_navigation_bar -- --ignored --test-threads=1
```

## Still needs interactive verification

- Narrow-window layouts, especially the dictionary picker and split panes.
- Complete keyboard-only study flows and dictionary entry activation after
  switching dictionaries.

The desktop app was subsequently restarted with the rebuilt binary for review.
