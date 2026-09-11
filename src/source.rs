//! A readable markdown source and the URL it came from.

use std::io::Read;

/// A readable markdown source.
pub struct Source {
    /// Where the bytes come from.
    pub reader: Box<dyn Read>,
    /// The resolved location, used to derive the base URL and the file type.
    pub url: String,
}

impl Source {
    /// A source with an empty URL.
    pub fn from_reader(reader: Box<dyn Read>) -> Source {
        Source {
            reader,
            url: String::new(),
        }
    }
}

impl std::fmt::Debug for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Source").field("url", &self.url).finish()
    }
}
