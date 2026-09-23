//! The file-dialog seam (M9.pre).
//!
//! `rfd` is a desktop-only dependency: it has no Android backend and fails to
//! compile there, so every dialog in the app is reached through this type
//! instead of through the crate. Seven call sites in `app/controller.rs` build a
//! picker, and each of them handles the three answers the same way — take the
//! path, say nothing on a cancel, and put the reason in the notice bar when the
//! platform has no dialog to open at all. The third case is what keeps an
//! Android button from looking broken: the app does not silently do nothing, it
//! says what is missing.

use std::path::PathBuf;

/// What the user was being asked for.
#[derive(Clone, Copy)]
enum Kind {
    Open,
    Save,
}

/// The outcome of one dialog: a path, a cancel, or a platform that cannot ask.
pub enum Chosen {
    Path(PathBuf),
    Cancelled,
    Unsupported(&'static str),
}

/// A dialog under construction. The filter list is a `Vec` because a picker is
/// built once per click and thrown away; nothing here is hot enough to matter.
pub struct Picker {
    kind: Kind,
    title: Option<&'static str>,
    name: Option<String>,
    filters: Vec<(&'static str, &'static [&'static str])>,
}

impl Picker {
    pub fn open() -> Self {
        Self { kind: Kind::Open, title: None, name: None, filters: Vec::new() }
    }

    pub fn save() -> Self {
        Self { kind: Kind::Save, title: None, name: None, filters: Vec::new() }
    }

    pub fn title(mut self, title: &'static str) -> Self {
        self.title = Some(title);
        self
    }

    /// The name the save dialog opens with. Ignored by an open dialog, which is
    /// why the type carries the difference rather than a runtime check.
    pub fn name(mut self, name: &str) -> Self {
        self.name = Some(name.to_string());
        self
    }

    pub fn filter(mut self, label: &'static str, extensions: &'static [&'static str]) -> Self {
        self.filters.push((label, extensions));
        self
    }

    /// Show it. Blocking is deliberate, exactly as it was when the call sites
    /// named `rfd` directly: the dialog runs on the click that opened it, before
    /// the editor takes another event, so there is no half-finished import to
    /// reconcile afterwards.
    #[cfg(not(target_os = "android"))]
    pub fn pick(self) -> Chosen {
        let mut dialog = rfd::FileDialog::new();
        if let Some(title) = self.title {
            dialog = dialog.set_title(title);
        }
        if let Some(name) = &self.name {
            dialog = dialog.set_file_name(name);
        }
        for (label, extensions) in &self.filters {
            dialog = dialog.add_filter(*label, extensions);
        }
        let path = match self.kind {
            Kind::Open => dialog.pick_file(),
            Kind::Save => dialog.save_file(),
        };
        match path {
            Some(path) => Chosen::Path(path),
            None => Chosen::Cancelled,
        }
    }

    #[cfg(target_os = "android")]
    pub fn pick(self) -> Chosen {
        // The door is the Storage Access Framework (`ACTION_OPEN_DOCUMENT` /
        // `ACTION_CREATE_DOCUMENT` through an Intent), which needs an activity
        // result callback wired into `android_main` and a copy through a
        // `ContentResolver` on each side. Tracked as M9.5 in
        // docs/ANDROID_NOTES.md; until then the button names what the platform
        // cannot do, which is what keeps it from reading as broken — and the
        // two answers differ because what you lost is the thing you asked for.
        Chosen::Unsupported(match self.kind {
            Kind::Open => "Android cannot open a file from outside the app yet",
            Kind::Save => "Android cannot save a file outside the app yet",
        })
    }
}
