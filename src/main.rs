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

fn main() -> io::Result<()> {
    let args = Args::parse();

    let file = File::open(&args.file)?;
    let mmap = unsafe { MmapOptions::new().map(&file)? };
    let bytes: &[u8] = &mmap;

    let chunk_size = 1_048_576;
    let stdout = io::stdout();
    let mut handle = io::BufWriter::new(stdout.lock());

    let results: Vec<String> = bytes
        .par_chunks(chunk_size)
        .map(|chunk| {
            let mut local_output = String::new();
            let mut start_idx = None;

            for (i, &byte) in chunk.iter().enumerate() {
                if byte.is_ascii_graphic() || byte == b' ' || byte == b'\t' {
                    if start_idx.is_none() {
                        start_idx = Some(i);
                    }
                } else {
                    if let Some(start) = start_idx {
                        if let Ok(valid_str) = std::str::from_utf8(&chunk[start..i]) {
                            local_output.push_str(valid_str);
                            local_output.push('\n');
                        }
                        start_idx = None;
                    }
                }
            }

            if let Some(start) = start_idx {
                if let Ok(valid_str) = std::str::from_utf8(&chunk[start..]) {
                    local_output.push_str(valid_str);
                    local_output.push('\n');
                }
            }

            local_output
        })
        .collect();

    for chunk_string in results {
        if !chunk_string.is_empty() {
            handle.write_all(chunk_string.as_bytes())?;
        }
    }

    Ok(())
}
