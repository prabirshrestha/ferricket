# Ferricket

Ferricket is a fast, file-native issue tracker for people and coding agents. The Cargo package is `ferricket`, the CLI is `fer`, and the source of truth is compatible Markdown in `.tickets/`—no database or daemon.

It includes an async Rust CLI, a keyboard-first terminal UI, and a Linear-style React web UI embedded in the same binary.

## Install

~~~sh
make install
~~~

This builds the embedded frontend and installs `fer` into the local Cargo bin directory,
normally `~/.cargo/bin`.

## Start a workspace

~~~sh
cd your-project
fer init
fer create "Add authentication" --priority 1 --tags backend,security
fer create "Build login form" --parent fer-ab12
fer dep fer-ab12 fer-cd34
fer ready
~~~

`fer init [path]` creates `.tickets/` and adds an idempotent Ferricket section to `AGENTS.md`. It preserves any existing `AGENTS.md` content. Existing ticket repositories work without initialization or migration.

Ferricket searches the current directory and its parents for `.tickets/`. Set `TICKETS_DIR` to use an explicit location. Ticket commands accept exact IDs or unambiguous partial IDs.

## Everyday commands

~~~text
fer init [path]              Initialize .tickets and AGENTS.md
fer create [title] [options] Create a ticket and print its ID
fer show <id>                Display a ticket and its relationships
fer edit <id>                Open the Markdown file in $EDITOR
fer start <id>               Set status to in_progress
fer close <id>               Set status to closed
fer reopen <id>              Set status to open
fer status <id> <status>     Set open, in_progress, or closed
fer add-note <id> [text]     Append a timestamped note; accepts stdin
fer dep <id> <dep-id>        Add a dependency
fer undep <id> <dep-id>      Remove a dependency
fer dep tree [--full] <id>   Show a dependency tree
fer dep cycle                Find active dependency cycles
fer link <id> <id> [id...]   Link tickets symmetrically
fer unlink <id> <id>         Remove a symmetric link
fer ls|list [filters]        List tickets
fer ready|blocked [filters]  List actionable or blocked tickets
fer closed [--limit N]       List recently modified closed tickets
fer query [jq-filter]        Emit JSONL; filtered queries use jq
fer tui [path]               Open the terminal UI
fer ui [path]                Open the bundled web UI
fer super <command>          Bypass an external plugin
~~~

Run `fer --help` or `fer <command> --help` for command-specific options. Creation supports description, design, acceptance criteria, type, priority, assignee, external reference, parent, and labels.

## Web UI

~~~sh
fer ui                       # current workspace, open browser
fer ui ../another-project    # explicit project or .tickets directory
fer ui --no-open             # print URL without opening it
fer ui --no-watch            # disable filesystem watching
~~~

The embedded React UI provides:

- grouped list, parent tree, draggable status board, staged dependency roadmap, and zoomable dependency DAG layouts;
- status-aware roadmap cards with focused connection tracing, plus an opt-in all-connections view;
- a full-screen canvas mode for the roadmap and DAG that hides the app shell and exits with `Esc`;
- a `/` ticket finder that jumps to a roadmap card or DAG node by ID, title, label, or assignee;
- a virtualized pan-and-zoom DAG with asynchronous worker layout, left-to-right or top-to-bottom direction, fit controls, minimap, and prerequisite-path focus;
- route-level, bookmarkable ticket details with a properties rail;
- advanced visual and Gmail-style filters with suggestions;
- `Cmd+K`/`Ctrl+K` navigation and a collapsible shadcn sidebar;
- a Tiptap WYSIWYG editor with Notion-style `/` block commands and optional raw Markdown;
- bulk updates, notes, searchable labels/parents/assignees, and file actions;
- persistent display/filter/view choices (including DAG direction) and light, dark, or system theme;
- revision/ETag checks that reject edits when a ticket changed on disk.

Insights includes synchronized ticket-state and throughput trends, a shared smart/custom date range,
automatic or minute-based bins, hover inspection, and drag-to-zoom across charts.

Live refresh watches only direct `.md` children of the resolved `.tickets` directory. It is enabled by default and can be paused in the UI. The resolved workspace path is shown in the sidebar.

Binding outside localhost exposes a write-capable API without authentication. Only do this on a trusted network:

~~~sh
fer ui . --host 0.0.0.0 --port 4173 --no-open
~~~

## Terminal UI

~~~sh
fer tui
fer tui ../another-project
fer tui --no-watch
~~~

The TUI supports parent/sub-ticket trees, navigable relationships, sticky ticket identity, Vim-style scrolling/search, live Gmail-style filters, multi-field creation, bulk changes, notes, and conflict-safe writes. Use `Tab`/`Shift-Tab` or `h`/`l` to move between panes, `j`/`k` to select, `Enter` to follow a relationship, `L` to pause/resume live updates, and `?` for the complete key map.

## Query syntax

The web Filter control and TUI `/` prompt use the same query language. Terms use AND by default; uppercase `OR`, braces, quoted values, negation, and `*` wildcards are supported.

~~~text
status:todo label:backend -assignee:unassigned
status:active {label:backend label:rust} -is:blocked
id:fer-* OR title:"Build filters"
has:children after:2026-07-01
~~~

Fields include `status:`, `priority:`, `type:`, `assignee:`, `label:`/`tag:`, `parent:`, `id:`, `title:`, `blocked:`, `before:`, `after:`, `created:`, `is:`, and `has:`. State predicates include `is:blocked|unblocked|ready|assigned|unassigned|subticket|top-level|parent`. Presence predicates include `has:children|deps|links|labels|description|notes|assignee|parent|design|acceptance|external-ref`. `assignee:me` resolves to the current Git user.

## Storage and compatibility

Tickets remain compatible `.tickets/*.md` files with YAML frontmatter. Existing repositories require no conversion. Workspace UI preferences live asynchronously in the user’s Ferricket config directory, keyed by the resolved `.tickets` path, so UI state does not modify ticket repositories.

Plugins named `fer-<command>`, `tk-<command>`, or `ticket-<command>` can extend or override commands. They receive `TICKETS_DIR`, `FER_SCRIPT`, and the compatibility alias `TK_SCRIPT`.

## Development

The frontend was initialized with `bun init --react=shadcn --yes web`. It uses React 19, Tailwind CSS 4, shadcn Base Nova components backed by Base UI, Tiptap, dnd-kit, React Flow with Dagre layout, Lucide icons, and Bun’s bundler. `web/dist` is content-hashed and minified because it is the production bundle embedded into `fer`; maintainable source lives under `web/src`.

~~~sh
make build      # frozen Bun install, frontend build, Rust build
make check      # frontend format/types/tests/build + Rust fmt/tests/Clippy
make clean      # remove Cargo output and transient TypeScript build metadata
make install    # release-build and install fer into the local Cargo bin directory
make uninstall  # remove the locally installed fer binary
~~~

The install directory respects `CARGO_INSTALL_ROOT` or `CARGO_HOME` when either is configured.
Published releases contain the prebuilt web assets, so installing them does not require Bun.
`make clean` preserves the committed `web/dist` bundle, downloaded frontend dependencies, and
any `fer` executable already installed in the local Cargo bin directory.

See [AGENTS.md](AGENTS.md) for architecture and contribution rules.

## License

MIT
