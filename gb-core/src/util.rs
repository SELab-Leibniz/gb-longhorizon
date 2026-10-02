//! Small dependency-free helpers shared by the core, CLI and harness.

/// 64-bit FNV-1a hash. Used to fingerprint framebuffers; the harness
/// compares these against golden values produced by a reference emulator,
/// so the algorithm must not change.
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    bytes
        .iter()
        .fold(OFFSET, |h, &b| (h ^ u64::from(b)).wrapping_mul(PRIME))
}

/// Encode a 160×144 shade buffer as binary PGM (P5), 0 = black … 255 = white,
/// so frames can be opened by any image viewer without a PNG dependency.
pub fn framebuffer_to_pgm(fb: &[u8], width: usize, height: usize) -> Vec<u8> {
    debug_assert_eq!(fb.len(), width * height);
    let mut out = format!("P5\n{width} {height}\n255\n").into_bytes();
    out.extend(fb.iter().map(|&shade| match shade {
        0 => 255u8,
        1 => 170,
        2 => 85,
        _ => 0,
    }));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv_known_vectors() {
        // Standard FNV-1a test vectors.
        assert_eq!(fnv1a64(b""), 0xcbf29ce484222325);
        assert_eq!(fnv1a64(b"a"), 0xaf63dc4c8601ec8c);
        assert_eq!(fnv1a64(b"foobar"), 0x85944171f73967e8);
    }

    #[test]
    fn pgm_header_and_size() {
        let fb = vec![0u8, 1, 2, 3];
        let pgm = framebuffer_to_pgm(&fb, 2, 2);
        assert!(pgm.starts_with(b"P5\n2 2\n255\n"));
        assert_eq!(&pgm[pgm.len() - 4..], &[255, 170, 85, 0]);
    }
}
