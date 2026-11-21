//! Camera image planes (#459): pure plate decoding and frame naming. No UI, renderer or editor
//! state lives here, so the library tests reach it directly.

pub mod cache;
pub mod decode;
pub mod model;
pub mod ops;
pub mod sequence;
pub mod worker;
