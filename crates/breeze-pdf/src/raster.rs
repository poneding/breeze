use anyhow::{Result, ensure};

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
            width > 0 && height > 0 && width <= 512 && height <= 512,
            "分块尺寸超出预算"
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
