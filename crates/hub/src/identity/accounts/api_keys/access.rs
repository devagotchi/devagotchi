//! Access requests carry nested task scope; history cannot infer a project from a machine.
use super::KeyLimits;
impl KeyLimits {
    pub(crate) fn access_request(&self, machine: &str, project: Option<&str>) -> Option<String> {
        if !self.machines.is_empty() && !self.machines.iter().any(|allowed| allowed == machine) {
            return Some("select a machine allowed by this key".into());
        }
        if !self.projects.is_empty()
            && !project
                .is_some_and(|project| self.projects.iter().any(|allowed| allowed == project))
        {
            return Some("access requests from this key need an allowed task project".into());
        }
        None
    }
    pub(crate) fn access_history(&self, machine: Option<&str>) -> Option<String> {
        if !self.projects.is_empty() {
            return Some("project-limited keys cannot read environment access history".into());
        }
        if !self.machines.is_empty()
            && !machine
                .is_some_and(|machine| self.machines.iter().any(|allowed| allowed == machine))
        {
            return Some("select a machine allowed by this key".into());
        }
        None
    }
}
