/// The cache API (saving and restoring from a remote cache)
pub mod cache;

/// The core API (logging, inputs and outputs)
pub mod core;

/// The exec API (executing processes and retrieving output)
pub mod exec;

/// The IO API (file system utilities)
pub mod io;

pub(self) mod push_line_splitter;

/// The tool cache API (downloading and extracting files)
pub mod tool_cache;
