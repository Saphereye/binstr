mod elf;
mod simd;

use clap::Parser;
use memmap2::MmapOptions;
use rayon::prelude::*;
use std::fs::File;
use std::io::{self, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};

#[derive(Parser, Debug)]
#[command(
    name = "binstr",
    about = "Display printable strings in files (stdin if no files given).",
    after_help = "On a terminal, offsets, headings, and color are enabled by default.\nPiped output is plain unless -t or -p is used."
)]
#[expect(clippy::struct_excessive_bools)]
struct Args {
    #[arg(value_name = "FILE")]
    files: Vec<PathBuf>,

    /// Scan the entire file
    #[arg(short = 'a', default_value_t = true)]
    all: bool,

    /// Skip executable ELF sections
    #[arg(short = 'd', long = "data")]
    data_only: bool,

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

const fn is_string_byte(b: u8) -> bool {
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

fn chunk_boundaries_ws(bytes: &[u8], chunk_size: usize) -> Vec<usize> {
    let mut boundaries = vec![0];
    let mut pos = chunk_size.min(bytes.len());

    while pos < bytes.len() {
        while pos < bytes.len() {
            if matches!(bytes[pos], 0x09 | 0x0A | 0x0B | 0x0C | 0x0D | 0x20..=0x7E) {
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

#[expect(clippy::struct_excessive_bools)]
struct ExtractConfig<'a> {
    min_len: usize,
    base_off: usize,
    show_offset: bool,
    radix: Radix,
    color: bool,
    offset_width: usize,
    gnu_offset: bool,
    whitespace: bool,
    sep: u8,
    prefix: Option<&'a [u8]>,
}

#[expect(clippy::struct_excessive_bools)]
#[derive(Clone, Copy)]
struct ScanConfig {
    min_len: usize,
    whitespace: bool,
    data_only: bool,
    show_offset: bool,
    radix: Radix,
    color: bool,
    gnu_offset: bool,
    sep: u8,
    line_prefix: bool,
    heading: bool,
}

fn extract_strings(chunk: &[u8], cfg: &ExtractConfig<'_>) -> Vec<u8> {
    let mut output: Vec<u8> = Vec::with_capacity(
        chunk.len()
            + 1
            + if cfg.show_offset {
                chunk.len() / cfg.min_len.max(1) * 24
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
            if !cfg.whitespace && i + simd::BLOCK <= chunk.len() {
                match simd::step32(chunk.as_ptr().add(i), start.is_some()) {
                    simd::Step32::Blank(n) => {
                        if let Some(s) = start.take() {
                            emit(base, &mut len, chunk, s, i, cfg);
                        }
                        i += n;
                    }
                    simd::Step32::Solid(n) => {
                        start.get_or_insert(i);
                        i += n;
                    }
                    simd::Step32::Closed(n) => {
                        let end = i + n;
                        emit(base, &mut len, chunk, start.take().unwrap(), end, cfg);
                        i = end + 1;
                    }
                    simd::Step32::Opened { skip, len: run } => {
                        i += skip;
                        let end = i + run;
                        if run < simd::BLOCK - skip {
                            emit(base, &mut len, chunk, i, end, cfg);
                            i = end + 1;
                        } else {
                            start = Some(i);
                            i = end;
                        }
                    }
                }
                continue;
            }

            let ok = if cfg.whitespace {
                matches!(chunk[i], 0x09 | 0x0A | 0x0B | 0x0C | 0x0D | 0x20..=0x7E)
            } else {
                is_string_byte(chunk[i])
            };
            if ok {
                start.get_or_insert(i);
            } else if let Some(s) = start.take() {
                emit(base, &mut len, chunk, s, i, cfg);
            }
            i += 1;
        }

        if let Some(s) = start {
            emit(base, &mut len, chunk, s, chunk.len(), cfg);
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
    cfg: &ExtractConfig<'_>,
) {
    let run_len = end - start;
    if run_len < cfg.min_len {
        return;
    }
    unsafe {
        if let Some(prefix) = cfg.prefix {
            std::ptr::copy_nonoverlapping(prefix.as_ptr(), base.add(*len), prefix.len());
            *len += prefix.len();
        }
        if cfg.show_offset {
            if cfg.color {
                std::ptr::copy_nonoverlapping(SGR_DIM.as_ptr(), base.add(*len), SGR_DIM.len());
                *len += SGR_DIM.len();
            }
            write_offset(
                base,
                len,
                cfg.base_off + start,
                cfg.radix,
                cfg.offset_width,
                cfg.gnu_offset,
            );
            if cfg.color {
                std::ptr::copy_nonoverlapping(SGR_RESET.as_ptr(), base.add(*len), SGR_RESET.len());
                *len += SGR_RESET.len();
            }
        }
        std::ptr::copy_nonoverlapping(chunk.as_ptr().add(start), base.add(*len), run_len);
        *len += run_len;
        *base.add(*len) = cfg.sep;
        *len += 1;
    }
}

fn write_offset(base: *mut u8, len: &mut usize, n: usize, radix: Radix, width: usize, gnu: bool) {
    let mut buf = [0u8; 24];
    let (b, digits): (usize, &[u8]) = match radix {
        Radix::D => (10, b"0123456789"),
        Radix::O => (8, b"01234567"),
        Radix::X => (16, b"0123456789abcdef"),
    };
    let mut i = 23;
    buf[i] = if gnu { b' ' } else { b':' };
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
    let digit_len = 23 - i;
    let width = if gnu {
        7_usize.max(width).max(digit_len)
    } else {
        width.max(digit_len)
    };
    let pad = width.saturating_sub(digit_len);
    i -= pad;
    buf[i..i + pad].fill(b' ');
    let written = 24 - i;
    unsafe {
        std::ptr::copy_nonoverlapping(buf.as_ptr().add(i), base.add(*len), written);
    }
    *len += written;
}

fn scan_ranges(bytes: &[u8], data_only: bool) -> Vec<(usize, usize)> {
    if data_only {
        elf::data_ranges(bytes).unwrap_or_else(|| vec![(0, bytes.len())])
    } else {
        vec![(0, bytes.len())]
    }
}

fn process_slice(
    slice: &[u8],
    base_off: usize,
    offset_width: usize,
    cfg: ScanConfig,
    prefix: Option<&[u8]>,
) -> Vec<Vec<u8>> {
    let chunk_size = 1_048_576;
    let boundaries = if cfg.whitespace {
        chunk_boundaries_ws(slice, chunk_size)
    } else {
        chunk_boundaries(slice, chunk_size)
    };
    let windows: Vec<[usize; 2]> = boundaries.windows(2).map(|w| [w[0], w[1]]).collect();
    windows
        .par_iter()
        .map(|w| {
            extract_strings(
                &slice[w[0]..w[1]],
                &ExtractConfig {
                    min_len: cfg.min_len,
                    base_off: base_off + w[0],
                    show_offset: cfg.show_offset,
                    radix: cfg.radix,
                    color: cfg.color,
                    offset_width,
                    gnu_offset: cfg.gnu_offset,
                    whitespace: cfg.whitespace,
                    sep: cfg.sep,
                    prefix,
                },
            )
        })
        .collect()
}

fn scan_bytes(
    bytes: &[u8],
    file_path: &Path,
    cfg: ScanConfig,
    stdout: &mut impl Write,
) -> io::Result<()> {
    let off_width = if cfg.show_offset {
        offset_width(bytes.len().saturating_sub(1), cfg.radix)
    } else {
        0
    };
    let prefix = if cfg.line_prefix {
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

    let results = if cfg.data_only {
        let mut out = Vec::new();
        for (start, end) in scan_ranges(bytes, true) {
            out.extend(process_slice(
                &bytes[start..end],
                start,
                off_width,
                cfg,
                prefix_ref,
            ));
        }
        out
    } else {
        process_slice(bytes, 0, off_width, cfg, prefix_ref)
    };

    let has_matches = results.iter().any(|r| !r.is_empty());
    if has_matches && cfg.heading {
        let path_str = if file_path.as_os_str() == "-" {
            "stdin"
        } else {
            file_path
                .to_str()
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "non-UTF8 path"))?
        };
        if cfg.color && !write_out(stdout, SGR_BOLD)? {
            return Ok(());
        }
        if !write_out(stdout, path_str.as_bytes())? {
            return Ok(());
        }
        if cfg.color && !write_out(stdout, SGR_RESET)? {
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
    let gnu_offset = explicit_radix || !interactive;
    let color = interactive && !line_prefix && std::env::var_os("NO_COLOR").is_none();

    let files = if args.files.is_empty() {
        vec![PathBuf::from("-")]
    } else {
        args.files
    };

    let stdout = io::stdout();
    let mut stdout = io::BufWriter::new(stdout.lock());
    let scan_cfg = ScanConfig {
        min_len: args.min_len,
        whitespace: args.include_all_whitespace,
        data_only: args.data_only,
        show_offset,
        radix,
        color,
        gnu_offset,
        sep,
        line_prefix,
        heading,
    };

    for file_path in &files {
        if file_path.as_os_str() == "-" {
            let mut bytes = Vec::new();
            io::stdin().lock().read_to_end(&mut bytes)?;
            scan_bytes(&bytes, file_path, scan_cfg, &mut stdout)?;
        } else {
            let file = File::open(file_path)?;
            let mmap = unsafe { MmapOptions::new().map(&file)? };
            scan_bytes(&mmap, file_path, scan_cfg, &mut stdout)?;
        }
    }
    Ok(())
}
