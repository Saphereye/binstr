mod simd;

use clap::Parser;
use memmap2::MmapOptions;
use rayon::prelude::*;
use std::fs::File;
use std::io::{self, IsTerminal, Read, Write};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "binstr",
    about = "Display printable strings in files (stdin if no files given).",
    after_help = "On a terminal, offsets, headings, and color are enabled by default.\nPiped output is plain unless -t or -p is used.",
)]
struct Args {
    #[arg(value_name = "FILE")]
    files: Vec<PathBuf>,

    /// Scan the entire file
    #[arg(short = 'a', default_value_t = true)]
    all: bool,

    /// Minimum string length
    #[arg(short = 'n', long = "bytes", default_value_t = 4)]
    min_len: usize,

    /// Print the file name before each string
    #[arg(short = 'f')]
    print_file_name: bool,

    /// Suppress the file name heading
    #[arg(short = 'I')]
    no_filename: bool,

    /// Suppress the byte-offset prefix
    #[arg(short = 'N')]
    no_offset: bool,

    /// Print the offset before each string
    #[arg(short = 't', value_enum)]
    radix: Option<Radix>,

    /// An alias for -t o
    #[arg(short = 'o')]
    octal_offset: bool,

    /// Include all whitespace as valid string characters
    #[arg(short = 'w')]
    include_all_whitespace: bool,

    /// String used to separate strings in output
    #[arg(short = 's', long = "output-separator", default_value = "\n")]
    separator: String,

    /// Enable headings, offsets, and color when not attached to a terminal
    #[arg(short = 'p')]
    pretty: bool,
}

#[derive(Clone, Copy, clap::ValueEnum, Debug)]
enum Radix {
    D,
    O,
    X,
}

const SGR_DIM: &[u8] = b"\x1b[2m";
const SGR_BOLD: &[u8] = b"\x1b[1m";
const SGR_RESET: &[u8] = b"\x1b[0m";

fn is_string_byte(b: u8, whitespace: bool) -> bool {
    if whitespace {
        matches!(b, 0x09 | 0x0A | 0x0B | 0x0C | 0x0D | 0x20..=0x7E)
    } else {
        let in_range = (b.wrapping_sub(0x20) <= 0x5E) as u8;
        let is_tab = (b == b'\t') as u8;
        (in_range | is_tab) != 0
    }
}

