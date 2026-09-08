//! 项目域：登记（history.toml）与项目内条目（树/预览/文稿/目录）。

pub(crate) mod api;
pub(crate) mod files;
pub(crate) mod store;

pub(crate) use store::ensure_registered;
