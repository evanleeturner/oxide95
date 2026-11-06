// SPDX-License-Identifier: MIT
//! The WAVE parse: the chunks a sample's file holds, read in place.

/// `WAVE_FORMAT_PCM`, the only format a sample here may hold.
pub const FORMAT_PCM: u16 = 1;

/// A parsed file: its `fmt ` numbers and its sound bytes.
pub struct Wave<'a> {
    pub format: u16,
    pub channels: u16,
    pub rate_hz: u32,
    pub bits: u16,
    pub data: &'a [u8],
}

#[derive(Debug, PartialEq, Eq)]
pub enum WaveError {
    /// Not `RIFF` .. `WAVE` at its head.
    NotWave,
    /// No `fmt ` chunk of the size PCM's needs.
    NoFormat,
    NoData,
}

fn u16_at(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

/// Walks the chunks. A chunk's size past the file's end is cut at the end: the
/// caller hands what it could reach, and short sound is not a reason to stop.
pub fn parse(file: &[u8]) -> Result<Wave<'_>, WaveError> {
    if file.len() < 12 || &file[..4] != b"RIFF" || &file[8..12] != b"WAVE" {
        return Err(WaveError::NotWave);
    }
    let mut format = None;
    let mut at = 12;
    while at + 8 <= file.len() {
        let size = u32_at(file, at + 4) as usize;
        let body = at + 8;
        let end = body.saturating_add(size).min(file.len());
        match &file[at..at + 4] {
            b"fmt " if end - body >= 16 => {
                format = Some((
                    u16_at(file, body),
                    u16_at(file, body + 2),
                    u32_at(file, body + 4),
                    u16_at(file, body + 14),
                ));
            }
            b"data" => {
                let (format, channels, rate_hz, bits) = format.ok_or(WaveError::NoFormat)?;
                return Ok(Wave { format, channels, rate_hz, bits, data: &file[body..end] });
            }
            _ => {}
        }
        at = end + (size & 1);
    }
    Err(if format.is_some() { WaveError::NoData } else { WaveError::NoFormat })
}