fn chunk_boundaries(bytes: &[u8], chunk_size: usize, whitespace: bool) -> Vec<usize> {
    let mut boundaries = vec![0];
    let mut pos = chunk_size.min(bytes.len());

    while pos < bytes.len() {
        while pos < bytes.len() {
            if !whitespace && pos + simd::BLOCK <= bytes.len() {
                let run = simd::string_run(unsafe { bytes.as_ptr().add(pos) });
                pos += run;
                if run < simd::BLOCK {
                    break;
                }
                continue;
            }
            if is_string_byte(bytes[pos], whitespace) {
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

const fn offset_width(max: usize, radix: Radix) -> usize {
    match radix {
        Radix::D => max.ilog10() as usize + 1,
        Radix::O => (usize::BITS as usize - max.leading_zeros() as usize).div_ceil(3),
        Radix::X => (usize::BITS as usize - max.leading_zeros() as usize).div_ceil(4),
    }
}

fn write_out(out: &mut impl Write, buf: &[u8]) -> io::Result<bool> {
    match out.write_all(buf) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => Ok(false),
        Err(e) => Err(e),
    }
}

fn extract_strings(
    chunk: &[u8],
    min_len: usize,
    base_off: usize,
    show_offset: bool,
    radix: Radix,
    color: bool,
    offset_width: usize,
    whitespace: bool,
    sep: u8,
    prefix: Option<&[u8]>,
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
            if !whitespace && i + simd::BLOCK <= chunk.len() {
                match simd::step32(chunk.as_ptr().add(i), start.is_some()) {
                    simd::Step32::Blank(n) => {
                        if let Some(s) = start.take() {
                            emit(
                                base,
                                &mut len,
                                chunk,
                                s,
                                i,
                                min_len,
                                base_off,
                                show_offset,
                                radix,
                                color,
                                offset_width,
                                sep,
                                prefix,
                            );
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
                            color,
                            offset_width,
                            sep,
                            prefix,
                        );
                        i = end + 1;
                    }
                    simd::Step32::Opened { skip, len: run } => {
                        i += skip;
                        let end = i + run;
                        if run < simd::BLOCK - skip {
                            emit(
                                base,
                                &mut len,
                                chunk,
                                i,
                                end,
                                min_len,
                                base_off,
                                show_offset,
                                radix,
                                color,
                                offset_width,
                                sep,
                                prefix,
                            );
                            i = end + 1;
                        } else {
                            start = Some(i);
                            i = end;
                        }
                    }
                }
                continue;
            }

            if is_string_byte(chunk[i], whitespace) {
                start.get_or_insert(i);
            } else if let Some(s) = start.take() {
                emit(
                    base,
                    &mut len,
                    chunk,
                    s,
                    i,
                    min_len,
                    base_off,
                    show_offset,
                    radix,
                    color,
                    offset_width,
                    sep,
                    prefix,
                );
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
                color,
                offset_width,
                sep,
                prefix,
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
    color: bool,
    offset_width: usize,
    sep: u8,
    prefix: Option<&[u8]>,
) {
    let run_len = end - start;
    if run_len < min_len {
        return;
    }
    unsafe {
        if let Some(prefix) = prefix {
            std::ptr::copy_nonoverlapping(prefix.as_ptr(), base.add(*len), prefix.len());
            *len += prefix.len();
        }
        if show_offset {
            if color {
                std::ptr::copy_nonoverlapping(SGR_DIM.as_ptr(), base.add(*len), SGR_DIM.len());
                *len += SGR_DIM.len();
            }
            write_offset(base, len, base_off + start, radix, offset_width);
            if color {
                std::ptr::copy_nonoverlapping(SGR_RESET.as_ptr(), base.add(*len), SGR_RESET.len());
                *len += SGR_RESET.len();
            }
        }
        std::ptr::copy_nonoverlapping(chunk.as_ptr().add(start), base.add(*len), run_len);
        *len += run_len;
        *base.add(*len) = sep;
        *len += 1;
    }
}

fn write_offset(base: *mut u8, len: &mut usize, n: usize, radix: Radix, width: usize) {
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
    let pad = width.saturating_sub(23 - i);
    i -= pad;
    buf[i..i + pad].fill(b' ');
    let written = 24 - i;
    unsafe {
        std::ptr::copy_nonoverlapping(buf.as_ptr().add(i), base.add(*len), written);
    }
    *len += written;
}

fn scan_bytes(
    bytes: &[u8],
    file_path: &PathBuf,
    min_len: usize,
    whitespace: bool,
    interactive: bool,
    show_offset: bool,
    radix: Radix,
    color: bool,
    sep: u8,
    line_prefix: bool,
    heading: bool,
    stdout: &mut impl Write,
) -> io::Result<()> {
    let off_width = if interactive && show_offset {
        offset_width(bytes.len().saturating_sub(1), radix)
    } else {
        0
    };
    let prefix = if line_prefix {
        let path_str = if file_path.as_os_str() == "-" {
            "stdin"
        } else {
            file_path
                .to_str()
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "non-UTF8 path"))?
        };
        Some(format!("{path_str}: ").into_bytes())
    } else {
        None
    };
    let prefix_ref = prefix.as_deref();

    let chunk_size = 1_048_576;
    let boundaries = chunk_boundaries(bytes, chunk_size, whitespace);
    let windows: Vec<[usize; 2]> = boundaries.windows(2).map(|w| [w[0], w[1]]).collect();

    let results: Vec<Vec<u8>> = windows
        .par_iter()
        .map(|w| {
            extract_strings(
                &bytes[w[0]..w[1]],
                min_len,
                w[0],
                show_offset,
                radix,
                color,
                off_width,
                whitespace,
                sep,
                prefix_ref,
            )
        })
        .collect();

    let has_matches = results.iter().any(|r| !r.is_empty());
    if has_matches && heading {
        let path_str = if file_path.as_os_str() == "-" {
            "stdin"
        } else {
            file_path
                .to_str()
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "non-UTF8 path"))?
        };
        if color && !write_out(stdout, SGR_BOLD)? {
            return Ok(());
        }
        if !write_out(stdout, path_str.as_bytes())? {
            return Ok(());
        }
        if color && !write_out(stdout, SGR_RESET)? {
            return Ok(());
        }
        if !write_out(stdout, b"\n")? {
            return Ok(());
        }
    }

    for result in results {
        if !write_out(stdout, &result)? {
            return Ok(());
        }
    }
    Ok(())
}

fn main() -> io::Result<()> {
    let args = Args::parse();
    let _ = args.all;

    let sep = args
        .separator
        .bytes()
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "empty separator"))?;

    let interactive = args.pretty || io::stdout().is_terminal();
    let explicit_radix = args.radix.is_some() || args.octal_offset;
    let show_offset = !args.no_offset && (interactive || explicit_radix);
    let radix = args
        .radix
        .or(args.octal_offset.then_some(Radix::O))
        .unwrap_or(Radix::D);
    let line_prefix = args.print_file_name && !args.no_filename;
    let heading = interactive && !args.no_filename && !line_prefix;
    let color = interactive && !line_prefix && std::env::var_os("NO_COLOR").is_none();

    let files = if args.files.is_empty() {
        vec![PathBuf::from("-")]
    } else {
        args.files
    };

    let stdout = io::stdout();
    let mut stdout = io::BufWriter::new(stdout.lock());

    for file_path in &files {
        if file_path.as_os_str() == "-" {
            let mut bytes = Vec::new();
            io::stdin().lock().read_to_end(&mut bytes)?;
            scan_bytes(
                &bytes,
                file_path,
                args.min_len,
                args.include_all_whitespace,
                interactive,
                show_offset,
                radix,
                color,
                sep,
                line_prefix,
                heading,
                &mut stdout,
            )?;
        } else {
            let file = File::open(file_path)?;
            let mmap = unsafe { MmapOptions::new().map(&file)? };
            scan_bytes(
                &mmap,
                file_path,
                args.min_len,
                args.include_all_whitespace,
                interactive,
                show_offset,
                radix,
                color,
                sep,
                line_prefix,
                heading,
                &mut stdout,
            )?;
        }
    }
    Ok(())
}
