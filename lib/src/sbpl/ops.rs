/// Broad category for a sandbox operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationKind {
    FileRead,
    FileWrite,
    Network,
    ProcessExec,
    MachLookup,
    Sysctl,
    Signal,
    Other,
}

/// Classify a sandbox operation string (from `log` output) into a semantic category.
pub fn classify_operation(op: &str) -> OperationKind {
    match op {
        s if s.starts_with("file-read") => OperationKind::FileRead,
        s if s.starts_with("file-write") => OperationKind::FileWrite,
        s if s.starts_with("network") => OperationKind::Network,
        s if s.starts_with("process-exec") => OperationKind::ProcessExec,
        s if s.starts_with("mach-lookup") => OperationKind::MachLookup,
        s if s.starts_with("sysctl") => OperationKind::Sysctl,
        s if s.starts_with("signal") => OperationKind::Signal,
        _ => OperationKind::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_file_read() {
        assert_eq!(
            classify_operation("file-read-data"),
            OperationKind::FileRead
        );
        assert_eq!(
            classify_operation("file-read-metadata"),
            OperationKind::FileRead
        );
    }

    #[test]
    fn classify_file_write() {
        assert_eq!(
            classify_operation("file-write-data"),
            OperationKind::FileWrite
        );
        assert_eq!(
            classify_operation("file-write-create"),
            OperationKind::FileWrite
        );
    }

    #[test]
    fn classify_network() {
        assert_eq!(
            classify_operation("network-outbound"),
            OperationKind::Network
        );
        assert_eq!(
            classify_operation("network-inbound"),
            OperationKind::Network
        );
    }

    #[test]
    fn classify_process_exec() {
        assert_eq!(
            classify_operation("process-exec"),
            OperationKind::ProcessExec
        );
        assert_eq!(
            classify_operation("process-exec*"),
            OperationKind::ProcessExec
        );
    }

    #[test]
    fn classify_mach_lookup() {
        assert_eq!(classify_operation("mach-lookup"), OperationKind::MachLookup);
    }

    #[test]
    fn classify_sysctl() {
        assert_eq!(classify_operation("sysctl-read"), OperationKind::Sysctl);
        assert_eq!(classify_operation("sysctl-write"), OperationKind::Sysctl);
    }

    #[test]
    fn classify_signal() {
        assert_eq!(classify_operation("signal"), OperationKind::Signal);
    }

    #[test]
    fn classify_unknown() {
        assert_eq!(classify_operation("ipc-posix-shm"), OperationKind::Other);
        assert_eq!(classify_operation(""), OperationKind::Other);
    }
}
