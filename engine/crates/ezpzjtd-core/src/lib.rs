//! ezpzjtd-core: reader for JustSystems Ichitaro documents (`.jtd`, `.jtt`).
//!
//! Layers, from the bottom up:
//! - [`cfb`]: the compound-file container (a small file system inside one file)
//! - [`ssmg`]: the block store inside `/DocumentText` (`SsmgV.01`, `TextV.01`, `QLSTV.01`)
//! - [`text`]: text units → tokens (text, records, inline segments, controls)
//! - [`style`]: character style events (size, bold, colour, …)
//! - [`props`]: OLE property sets (`SummaryInformation`)
//! - [`doc`]: the assembled model (sheets → paragraphs / tables)
//! - [`export`]: HTML and Markdown output
//!
//! ```no_run
//! let bytes = std::fs::read("sample.jtd").unwrap();
//! let doc = ezpzjtd_core::open(bytes).unwrap();
//! println!("{}", doc.plain_text());
//! ```

pub mod cfb;
pub mod cfbw;
pub mod doc;
pub mod docx;
pub mod edit;
pub mod error;
pub mod export;
pub mod jtdw;
pub mod layout;
pub mod page;
pub mod pdf;
pub mod props;
pub mod save;
pub mod ssmg;
pub mod style;
pub mod text;

pub use doc::{open, Document};
pub use error::{Error, Result};
