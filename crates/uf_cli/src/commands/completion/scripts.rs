//! The shipped completion scripts, one per shell.
//!
//! Each is a few lines because none of them decide anything: they collect the
//! words typed so far, hand them to `uf __complete`, and offer whatever comes
//! back. That keeps every rule about what may follow what in one place, in
//! Rust, where it is tested — and it means the scripts never go stale, because
//! there is nothing in them to go stale.

use crate::cli::Shell;

/// The completion script for `shell`.
pub(super) fn script(shell: Shell) -> &'static str {
    match shell {
        Shell::Bash => BASH,
        Shell::Zsh => ZSH,
        Shell::Fish => FISH,
        Shell::Elvish => ELVISH,
        Shell::PowerShell => POWERSHELL,
    }
}

/// `COMP_WORDS` includes the command itself, so it is dropped; `COMP_CWORD` is
/// the index being completed, which may be past the end when the cursor is on a
/// fresh word, and the empty string is exactly what `__complete` expects there.
const BASH: &str = r#"# uf completion for bash. Add to ~/.bashrc:
#   eval "$(uf completion bash)"
_uf_complete() {
  local words index
  words=("${COMP_WORDS[@]:1:COMP_CWORD}")
  COMPREPLY=($(compgen -W "$(uf __complete -- "${words[@]}" 2>/dev/null)" -- "${COMP_WORDS[COMP_CWORD]}"))
}
complete -F _uf_complete uf ufr ufx
"#;

/// `words` holds the whole line including the command; `CURRENT` is 1-based.
/// `compadd -- ${(f)...}` splits the reply on newlines, so a candidate that
/// contains a space still arrives as one candidate.
const ZSH: &str = r#"# uf completion for zsh. Add to ~/.zshrc:
#   eval "$(uf completion zsh)"
_uf_complete() {
  local -a candidates
  candidates=(${(f)"$(uf __complete -- ${words[2,CURRENT]} 2>/dev/null)"})
  compadd -- $candidates
}
compdef _uf_complete uf ufr ufx
"#;

/// fish completes one token at a time and has no notion of "the words so far"
/// as an array, so the current buffer is split by `commandline -opc`.
const FISH: &str = r#"# uf completion for fish. Add to ~/.config/fish/config.fish:
#   uf completion fish | source
function __uf_complete
    set -l words (commandline -opc) (commandline -ct)
    uf __complete -- $words[2..-1] 2>/dev/null
end
complete -c uf -f -a '(__uf_complete)'
complete -c ufr -f -a '(__uf_complete)'
complete -c ufx -f -a '(__uf_complete)'
"#;

const ELVISH: &str = r#"# uf completion for elvish. Add to ~/.config/elvish/rc.elv:
#   eval (uf completion elvish | slurp)
set edit:completion:arg-completer[uf] = {|@words|
  var rest = $words[1..]
  each {|candidate| put $candidate } [(uf __complete -- $@rest 2>/dev/null)]
}
set edit:completion:arg-completer[ufr] = $edit:completion:arg-completer[uf]
set edit:completion:arg-completer[ufx] = $edit:completion:arg-completer[uf]
"#;

const POWERSHELL: &str = r#"# uf completion for PowerShell. Add to $PROFILE:
#   uf completion powershell | Out-String | Invoke-Expression
Register-ArgumentCompleter -Native -CommandName uf, ufr, ufx -ScriptBlock {
    param($wordToComplete, $commandAst, $cursorPosition)
    $words = $commandAst.CommandElements | Select-Object -Skip 1 | ForEach-Object { $_.ToString() }
    uf __complete -- @words 2>$null | ForEach-Object {
        [System.Management.Automation.CompletionResult]::new($_, $_, 'ParameterValue', $_)
    }
}
"#;
