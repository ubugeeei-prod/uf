use compact_str::CompactString;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

pub type CommandList = SmallVec<[CliCommand; 8]>;
pub type ArgumentList = SmallVec<[CliArgument; 8]>;
pub type OptionList = SmallVec<[CliOption; 8]>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CliApp {
    pub name: CompactString,
    pub about: CompactString,
    pub commands: CommandList,
}

impl CliApp {
    pub fn new(name: impl Into<CompactString>) -> Self {
        Self {
            name: name.into(),
            about: CompactString::new(""),
            commands: SmallVec::new(),
        }
    }

    pub fn about(mut self, about: impl Into<CompactString>) -> Self {
        self.about = about.into();
        self
    }

    pub fn command(mut self, command: CliCommand) -> Self {
        self.commands.push(command);
        self
    }

    pub fn resolve<'app, 'argv>(
        &'app self,
        argv: &'argv [&'argv str],
    ) -> Option<ResolvedCommand<'app, 'argv>> {
        let name = *argv.first()?;
        let command = self
            .commands
            .iter()
            .find(|command| command.name.as_str() == name)?;
        Some(ResolvedCommand {
            command,
            rest: &argv[1..],
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CliCommand {
    pub name: CompactString,
    pub about: CompactString,
    pub arguments: ArgumentList,
    pub options: OptionList,
}

impl CliCommand {
    pub fn new(name: impl Into<CompactString>) -> Self {
        Self {
            name: name.into(),
            about: CompactString::new(""),
            arguments: SmallVec::new(),
            options: SmallVec::new(),
        }
    }

    pub fn about(mut self, about: impl Into<CompactString>) -> Self {
        self.about = about.into();
        self
    }

    pub fn arg(mut self, argument: CliArgument) -> Self {
        self.arguments.push(argument);
        self
    }

    pub fn option(mut self, option: CliOption) -> Self {
        self.options.push(option);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CliArgument {
    pub name: CompactString,
    pub required: bool,
}

impl CliArgument {
    pub fn required(name: impl Into<CompactString>) -> Self {
        Self {
            name: name.into(),
            required: true,
        }
    }

    pub fn optional(name: impl Into<CompactString>) -> Self {
        Self {
            name: name.into(),
            required: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CliOption {
    pub long: CompactString,
    pub short: Option<char>,
    pub value: OptionValue,
}

impl CliOption {
    pub fn flag(long: impl Into<CompactString>) -> Self {
        Self {
            long: long.into(),
            short: None,
            value: OptionValue::Bool,
        }
    }

    pub fn string(long: impl Into<CompactString>) -> Self {
        Self {
            long: long.into(),
            short: None,
            value: OptionValue::String,
        }
    }

    pub fn short(mut self, short: char) -> Self {
        self.short = Some(short);
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OptionValue {
    Bool,
    String,
    Number,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedCommand<'app, 'argv> {
    pub command: &'app CliCommand,
    pub rest: &'argv [&'argv str],
}

pub fn define_cli(name: impl Into<CompactString>) -> CliApp {
    CliApp::new(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defines_command_graph() {
        let app = define_cli("ufx").about("example").command(
            CliCommand::new("deploy")
                .about("deploy anywhere")
                .arg(CliArgument::required("target"))
                .option(CliOption::flag("dry-run").short('d')),
        );

        assert_eq!(app.name, "ufx");
        assert_eq!(app.commands.len(), 1);
        assert_eq!(app.commands[0].arguments[0].name, "target");
        assert_eq!(app.commands[0].options[0].short, Some('d'));
    }

    #[test]
    fn resolves_top_level_command() {
        let app = define_cli("ufx").command(CliCommand::new("check"));
        let resolved = app.resolve(&["check", "--strict"]).expect("command");

        assert_eq!(resolved.command.name, "check");
        assert_eq!(resolved.rest, ["--strict"]);
    }
}
