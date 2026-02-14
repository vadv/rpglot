//! Container environment detection.
//!
//! Provides functionality to detect if the current process is running inside a container
//! (Docker, Kubernetes, Podman, LXC, etc.) or on bare metal.

use std::env;
use std::fs;
use std::path::Path;
use std::sync::LazyLock;

/// Cached result of container detection.
static IS_CONTAINER: LazyLock<bool> = LazyLock::new(detect_container);

/// Returns `true` if the current process is running inside a container.
///
/// The result is cached after the first call.
pub fn is_container() -> bool {
    *IS_CONTAINER
}

/// Performs container detection using multiple methods.
fn detect_container() -> bool {
    check_k8s_env_vars() || check_service_account() || check_container_markers() || check_cgroup()
}

/// Checks for Kubernetes environment variables.
/// These are automatically injected by K8s into all pods.
fn check_k8s_env_vars() -> bool {
    env::var("KUBERNETES_SERVICE_HOST").is_ok()
}

/// Checks for Kubernetes service account files.
fn check_service_account() -> bool {
    Path::new("/var/run/secrets/kubernetes.io/serviceaccount/token").exists()
}

/// Checks for container marker files.
fn check_container_markers() -> bool {
    Path::new("/.dockerenv").exists() || Path::new("/run/.containerenv").exists()
}

/// Checks cgroup for container-specific patterns.
fn check_cgroup() -> bool {
    let cgroup_path = Path::new("/proc/1/cgroup");
    if !cgroup_path.exists() {
        return false;
    }

    let Ok(content) = fs::read_to_string(cgroup_path) else {
        return false;
    };

    let patterns = [
        "kubepods",
        "docker",
        "containerd",
        "lxc",
        "/system.slice/containerd",
    ];
    patterns.iter().any(|p| content.contains(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_k8s_env_vars_without_env() {
        // In test environment without K8s, should return false
        // (unless actually running in K8s)
        let _ = check_k8s_env_vars();
    }

    #[test]
    fn test_is_container_returns_consistent_result() {
        let first = is_container();
        let second = is_container();
        assert_eq!(first, second);
    }
}
