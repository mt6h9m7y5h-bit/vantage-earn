use shared::Currency;

pub struct LocalizationEngine;

impl LocalizationEngine {
    pub fn detect_system_language(locale: &str) -> String {
        match locale {
            "de_DE" | "de" => "de".into(),
            "fr_FR" | "fr" => "fr".into(),
            "es_ES" | "es" => "es".into(),
            "en_US" | "en" => "en".into(),
            _ => "en".into(),
        }
    }

    pub fn default_currency_for_locale(locale: &str) -> Currency {
        match locale {
            "de_DE" | "de" | "fr_FR" | "fr" | "es_ES" | "es" => Currency::Eur,
            "en_GB" => Currency::Gbp,
            _ => Currency::Usd,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_german() {
        assert_eq!(LocalizationEngine::detect_system_language("de_DE"), "de");
    }

    #[test]
    fn defaults_unknown_to_english() {
        assert_eq!(LocalizationEngine::detect_system_language("ja_JP"), "en");
    }
}
