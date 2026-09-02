use std::arch::x86_64::*;

use super::{classify, BLOCK, Step32};

fn mask32(ptr: *const u8) -> u32 {
    unsafe {
        let v = _mm256_loadu_si256(ptr as *const __m256i);
        let lo = _mm256_set1_epi8(0x1F);
        let hi = _mm256_set1_epi8(0x7F);
        let ge_space = _mm256_cmpgt_epi8(v, lo);
        let le_tilde = _mm256_cmpgt_epi8(hi, v);
        let in_range = _mm256_and_si256(ge_space, le_tilde);
        let tab = _mm256_cmpeq_epi8(v, _mm256_set1_epi8(b'\t' as i8));
        _mm256_movemask_epi8(_mm256_or_si256(in_range, tab)) as u32
    }
}

pub fn string_run(ptr: *const u8) -> usize {
    let mask = mask32(ptr);
    if mask == u32::MAX {
        return BLOCK;
    }
    if mask & 1 != 0 {
        return mask.trailing_ones() as usize;
    }
    0
}

pub fn step32(ptr: *const u8, in_run: bool) -> Step32 {
    classify(mask32(ptr), in_run)
}
