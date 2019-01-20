//! GPU renderer for scene graphs.

#[macro_use]
extern crate vulkano;
#[macro_use]
extern crate failure_derive;
#[macro_use]
extern crate log;
pub extern crate narwhal_platform as platform;

pub mod data;
pub mod eval;
pub mod node;
pub mod render;
mod util;
