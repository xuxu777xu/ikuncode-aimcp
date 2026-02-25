use crate::shared::find_binary;
use std::path::PathBuf;

#[derive(Clone)]
pub struct Capabilities {
    pub gemini_available: bool,
    pub gemini_path: Option<PathBuf>,
    pub codex_available: bool,
    pub codex_path: Option<PathBuf>,
    pub grok_available: bool,
}

pub fn detect() -> Capabilities {
    let gemini_path = find_binary("gemini", "GEMINI_BIN");
    let codex_path = find_binary("codex", "CODEX_BIN");
    let grok_available = std::env::var("GROK_API_URL").is_ok()
        && std::env::var("GROK_API_KEY").is_ok();

    let caps = Capabilities {
        gemini_available: gemini_path.is_some(),
        gemini_path,
        codex_available: codex_path.is_some(),
        codex_path,
        grok_available,
    };

    // Log detection results to stderr
    eprintln!("[aimcp] Tools detection:");
    eprintln!("  Gemini:  {}", if caps.gemini_available {
        format!("✓ ({})", caps.gemini_path.as_ref().unwrap().display())
    } else { "✗ (not found)".into() });
    eprintln!("  Codex:   {}", if caps.codex_available {
        format!("✓ ({})", caps.codex_path.as_ref().unwrap().display())
    } else { "✗ (not found)".into() });
    eprintln!("  Grok:    {}", if caps.grok_available {
        "✓ (API key configured)".into()
    } else { "✗ (GROK_API_URL or GROK_API_KEY not set)".to_string() });

    caps
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capabilities_struct_defaults() {
        let caps = Capabilities {
            gemini_available: false,
            gemini_path: None,
            codex_available: false,
            codex_path: None,
            grok_available: false,
        };
        assert!(!caps.gemini_available);
        assert!(!caps.codex_available);
        assert!(!caps.grok_available);
    }
}
