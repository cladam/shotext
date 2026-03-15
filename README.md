# shotext

**Your screenshots are a goldmine, stop losing what's in them.**

Every day you capture code snippets, meeting notes, error messages, and diagrams as screenshots. They pile up on your
Desktop and become a black hole: impossible to search, impossible to organise. **shotext** fixes that.

It watches your screenshots folder, automatically runs OCR to extract the text from every image, and stores it in a
full-text search index. Seconds after you press ⌘⇧3, the content of that screenshot is searchable from your terminal.

## How It Works

```
Screenshot taken ──▶ Detected ──▶ OCR (Tesseract) ──▶ Indexed (Tantivy) ──▶ Searchable
```

1. **Capture**: Take a screenshot as you normally would on macOS.
2. **Detect**: shotext watches `~/Desktop` (or a custom folder) and picks up new `.png` files automatically.
3. **Extract**: Tesseract OCR reads the text from the image. Each file is blake3-hashed for deduplication.
4. **Index**: The extracted text, file path, and timestamp are stored in a [Sled](https://github.com/spacejam/sled)
   database and a [Tantivy](https://github.com/quickwit-oss/tantivy) full-text search index.
5. **Search**: Find any screenshot by what was _in_ it, not by its filename.

## Installation

### Prerequisites

- **Rust** toolchain (`cargo`)
- **Tesseract** OCR engine and language data

```bash
# Install Tesseract via Homebrew
brew install tesseract

# Clone and build
git clone https://github.com/cladam/shotext.git
cd shotext
cargo build --release

# The binary is at target/release/shotext
```

#### Installing from crates.io

The easiest way to install `shotext` is to download it from [crates.io](https://crates.io/crates/shotext). You can do it
using the following command:

```bash
cargo install shotext
```

## Quick Start

```bash
# 1. Initialise config (creates ~/.config/shotext/config.toml)
shotext config

# 2. Index all existing screenshots on your Desktop
shotext ingest

# 3. Start watching for new screenshots in real-time
shotext watch

# 4. Search, what was in that error message?
shotext search "connection refused"

# 5. Or launch the interactive fuzzy finder
shotext search

# 6. Open the experimental GUI dashboard
shotext x
```

## Commands

| Command                  | Description                                                                                                  |
|--------------------------|--------------------------------------------------------------------------------------------------------------|
| `shotext ingest`         | Scan the screenshots folder and index all new images. Use `-f` to force re-index everything.                 |
| `shotext watch`          | Watch the folder for new screenshots and index them the moment they appear.                                  |
| `shotext search <query>` | Full-text search across all extracted text. Supports stemming ("develop" → "developer", "developing").       |
| `shotext search`         | Interactive fuzzy finder (powered by [skim](https://github.com/lotabout/skim)) over all indexed screenshots. |
| `shotext list`           | List all indexed screenshots. Add `-v` for dates and text snippets.                                          |
| `shotext view <target>`  | Open a screenshot in a native GUI viewer alongside its extracted text. Accepts a file path or a blake3 hash. |
| `shotext x`              | Launch the experimental Insights dashboard — a full GUI for browsing, searching, and viewing screenshots.     |
| `shotext config`         | Show the current configuration. Use `-e` to open it in your `$EDITOR`.                                       |

## Experimental: Insights Dashboard (`shotext x`)

The `x` command launches a native desktop GUI that brings together browsing, searching, and viewing in a single window.

### Layout

```
┌──────────────────┬─────────────────────────────────┬──────────────────┐
│  🔍 Search…      │                                 │ Extracted Text   │
│  (⌘F)            │                                 │                  │
│──────────────────│        Screenshot Image          │ The quick brown  │
│ 📄 screenshot-1  │        (zoom-to-fit)             │ fox jumped over  │
│   2025-06-14     │                                 │ the lazy dog…    │
│   error message… │                                 │                  │
│                  │                                 │   📋 Copy   ✕    │
│ 📄 screenshot-2  │                                 │                  │
│   2025-06-13     │                                 │                  │
│   meeting notes… │                                 │                  │
└──────────────────┴─────────────────────────────────┴──────────────────┘
  Left Sidebar              Center Pane             Right Drawer (toggle)
```

### Features

- **Live search** — type in the search bar and results filter in real-time using Tantivy full-text search, with a substring fallback
- **Keyboard shortcuts** — `⌘F` to focus search, `↑`/`↓` arrows to navigate the list
- **Lazy image loading** — only the selected screenshot is loaded into memory
- **Collapsible text drawer** — the OCR text panel slides out from the right and can be closed/reopened
- **Virtualised scroll** — the sidebar only renders visible rows, keeping things smooth even with thousands of screenshots

> **Note:** This is an experimental feature. The command name `x` signals that the interface may change between releases.

## Configuration

Config lives at `~/.config/shotext/config.toml` and is created automatically on first run:

```toml
[paths]
screenshots = "/Users/you/Desktop"   # Folder to watch / ingest
database = "/Users/you/Library/Application Support/shotext/index.db"

[ocr]
language = "eng"   # Tesseract language code (eng, swe, deu, …)
```

## Tech Stack

| Component  | Crate                                              | Role                                   |
|------------|----------------------------------------------------|----------------------------------------|
| CLI        | [clap](https://github.com/clap-rs/clap)            | Command parsing with derive macros     |
| OCR        | [tesseract](https://crates.io/crates/tesseract)    | Text extraction via libtesseract       |
| Hashing    | [blake3](https://github.com/BLAKE3-team/BLAKE3)    | Fast deduplication of screenshot files |
| Database   | [sled](https://github.com/spacejam/sled)           | Embedded key-value store for metadata  |
| Search     | [tantivy](https://github.com/quickwit-oss/tantivy) | Full-text search index with stemming   |
| Watcher    | [notify](https://github.com/notify-rs/notify)      | Filesystem polling for new screenshots |
| Fuzzy find | [skim](https://github.com/lotabout/skim)           | Interactive terminal search UI         |
| Viewer     | [eframe/egui](https://github.com/emilk/egui)       | Native GUI window for image + text     |

## License

[MIT](LICENSE)
