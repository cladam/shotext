# Project: Shotext

High-performance OCR & Full-Text Search Indexer for macOS Screenshots.

## 1. Vision

Shotext solves the "black hole" problem of screenshots. Most users capture presentations, code snippets, or work meetings that remain as static images, never to be searched again. 
Shotext acts as a "second brain" by automatically extracting text from these images and providing a lightning-fast fuzzy-search interface to find them later.

## 2. Core Architecture

The tool is built as a modular Rust CLI using a **Pipeline-based architecture**:

### A. The Watcher (Ingestion)

* **Trigger:** Uses the `notify` crate to monitor the macOS screenshot directory.
* **Filtering:** Specifically targets `.png` files matching the Apple "Screen Shot..." naming convention.
* **Deduplication:** Uses `blake3` hashing to generate a unique ID for every image. Before running OCR, it checks a **Sled** database to see if the hash has already been processed.

### B. The Engine (OCR)

* **Processing:** Integrates with `libtesseract` via the `tesseract-rs` bindings.
* **Refinement:** (Optional) Uses the `image` crate to pre-process Retina (2x) screenshots into grayscale or high-contrast buffers to increase Tesseract’s accuracy.

### C. The Indexer (Storage)

* **Metadata:** **Sled** stores the mapping of `Hash -> Path, Timestamp, Raw Text`.
* **Search:** **Tantivy** creates a schema-based index of the extracted text. This allows for:
* **Stemming:** Searching for "Develop" finds "Developer" or "Developing."
* **Snippets:** Highlighting the specific sentence where the search term was found.

### D. The Interface (UX)

* **CLI Parsing:** Implemented with `clap` (Derive) for a consistent developer experience (`shotext ingest`, `shotext watch`, `shotext search`).
* **Fuzzy Finder:** Integrated with the `skim` library to provide an interactive, real-time search UI directly in the terminal.

## 3. Command-Line Interface (CLI) Definition

Following the patterns in `tbdflow` and `medi`:

| Command | Action |
| --- | --- |
| `shotext ingest` | Walks the screenshot directory and indexes all new images. |
| `shotext watch` | Runs as a background daemon to index screenshots the moment they are taken. |
| `shotext search <query>` | Launches an interactive `skim` UI to fuzzy-search through the index. |
| `shotext config` | Manages the TOML configuration (folder paths, Tesseract language). |

## 4. Why This Stack?

* **Performance:** Rust ensures that even with hundreds of screenshots, the search latency remains sub-millisecond.
* **Reliability:** `sled` provides an atomic, transactional key-value store, ensuring the database doesn't corrupt if the process is interrupted.
* **Native Feel:** By using `skim` and `clap`, the tool feels like a native part of the modern developer's terminal toolkit.

## 5. Roadmapped features

* **QuickLook Integration:** While in the `skim` search results, pressing a key (e.g. `Enter`) triggers macOS `qlmanage` to preview the original image.
* **Clipboard Export:** A flag to instantly copy the extracted text of a found screenshot to the system clipboard.
at bridges your `cli.rs` commands to the `sled` and `tantivy` initialization logic?**