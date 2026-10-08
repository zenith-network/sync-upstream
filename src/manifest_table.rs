//! This module defines the syntax of `upstream.toml`.

use std::{collections::BTreeMap, path::PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ManifestTable {
    #[serde(default)]
    pub default: DefaultTable,
    #[serde(default)]
    pub capsules: BTreeMap<String, CapsuleTable>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Default)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct DefaultTable {
    pub git: Option<GitSourceTable>,
    pub path: Option<PathBuf>,
    pub filter: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct CapsuleTable {
    pub src: PathBuf,
    pub dst: PathBuf,
    pub git: Option<GitSourceTable>,
    pub path: Option<PathBuf>,
    pub filter: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct GitSourceTable {
    pub url: Option<String>,
    #[serde(flatten)]
    pub ref_: GitRefValues,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct GitRefValues {
    pub rev: Option<String>,
    pub tag: Option<String>,
    pub branch: Option<String>,
}
