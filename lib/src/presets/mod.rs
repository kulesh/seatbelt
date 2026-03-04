pub fn get_preset(name: &str) -> Option<&'static str> {
    match name {
        "ai-agent-strict" => Some(include_str!("profiles/ai-agent-strict.yaml")),
        "ai-agent-networked" => Some(include_str!("profiles/ai-agent-networked.yaml")),
        "ai-agent-permissive" => Some(include_str!("profiles/ai-agent-permissive.yaml")),
        "read-only" => Some(include_str!("profiles/read-only.yaml")),
        "build-tool" => Some(include_str!("profiles/build-tool.yaml")),
        "network-only" => Some(include_str!("profiles/network-only.yaml")),
        _ => None,
    }
}

pub fn list_presets() -> &'static [&'static str] {
    &[
        "ai-agent-strict",
        "ai-agent-networked",
        "ai-agent-permissive",
        "read-only",
        "build-tool",
        "network-only",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::schema::Profile;

    #[test]
    fn all_presets_parse() {
        for name in list_presets() {
            let yaml = get_preset(name).unwrap_or_else(|| panic!("preset '{}' not found", name));
            // ai-agent-networked uses extends, so it needs deep_merge to fully parse.
            // Here we just confirm the raw YAML is valid serde_yaml::Value.
            let _: serde_yaml::Value = serde_yaml::from_str(yaml)
                .unwrap_or_else(|e| panic!("preset '{}' invalid: {}", name, e));
        }
    }

    #[test]
    fn standalone_presets_parse_to_profile() {
        let standalone = [
            "ai-agent-strict",
            "ai-agent-permissive",
            "read-only",
            "build-tool",
            "network-only",
        ];
        for name in standalone {
            let yaml = get_preset(name).unwrap();
            let profile: Profile =
                serde_yaml::from_str(yaml).unwrap_or_else(|e| panic!("preset '{}': {}", name, e));
            assert_eq!(profile.version, 1);
        }
    }

    #[test]
    fn unknown_preset_returns_none() {
        assert!(get_preset("nonexistent").is_none());
    }

    #[test]
    fn ai_agent_networked_has_extends() {
        let yaml = get_preset("ai-agent-networked").unwrap();
        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(
            value.get("extends").and_then(|v| v.as_str()),
            Some("ai-agent-strict")
        );
    }
}
