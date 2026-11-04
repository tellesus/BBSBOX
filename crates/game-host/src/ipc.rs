// SPDX-License-Identifier: AGPL-3.0-or-later
//! Bounded, length-framed JSON for the core-to-worker transport.
use serde::{Serialize, de::DeserializeOwned};
use std::io::{self, Read, Write};

const MAX_FRAME: usize = 64 * 1024;

pub fn send(writer: &mut impl Write, message: &impl Serialize) -> io::Result<()> {
    let data = serde_json::to_vec(message).map_err(io::Error::other)?;
    if data.len() > MAX_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "oversized game frame",
        ));
    }
    writer.write_all(&(data.len() as u32).to_be_bytes())?;
    writer.write_all(&data)?;
    writer.flush()
}

pub fn receive<T: DeserializeOwned>(reader: &mut impl Read) -> io::Result<Option<T>> {
    let mut size = [0; 4];
    match reader.read_exact(&mut size[..1]) {
        Ok(()) => (),
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(error),
    }
    reader.read_exact(&mut size[1..])?;
    let size = u32::from_be_bytes(size) as usize;
    if !(1..=MAX_FRAME).contains(&size) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid game frame length",
        ));
    }
    let mut data = vec![0; size];
    reader.read_exact(&mut data)?;
    serde_json::from_slice(&data)
        .map(Some)
        .map_err(io::Error::other)
}
