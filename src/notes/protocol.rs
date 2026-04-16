use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

use color_eyre::eyre::{Result, WrapErr};
use serde::{Deserialize, Serialize};

/// Message sent from presenter to notes viewer over the socket.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NoteMessage {
    SlideChanged {
        index: usize,
        visible_chunks: usize,
    },
    /// Request the notes viewer to change the Ghostty font size.
    FontSize {
        size: u16,
    },
    Quit,
}

/// Default socket path based on PID.
pub fn default_socket_path() -> PathBuf {
    std::env::temp_dir().join(format!("phosphor-notes-{}.sock", std::process::id()))
}

/// Encode a message as a newline-delimited JSON line.
pub fn encode(msg: &NoteMessage) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec(msg).wrap_err("Failed to encode message")?;
    bytes.push(b'\n');
    Ok(bytes)
}

/// Decode a message from a buffered reader. Blocks until a line is available.
pub fn decode<R: std::io::Read>(reader: &mut BufReader<R>) -> Result<NoteMessage> {
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .wrap_err("Failed to read message")?;
    if line.is_empty() {
        return Err(color_eyre::eyre::eyre!("Connection closed"));
    }
    let msg: NoteMessage = serde_json::from_str(&line).wrap_err("Failed to decode message")?;
    Ok(msg)
}

/// Write a message to a writer.
pub fn send<W: Write>(writer: &mut W, msg: &NoteMessage) -> Result<()> {
    let bytes = encode(msg)?;
    writer.write_all(&bytes).wrap_err("Failed to send message")?;
    writer.flush().wrap_err("Failed to flush")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_decode_roundtrip() {
        let msg = NoteMessage::SlideChanged {
            index: 2,
            visible_chunks: 3,
        };
        let encoded = encode(&msg).unwrap();
        let mut reader = BufReader::new(&encoded[..]);
        let decoded = decode(&mut reader).unwrap();
        match decoded {
            NoteMessage::SlideChanged {
                index,
                visible_chunks,
            } => {
                assert_eq!(index, 2);
                assert_eq!(visible_chunks, 3);
            }
            _ => panic!("Expected SlideChanged"),
        }
    }
}
