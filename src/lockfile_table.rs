//! This module defines the syntax of `upstream.lock`.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LockfileTable {
    #[serde(default, rename = "capsule")]
    pub capsules: Vec<LockedCapsuleTable>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LockedCapsuleTable {
    pub name: String,
    pub source: String,
    pub src: PathBuf,
    pub dst: PathBuf,
    pub filter: Option<String>,
}
