use std::io::Write;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};

use color_eyre::eyre::{Result, WrapErr};

use super::protocol::{self, NoteMessage};

pub struct NotesServer {
    listener: UnixListener,
    clients: Vec<UnixStream>,
    socket_path: PathBuf,
}

impl NotesServer {
    pub fn bind(path: &Path) -> Result<Self> {
        // Remove stale socket if it exists
        if path.exists() {
            std::fs::remove_file(path).ok();
        }

        let listener =
            UnixListener::bind(path).wrap_err_with(|| format!("Failed to bind socket at {:?}", path))?;
        listener
            .set_nonblocking(true)
            .wrap_err("Failed to set non-blocking")?;

        Ok(Self {
            listener,
            clients: Vec::new(),
            socket_path: path.to_path_buf(),
        })
    }

    /// Accept any pending connections (non-blocking).
    pub fn accept_pending(&mut self) {
        loop {
            match self.listener.accept() {
                Ok((stream, _)) => {
                    // Set the client stream to blocking for writes
                    stream.set_nonblocking(false).ok();
                    self.clients.push(stream);
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(_) => break,
            }
        }
    }

    /// Broadcast a message to all connected clients. Drops disconnected clients.
    pub fn broadcast(&mut self, msg: &NoteMessage) {
        let bytes = match protocol::encode(msg) {
            Ok(b) => b,
            Err(_) => return,
        };

        self.clients.retain_mut(|stream| {
            stream.write_all(&bytes).is_ok() && stream.flush().is_ok()
        });
    }

    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }
}

impl Drop for NotesServer {
    fn drop(&mut self) {
        // Send quit to all clients
        self.broadcast(&NoteMessage::Quit);
        // Clean up socket file
        std::fs::remove_file(&self.socket_path).ok();
    }
}
