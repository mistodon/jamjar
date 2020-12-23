#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(dead_code)]

use std::borrow::Cow;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[allow(non_camel_case_types)]
pub struct Config {
    pub kind: Cow<'static, str>,
    pub lines: i32,
    pub name: Cow<'static, str>,
}

