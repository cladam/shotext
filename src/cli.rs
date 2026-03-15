use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "shotext")]
#[command(about = "OCR and Index your Mac screenshots", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Custom config file path
    #[arg(short, long, value_name = "FILE")]
    pub config: Option<PathBuf>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Ingest all screenshots from the configured folder
    Ingest {
        /// Force re-indexing of already processed files
        #[arg(short, long)]
        force: bool,
    },

    /// Start a background watcher for new screenshots
    Watch,

    /// List all indexed screenshots
    List {
        /// Show date and text snippet alongside the path
        #[arg(short, long)]
        verbose: bool,
    },

    /// Search through indexed screenshots using fuzzy find
    Search {
        /// The search query for Tantivy
        query: Option<String>,
    },

    /// View the screenshot along with the extracted text
    View {
        /// The hash or path of the screenshot to open in the GUI
        target: String,
    },

    /// Experimental UI
    X,

    /// Initialise or show current configuration
    Config {
        #[arg(short, long)]
        edit: bool,
    },
}
