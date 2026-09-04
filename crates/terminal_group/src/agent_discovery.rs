use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstalledAgent {
    pub name: &'static str,
    pub command: &'static str,
    pub path: PathBuf,
}

const KNOWN_AGENTS: [(&str, &str); 8] = [
    ("Pi", "pi"),
    ("Claude Code", "claude"),
    ("Codex CLI", "codex"),
    ("Aider", "aider"),
    ("OpenCode", "opencode"),
    ("Gemini CLI", "gemini"),
    ("Goose", "goose"),
    ("Amp", "amp"),
];

pub fn installed_agents() -> Vec<InstalledAgent> {
    KNOWN_AGENTS
        .iter()
        .filter_map(|&(name, command)| {
            which::which(command).ok().map(|path| InstalledAgent {
                name,
                command,
                path,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::KNOWN_AGENTS;
    use std::collections::HashSet;

    #[test]
    fn known_agents_have_unique_commands() {
        let commands = KNOWN_AGENTS
            .iter()
            .map(|(_, command)| *command)
            .collect::<HashSet<_>>();
        assert_eq!(commands.len(), KNOWN_AGENTS.len());
    }
}
