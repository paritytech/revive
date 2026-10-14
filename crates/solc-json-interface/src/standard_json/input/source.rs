//! The `solc --standard-json` input source.

use std::path::Path;
use std::path::PathBuf;
use std::sync::OnceLock;

use serde::Deserialize;
use serde::Serialize;

/// The input path that selects the standard input.
pub const STDIN_PATH: &str = "-";

/// The source unit name `solc` gives to the standard input.
pub const STDIN_SOURCE_NAME: &str = "<stdin>";

/// The standard input content, read at most once since it can only be consumed once.
static STDIN_CONTENT: OnceLock<Result<String, String>> = OnceLock::new();

/// The `solc --standard-json` input source.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    /// The source code file content.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// The source file URLs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub urls: Option<Vec<String>>,
}

impl Source {
    /// Returns the source unit name of the input `path`.
    pub fn name(path: &Path) -> String {
        let name = path.to_string_lossy();
        if name == STDIN_PATH {
            STDIN_SOURCE_NAME.to_owned()
        } else {
            name.to_string()
        }
    }

    /// Reads the source from the file system, or from the standard input if `path` is [`STDIN_PATH`].
    ///
    /// The standard input is consumed on the first read and the same content is returned afterwards.
    pub fn try_read(path: &Path) -> anyhow::Result<Self> {
        let content = if path.to_string_lossy() == STDIN_PATH {
            STDIN_CONTENT
                .get_or_init(|| {
                    std::io::read_to_string(std::io::stdin()).map_err(|error| error.to_string())
                })
                .clone()
                .map_err(|error| anyhow::anyhow!("{STDIN_SOURCE_NAME} reading: {error}"))
        } else {
            std::fs::read_to_string(path)
                .map_err(|error| anyhow::anyhow!("File {path:?} reading: {error}"))
        }?;

        Ok(Self {
            content: Some(content),
            urls: None,
        })
    }

    /// Tries to resolve the source code.
    ///
    /// At the moment only one URL pointing to the file system is supported.
    pub fn try_resolve(&mut self) -> anyhow::Result<()> {
        match (self.content.as_ref(), self.urls.as_ref()) {
            (Some(_), None) => Ok(()),
            (None, Some(urls)) => {
                let mut errors = Vec::with_capacity(urls.len());
                for url in urls.iter() {
                    let url_path = PathBuf::from(url);
                    match Source::try_read(url_path.as_path()) {
                        Ok(resolved) => {
                            *self = resolved;
                            break;
                        }
                        Err(error) => errors.push(error),
                    }
                }
                if !errors.is_empty() {
                    anyhow::bail!(
                        "{}",
                        errors
                            .into_iter()
                            .map(|error| error.to_string())
                            .collect::<Vec<String>>()
                            .join("\n")
                    );
                }
                Ok(())
            }
            (Some(_), Some(_)) => anyhow::bail!("Both `content` and `urls` cannot be set."),
            (None, None) => anyhow::bail!("Either `content` or `urls` must be set."),
        }
    }

    /// Takes ownership of the source code and returns it.
    pub fn take_content(&mut self) -> Option<String> {
        self.content.take()
    }

    /// Returns the source code reference, if the source has been previously read or resolved.
    pub fn content(&self) -> Option<&str> {
        self.content.as_deref()
    }
}

impl From<String> for Source {
    fn from(content: String) -> Self {
        Self {
            content: Some(content),
            urls: None,
        }
    }
}

impl From<&Path> for Source {
    fn from(path: &Path) -> Self {
        Self {
            content: None,
            urls: Some(vec![path.to_string_lossy().to_string()]),
        }
    }
}
