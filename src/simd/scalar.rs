use super::{BLOCK, Step32, classify};

#[inline(always)]
fn is_string_byte(b: u8) -> bool {
    let in_range = (b.wrapping_sub(0x20) <= 0x5E) as u8;
    let is_tab = (b == b'\t') as u8;
    (in_range | is_tab) != 0
}

fn mask32(ptr: *const u8) -> u32 {
    let mut mask = 0u32;
    for i in 0..BLOCK {
        if is_string_byte(unsafe { *ptr.add(i) }) {
            mask |= 1 << i;
        }
    }
    mask
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
