use std::{fs, io, path::Path};

use miette::{Diagnostic, LabeledSpan, NamedSource, SourceSpan};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use toml_span::{
    DeserError, Deserialize as TomlDeserialize, Error as TomlError, ErrorKind, Span, Value,
    de_helpers::TableHelper,
};

#[derive(Debug, Error, Diagnostic)]
pub enum ConfigError {
    #[error("failed to read demo config at {}", path.display())]
    #[diagnostic(code(sepia::config::read_failed))]
    Read {
        path: std::path::PathBuf,
        #[source]
        source: io::Error,
    },
    #[error(transparent)]
    #[diagnostic(transparent)]
    Invalid(Box<InvalidConfigError>),
}

#[derive(Debug, Error, Diagnostic)]
#[error("{message} at {}", path.display())]
#[diagnostic(code(sepia::config::invalid))]
pub struct InvalidConfigError {
    message: String,
    path: std::path::PathBuf,
    #[source_code]
    src: NamedSource<String>,
    #[label(collection)]
    labels: Vec<LabeledSpan>,
    #[help]
    help: Option<String>,
}

#[derive(Debug, Error, Diagnostic)]
#[diagnostic(code(sepia::config::invalid))]
pub enum ConfigValidationError {
    #[error("demo config `name` must not be empty")]
    EmptyName,
    #[error("demo config `url` must not be empty")]
    EmptyUrl,
    #[error("capture.output_fps must be greater than 0")]
    EmptyOutputFps,
    #[error("step name must not be empty")]
    EmptyStepName,
    #[error(
        "step `{step}` has multiple actions. Use exactly one of wait_ms, eval, fill, or scroll"
    )]
    MultipleStepActions { step: String },
    #[error("step `{step}` frames must be greater than 0")]
    EmptyFrames { step: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct DemoConfig {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub url: String,
    #[serde(default)]
    pub session: Option<String>,
    #[serde(default)]
    pub capture: CaptureConfig,
    #[serde(default)]
    pub steps: Vec<StepConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct CaptureConfig {
    #[serde(default = "default_output_fps")]
    pub output_fps: u32,
    #[serde(default = "default_hold_ms")]
    pub default_hold_ms: u64,
    #[serde(default = "default_action_ms")]
    pub default_action_ms: u64,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            output_fps: default_output_fps(),
            default_hold_ms: default_hold_ms(),
            default_action_ms: default_action_ms(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct StepConfig {
    pub name: String,
    #[serde(default)]
    pub wait_ms: Option<u64>,
    #[serde(default)]
    pub eval: Option<String>,
    #[serde(default)]
    pub fill: Option<FillConfig>,
    #[serde(default)]
    pub scroll: Option<ScrollConfig>,
    #[serde(default)]
    pub hold_ms: Option<u64>,
    #[serde(default)]
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub frames: Option<u32>,
    #[serde(default)]
    pub screenshot: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct FillConfig {
    pub selector: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ScrollConfig {
    pub selector: String,
    pub pixels: i64,
}

impl DemoConfig {
    pub fn from_path(path: &Path) -> Result<Self, ConfigError> {
        let text = fs::read_to_string(path).map_err(|source| ConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        Self::from_str(&text, path)
    }

    pub fn from_str(text: &str, source_name: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = source_name.as_ref();
        let mut value = toml_span::parse(text).map_err(|error| {
            invalid_config_error(path, text, "failed to parse TOML demo config", vec![error])
        })?;
        <Self as TomlDeserialize>::deserialize(&mut value).map_err(|error| {
            invalid_config_error(path, text, "invalid Sepia demo config", error.errors)
        })
    }

    pub fn validate(&self) -> Result<(), ConfigValidationError> {
        if self.name.trim().is_empty() {
            return Err(ConfigValidationError::EmptyName);
        }
        if self.url.trim().is_empty() {
            return Err(ConfigValidationError::EmptyUrl);
        }
        if self.capture.output_fps == 0 {
            return Err(ConfigValidationError::EmptyOutputFps);
        }
        for step in &self.steps {
            step.validate()?;
        }
        Ok(())
    }
}

impl StepConfig {
    pub fn action_count(&self) -> usize {
        usize::from(self.wait_ms.is_some())
            + usize::from(self.eval.is_some())
            + usize::from(self.fill.is_some())
            + usize::from(self.scroll.is_some())
    }

    pub fn validate(&self) -> Result<(), ConfigValidationError> {
        if self.name.trim().is_empty() {
            return Err(ConfigValidationError::EmptyStepName);
        }
        if self.action_count() > 1 {
            return Err(ConfigValidationError::MultipleStepActions {
                step: self.name.clone(),
            });
        }
        if matches!(self.frames, Some(0)) {
            return Err(ConfigValidationError::EmptyFrames {
                step: self.name.clone(),
            });
        }
        Ok(())
    }
}

impl<'de> TomlDeserialize<'de> for DemoConfig {
    fn deserialize(value: &mut Value<'de>) -> Result<Self, DeserError> {
        let mut table = TableHelper::new(value)?;
        let name = table.required_s::<String>("name").ok();
        let description = table.optional::<String>("description");
        let url = table.required_s::<String>("url").ok();
        let session = table.optional::<String>("session");
        let capture = table
            .optional::<CaptureConfig>("capture")
            .unwrap_or_default();
        let steps = table
            .optional::<Vec<StepConfig>>("steps")
            .unwrap_or_default();
        table.finalize(None)?;

        let name = name.expect("required field errors are returned by TableHelper::finalize");
        let url = url.expect("required field errors are returned by TableHelper::finalize");
        let mut errors = Vec::new();
        if name.value.trim().is_empty() {
            errors.push(custom_error(
                "demo config `name` must not be empty",
                name.span,
            ));
        }
        if url.value.trim().is_empty() {
            errors.push(custom_error(
                "demo config `url` must not be empty",
                url.span,
            ));
        }
        if !errors.is_empty() {
            return Err(DeserError { errors });
        }

        Ok(Self {
            name: name.value,
            description,
            url: url.value,
            session,
            capture,
            steps,
        })
    }
}

impl<'de> TomlDeserialize<'de> for CaptureConfig {
    fn deserialize(value: &mut Value<'de>) -> Result<Self, DeserError> {
        let mut table = TableHelper::new(value)?;
        let output_fps = table.optional_s::<u32>("output_fps");
        let configured_default_hold_ms = table.optional::<u64>("default_hold_ms");
        let configured_default_action_ms = table.optional::<u64>("default_action_ms");
        table.finalize(None)?;

        if let Some(output_fps) = &output_fps
            && output_fps.value == 0
        {
            return Err(
                custom_error("capture.output_fps must be greater than 0", output_fps.span).into(),
            );
        }

        Ok(Self {
            output_fps: output_fps.map_or_else(default_output_fps, |value| value.value),
            default_hold_ms: configured_default_hold_ms.unwrap_or_else(default_hold_ms),
            default_action_ms: configured_default_action_ms.unwrap_or_else(default_action_ms),
        })
    }
}

impl<'de> TomlDeserialize<'de> for StepConfig {
    fn deserialize(value: &mut Value<'de>) -> Result<Self, DeserError> {
        let mut table = TableHelper::new(value)?;
        let name = table.required_s::<String>("name").ok();
        let wait_ms = table.optional_s::<u64>("wait_ms");
        let eval = table.optional_s::<String>("eval");
        let fill = table.optional_s::<FillConfig>("fill");
        let scroll = table.optional_s::<ScrollConfig>("scroll");
        let hold_ms = table.optional::<u64>("hold_ms");
        let duration_ms = table.optional::<u64>("duration_ms");
        let frames = table.optional_s::<u32>("frames");
        let screenshot = table.optional::<bool>("screenshot").unwrap_or(false);
        table.finalize(None)?;

        let name = name.expect("required field errors are returned by TableHelper::finalize");
        let mut errors = Vec::new();
        if name.value.trim().is_empty() {
            errors.push(custom_error("step name must not be empty", name.span));
        }

        let actions = [
            wait_ms.as_ref().map(|value| ("wait_ms", value.span)),
            eval.as_ref().map(|value| ("eval", value.span)),
            fill.as_ref().map(|value| ("fill", value.span)),
            scroll.as_ref().map(|value| ("scroll", value.span)),
        ];
        let actions = actions.into_iter().flatten().collect::<Vec<_>>();
        if actions.len() > 1 {
            let message = format!(
                "step `{}` has multiple actions. Use exactly one of wait_ms, eval, fill, or scroll.",
                name.value
            );
            errors.extend(actions.into_iter().map(|(action, span)| {
                custom_error(format!("{message} `{action}` is one of the actions."), span)
            }));
        }

        if let Some(frames) = &frames
            && frames.value == 0
        {
            errors.push(custom_error(
                format!("step `{}` frames must be greater than 0", name.value),
                frames.span,
            ));
        }

        if !errors.is_empty() {
            return Err(DeserError { errors });
        }

        Ok(Self {
            name: name.value,
            wait_ms: wait_ms.map(|value| value.value),
            eval: eval.map(|value| value.value),
            fill: fill.map(|value| value.value),
            scroll: scroll.map(|value| value.value),
            hold_ms,
            duration_ms,
            frames: frames.map(|value| value.value),
            screenshot,
        })
    }
}

impl<'de> TomlDeserialize<'de> for FillConfig {
    fn deserialize(value: &mut Value<'de>) -> Result<Self, DeserError> {
        let mut table = TableHelper::new(value)?;
        let selector = table.required_s::<String>("selector").ok();
        let text = table.required_s::<String>("text").ok();
        table.finalize(None)?;

        Ok(Self {
            selector: selector
                .expect("required field errors are returned by TableHelper::finalize")
                .value,
            text: text
                .expect("required field errors are returned by TableHelper::finalize")
                .value,
        })
    }
}

impl<'de> TomlDeserialize<'de> for ScrollConfig {
    fn deserialize(value: &mut Value<'de>) -> Result<Self, DeserError> {
        let mut table = TableHelper::new(value)?;
        let selector = table.required_s::<String>("selector").ok();
        let pixels = table.required_s::<i64>("pixels").ok();
        table.finalize(None)?;

        Ok(Self {
            selector: selector
                .expect("required field errors are returned by TableHelper::finalize")
                .value,
            pixels: pixels
                .expect("required field errors are returned by TableHelper::finalize")
                .value,
        })
    }
}

fn invalid_config_error(
    path: &Path,
    text: &str,
    message: impl Into<String>,
    errors: Vec<TomlError>,
) -> ConfigError {
    let source_len = text.len();
    let labels = errors
        .iter()
        .flat_map(|error| labels_for_toml_error(error, source_len))
        .collect::<Vec<_>>();
    let help = help_for_toml_errors(&errors);

    ConfigError::Invalid(Box::new(InvalidConfigError {
        message: message.into(),
        path: path.to_path_buf(),
        src: NamedSource::new(path.display().to_string(), text.to_owned()).with_language("toml"),
        labels,
        help,
    }))
}

fn labels_for_toml_error(error: &TomlError, source_len: usize) -> Vec<LabeledSpan> {
    match &error.kind {
        ErrorKind::DuplicateKey { key, first } => vec![
            LabeledSpan::at(
                source_span(*first, source_len),
                format!("first `{key}` here"),
            ),
            LabeledSpan::new_primary_with_span(
                Some(format!("duplicate key `{key}`")),
                source_span(error.span, source_len),
            ),
        ],
        ErrorKind::DuplicateTable { name, first } => vec![
            LabeledSpan::at(
                source_span(*first, source_len),
                format!("first `{name}` table here"),
            ),
            LabeledSpan::new_primary_with_span(
                Some(format!("duplicate table `{name}`")),
                source_span(error.span, source_len),
            ),
        ],
        ErrorKind::DottedKeyInvalidType { first } => vec![
            LabeledSpan::new_primary_with_span(
                Some("attempted to extend a table here".to_owned()),
                source_span(error.span, source_len),
            ),
            LabeledSpan::at(source_span(*first, source_len), "but this was not a table"),
        ],
        ErrorKind::UnexpectedKeys { keys, .. } => keys
            .iter()
            .map(|(key, span)| {
                LabeledSpan::new_primary_with_span(
                    Some(format!("unexpected key `{key}`")),
                    source_span(*span, source_len),
                )
            })
            .collect(),
        ErrorKind::MissingField(field) => vec![LabeledSpan::new_primary_with_span(
            Some(format!("missing required field `{field}`")),
            source_span(error.span, source_len),
        )],
        _ => vec![LabeledSpan::new_primary_with_span(
            Some(error.to_string()),
            source_span(error.span, source_len),
        )],
    }
}

fn help_for_toml_errors(errors: &[TomlError]) -> Option<String> {
    if errors.iter().any(|error| {
        matches!(
            error.kind,
            ErrorKind::UnexpectedKeys { .. } | ErrorKind::MissingField(_)
        )
    }) {
        Some(
            "Allowed top-level keys are name, description, url, session, capture, and steps. \
             Step actions are wait_ms, eval, fill, and scroll. Use at most one per step."
                .to_owned(),
        )
    } else if errors.iter().any(|error| match &error.kind {
        ErrorKind::Custom(message) => message.contains("multiple actions"),
        _ => false,
    }) {
        Some("Split separate browser actions into separate [[steps]] entries.".to_owned())
    } else {
        None
    }
}

fn source_span(span: Span, source_len: usize) -> SourceSpan {
    let start = span.start.min(source_len);
    let end = if span.end > span.start {
        span.end.min(source_len)
    } else {
        (start + usize::from(start < source_len)).min(source_len)
    };
    (start, end.saturating_sub(start)).into()
}

fn custom_error(message: impl Into<std::borrow::Cow<'static, str>>, span: Span) -> TomlError {
    TomlError {
        kind: ErrorKind::Custom(message.into()),
        span,
        line_info: None,
    }
}

fn default_output_fps() -> u32 {
    24
}

fn default_hold_ms() -> u64 {
    700
}

fn default_action_ms() -> u64 {
    400
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_readable_config_with_timeline_granularity() {
        let config = DemoConfig::from_str(
            r#"
name = "windowed-browse"
description = "Windowed package and advisory browse demo"
url = "http://localhost:3001"

[capture]
output_fps = 24
default_hold_ms = 800

[[steps]]
name = "Scroll packages"
scroll = { selector = ".package-list", pixels = 900 }
duration_ms = 1600
frames = 32
screenshot = true
"#,
            "demo.toml",
        )
        .unwrap();

        config.validate().unwrap();
        assert_eq!(config.capture.output_fps, 24);
        assert_eq!(config.steps[0].frames, Some(32));
    }

    #[test]
    fn snapshots_unknown_field_diagnostic() {
        let err = DemoConfig::from_str(
            r#"
name = "windowed-browse"
url = "http://localhost:3001"
click = "button"
"#,
            "demo.toml",
        )
        .unwrap_err();

        insta::assert_snapshot!(render_config_error(&err));
    }

    #[test]
    fn snapshots_multiple_actions_diagnostic() {
        let err = DemoConfig::from_str(
            r#"
name = "windowed-browse"
url = "http://localhost:3001"

[[steps]]
name = "Too much"
wait_ms = 10
eval = "console.log(1)"
"#,
            "demo.toml",
        )
        .unwrap_err();

        insta::assert_snapshot!(render_config_error(&err));
    }

    #[test]
    fn rejects_multiple_actions_in_one_step() {
        let step = StepConfig {
            name: "too much".into(),
            wait_ms: Some(1),
            eval: Some("console.log(1)".into()),
            fill: None,
            scroll: None,
            hold_ms: None,
            duration_ms: None,
            frames: None,
            screenshot: false,
        };

        assert!(step.validate().is_err());
    }

    #[test]
    fn checked_in_script_schema_matches_config_types() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("schemas/sepia-script.schema.json");
        let expected = generated_script_schema_json();

        if std::env::var_os("UPDATE_SEPIA_SCHEMA").is_some() {
            std::fs::write(&path, &expected).unwrap();
        }

        let actual = std::fs::read_to_string(&path).unwrap_or_else(|err| {
            panic!("failed to read {}: {err}", path.display());
        });
        assert_eq!(
            actual,
            expected,
            "{} is stale; run `UPDATE_SEPIA_SCHEMA=1 cargo test checked_in_script_schema_matches_config_types`",
            path.display()
        );
    }

    fn render_config_error(error: &ConfigError) -> String {
        let mut rendered = String::new();
        miette::GraphicalReportHandler::new_themed(miette::GraphicalTheme::none())
            .with_width(100)
            .with_links(false)
            .without_cause_chain()
            .render_report(&mut rendered, error)
            .unwrap();
        rendered
    }

    fn generated_script_schema_json() -> String {
        let mut schema = serde_json::to_value(schemars::schema_for!(DemoConfig)).unwrap();
        enhance_script_schema(&mut schema);
        format!("{}\n", serde_json::to_string_pretty(&schema).unwrap())
    }

    fn enhance_script_schema(schema: &mut serde_json::Value) {
        schema["$id"] =
            serde_json::json!("https://github.com/tdejager/sepia/schemas/sepia-script.schema.json");
        schema["description"] = serde_json::json!("Sepia browser demo script TOML configuration.");

        schema["properties"]["name"]["description"] =
            serde_json::json!("Human-readable demo name.");
        schema["properties"]["name"]["minLength"] = serde_json::json!(1);
        schema["properties"]["name"]["pattern"] = serde_json::json!(r"\S");
        schema["properties"]["url"]["description"] =
            serde_json::json!("Initial browser URL to open before running the script.");
        schema["properties"]["url"]["minLength"] = serde_json::json!(1);
        schema["properties"]["url"]["pattern"] = serde_json::json!(r"\S");
        schema["properties"]["session"]["description"] =
            serde_json::json!("Optional stable session name used for output directories.");
        schema["properties"]["steps"]["description"] =
            serde_json::json!("Ordered browser actions and captured moments.");

        schema["$defs"]["CaptureConfig"]["description"] =
            serde_json::json!("Capture timing and output settings.");
        schema["$defs"]["CaptureConfig"]["properties"]["output_fps"]["minimum"] =
            serde_json::json!(1);
        schema["$defs"]["CaptureConfig"]["properties"]["output_fps"]["description"] =
            serde_json::json!("Frames per second in the generated MP4.");
        schema["$defs"]["CaptureConfig"]["properties"]["default_hold_ms"]["description"] =
            serde_json::json!("Default hold after a step when hold_ms is omitted.");
        schema["$defs"]["CaptureConfig"]["properties"]["default_action_ms"]["description"] =
            serde_json::json!("Default animated action duration when duration_ms is omitted.");

        let step = &mut schema["$defs"]["StepConfig"];
        step["description"] = serde_json::json!(
            "A named script step. Use at most one action: wait_ms, eval, fill, or scroll."
        );
        step["properties"]["name"]["minLength"] = serde_json::json!(1);
        step["properties"]["name"]["pattern"] = serde_json::json!(r"\S");
        step["properties"]["wait_ms"]["description"] =
            serde_json::json!("Wait for this many milliseconds before capturing the state.");
        step["properties"]["eval"]["description"] =
            serde_json::json!("JavaScript to evaluate in the page.");
        step["properties"]["fill"]["description"] =
            serde_json::json!("Fill an input matched by selector.");
        step["properties"]["scroll"]["description"] =
            serde_json::json!("Scroll an element matched by selector.");
        step["properties"]["hold_ms"]["description"] =
            serde_json::json!("Milliseconds to hold the resulting state for viewers.");
        step["properties"]["duration_ms"]["description"] =
            serde_json::json!("Milliseconds used to animate an action such as scrolling.");
        step["properties"]["frames"]["description"] =
            serde_json::json!("Explicit number of frames to capture for this step.");
        step["properties"]["frames"]["minimum"] = serde_json::json!(1);
        step["properties"]["screenshot"]["description"] =
            serde_json::json!("Whether to save a screenshot after this step.");
        step["allOf"] = serde_json::json!([
            {
                "not": {
                    "anyOf": [
                        { "required": ["wait_ms", "eval"] },
                        { "required": ["wait_ms", "fill"] },
                        { "required": ["wait_ms", "scroll"] },
                        { "required": ["eval", "fill"] },
                        { "required": ["eval", "scroll"] },
                        { "required": ["fill", "scroll"] }
                    ]
                }
            }
        ]);

        schema["$defs"]["FillConfig"]["description"] = serde_json::json!("Input fill action.");
        schema["$defs"]["FillConfig"]["properties"]["selector"]["description"] =
            serde_json::json!("CSS selector for the input to fill.");
        schema["$defs"]["FillConfig"]["properties"]["text"]["description"] =
            serde_json::json!("Text to enter into the matched input.");

        schema["$defs"]["ScrollConfig"]["description"] =
            serde_json::json!("Element scroll action.");
        schema["$defs"]["ScrollConfig"]["properties"]["selector"]["description"] =
            serde_json::json!("CSS selector for the element to scroll.");
        schema["$defs"]["ScrollConfig"]["properties"]["pixels"]["description"] =
            serde_json::json!("Vertical pixels to scroll. Negative values scroll upward.");
    }
}
