/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Sizes and type boundaries for the guest's Objective-C type encodings.
pub(super) fn parse(bytes: &[u8]) -> (usize, u32, u32) {
    let mut pos = 0;
    while bytes.get(pos).is_some_and(|c| b"rnNoORV".contains(c)) {
        pos += 1;
    }
    let code = bytes[pos];
    pos += 1;
    let (size, alignment) = match code {
        b'v' => (0, 1),
        b'c' | b'C' | b'B' => (1, 1),
        b's' | b'S' => (2, 2),
        b'i' | b'I' | b'l' | b'L' | b'f' => (4, 4),
        b'q' | b'Q' => (8, 8),
        b'd' => (8, 4),
        b'@' => {
            if bytes.get(pos) == Some(&b'?') {
                pos += 1;
            }
            if bytes.get(pos) == Some(&b'"') {
                pos += 1;
                while bytes[pos] != b'"' {
                    pos += 1;
                }
                pos += 1;
            }
            (4, 4)
        }
        b':' | b'#' | b'*' => (4, 4),
        b'^' => {
            pos += parse(&bytes[pos..]).0;
            (4, 4)
        }
        b'[' => {
            let mut count = 0;
            while bytes[pos].is_ascii_digit() {
                count = count * 10 + u32::from(bytes[pos] - b'0');
                pos += 1;
            }
            let (read, size, alignment) = parse(&bytes[pos..]);
            pos += read;
            assert_eq!(bytes[pos], b']');
            pos += 1;
            (count * size, alignment)
        }
        b'{' | b'(' => {
            let close = if code == b'{' { b'}' } else { b')' };
            while bytes[pos] != b'=' && bytes[pos] != close {
                pos += 1;
            }
            let (mut size, mut alignment) = (0u32, 1u32);
            if bytes[pos] == b'=' {
                pos += 1;
                while bytes[pos] != close {
                    if bytes[pos] == b'"' {
                        pos += 1;
                        while bytes[pos] != b'"' {
                            pos += 1;
                        }
                        pos += 1;
                    }
                    let (read, field_size, field_alignment) = parse(&bytes[pos..]);
                    pos += read;
                    alignment = alignment.max(field_alignment);
                    size = if code == b'(' {
                        size.max(field_size)
                    } else {
                        size.div_ceil(field_alignment) * field_alignment + field_size
                    };
                }
            }
            pos += 1;
            (size.div_ceil(alignment) * alignment, alignment)
        }
        _ => panic!("Unsupported Objective-C type encoding: {}", code as char),
    };
    (pos, size, alignment)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn geometry_pointer_and_array_sizes() {
        assert_eq!(parse(b"{CGRect={CGPoint=ff}{CGSize=ff}}"), (32, 16, 4));
        assert_eq!(parse(b"^{Opaque}"), (9, 4, 4));
        assert_eq!(parse(b"[16f]"), (5, 64, 4));
        assert_eq!(parse(b"{Pair=cq}"), (9, 16, 8));
    }
}
