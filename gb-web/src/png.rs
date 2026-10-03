//! Minimal PNG encoder (8-bit greyscale and 8-bit RGB) and zlib. Uses only
//! stored (uncompressed) deflate blocks, so any valid PNG decoder accepts it.

/// CRC-32 (IEEE 802.3) as used by PNG.
fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= u32::from(b);
        for _ in 0..8 {
            let mask = 0u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

/// Adler-32 as used by zlib.
fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in data {
        a = (a + u32::from(byte)) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

/// Wrap `data` in a zlib stream using stored deflate blocks.
fn zlib_stored(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x01];
    let mut chunks = data.chunks(65535).peekable();
    // `data` is never empty for a frame, but handle it defensively.
    if data.is_empty() {
        out.extend_from_slice(&[0x01, 0x00, 0x00, 0xFF, 0xFF]);
    }
    while let Some(chunk) = chunks.next() {
        let last = chunks.peek().is_none();
        out.push(if last { 1 } else { 0 });
        let n = chunk.len() as u16;
        out.extend_from_slice(&n.to_le_bytes());
        out.extend_from_slice(&(!n).to_le_bytes());
        out.extend_from_slice(chunk);
    }
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

/// One PNG chunk: length, type, payload, CRC.
fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], payload: &[u8]) {
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    let mut crc_input = Vec::with_capacity(4 + payload.len());
    crc_input.extend_from_slice(kind);
    crc_input.extend_from_slice(payload);
    out.extend_from_slice(&crc_input);
    out.extend_from_slice(&crc32(&crc_input).to_be_bytes());
}

/// Encode an 8-bit PNG. `channels` is 1 (greyscale) or 3 (RGB); `pixels`
/// holds `width * height * channels` samples in row-major order.
pub fn encode(width: u32, height: u32, channels: u8, pixels: &[u8]) -> Vec<u8> {
    const SIG: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let color_type = if channels == 3 { 2u8 } else { 0u8 };
    let stride = width as usize * channels as usize;

    let mut raw = Vec::with_capacity((stride + 1) * height as usize);
    for row in 0..height as usize {
        raw.push(0); // filter type: none
        let start = row * stride;
        raw.extend_from_slice(&pixels[start..start + stride]);
    }

    let mut out = Vec::new();
    out.extend_from_slice(&SIG);
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, color_type, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &zlib_stored(&raw));
    chunk(&mut out, b"IEND", &[]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_header_and_size() {
        let px = vec![0u8; 4 * 3 * 3];
        let png = encode(4, 3, 3, &px);
        assert_eq!(&png[..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
        assert_eq!(&png[8..12], &(13u32).to_be_bytes());
        assert_eq!(&png[12..16], b"IHDR");
        assert_eq!(&png[16..20], &4u32.to_be_bytes());
        assert_eq!(&png[20..24], &3u32.to_be_bytes());
        assert_eq!(png[24], 8);
        assert_eq!(png[25], 2);
        assert!(png.ends_with(&[0xAE, 0x42, 0x60, 0x82]));
    }

    #[test]
    fn crc_known_vectors() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    }
}
