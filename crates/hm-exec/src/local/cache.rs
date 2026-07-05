//! Host-side cache key derivation.
//!
//! Resolves a wire-typed [`Step`] to a deterministic cache key
//! so the scheduler can pass it to the runner for hit/miss decisions.
//!
//! Cache keys are computed by `harmont.keygen` at plan time and ride
//! along the JSON in `cache.key`.

use hm_plugin_protocol::{Step, Cache};

/// Derive a deterministic cache tag for a cacheable step.
///
/// Returns `None` when the step has no cache, a `"none"` policy, or no
/// cache key.
#[must_use]
pub(crate) fn stable_cache_tag(step: &Step) -> Option<String> {
    let cache = step.cache.as_ref()?;
    if matches!(cache, Cache::None){
        None
    } else {
        let key = step.key.clone();
        Some(format!("harmont-cache/{key}"))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use hm_plugin_protocol::{StepAction,Cache};

    fn step(cache: Option<Cache>) -> Step {
        Step {
            key: "build".into(),
            label: None,
            action: StepAction::Command{
                cmd: "true".into(),
                env: None,
            },
            image: None,
            timeout_seconds: None,
            cache,
            runner: None,
            runner_args: None,
        }
    }

    #[test]
    fn stable_cache_tag_for_cacheable_step() {
        let s = step(Some(Cache::Cache));
        let tag = stable_cache_tag(&s);
        assert_eq!(
            tag,
            Some("harmont-cache/build:0123456789abcdef".to_string())
        );
    }

    #[test]
    fn stable_cache_tag_none_for_uncacheable() {
        let s = step(None);
        assert_eq!(stable_cache_tag(&s), None);
    }

    #[test]
    fn stable_cache_tag_none_for_policy_none() {
        let s = step(Some(Cache::None));
        assert_eq!(stable_cache_tag(&s), None);
    }
}
