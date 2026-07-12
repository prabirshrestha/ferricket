# Ferricket web UI

The React frontend for fer ui. It was initialized with Bun's shadcn template, uses shadcn Base Nova components backed by Base UI, and is built by Bun's native bundler.

~~~sh
bun install
bun run check
bun run build
~~~

The Rust build embeds dist/; always rebuild and commit it after frontend changes.

Code is organized by responsibility:

- api/: HTTP transport
- hooks/: application state
- components/layout/: app shell
- components/ui/: shadcn-style primitives
- features/tickets/: ticket behavior and presentation
- types/: shared frontend models
