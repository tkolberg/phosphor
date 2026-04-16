use std::io::BufReader;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

use color_eyre::eyre::{Result, WrapErr};

use super::protocol::{self, NoteMessage};

pub struct NotesClient {
    reader: BufReader<UnixStream>,
}

impl NotesClient {
    pub fn connect(path: &Path) -> Result<Self> {
        let stream = UnixStream::connect(path)
            .wrap_err_with(|| format!("Failed to connect to notes socket at {:?}", path))?;
        // Set a short read timeout so recv() doesn't block the TUI event loop
        stream
            .set_read_timeout(Some(Duration::from_millis(50)))
            .wrap_err("Failed to set read timeout")?;
        Ok(Self {
            reader: BufReader::new(stream),
        })
    }

    /// Try to receive the next message. Returns None on timeout.
    pub fn recv(&mut self) -> Option<NoteMessage> {
        protocol::decode(&mut self.reader).ok()
    }
}
