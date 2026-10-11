use breeze_core::library::CategoryColor;

pub(super) fn label(color: CategoryColor) -> &'static str {
    match color {
        CategoryColor::Red => tr!("红色"),
        CategoryColor::Orange => tr!("橙色"),
        CategoryColor::Amber => tr!("琥珀色"),
        CategoryColor::Green => tr!("绿色"),
        CategoryColor::Teal => tr!("青色"),
        CategoryColor::Blue => tr!("蓝色"),
        CategoryColor::Violet => tr!("紫色"),
        CategoryColor::Pink => tr!("粉色"),
    }
}

// Document colors must remain stable when the application changes appearance.
pub(super) fn hex(color: CategoryColor) -> u32 {
    match color {
        CategoryColor::Red => 0xef4444,
        CategoryColor::Orange => 0xf97316,
        CategoryColor::Amber => 0xf59e0b,
        CategoryColor::Green => 0x22c55e,
        CategoryColor::Teal => 0x14b8a6,
        CategoryColor::Blue => 0x3b82f6,
        CategoryColor::Violet => 0x8b5cf6,
        CategoryColor::Pink => 0xec4899,
    }
}

pub(super) fn components(color: CategoryColor) -> [f32; 3] {
    let value = hex(color);
    [
        ((value >> 16) & 255) as f32 / 255.,
        ((value >> 8) & 255) as f32 / 255.,
        (value & 255) as f32 / 255.,
    ]
}
