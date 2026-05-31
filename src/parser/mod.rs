pub mod dom;
pub mod links;

pub use dom::Dom;
pub use links::{discover_links, is_same_site};
