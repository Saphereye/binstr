pub fn data_ranges(bytes: &[u8]) -> Option<Vec<(usize, usize)>> {
    let exclude = exec_sections(bytes)?;
    Some(invert_ranges(exclude, bytes.len()))
}

fn exec_sections(bytes: &[u8]) -> Option<Vec<(usize, usize)>> {
    if bytes.len() < 64 || bytes.get(0..4) != Some(b"\x7fELF") || bytes[4] != 2 {
        return None;
    }
    let le = bytes[5] == 1;
    let read_u16 = |off: usize| -> u16 {
        if le {
            u16::from_le_bytes(bytes[off..off + 2].try_into().unwrap())
        } else {
            u16::from_be_bytes(bytes[off..off + 2].try_into().unwrap())
        }
    };
    let read_u64 = |off: usize| -> u64 {
        if le {
            u64::from_le_bytes(bytes[off..off + 8].try_into().unwrap())
        } else {
            u64::from_be_bytes(bytes[off..off + 8].try_into().unwrap())
        }
    };

    let shoff = usize::try_from(read_u64(40)).ok()?;
    let shentsize = usize::from(read_u16(58));
    let shnum = usize::from(read_u16(60));
    if shentsize < 64 || shoff.saturating_add(shnum.saturating_mul(shentsize)) > bytes.len() {
        return None;
    }

    let mut exclude = Vec::new();
    for n in 0..shnum {
        let i = shoff + n * shentsize;
        let sh_flags = read_u64(i + 8);
        if sh_flags & 0x4 == 0 {
            continue;
        }
        let sh_offset = usize::try_from(read_u64(i + 24)).ok()?;
        let sh_size = usize::try_from(read_u64(i + 32)).ok()?;
        if sh_size == 0 {
            continue;
        }
        let end = sh_offset.saturating_add(sh_size);
        if end <= bytes.len() && end > sh_offset {
            exclude.push((sh_offset, end));
        }
    }

    if exclude.is_empty() {
        None
    } else {
        exclude.sort_unstable_by_key(|r| r.0);
        Some(exclude)
    }
}

fn invert_ranges(exclude: Vec<(usize, usize)>, len: usize) -> Vec<(usize, usize)> {
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (start, end) in exclude {
        if let Some(last) = merged.last_mut()
            && start <= last.1
        {
            last.1 = last.1.max(end);
            continue;
        }
        merged.push((start, end));
    }

    let mut include = Vec::new();
    let mut pos = 0;
    for (start, end) in merged {
        if start > pos {
            include.push((pos, start));
        }
        pos = pos.max(end);
    }
    if pos < len {
        include.push((pos, len));
    }
    include
}
