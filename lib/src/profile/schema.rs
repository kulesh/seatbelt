use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub version: u8,

    #[serde(default)]
    pub name: Option<String>,

    #[serde(default)]
    pub description: Option<String>,

    #[serde(default)]
    pub extends: Option<String>,

    #[serde(default)]
    pub filesystem: FilesystemRules,

    #[serde(default)]
    pub network: NetworkRules,

    #[serde(default)]
    pub process: ProcessRules,

    #[serde(default)]
    pub system: SystemRules,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FilesystemRules {
    #[serde(default)]
    pub read: Vec<String>,

    #[serde(default)]
    pub write: Vec<String>,

    #[serde(default)]
    pub deny: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NetworkRules {
    #[serde(default)]
    pub outbound: OutboundNetworkRules,

    #[serde(default)]
    pub inbound: InboundNetworkRules,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutboundNetworkRules {
    #[serde(default)]
    pub allow: bool,

    #[serde(default)]
    pub allow_domains: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InboundNetworkRules {
    #[serde(default)]
    pub allow: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessRules {
    #[serde(default)]
    pub allow_exec: Vec<String>,

    #[serde(default)]
    pub allow_exec_any: bool,

    #[serde(default = "default_true")]
    pub allow_fork: bool,
}

impl Default for ProcessRules {
    fn default() -> Self {
        Self {
            allow_exec: Vec::new(),
            allow_exec_any: false,
            allow_fork: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SystemRules {
    #[serde(default = "default_true")]
    pub allow_sysctl_read: bool,

    #[serde(default)]
    pub allow_sysctl_write: bool,

    #[serde(default)]
    pub allow_mach_lookup: Vec<String>,
}

impl Default for SystemRules {
    fn default() -> Self {
        Self {
            allow_sysctl_read: true,
            allow_sysctl_write: false,
            allow_mach_lookup: Vec::new(),
        }
    }
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_minimal() {
        let yaml = "version: 1\n";
        let profile: Profile = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(profile.version, 1);
        assert!(profile.name.is_none());
        assert!(profile.filesystem.read.is_empty());
        assert!(profile.process.allow_fork);
        assert!(profile.system.allow_sysctl_read);
    }

    #[test]
    fn deserialize_full() {
        let yaml = r#"
version: 1
name: test
description: a test profile
filesystem:
  read:
    - /usr/lib
    - (cwd)
  write:
    - (cwd)
  deny:
    - (home)/.ssh/id_*
network:
  outbound:
    allow: true
  inbound:
    allow: false
process:
  allow_exec:
    - /usr/bin/git
  allow_exec_any: false
  allow_fork: true
system:
  allow_sysctl_read: true
  allow_sysctl_write: false
  allow_mach_lookup:
    - com.apple.system.logger
"#;
        let profile: Profile = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(profile.name.as_deref(), Some("test"));
        assert_eq!(profile.filesystem.read.len(), 2);
        assert_eq!(profile.filesystem.deny, vec!["(home)/.ssh/id_*"]);
        assert!(profile.network.outbound.allow);
        assert!(!profile.network.inbound.allow);
        assert_eq!(profile.process.allow_exec, vec!["/usr/bin/git"]);
        assert_eq!(
            profile.system.allow_mach_lookup,
            vec!["com.apple.system.logger"]
        );
    }

    #[test]
    fn unknown_field_rejected() {
        let yaml = "version: 1\ntypo_field: true\n";
        let result: Result<Profile, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn unknown_nested_field_rejected() {
        let yaml = "version: 1\nfilesystem:\n  reed: []\n";
        let result: Result<Profile, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn defaults_applied() {
        let yaml = "version: 1\n";
        let profile: Profile = serde_yaml::from_str(yaml).unwrap();
        assert!(profile.process.allow_fork);
        assert!(profile.system.allow_sysctl_read);
        assert!(!profile.system.allow_sysctl_write);
        assert!(!profile.process.allow_exec_any);
        assert!(!profile.network.outbound.allow);
    }

    #[test]
    fn roundtrip() {
        let yaml = "version: 1\nname: roundtrip\n";
        let profile: Profile = serde_yaml::from_str(yaml).unwrap();
        let serialized = serde_yaml::to_string(&profile).unwrap();
        let reparsed: Profile = serde_yaml::from_str(&serialized).unwrap();
        assert_eq!(reparsed.name.as_deref(), Some("roundtrip"));
        assert_eq!(reparsed.version, 1);
    }
}
