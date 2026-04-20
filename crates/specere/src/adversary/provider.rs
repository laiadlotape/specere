//! LLM provider abstraction for FR-EQ-020.
//!
//! Three implementations:
//! - `MockProvider` — deterministic canned responses from a fixture dir;
//!   used in CI and integration tests. Returns cost = 0.0.
//! - `AnthropicProvider` — gated behind `ANTHROPIC_API_KEY`, POSTs to
//!   `api.anthropic.com/v1/messages` via `reqwest::blocking`.
//! - `OpenAiProvider` — gated behind `OPENAI_API_KEY`, POSTs to
//!   `api.openai.com/v1/chat/completions`.
//!
//! Each `ask` returns a `Suggestion { script, rationale, cost_usd }`.
//! The real providers always report a non-zero cost (token-count × rate);
//! the mock always reports 0.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

pub struct Suggestion {
    /// Shell script that, when run in the sandbox, is expected to
    /// reproduce a spec violation (exit != 0).
    pub script: String,
    /// One-line human-readable summary (what the LLM *thinks* this
    /// falsifies).
    pub rationale: String,
    /// USD cost of this request.
    pub cost_usd: f64,
}

pub trait Provider: Send + Sync {
    fn name(&self) -> &'static str;
    /// Propose one counterexample shell script. `iteration` lets the
    /// provider avoid repeating itself (mock uses it to index the
    /// fixture; real providers pass it into the prompt).
    fn ask(
        &self,
        spec_text: &str,
        support: &[String],
        tests: &[String],
        iteration: u32,
    ) -> Result<Suggestion>;
}

pub fn build(kind: &str, repo: &Path, fixture_dir: Option<PathBuf>) -> Result<Box<dyn Provider>> {
    match kind {
        "mock" => Ok(Box::new(MockProvider::new(
            fixture_dir.unwrap_or_else(|| repo.join(".specere/adversary-fixtures")),
        ))),
        "anthropic" => Ok(Box::new(AnthropicProvider::from_env()?)),
        "openai" => Ok(Box::new(OpenAiProvider::from_env()?)),
        other => Err(anyhow::anyhow!(
            "--provider: unknown {other:?} (expected mock|anthropic|openai)"
        )),
    }
}

// ---------- Mock ----------

pub struct MockProvider {
    fixture_dir: PathBuf,
}

impl MockProvider {
    pub fn new(fixture_dir: PathBuf) -> Self {
        MockProvider { fixture_dir }
    }
}

impl Provider for MockProvider {
    fn name(&self) -> &'static str {
        "mock"
    }

    fn ask(
        &self,
        _spec_text: &str,
        _support: &[String],
        _tests: &[String],
        iteration: u32,
    ) -> Result<Suggestion> {
        // Fixture layout: `{fixture_dir}/iter_{N}.sh`. Missing files emit
        // a benign no-op script so the loop keeps iterating deterministically.
        let path = self.fixture_dir.join(format!("iter_{iteration}.sh"));
        if path.exists() {
            let script = std::fs::read_to_string(&path)
                .with_context(|| format!("read mock fixture {}", path.display()))?;
            Ok(Suggestion {
                script,
                rationale: format!("mock fixture iter_{iteration}"),
                cost_usd: 0.0,
            })
        } else {
            Ok(Suggestion {
                script: "# no fixture for this iteration\nexit 0\n".to_string(),
                rationale: format!("mock no-op iter_{iteration}"),
                cost_usd: 0.0,
            })
        }
    }
}

// ---------- Anthropic ----------

pub struct AnthropicProvider {
    api_key: String,
    model: String,
}

impl AnthropicProvider {
    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var("ANTHROPIC_API_KEY")
            .context("ANTHROPIC_API_KEY env var required for --provider anthropic")?;
        let model = std::env::var("SPECERE_ADVERSARY_MODEL")
            .unwrap_or_else(|_| "claude-sonnet-4-6".to_string());
        Ok(Self { api_key, model })
    }
}

#[derive(Deserialize)]
struct AnthropicResp {
    #[serde(default)]
    content: Vec<AnthropicContentBlock>,
    #[serde(default)]
    usage: AnthropicUsage,
}

#[derive(Deserialize, Default)]
struct AnthropicUsage {
    #[serde(default)]
    input_tokens: u32,
    #[serde(default)]
    output_tokens: u32,
}

#[derive(Deserialize)]
struct AnthropicContentBlock {
    #[serde(default, rename = "type")]
    _type: String,
    #[serde(default)]
    text: String,
}

impl Provider for AnthropicProvider {
    fn name(&self) -> &'static str {
        "anthropic"
    }

    fn ask(
        &self,
        spec_text: &str,
        support: &[String],
        tests: &[String],
        iteration: u32,
    ) -> Result<Suggestion> {
        let prompt = build_prompt(spec_text, support, tests, iteration);
        let body = serde_json::json!({
            "model": self.model,
            "max_tokens": 1024,
            "messages": [{"role": "user", "content": prompt}],
        });
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()?;
        let resp = client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .context("POST anthropic /v1/messages")?;
        let status = resp.status();
        let text = resp.text().context("anthropic response body")?;
        if !status.is_success() {
            return Err(anyhow::anyhow!("anthropic error {status}: {text}"));
        }
        let parsed: AnthropicResp =
            serde_json::from_str(&text).context("parse anthropic response")?;
        let script = extract_shell_block(
            &parsed
                .content
                .iter()
                .map(|b| b.text.clone())
                .collect::<Vec<_>>()
                .join("\n"),
        );
        // Rough pricing: Sonnet 4.6 — $3/M in, $15/M out (2026 pricing).
        let cost_usd = (parsed.usage.input_tokens as f64) * 3.0 / 1_000_000.0
            + (parsed.usage.output_tokens as f64) * 15.0 / 1_000_000.0;
        Ok(Suggestion {
            script,
            rationale: format!("anthropic {} iter_{}", self.model, iteration),
            cost_usd,
        })
    }
}

