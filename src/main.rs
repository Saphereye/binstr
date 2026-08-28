use clap::Parser;
use memmap2::MmapOptions;
use rayon::prelude::*;
use std::fs::File;
use std::io::{self, Write};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "binstr")]
struct Args {
    #[arg(required = true)]
    file: PathBuf,
}

fn is_string_byte(b: u8) -> bool {
    b.is_ascii_graphic() || matches!(b, b' ' | b'\t')
}

fn chunk_boundaries(bytes: &[u8], chunk_size: usize) -> Vec<usize> {
    let mut boundaries = vec![0];
    let mut pos = chunk_size.min(bytes.len());

    while pos < bytes.len() {
        while pos < bytes.len() && is_string_byte(bytes[pos]) {
            pos += 1;
        }

        boundaries.push(pos);
        pos = (pos + chunk_size).min(bytes.len());
    }

    if *boundaries.last().unwrap() != bytes.len() {
        boundaries.push(bytes.len());
    }

    boundaries
}

fn extract_strings(chunk: &[u8]) -> String {
    let mut output = String::new();
    let mut start = None;

    for (i, &byte) in chunk.iter().enumerate() {
        if is_string_byte(byte) {
            start.get_or_insert(i);
        } else if let Some(start) = start.take() {
            // SAFETY: `is_string_byte` only accepts ASCII bytes, all of which are valid UTF-8.
            let string = unsafe { std::str::from_utf8_unchecked(&chunk[start..i]) };
            output.push_str(string);
            output.push('\n');
        }
    }

    if let Some(start) = start {
        // SAFETY: `is_string_byte` only accepts ASCII bytes, all of which are valid UTF-8.
        let string = unsafe { std::str::from_utf8_unchecked(&chunk[start..]) };
        output.push_str(string);
        output.push('\n');
    }

    output
}

fn main() -> io::Result<()> {
    let args = Args::parse();

    let file = File::open(&args.file)?;
    let mmap = unsafe { MmapOptions::new().map(&file)? };
    let bytes: &[u8] = &mmap;

    let chunk_size = 1_048_576;
    let boundaries = chunk_boundaries(bytes, chunk_size);
    let chunks: Vec<_> = boundaries
        .windows(2)
        .map(|window| &bytes[window[0]..window[1]])
        .collect();

    let results: Vec<String> = chunks
        .par_iter()
        .map(|chunk| extract_strings(chunk))
        .collect();

    let stdout = io::stdout();
    let mut stdout = io::BufWriter::new(stdout.lock());

    for result in results {
        stdout.write_all(result.as_bytes())?;
    }

    Ok(())
}
