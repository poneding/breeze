use crate::Pixels;
use anyhow::{Result, ensure};
use breeze_core::Rotation;

/// Largest device region Breeze may request in one call, in pixels.
///
/// A whole viewport is rendered as a single region so its pixels arrive in one
/// result instead of one job per 508px cell. The renderer applies the same bound
/// to its output allocation, so this is a shared contract, not a caller hint.
/// The 12 Mpx ceiling is about 48 MiB of RGBA output and exceeds one
/// 2304-logical-px window's viewport at 2x display scale.
pub const MAX_REGION_PIXELS: u64 = 12 * 1024 * 1024;

/// A region in the rotated page's physical-pixel coordinate system.
/// Scale is pixels per PDF point, including the native display scale.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DeviceTile {
    scale_bits: u32,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}
impl DeviceTile {
    pub fn new(scale: f32, x: u32, y: u32, width: u32, height: u32) -> Result<Self> {
        ensure!(
            scale.is_finite() && scale > 0. && scale <= 64.,
            "不支持的页面缩放"
        );
        ensure!(
            width > 0 && height > 0 && u64::from(width) * u64::from(height) <= MAX_REGION_PIXELS,
            "渲染区域超出预算"
        );
        ensure!(
            u64::from(x) + u64::from(width) <= 16_777_216
                && u64::from(y) + u64::from(height) <= 16_777_216,
            "页面尺寸超出精度范围"
        );
        Ok(Self {
            scale_bits: scale.to_bits(),
            x,
            y,
            width,
            height,
        })
    }
    pub fn scale(self) -> f32 {
        f32::from_bits(self.scale_bits)
    }
    pub fn region(self) -> (u32, u32, u32, u32) {
        (self.x, self.y, self.width, self.height)
    }
}

/// Maps a tile of the rotated view onto the region to rasterize in the page's
/// own device grid. `points` is the page size before the view rotation.
///
/// A quarter turn maps the pixel grid onto itself, so every rotated tile has
/// an axis-aligned source region whose width and height are exchanged. Both
/// grids use the same `ceil(points * scale)` extent, which is why the mapping
/// stays exact instead of drifting by a pixel per tile.
pub(super) fn tile_source_region(
    tile: DeviceTile,
    rotation: Rotation,
    points: (f32, f32),
) -> Result<(u32, u32, u32, u32)> {
    let scale = tile.scale();
    let full_w = (f64::from(points.0) * f64::from(scale)).ceil() as i64;
    let full_h = (f64::from(points.1) * f64::from(scale)).ceil() as i64;
    let (x, y, w, h) = tile.region();
    let (x, y, w, h) = (i64::from(x), i64::from(y), i64::from(w), i64::from(h));
    // Signed arithmetic keeps a tile outside the page detectable instead of
    // wrapping into a plausible but wrong region.
    let (left, top, width, height) = match rotation {
        Rotation::None => (x, y, w, h),
        // The view column comes from a source row, so the extents exchange.
        Rotation::Right => (y, full_h - x - w, h, w),
        Rotation::Half => (full_w - x - w, full_h - y - h, w, h),
        Rotation::Left => (full_w - y - h, x, h, w),
    };
    ensure!(
        left >= 0
            && top >= 0
            && width > 0
            && height > 0
            && left + width <= full_w
            && top + height <= full_h,
        "旋转后的分块超出页面"
    );
    Ok((left as u32, top as u32, width as u32, height as u32))
}

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
        pixels.bgra.chunks_exact(4).map(|pixel| pixel[0]).collect()
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

    #[test]
    fn rotated_tiles_cover_the_rotated_page_without_gaps() {
        let scale = 2.;
        let points = (600., 800.);
        let tile = DeviceTile::new(scale, 510, 510, 512, 512).unwrap();
        assert_eq!(
            tile_source_region(tile, Rotation::None, points).unwrap(),
            (510, 510, 512, 512)
        );
        // A right turn reads a source column block and swaps the extents.
        assert_eq!(
            tile_source_region(tile, Rotation::Right, points).unwrap(),
            (510, 1600 - 510 - 512, 512, 512)
        );
        let right = rotate_region(image(512, 512), Rotation::Right);
        assert_eq!((right.width, right.height), (512, 512));
        let edge = DeviceTile::new(scale, 688, 0, 512, 512).unwrap();
        assert_eq!(
            tile_source_region(edge, Rotation::Half, points).unwrap(),
            (1200 - 688 - 512, 1600 - 512, 512, 512)
        );
        assert_eq!(
            tile_source_region(edge, Rotation::Left, points).unwrap(),
            (1200 - 512, 688, 512, 512)
        );
        assert!(tile_source_region(edge, Rotation::Right, points).is_ok());
    }

    #[test]
    fn rotated_tiles_stay_inside_the_page_bounds() {
        let points = (612., 792.);
        let scale = 3.;
        let full = (points.0 * scale) as u32;
        let tall = (points.1 * scale) as u32;
        let rotation = Rotation::Right;
        // Every tile of the rotated grid must map into the page's own grid.
        let mut x = 0;
        while x < tall {
            let mut y = 0;
            while y < full {
                let w = 508.min(tall - x);
                let h = 508.min(full - y);
                let tile = DeviceTile::new(scale, x, y, w, h).unwrap();
                let region = tile_source_region(tile, rotation, points).unwrap();
                assert!(u64::from(region.0) + u64::from(region.2) <= u64::from(full));
                assert!(u64::from(region.1) + u64::from(region.3) <= u64::from(tall));
                // The source region is exchanged for a quarter turn and the
                // rotated result must come back in the view tile's own shape.
                let pixels = rotate_region(image(region.2, region.3), rotation);
                assert_eq!((pixels.width, pixels.height), (w, h));
                y += h;
            }
            x += 508;
        }
    }
}
