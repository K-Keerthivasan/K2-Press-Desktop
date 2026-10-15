//! Post Studio data (K2 Press on PhotoCraft): channel brands, post plans, CSV import, and the post
//! layouts, which compile to plain engine command steps (`file.new`, `shape.create`, `type.create`,
//! `file.placeEmbedded`, …). A built post is therefore an ordinary layered document: every
//! headline, image and shape stays editable with the rest of PhotoCraft.
//!
//! Pure data and functions: no file system, network or clock, so this crate builds everywhere
//! (wasm included). The library, AI and publishing live in `photocraft-studio`.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod brand;
pub mod csv;
pub mod layout;
pub mod plan;
pub mod text;

pub use brand::{Brand, ContentType, Theme};
pub use layout::{Format, SlideKind, SlideSpec, Step, Style, slides};
pub use plan::{ContentSlide, OutroCard, Plan, TitleCard};

#[cfg(test)]
mod tests;
