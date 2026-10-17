use serde::{Deserialize, Serialize};

/// Portable slash-separated project-relative file path. Deserialization uses
/// exactly the same validation as editor construction.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ProjectPath(String);

impl ProjectPath {
    /// # Errors
    /// Rejects traversal, absolute/device paths, ambiguous Windows basenames,
    /// control characters and paths exceeding 240 bytes.
    pub fn new(value: String) -> Result<Self, String> {
        if value.is_empty()
            || value.len() > 240
            || value.contains('\\')
            || value.chars().any(|character| {
                character.is_control()
                    || matches!(character, ':' | '*' | '?' | '"' | '<' | '>' | '|')
            })
            || value.split('/').any(invalid_component)
        {
            return Err(format!("unsafe project-relative path '{value}'"));
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub(super) fn require_role(&self, directory: &str, extension: &str) -> Result<(), String> {
        let (parent, _) = self
            .0
            .split_once('/')
            .ok_or("project file needs a role directory")?;
        if parent != directory
            || std::path::Path::new(&self.0)
                .extension()
                .is_none_or(|actual| actual != extension)
        {
            return Err(format!(
                "project path '{}' must be under {directory}/ with .{extension} extension",
                self.0
            ));
        }
        Ok(())
    }
}

fn invalid_component(component: &str) -> bool {
    if component.is_empty()
        || component == "."
        || component == ".."
        || component.ends_with(['.', ' '])
        || component.starts_with(' ')
    {
        return true;
    }
    let base = component
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ["COM", "LPT"].iter().any(|prefix| {
            base.strip_prefix(prefix).is_some_and(|suffix| {
                matches!(suffix, "¹" | "²" | "³")
                    || (suffix.len() == 1 && matches!(suffix.as_bytes().first(), Some(b'1'..=b'9')))
            })
        })
}

impl TryFrom<String> for ProjectPath {
    type Error = String;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<ProjectPath> for String {
    fn from(value: ProjectPath) -> Self {
        value.0
    }
}
