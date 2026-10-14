//! User-owned categories with stable nonzero identifiers.
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Category {
    pub id: usize,
    pub name: String,
    #[serde(default)]
    pub color: CategoryColor,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CategoryColor {
    Red,
    Orange,
    Amber,
    Green,
    Teal,
    #[default]
    Blue,
    Violet,
    Pink,
}
impl CategoryColor {
    pub const ALL: [Self; 8] = [
        Self::Red,
        Self::Orange,
        Self::Amber,
        Self::Green,
        Self::Teal,
        Self::Blue,
        Self::Violet,
        Self::Pink,
    ];
}
/// Display names the library reserves for documents without a category. The
/// Unassigned filter is always present, so a user category may not reuse its
/// label in any supported interface language; otherwise the sidebar shows two
/// indistinguishable entries.
const RESERVED_CATEGORY_NAMES: [&str; 3] = ["Unassigned", "未分类", "未分類"];

pub fn validate_name(
    name: &str,
    categories: &[Category],
    editing: Option<usize>,
) -> Result<String, &'static str> {
    let name = name.trim();
    if RESERVED_CATEGORY_NAMES
        .iter()
        .any(|reserved| reserved.eq_ignore_ascii_case(name))
    {
        return Err("此名称保留给未分类文档");
    }
    if name.is_empty() || name.chars().count() > 40 || name.chars().any(char::is_control) {
        return Err("分类名称需为 1–40 个字符，不能含控制字符");
    }
    if categories
        .iter()
        .any(|c| Some(c.id) != editing && c.name.to_lowercase() == name.to_lowercase())
    {
        return Err("此分类名称已存在");
    }
    Ok(name.to_owned())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn names_are_trimmed_bounded_and_unique() {
        let categories = vec![Category {
            id: 1,
            name: "Work".into(),
            color: CategoryColor::default(),
        }];
        assert_eq!(
            validate_name(" Work ", &categories, Some(1)),
            Ok("Work".into())
        );
        assert!(validate_name("work", &categories, None).is_err());
        // Every language's Unassigned label is reserved, including case variants.
        for reserved in ["Unassigned", "unassigned", "未分类", "未分類"] {
            assert!(
                validate_name(reserved, &categories, None).is_err(),
                "{reserved} must stay reserved"
            );
        }
        for name in ["", "\n", "a\tb", &"a".repeat(41)] {
            assert!(validate_name(name, &categories, None).is_err());
        }
    }
}
