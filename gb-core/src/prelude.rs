//! Re-exports of the `alloc` items the core uses, so the rest of the crate
//! compiles unchanged with and without the `std` feature.

pub(crate) use alloc::boxed::Box;
pub(crate) use alloc::format;
pub(crate) use alloc::string::String;
pub(crate) use alloc::vec;
pub(crate) use alloc::vec::Vec;
