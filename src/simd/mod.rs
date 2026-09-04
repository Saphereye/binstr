pub enum Step32 {
    Blank(usize),
    Solid(usize),
    Closed(usize),
    Opened { skip: usize, len: usize },
}

pub const BLOCK: usize = 32;
pub const PREFETCH_AHEAD: usize = 512;

#[inline(always)]
pub unsafe fn prefetch(base: *const u8, off: usize, len: usize) {
    let ahead = off + PREFETCH_AHEAD;
    if ahead <= len {
        #[cfg(target_arch = "x86_64")]
        unsafe {
            use std::arch::x86_64::{_MM_HINT_T0, _mm_prefetch};
            _mm_prefetch(base.add(ahead).cast(), _MM_HINT_T0);
        }
    }
}

pub const fn classify(mask: u32, in_run: bool) -> Step32 {
    if mask == 0 {
        return Step32::Blank(BLOCK);
    }
    if mask == u32::MAX {
        return Step32::Solid(BLOCK);
    }
    if in_run {
        let run = mask.trailing_ones() as usize;
        return if run < BLOCK {
            Step32::Closed(run)
        } else {
            Step32::Solid(BLOCK)
        };
    }
    let skip = mask.trailing_zeros() as usize;
    let run = (mask >> skip).trailing_ones() as usize;
    Step32::Opened { skip, len: run }
}

#[cfg(target_feature = "avx2")]
mod avx2;
#[cfg(target_feature = "avx2")]
pub use avx2::*;

#[cfg(not(target_feature = "avx2"))]
mod scalar;
#[cfg(not(target_feature = "avx2"))]
pub use scalar::*;
