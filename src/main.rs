mod simd;

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
    files: Vec<PathBuf>,

    /// Minimum length of matched string
    #[arg(short = 'n', long = "bytes", default_value_t = 4)]
    min_len: usize,

    /// Suppress filename prefix
    #[arg(short = 'I', long = "no-filename")]
    no_filename: bool,

    /// Suppress byte-offset prefix
    #[arg(short = 'N', long = "no-offset")]
    no_offset: bool,

    /// Radix of the string location byte offset
    #[arg(short = 't', long, value_enum, default_value = "d")]
    radix: Radix,
}

#[derive(Clone, Copy, clap::ValueEnum, Debug)]
enum Radix {
    D,
    O,
    X,
}

fn is_string_byte(b: u8) -> bool {
    let in_range = (b.wrapping_sub(0x20) <= 0x5E) as u8;
    let is_tab = (b == b'\t') as u8;
    (in_range | is_tab) != 0
}

fn chunk_boundaries(bytes: &[u8], chunk_size: usize) -> Vec<usize> {
    let mut boundaries = vec![0];
    let mut pos = chunk_size.min(bytes.len());

    while pos < bytes.len() {
        while pos < bytes.len() {
            if pos + simd::BLOCK <= bytes.len() {
                let run = simd::string_run(unsafe { bytes.as_ptr().add(pos) });
                pos += run;
                if run < simd::BLOCK {
                    break;
                }
                continue;
            }
            if is_string_byte(bytes[pos]) {
                pos += 1;
            } else {
                break;
            }
        }
        boundaries.push(pos);
        pos = (pos + chunk_size).min(bytes.len());
    }

    if *boundaries.last().unwrap() != bytes.len() {
        boundaries.push(bytes.len());
    }

    boundaries
}

fn extract_strings(
    chunk: &[u8],
    min_len: usize,
    base_off: usize,
    show_offset: bool,
    radix: Radix,
) -> Vec<u8> {
    let mut output: Vec<u8> = Vec::with_capacity(
        chunk.len()
            + 1
            + if show_offset {
                chunk.len() / min_len.max(1) * 24
            } else {
                0
            },
    );
    let mut len = 0usize;
    let mut start = None;

    unsafe {
        let base = output.as_mut_ptr();
        let mut i = 0usize;

        while i < chunk.len() {
            if i + simd::BLOCK <= chunk.len() {
                match simd::step32(chunk.as_ptr().add(i), start.is_some()) {
                    simd::Step32::Blank(n) => {
                        if let Some(s) = start.take() {
                            emit(base, &mut len, chunk, s, i, min_len, base_off, show_offset, radix);
                        }
                        i += n;
                    }
                    simd::Step32::Solid(n) => {
                        start.get_or_insert(i);
                        i += n;
                    }
                    simd::Step32::Closed(n) => {
                        let end = i + n;
                        emit(
                            base,
                            &mut len,
                            chunk,
                            start.take().unwrap(),
                            end,
                            min_len,
                            base_off,
                            show_offset,
                            radix,
                        );
                        i = end + 1;
                    }
                    simd::Step32::Opened { skip, len: run } => {
                        i += skip;
                        let end = i + run;
                        if run < simd::BLOCK - skip {
                            emit(base, &mut len, chunk, i, end, min_len, base_off, show_offset, radix);
                            i = end + 1;
                        } else {
                            start = Some(i);
                            i = end;
                        }
                    }
                }
                continue;
            }

            if is_string_byte(chunk[i]) {
                start.get_or_insert(i);
            } else if let Some(s) = start.take() {
                emit(base, &mut len, chunk, s, i, min_len, base_off, show_offset, radix);
            }
            i += 1;
        }

        if let Some(s) = start {
            emit(
                base,
                &mut len,
                chunk,
                s,
                chunk.len(),
                min_len,
                base_off,
                show_offset,
                radix,
            );
        }
        output.set_len(len);
    }
    output
}

unsafe fn emit(
    base: *mut u8,
    len: &mut usize,
    chunk: &[u8],
    start: usize,
    end: usize,
    min_len: usize,
    base_off: usize,
    show_offset: bool,
    radix: Radix,
) {
    let run_len = end - start;
    if run_len < min_len {
        return;
    }
    unsafe {
        if show_offset {
            write_offset(base, len, base_off + start, radix);
        }
        std::ptr::copy_nonoverlapping(chunk.as_ptr().add(start), base.add(*len), run_len);
        *len += run_len;
        *base.add(*len) = b'\n';
        *len += 1;
    }
}

fn write_offset(base: *mut u8, len: &mut usize, n: usize, radix: Radix) {
    let mut buf = [0u8; 24];
    let (b, digits): (usize, &[u8]) = match radix {
        Radix::D => (10, b"0123456789"),
        Radix::O => (8, b"01234567"),
        Radix::X => (16, b"0123456789abcdef"),
    };
    let mut i = 23;
    buf[i] = b':';
    if n == 0 {
        i -= 1;
        buf[i] = b'0';
    } else {
        let mut n = n;
        while n > 0 {
            i -= 1;
            buf[i] = digits[n % b];
            n /= b;
        }
    }
    let written = 24 - i;
    unsafe {
        std::ptr::copy_nonoverlapping(buf.as_ptr().add(i), base.add(*len), written);
    }
    *len += written;
}

fn main() -> io::Result<()> {
    let args = Args::parse();

    for file_path in &args.files {
        let file = File::open(file_path)?;
        let mmap = unsafe { MmapOptions::new().map(&file)? };
        let bytes: &[u8] = &mmap;

        let chunk_size = 1_048_576;
        let boundaries = chunk_boundaries(bytes, chunk_size);
        let windows: Vec<[usize; 2]> = boundaries.windows(2).map(|w| [w[0], w[1]]).collect();

        let results: Vec<Vec<u8>> = windows
            .par_iter()
            .map(|w| {
                extract_strings(
                    &bytes[w[0]..w[1]],
                    args.min_len,
                    w[0],
                    !args.no_offset,
                    args.radix,
                )
            })
            .collect();

        let stdout = io::stdout();
        let mut stdout = io::BufWriter::new(stdout.lock());

        let has_matches = results.iter().any(|r| !r.is_empty());
        if has_matches && !args.no_filename {
            let path_str = file_path
                .to_str()
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "non-UTF8 path"))?;
            stdout.write_all(path_str.as_bytes())?;
            stdout.write_all(b"\n")?;
        }

        for result in results {
            stdout.write_all(&result)?;
        }
    }
    Ok(())
}
