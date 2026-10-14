use crate::Pixels;
use breeze_core::Rotation;

/// Rotates a rendered region clockwise by `rotation`, in place of a
/// transform, because PDF rendering has no view-rotation input. Quarter turns
/// exchange the output width and height; RGB channels keep their order.
pub(super) fn rotate_region(pixels: Pixels, rotation: Rotation) -> Pixels {
    let Pixels {
        width,
        height,
        bgra,
    } = pixels;
    if rotation == Rotation::None {
        return Pixels {
            width,
            height,
            bgra,
        };
    }
    let src_w = width as usize;
    let src_h = height as usize;
    let (out_w, out_h) = if rotation.swaps_axes() {
        (src_h, src_w)
    } else {
        (src_w, src_h)
    };
    let mut out = vec![0; bgra.len()];
    for row in 0..src_h {
        for column in 0..src_w {
            // Destination of one source pixel under a clockwise quarter turn,
            // half turn, or counterclockwise quarter turn.
            let (out_column, out_row) = match rotation {
                Rotation::Right => (src_h - 1 - row, column),
                Rotation::Half => (src_w - 1 - column, src_h - 1 - row),
                Rotation::Left => (row, src_w - 1 - column),
                Rotation::None => (column, row),
            };
            let source = (row * src_w + column) * 4;
            let target = (out_row * out_w + out_column) * 4;
            out[target..target + 4].copy_from_slice(&bgra[source..source + 4]);
        }
    }
    Pixels {
        width: out_w as u32,
        height: out_h as u32,
        bgra: out,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(width: u32, height: u32) -> Pixels {
        // One distinct first-byte value per source pixel identifies the mapping.
        let bytes = (0..width * height)
            .flat_map(|index| [index as u8, 0, 0, 255])
            .collect();
        Pixels {
            width,
            height,
            bgra: bytes,
        }
    }

    fn first_bytes(pixels: &Pixels) -> Vec<u8> {
        pixels
            .bgra
            .as_chunks::<4>()
            .0
            .iter()
            .map(|pixel| pixel[0])
            .collect()
    }

    #[test]
    fn quarter_turns_swap_dimensions_and_keep_every_pixel() {
        let source = image(3, 2);
        // Source rows: 0 1 2 / 3 4 5.
        let right = rotate_region(source.clone(), Rotation::Right);
        assert_eq!((right.width, right.height), (2, 3));
        assert_eq!(first_bytes(&right), vec![3, 0, 4, 1, 5, 2]);
        let left = rotate_region(source.clone(), Rotation::Left);
        assert_eq!((left.width, left.height), (2, 3));
        assert_eq!(first_bytes(&left), vec![2, 5, 1, 4, 0, 3]);
        let half = rotate_region(source, Rotation::Half);
        assert_eq!((half.width, half.height), (3, 2));
        assert_eq!(first_bytes(&half), vec![5, 4, 3, 2, 1, 0]);
        assert!(rotate_region(image(3, 2), Rotation::None).bgra[0] == 0);
    }

    #[test]
    fn four_quarter_turns_restore_the_source_pixels() {
        let source = image(5, 4);
        let mut turned = source.clone();
        for _ in 0..4 {
            turned = rotate_region(turned, Rotation::Right);
        }
        assert_eq!((turned.width, turned.height), (5, 4));
        assert_eq!(turned.bgra, source.bgra);
    }
}
