use std::path::Path;

use serde_yaml::Value;

use crate::error::{Result, SeatbeltError};
use crate::presets;
use crate::profile::schema::Profile;

/// Load a profile from a YAML file, resolving `extends` if present.
pub fn load_profile(path: &Path) -> Result<Profile> {
    let raw = std::fs::read_to_string(path)
        .map_err(|_| SeatbeltError::ProfileNotFound(path.to_path_buf()))?;
    load_profile_from_str(&raw)
}

/// Load a profile from a YAML string, resolving `extends` if present.
pub fn load_profile_from_str(yaml: &str) -> Result<Profile> {
    let child_value: Value = serde_yaml::from_str(yaml)?;

    let merged = if let Some(Value::String(preset_name)) = child_value.get("extends") {
        let parent_yaml = presets::get_preset(preset_name)
            .ok_or_else(|| SeatbeltError::UnknownPreset(preset_name.clone()))?;
        let parent_value: Value = serde_yaml::from_str(parent_yaml)?;
        deep_merge_yaml(parent_value, child_value)
    } else {
        child_value
    };

    let profile: Profile = serde_yaml::from_value(merged)?;
    Ok(profile)
}

/// Deep merge two YAML Values. Child values override parent at leaf level.
/// Mappings: recursively merge. All other types (scalars, sequences): child replaces parent.
fn deep_merge_yaml(parent: Value, child: Value) -> Value {
    match (parent, child) {
        (Value::Mapping(mut parent_map), Value::Mapping(child_map)) => {
            for (key, child_val) in child_map {
                let merged_val = if let Some(parent_val) = parent_map.remove(&key) {
                    deep_merge_yaml(parent_val, child_val)
                } else {
                    child_val
                };
                parent_map.insert(key, merged_val);
            }
            Value::Mapping(parent_map)
        }
        (_parent, child) => child,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_minimal_profile() {
        let profile = load_profile_from_str("version: 1\n").unwrap();
        assert_eq!(profile.version, 1);
    }

    #[test]
    fn extends_inherits_parent_filesystem() {
        let yaml = r#"
version: 1
extends: ai-agent-strict
network:
  outbound:
    allow: true
"#;
        let profile = load_profile_from_str(yaml).unwrap();
        // Parent's filesystem rules should be preserved
        assert!(!profile.filesystem.read.is_empty());
        assert!(profile.filesystem.read.contains(&"(cwd)".to_string()));
        // Child's network override should apply
        assert!(profile.network.outbound.allow);
    }

    #[test]
    fn extends_child_list_replaces_parent_list() {
        let yaml = r#"
version: 1
extends: ai-agent-strict
filesystem:
  read:
    - /only/this
"#;
        let profile = load_profile_from_str(yaml).unwrap();
        assert_eq!(profile.filesystem.read, vec!["/only/this"]);
    }

    #[test]
    fn extends_preserves_absent_keys() {
        let yaml = r#"
version: 1
extends: ai-agent-strict
"#;
        let profile = load_profile_from_str(yaml).unwrap();
        // Parent's deny list should be inherited
        assert!(!profile.filesystem.deny.is_empty());
        // Parent's process settings should be inherited
        assert!(profile.process.allow_exec_any);
    }

    #[test]
    fn extends_scalar_override() {
        let yaml = r#"
version: 1
extends: ai-agent-strict
process:
  allow_exec_any: false
"#;
        let profile = load_profile_from_str(yaml).unwrap();
        assert!(!profile.process.allow_exec_any);
        // allow_fork from parent should still be true
        assert!(profile.process.allow_fork);
    }

    #[test]
    fn unknown_preset_error() {
        let yaml = "version: 1\nextends: nonexistent\n";
        let err = load_profile_from_str(yaml).unwrap_err();
        assert!(matches!(err, SeatbeltError::UnknownPreset(_)));
    }

    #[test]
    fn file_not_found_error() {
        let err = load_profile(Path::new("/tmp/definitely-not-here.yaml")).unwrap_err();
        assert!(matches!(err, SeatbeltError::ProfileNotFound(_)));
    }

    #[test]
    fn deep_merge_maps_recurse() {
        let parent: Value = serde_yaml::from_str("a:\n  b: 1\n  c: 2\n").unwrap();
        let child: Value = serde_yaml::from_str("a:\n  b: 10\n").unwrap();
        let merged = deep_merge_yaml(parent, child);
        let a = merged.get("a").unwrap();
        assert_eq!(a.get("b").unwrap().as_i64(), Some(10));
        assert_eq!(a.get("c").unwrap().as_i64(), Some(2));
    }

    #[test]
    fn deep_merge_list_replaces() {
        let parent: Value = serde_yaml::from_str("items:\n  - a\n  - b\n").unwrap();
        let child: Value = serde_yaml::from_str("items:\n  - x\n").unwrap();
        let merged = deep_merge_yaml(parent, child);
        let items = merged.get("items").unwrap().as_sequence().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].as_str(), Some("x"));
    }

    #[test]
    fn load_from_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.yaml");
        std::fs::write(&path, "version: 1\nname: from-file\n").unwrap();
        let profile = load_profile(&path).unwrap();
        assert_eq!(profile.name.as_deref(), Some("from-file"));
    }
}
