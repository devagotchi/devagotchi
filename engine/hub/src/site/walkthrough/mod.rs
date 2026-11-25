//! The landing page's setup walkthrough (`content/landing.json` `hero.walkthrough`): the
//! steps of getting started, each a scene of one or two screens (a terminal, a browser
//! window, a phone, an AI chat) drawn as HTML. The page moves to the next step on its own
//! after the step's seconds, or when a step is chosen (`site/web/landing/walkthrough.rs`).

use serde::Deserialize;

use super::model::Variants;

/// Most steps a walkthrough has: the stylesheet shows the chosen one of this many.
pub const MAX_STEPS: usize = 8;
/// Most screens one step draws side by side.
pub const MAX_SCREENS: usize = 2;
/// How long a step shows before the next, when it says nothing: four seconds.
pub const DEFAULT_SECONDS: f32 = 4.0;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Walkthrough {
    /// What it is, for a screen reader ("Setting up {product}, step by step").
    pub label: String,
    /// The label of the button that stops it moving on its own, and starts it again.
    pub pause: String,
    pub play: String,
    /// The steps, by front: a hosted hub starts at connecting an AI, a hub of one's own at
    /// its server.
    pub steps: Variants<Vec<WalkStep>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WalkStep {
    /// The step's short name on its tab ("Server").
    pub tab: String,
    /// What happens in it, in one line under the screens.
    pub caption: String,
    /// How long it shows before the next, 2 to 10 (default 4).
    #[serde(default)]
    pub seconds: Option<f32>,
    pub screens: Vec<Screen>,
}

impl WalkStep {
    pub fn seconds(&self) -> f32 {
        self.seconds.unwrap_or(DEFAULT_SECONDS)
    }
}

/// What a screen is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScreenKind {
    /// A terminal: `title` on its bar; a line starting `$ ` is typed, a line starting `! `
    /// is the one to notice (highlighted), any other is output.
    Terminal,
    /// A browser window: `title` is its address.
    Window,
    /// A phone's browser: `title` is its address; with `notice` set it is the lock screen
    /// with that notification instead.
    Phone,
    /// An AI app's conversation: each line starts `you: `, `ai: ` or `tool: ` (then the tool's
    /// name, a space, and what it did).
    Chat,
}

/// One screen. Which fields it uses depends on its kind; the rest stay empty.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Screen {
    pub kind: ScreenKind,
    /// The terminal's or window's title bar, or the page's address.
    #[serde(default)]
    pub title: String,
    /// The page's heading.
    #[serde(default)]
    pub heading: String,
    /// Terminal or chat lines; a page's paragraphs.
    #[serde(default)]
    pub lines: Vec<String>,
    /// A page's form fields, as they are filled in.
    #[serde(default)]
    pub fields: Vec<Field>,
    /// A page's code box (an address, the line to paste).
    #[serde(default)]
    pub code: String,
    /// The page's main button, tapped at the end of the step.
    #[serde(default)]
    pub button: String,
    /// A second button beside it.
    #[serde(default)]
    pub secondary: String,
    /// A status the page shows once the step is done ("Connected", "online").
    #[serde(default)]
    pub status: String,
    /// A phone's lock-screen notification: its title and its text.
    #[serde(default)]
    pub notice: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Field {
    pub label: String,
    pub value: String,
}

impl Walkthrough {
    pub fn check(&self, what: &str) -> Result<(), String> {
        self.steps.check(&format!("{what}.steps"))?;
        for (front, steps) in self.steps.each() {
            let what = format!("{what}.steps ({front})");
            if steps.is_empty() || steps.len() > MAX_STEPS {
                return Err(format!("{what}: 1 to {MAX_STEPS} steps"));
            }
            for (n, step) in steps.iter().enumerate() {
                let what = format!("{what} {}", n + 1);
                if step.tab.trim().is_empty() || step.caption.trim().is_empty() {
                    return Err(format!("{what}: a tab and a caption"));
                }
                if !(2.0..=10.0).contains(&step.seconds()) {
                    return Err(format!("{what}: seconds is 2 to 10"));
                }
                if step.screens.is_empty() || step.screens.len() > MAX_SCREENS {
                    return Err(format!("{what}: 1 to {MAX_SCREENS} screens"));
                }
                for screen in &step.screens {
                    screen.check(&what)?;
                }
            }
        }
        Ok(())
    }
}

impl Screen {
    fn check(&self, what: &str) -> Result<(), String> {
        match self.kind {
            ScreenKind::Chat => {
                for line in &self.lines {
                    if !["you: ", "ai: ", "tool: "]
                        .iter()
                        .any(|p| line.starts_with(p))
                    {
                        return Err(format!(
                            "{what}: a chat line starts `you: `, `ai: ` or `tool: `, not {line:?}"
                        ));
                    }
                }
            }
            ScreenKind::Phone if !self.notice.is_empty() && self.notice.len() != 2 => {
                return Err(format!(
                    "{what}: a phone's notice is its title and its text"
                ));
            }
            _ => {}
        }
        if self.lines.len() > 12 || self.fields.len() > 4 {
            return Err(format!("{what}: at most 12 lines and 4 fields a screen"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
