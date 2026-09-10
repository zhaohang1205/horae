# horae-cli

> GTD terminal task manager (`horae`) — with time-datafication, tags, and pomodoro.

This npm package provides the prebuilt binary wrapper for `horae`.

## Quick Start

Run directly with `npx` (no installation required):

```bash
npx horae-cli
# or quick capture
npx horae-cli capture "Review PR #42 @work ~today"
```

Or install globally:

```bash
npm install -g horae-cli
```

Once installed, use the `horae` command:

```bash
horae --help
horae               # Launch TUI
horae capture "Buy groceries @home"
```

## Supported Platforms

- Linux (x86_64, glibc & musl)
- macOS (Apple Silicon arm64 & Intel x86_64)
- Windows (x86_64)

For other platforms (e.g. Linux ARM), install directly from source via Cargo:

```bash
cargo install horae
```

## Environment Variables

- `HORAE_MIRROR`: Custom mirror for GitHub releases (e.g., `HORAE_MIRROR=https://ghproxy.net/https://github.com`)

## License

GPL-3.0
