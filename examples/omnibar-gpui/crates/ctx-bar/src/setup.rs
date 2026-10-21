//! In-app decisions setup (`ctx setup`): endpoint, key, optional EAVS listener
//! token, then one synthetic test call before anything is saved. Pure state;
//! the bar owns the input field, the background probe and the credentials file.
use crate::direct::{Credentials, DEFAULT_MODEL, check_endpoint};

/// Path appended when the user pastes only an EAVS origin.
pub const JEV_PATH: &str = "/jev/v1/systemone";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Endpoint,
    Key,
    Token,
    Testing,
}

#[derive(Debug, PartialEq)]
pub enum Outcome {
    /// Move to the next step; the field is cleared.
    Next,
    /// Run the probe with these credentials; save only if it succeeds.
    Test(Credentials),
    /// Keep the step; the message explains what to fix.
    Invalid,
}

pub struct Setup {
    step: Step,
    endpoint: Option<String>,
    key: Option<String>,
    model: String,
    message: Option<String>,
}

impl Default for Setup {
    fn default() -> Self {
        Self::new()
    }
}

impl Setup {
    pub fn new() -> Self {
        Self {
            step: Step::Endpoint,
            endpoint: None,
            key: None,
            model: DEFAULT_MODEL.into(),
            message: None,
        }
    }
    pub fn step(&self) -> Step {
        self.step
    }
    /// Key and token are typed into a masked field.
    pub fn masked(&self) -> bool {
        matches!(self.step, Step::Key | Step::Token)
    }
    pub fn placeholder(&self) -> &'static str {
        match self.step {
            Step::Endpoint => "https://your-eavs-host",
            Step::Key => "API key",
            Step::Token => "Listener token (optional)",
            Step::Testing => "Testing...",
        }
    }
    /// Status line for the current step. Never contains the key or token.
    pub fn prompt(&self) -> String {
        if let Some(message) = &self.message {
            return message.clone();
        }
        match self.step {
            Step::Endpoint => "Setup 1/3 · Jev endpoint URL, then Enter · Esc cancels".into(),
            Step::Key => "Setup 2/3 · API key (hidden), then Enter".into(),
            Step::Token => "Setup 3/3 · EAVS listener token if required, or Enter to skip".into(),
            Step::Testing => "Testing the endpoint with one synthetic request...".into(),
        }
    }
    pub fn edited(&mut self) {
        if self.step != Step::Testing {
            self.message = None;
        }
    }
    pub fn submit(&mut self, text: &str) -> Outcome {
        let text = text.trim();
        match self.step {
            Step::Endpoint => {
                let mut endpoint = text.to_string();
                if let Ok(url) = reqwest::Url::parse(text)
                    && matches!(url.path(), "" | "/")
                {
                    endpoint = format!("{}{JEV_PATH}", text.trim_end_matches('/'));
                }
                match check_endpoint(&endpoint) {
                    Ok(_) => {
                        self.endpoint = Some(endpoint);
                        self.step = Step::Key;
                        self.message = None;
                        Outcome::Next
                    }
                    Err(error) => {
                        self.message = Some(format!("Error: {error}"));
                        Outcome::Invalid
                    }
                }
            }
            Step::Key => {
                if text.is_empty() || text.chars().any(char::is_whitespace) {
                    self.message = Some("Error: paste the key without spaces".into());
                    return Outcome::Invalid;
                }
                self.key = Some(text.to_string());
                self.step = Step::Token;
                self.message = None;
                Outcome::Next
            }
            Step::Token => {
                if text.chars().any(char::is_whitespace) {
                    self.message = Some("Error: the token has spaces; paste it again".into());
                    return Outcome::Invalid;
                }
                let (Some(endpoint), Some(api_key)) = (self.endpoint.clone(), self.key.clone())
                else {
                    *self = Self::new();
                    return Outcome::Invalid;
                };
                self.step = Step::Testing;
                self.message = None;
                Outcome::Test(Credentials {
                    endpoint,
                    api_key,
                    listener_token: (!text.is_empty()).then(|| text.to_string()),
                    model: self.model.clone(),
                })
            }
            Step::Testing => Outcome::Invalid,
        }
    }
    /// The probe failed: keep the endpoint, ask for the key again.
    pub fn failed(&mut self, error: &str) {
        self.step = Step::Key;
        self.key = None;
        self.message = Some(format!("Error: {error} · re-enter the key or Esc"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walks_endpoint_key_token_then_tests_before_saving() {
        let mut setup = Setup::new();
        assert!(!setup.masked());
        assert_eq!(
            setup.submit("https://eavs.has.skillissues.net"),
            Outcome::Next
        );
        assert!(setup.masked(), "key is typed into a masked field");
        assert_eq!(setup.submit("sk-live-123"), Outcome::Next);
        let Outcome::Test(credentials) = setup.submit("") else {
            panic!("expected test")
        };
        assert_eq!(
            credentials.endpoint,
            format!("https://eavs.has.skillissues.net{JEV_PATH}")
        );
        assert_eq!(credentials.api_key, "sk-live-123");
        assert_eq!(credentials.listener_token, None);
        assert_eq!(setup.step(), Step::Testing);
        assert!(!setup.prompt().contains("sk-live"));
    }

    #[test]
    fn explicit_paths_are_kept_and_bad_input_stays_on_its_step() {
        let mut setup = Setup::new();
        assert_eq!(setup.submit("http://example.com"), Outcome::Invalid);
        assert!(setup.prompt().starts_with("Error:"));
        setup.edited();
        assert!(!setup.prompt().starts_with("Error:"));
        assert_eq!(
            setup.submit("https://host.example/custom/decide"),
            Outcome::Next
        );
        assert_eq!(setup.submit("has space"), Outcome::Invalid);
        assert_eq!(setup.submit("k"), Outcome::Next);
        let Outcome::Test(credentials) = setup.submit("tok") else {
            panic!()
        };
        assert_eq!(credentials.endpoint, "https://host.example/custom/decide");
        assert_eq!(credentials.listener_token.as_deref(), Some("tok"));
    }

    #[test]
    fn failed_probe_returns_to_the_key_without_leaking_it() {
        let mut setup = Setup::new();
        setup.submit("https://h.example");
        setup.submit("secret-key");
        setup.submit("");
        setup.failed("Jev rejected the key (HTTP 401)");
        assert_eq!(setup.step(), Step::Key);
        assert!(setup.prompt().contains("HTTP 401") && !setup.prompt().contains("secret-key"));
        assert_eq!(setup.submit("new-key"), Outcome::Next);
    }
}
