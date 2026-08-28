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

fn extract_strings(chunk: &[u8]) -> Vec<u8> {
    let mut output: Vec<u8> = Vec::with_capacity(chunk.len() + 1);
    let mut len = 0usize;
    let mut start = None;

    unsafe {
        let base = output.as_mut_ptr();
        for (i, &byte) in chunk.iter().enumerate() {
            if is_string_byte(byte) {
                start.get_or_insert(i);
            } else if let Some(s) = start.take() {
                let run_len = i - s;
                std::ptr::copy_nonoverlapping(chunk.as_ptr().add(s), base.add(len), run_len);
                len += run_len;
                *base.add(len) = b'\n';
                len += 1;
            }
        }
        if let Some(s) = start {
            let run_len = chunk.len() - s;
            std::ptr::copy_nonoverlapping(chunk.as_ptr().add(s), base.add(len), run_len);
            len += run_len;
            *base.add(len) = b'\n';
            len += 1;
        }
        output.set_len(len);
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

    let results: Vec<Vec<u8>> = chunks
        .par_iter()
        .map(|chunk| extract_strings(chunk))
        .collect();

    let stdout = io::stdout();
    let mut stdout = io::BufWriter::new(stdout.lock());

    for result in results {
        stdout.write_all(&result)?;
    }

    Ok(())
}
