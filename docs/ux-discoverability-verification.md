# UX discoverability verification

Resumed the existing Claude session's unfinished work on
`feature/ux-discoverability`.

## Verified

- `cargo test --workspace`: 226 passing tests; three display-dependent tests
  excluded from the default run.
- All three display-dependent regression tests passed separately on an isolated
  GTK Broadway display: chapter reload clears stale hover state, and a follow
  control can return from the guest header to its navigation bar; the explicit
  Move to tab action preserves Search's query, results, and selection.
- `cargo build -p bible-app`, `cargo fmt -p bible-app -- --check`, and
  `git diff --check` pass.
- Reviewed the previous session's dictionary-picker screenshots, including
  Easton's suggestions for “grace”.
- The latest restarted native app log was empty after the replacement and
  promotion checks. Earlier runs had GTK allocation/snapshot and GDK frame-timing
  warnings. The only Docker container is an unrelated
  study web app, whose recent logs contain successful health checks.

Run each GTK regression in its own process because GTK requires initialization
on a single thread. Start `gtk4-broadwayd :9`, then run:

```sh
GDK_BACKEND=broadway BROADWAY_DISPLAY=:9 cargo test -p bible-app loading_a_chapter_clears_the_previous_hover_even_on_error -- --ignored --test-threads=1
GDK_BACKEND=broadway BROADWAY_DISPLAY=:9 cargo test -p bible-app promoted_follow_control_returns_to_its_navigation_bar -- --ignored --test-threads=1
GDK_BACKEND=broadway BROADWAY_DISPLAY=:9 cargo test -p bible-app side_pane_move_action_preserves_search_widgets_and_results -- --ignored --test-threads=1
```

## Still needs interactive verification

- Narrow-window layouts, especially the dictionary picker and split panes.
- Complete keyboard-only study flows and dictionary entry activation after
  switching dictionaries.

The desktop app was subsequently restarted with the rebuilt binary for review.

The search return button was later removed at the user's request, along with
its two dedicated tests. Existing Esc behavior remains unchanged. The removal
passed workspace tests, the build, and formatting checks.

## Side-pane replacement follow-up

- Opening another study view closes the previous side pane, without implicitly
  moving it to the tab bar or restoring Search's original passage.
- The explicit Move to tab header button reparents the existing view into a
  regular tab in the same window.
- Verified in the running desktop app at 952px width: searched for “dream”,
  moved Search to a regular tab (23 results and the selection retained), opened
  a second Search beside Genesis 41, and selected Matthew Henry from the verse
  menu. Henry replaced only the side-pane Search; the explicitly saved Search
  tab remained. No nested Search/Henry switcher appeared.
- The running desktop app includes these follow-up edits.
