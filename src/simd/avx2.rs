use std::arch::x86_64::{
    __m256i, _mm256_and_si256, _mm256_cmpeq_epi8, _mm256_cmpgt_epi8, _mm256_loadu_si256,
    _mm256_movemask_epi8, _mm256_or_si256, _mm256_set1_epi8,
};

use super::{BLOCK, Step32, classify};

fn mask32(ptr: *const u8) -> u32 {
    #[expect(clippy::cast_ptr_alignment)]
    unsafe {
        let v = _mm256_loadu_si256(ptr.cast::<__m256i>());
        let lo = _mm256_set1_epi8(0x1F);
        let hi = _mm256_set1_epi8(0x7F);
        let ge_space = _mm256_cmpgt_epi8(v, lo);
        let le_tilde = _mm256_cmpgt_epi8(hi, v);
        let in_range = _mm256_and_si256(ge_space, le_tilde);
        let tab = _mm256_cmpeq_epi8(v, _mm256_set1_epi8(b'\t'.cast_signed()));
        _mm256_movemask_epi8(_mm256_or_si256(in_range, tab)).cast_unsigned()
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
