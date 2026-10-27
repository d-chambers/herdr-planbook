pub mod cli;
pub mod documents;
pub mod paths;
pub mod process;
pub mod store;
pub mod ui;

pub const SKILL: &str = include_str!("../skills/herdr-planbook/SKILL.md");
pub const GUIDE: &str = include_str!("../skills/herdr-planbook/references/projects.md");
pub const TEMPLATE: &str = include_str!("../skills/herdr-planbook/assets/project.md");
