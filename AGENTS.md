# Ferricket contributor guide

Ferricket is an async Rust port of the ticket file-native issue tracker. The package is ferricket and the installed executable is fer.

## Compatibility contract

- Keep .tickets/*.md compatible with the original ticket YAML-frontmatter format.
- Existing ticket repositories must work without migration.
- Partial IDs match anywhere in an ID; exact matches take precedence and ambiguous matches fail.
- Keep established command output stable unless a Ferricket name is required (ferricket, fer, fer-*, and FER_SCRIPT).
- External plugins are checked in this order: fer-command, then ticket-command. They receive TICKETS_DIR and FER_SCRIPT.

## Architecture

- src/storage.rs: async ticket discovery, parsing, mutation, relationships, and JSON/web projection.
- src/query.rs: shared Gmail-style query semantics for the TUI; keep it behaviorally aligned with web/src/features/filters/filter-types.ts.
- src/preferences.rs: async per-workspace user preferences stored outside ticket repositories.
- src/session_activity.rs: local-only Copilot CLI prompt capture and workspace session activity.
- src/github.rs: async `gh`-backed GitHub.com/GHE issue and pull-request reference discovery.
- src/cli.rs: argh command definitions, compatibility output, plugin dispatch, and command handlers.
- src/ui.rs: Axum API and embedded static-asset server used by fer ui.
- src/tui.rs: Ratatui terminal interface used by fer tui; keep mutations on async storage APIs.
- web/src/api: typed HTTP client functions.
- web/src/hooks: frontend state and synchronization.
- web/src/components/layout: reusable application shell components.
- web/src/components/ui: shadcn-style UI primitives.
- web/src/features/tickets: ticket-specific views and dialogs.
- web/src/features/filters: typed advanced filter state, condition matching, and Base UI filter controls.
- web/dist: production assets embedded into the Rust binary at compile time.

All filesystem, process, and network work in Rust should be async by default. Use Tokio APIs and bounded concurrent reads where appropriate. Do not add a database or cache that competes with .tickets as the source of truth.

Ticket mutations exposed over the web must use content-derived revisions and require `If-Match`; never silently overwrite a file that changed on disk. Filesystem watching is enabled by default for `fer ui` and `fer tui`, must use `RecursiveMode::NonRecursive`, and must publish changes only when direct Markdown children of the resolved `.tickets` directory change.

## Frontend workflow

The frontend was initialized with:

~~~sh
bun init --react=shadcn --yes web
~~~

It uses React, shadcn's Base Nova style, Base UI primitives, Tailwind CSS 4, and Bun's native bundler. Keep reusable primitives in components/ui; do not accumulate feature logic in App.tsx. Use Tailwind utility classes for component layout and feature styling; keep styles.css limited to theme tokens, font wiring, and global base rules. Do not add Radix UI dependencies.

~~~sh
mise install
make build
~~~

Commit web/dist because Cargo embeds it with include_dir. Rebuild it after every frontend change before compiling or packaging Rust.

## Verification

Run before handing off changes:

~~~sh
make check
~~~

For compatibility changes, run the upstream Behave suite against target/debug/fer.

Update README.md whenever commands, flags, the ticket format, or the frontend workflow changes.
