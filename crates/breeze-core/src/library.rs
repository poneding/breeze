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
pub fn validate_name(
    name: &str,
    categories: &[Category],
    editing: Option<usize>,
) -> Result<String, &'static str> {
    let name = name.trim();
    if name == "未分类" {
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
        for name in ["", "\n", "a\tb", &"a".repeat(41)] {
            assert!(validate_name(name, &categories, None).is_err());
        }
    }
}
