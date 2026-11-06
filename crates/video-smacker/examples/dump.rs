// SPDX-License-Identifier: MIT
//! One clip through the crate, for a check against another decoder; it names
//! no clip, and every path comes from the command line.
//!
//! `dump frames <clip> <samples-out>`: each frame as rgb24 on stdout, and all
//! the track's samples as little-endian 16-bit to `<samples-out>`.
//! `dump cuts <clip> <points>`: copies in memory cut at `<points>` places and
//! damaged at as many seeds, each parsed and every frame read; prints what
//! was refused. A panic here is the finding, so nothing catches one.
use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::io::{self, BufWriter, Write};

use video_smacker::{Clip, Picture, Refused};

/// Reads every frame and its sound; the first refusal ends the clip.
fn read_all(bytes: &[u8]) -> Result<u32, Refused> {
    let mut clip = Clip::parse(bytes)?;
    let mut picture = Picture::new();
    let mut read = 0;
    for _ in 0..clip.header().frames {
        clip.frame_samples()?;
        clip.decode_next_frame(&mut picture)?;
        read += 1;
    }
    Ok(read)
}

fn frames(path: &str, samples_out: &str) -> Result<(), Box<dyn Error>> {
    let mut clip = Clip::parse(&fs::read(path)?)?;
    let mut picture = Picture::new();
    let mut out = BufWriter::new(io::stdout().lock());
    let mut sound = BufWriter::new(fs::File::create(samples_out)?);
    for _ in 0..clip.header().frames {
        for sample in clip.frame_samples()? {
            sound.write_all(&sample.to_le_bytes())?;
        }
        clip.decode_next_frame(&mut picture)?;
        for &index in &picture.pixels {
            let at = usize::from(index) * 3;
            out.write_all(&picture.palette[at..at + 3])?;
        }
    }
    out.flush()?;
    sound.flush()?;
    Ok(())
}

fn cuts(path: &str, points: usize) -> Result<(), Box<dyn Error>> {
    let bytes = fs::read(path)?;
    let mut seen: BTreeMap<String, u32> = BTreeMap::new();
    for point in 1..=points {
        let answer = read_all(&bytes[..bytes.len() * point / (points + 1)]);
        let name = answer.map_or_else(|why| why.name().to_string(), |_| "read-whole".to_string());
        *seen.entry(format!("cut:{name}")).or_default() += 1;
    }
    let mut state = 0x9e37_79b9_7f4a_7c15_u64;
    for _ in 0..points {
        let mut damaged = bytes.clone();
        for _ in 0..8 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let at = usize::try_from(state % damaged.len() as u64)?;
            damaged[at] ^= state.to_le_bytes()[7] | 1;
        }
        let answer = read_all(&damaged);
        let name = answer.map_or_else(|why| why.name().to_string(), |_| "read-whole".to_string());
        *seen.entry(format!("damage:{name}")).or_default() += 1;
    }
    for (name, count) in seen {
        println!("{name} {count}");
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        ["frames", clip, samples_out] => frames(clip, samples_out),
        ["cuts", clip, points] => cuts(clip, points.parse()?),
        _ => Err("usage: dump frames <clip> <samples-out> | dump cuts <clip> <points>".into()),
    }
}