// ---------- OpenAI ----------

pub struct OpenAiProvider {
    api_key: String,
    model: String,
}

impl OpenAiProvider {
    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var("OPENAI_API_KEY")
            .context("OPENAI_API_KEY env var required for --provider openai")?;
        let model =
            std::env::var("SPECERE_ADVERSARY_MODEL").unwrap_or_else(|_| "gpt-5".to_string());
        Ok(Self { api_key, model })
    }
}

#[derive(Deserialize)]
struct OpenAiResp {
    #[serde(default)]
    choices: Vec<OpenAiChoice>,
    #[serde(default)]
    usage: OpenAiUsage,
}

#[derive(Deserialize, Default)]
struct OpenAiUsage {
    #[serde(default)]
    prompt_tokens: u32,
    #[serde(default)]
    completion_tokens: u32,
}

#[derive(Deserialize)]
struct OpenAiChoice {
    #[serde(default)]
    message: OpenAiMessage,
}

#[derive(Deserialize, Default)]
struct OpenAiMessage {
    #[serde(default)]
    content: String,
}

impl Provider for OpenAiProvider {
    fn name(&self) -> &'static str {
        "openai"
    }

    fn ask(
        &self,
        spec_text: &str,
        support: &[String],
        tests: &[String],
        iteration: u32,
    ) -> Result<Suggestion> {
        let prompt = build_prompt(spec_text, support, tests, iteration);
        let body = serde_json::json!({
            "model": self.model,
            "messages": [{"role": "user", "content": prompt}],
        });
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()?;
        let resp = client
            .post("https://api.openai.com/v1/chat/completions")
            .bearer_auth(&self.api_key)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .context("POST openai /v1/chat/completions")?;
        let status = resp.status();
        let text = resp.text().context("openai response body")?;
        if !status.is_success() {
            return Err(anyhow::anyhow!("openai error {status}: {text}"));
        }
        let parsed: OpenAiResp = serde_json::from_str(&text).context("parse openai response")?;
        let content = parsed
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .unwrap_or_default();
        let script = extract_shell_block(&content);
        // Rough pricing: GPT-5 class — $5/M in, $15/M out.
        let cost_usd = (parsed.usage.prompt_tokens as f64) * 5.0 / 1_000_000.0
            + (parsed.usage.completion_tokens as f64) * 15.0 / 1_000_000.0;
        Ok(Suggestion {
            script,
            rationale: format!("openai {} iter_{}", self.model, iteration),
            cost_usd,
        })
    }
}

// ---------- Shared helpers ----------

fn build_prompt(spec_text: &str, support: &[String], tests: &[String], iter: u32) -> String {
    format!(
        r#"You are an adversarial test-case generator. You read a spec and \
propose ONE shell script that, when run, would exit with a non-zero status \
if the spec were violated by a naive or buggy implementation. Prefer \
surprising edge cases over trivial ones. This is iteration {iter}; please \
propose a DIFFERENT angle than prior iterations.

SPEC:
{spec_text}

SUPPORT FILES:
{support_list}

EXISTING TESTS (for signal on what's already covered):
{tests_list}

Respond with a single fenced ```bash block containing the script. Exit 0 \
means "spec holds", non-zero means "violation reproduced"."#,
        spec_text = spec_text,
        support_list = support.join("\n"),
        tests_list = tests.join("\n"),
        iter = iter,
    )
}

fn extract_shell_block(raw: &str) -> String {
    if let Some(start) = raw.find("```bash") {
        let rest = &raw[start + "```bash".len()..];
        if let Some(end) = rest.find("```") {
            return rest[..end].trim_start_matches('\n').trim().to_string();
        }
    }
    if let Some(start) = raw.find("```sh") {
        let rest = &raw[start + "```sh".len()..];
        if let Some(end) = rest.find("```") {
            return rest[..end].trim_start_matches('\n').trim().to_string();
        }
    }
    if let Some(start) = raw.find("```") {
        let rest = &raw[start + 3..];
        if let Some(end) = rest.find("```") {
            return rest[..end].trim_start_matches('\n').trim().to_string();
        }
    }
    raw.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn mock_returns_fixture_content() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("iter_2.sh"), "exit 42\n").unwrap();
        let p = MockProvider::new(tmp.path().to_path_buf());
        let s = p.ask("spec", &[], &[], 2).unwrap();
        assert!(s.script.contains("exit 42"));
        assert_eq!(s.cost_usd, 0.0);
    }

    #[test]
    fn mock_noop_when_fixture_missing() {
        let tmp = TempDir::new().unwrap();
        let p = MockProvider::new(tmp.path().to_path_buf());
        let s = p.ask("spec", &[], &[], 99).unwrap();
        assert!(s.script.contains("exit 0"));
    }

    #[test]
    fn extract_bash_block() {
        let raw = "blah\n```bash\necho hi\nexit 1\n```\ntrailing";
        assert_eq!(extract_shell_block(raw), "echo hi\nexit 1");
    }

    #[test]
    fn extract_plain_code_block() {
        let raw = "```\nexit 7\n```";
        assert_eq!(extract_shell_block(raw), "exit 7");
    }
}
