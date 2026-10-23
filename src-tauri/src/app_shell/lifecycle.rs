//! Exit handling. For now: the before-exit hook registry used by downstream specs.

use std::sync::{Arc, Mutex};

/// What a hook could not save. Each entry is a short user-facing description, e.g.
/// `"注釈: 20261022_173512"`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExitCheck {
    pub unsaved: Vec<String>,
}

type Hook = Arc<dyn Fn() -> ExitCheck + Send + Sync + 'static>;

/// Managed as `Arc<BeforeExitHooks>`. Downstream specs register in their own `setup`.
#[derive(Default)]
pub struct BeforeExitHooks {
    hooks: Mutex<Vec<(&'static str, Hook)>>,
}

impl BeforeExitHooks {
    /// `hook` runs on a worker thread when the user quits. It tries to save pending data and
    /// returns what is still unsaved. It may block but must time out on its own (about 3 s).
    /// It runs again each time the user picks "retry", so it must be idempotent.
    pub fn register(
        &self,
        name: &'static str,
        hook: Box<dyn Fn() -> ExitCheck + Send + Sync + 'static>,
    ) {
        self.hooks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((name, Arc::from(hook)));
    }

    /// Runs every hook in registration order (without holding the registry lock) and merges
    /// what remains unsaved, in the same order.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "called by Lifecycle::quit in task 7.x")
    )]
    pub(crate) fn run_all(&self) -> ExitCheck {
        let hooks: Vec<(&'static str, Hook)> =
            self.hooks.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let mut result = ExitCheck::default();
        for (name, hook) in hooks {
            let check = hook();
            if !check.unsaved.is_empty() {
                log::warn!("exit: {name} left {} unsaved item(s)", check.unsaved.len());
            }
            result.unsaved.extend(check.unsaved);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unsaved(items: &[&str]) -> ExitCheck {
        ExitCheck {
            unsaved: items.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn no_hooks_means_nothing_unsaved() {
        assert_eq!(BeforeExitHooks::default().run_all(), ExitCheck::default());
    }

    #[test]
    fn merges_unsaved_items_in_registration_order() {
        let hooks = BeforeExitHooks::default();
        hooks.register(
            "capture-library",
            Box::new(|| unsaved(&["画像: 20261022_173500"])),
        );
        hooks.register("clean", Box::new(ExitCheck::default));
        hooks.register(
            "annotation-editor",
            Box::new(|| unsaved(&["注釈: 20261022_173512", "注釈: 20261022_173530"])),
        );

        assert_eq!(
            hooks.run_all(),
            unsaved(&[
                "画像: 20261022_173500",
                "注釈: 20261022_173512",
                "注釈: 20261022_173530",
            ])
        );
    }

    #[test]
    fn hooks_run_again_on_each_retry() {
        let hooks = Arc::new(BeforeExitHooks::default());
        let remaining = Arc::new(Mutex::new(2));
        let counter = remaining.clone();
        hooks.register(
            "flaky",
            Box::new(move || {
                let mut left = counter.lock().unwrap();
                *left -= 1;
                if *left > 0 {
                    unsaved(&["注釈: 1"])
                } else {
                    ExitCheck::default()
                }
            }),
        );

        assert_eq!(hooks.run_all().unsaved.len(), 1);
        assert_eq!(hooks.run_all(), ExitCheck::default());
    }
}
