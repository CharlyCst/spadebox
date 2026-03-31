#![deny(clippy::all)]

use napi_derive::napi;
use spadebox_core::{
    Sandbox,
    tools::{EditFileTool, EditParams, ReadFileTool, ReadParams, Tool, WriteFileTool, WriteParams},
};

fn to_napi_err(e: spadebox_core::SpadeboxError) -> napi::Error {
    napi::Error::from_reason(e.to_string())
}

#[napi]
pub struct SpadeBox {
    inner: Sandbox,
}

#[napi]
impl SpadeBox {
    #[napi(constructor)]
    pub fn new(root: String) -> napi::Result<Self> {
        Sandbox::new(&root)
            .map(|inner| SpadeBox { inner })
            .map_err(to_napi_err)
    }

    #[napi]
    pub async fn read_file(&self, path: String) -> napi::Result<String> {
        ReadFileTool::run(&self.inner, ReadParams { path })
            .await
            .map_err(to_napi_err)
    }

    #[napi]
    pub async fn write_file(&self, path: String, content: String) -> napi::Result<String> {
        WriteFileTool::run(&self.inner, WriteParams { path, content })
            .await
            .map_err(to_napi_err)
    }

    #[napi]
    pub async fn edit_file(
        &self,
        path: String,
        old_string: String,
        new_string: String,
        replace_all: Option<bool>,
    ) -> napi::Result<String> {
        EditFileTool::run(
            &self.inner,
            EditParams {
                path,
                old_string,
                new_string,
                replace_all: replace_all.unwrap_or(false),
            },
        )
        .await
        .map_err(to_napi_err)
    }
}
