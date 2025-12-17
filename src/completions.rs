use anyhow::Result;
use clap_complete::Shell;

pub fn handle_completions(shell: Shell) -> Result<()> {
    match shell {
        Shell::Bash => print_bash_completions(),
        Shell::Zsh => print_zsh_completions(),
        Shell::Fish => print_fish_completions(),
        _ => {
            eprintln!("Unsupported shell: {:?}", shell);
            eprintln!("Supported shells: bash, zsh, fish");
        }
    }
    Ok(())
}

fn print_bash_completions() {
    println!(
        r#"#!/bin/bash

_agentdev() {{
    local cur prev words cword
    if type _init_completion &>/dev/null; then
        _init_completion || return
    else
        # Fallback for older bash-completion
        COMPREPLY=()
        cur="${{COMP_WORDS[COMP_CWORD]}}"
        prev="${{COMP_WORDS[COMP_CWORD-1]}}"
        words=("${{COMP_WORDS[@]}}")
        cword=$COMP_CWORD
    fi

    # Main commands
    local commands="worktree sessions ui completions"
    local wt_subs="create open delete add rename list clean dir"

    # Complete main commands
    if [[ $cword -eq 1 ]]; then
        COMPREPLY=($(compgen -W "$commands" -- "$cur"))
        return
    fi

    # Complete subcommand arguments
    case "${{words[1]}}" in
        worktree)
            # Complete worktree subcommands
            if [[ $cword -eq 2 ]]; then
                COMPREPLY=($(compgen -W "$wt_subs" -- "$cur"))
                return
            fi
            case "${{words[2]}}" in
                open|dir|delete)
                    if [[ $cword -eq 3 ]]; then
                        local worktrees=$(agentdev complete-worktrees 2>/dev/null)
                        COMPREPLY=($(compgen -W "$worktrees" -- "$cur"))
                    fi
                    ;;
                rename)
                    if [[ $cword -eq 3 ]]; then
                        local worktrees=$(agentdev complete-worktrees 2>/dev/null)
                        COMPREPLY=($(compgen -W "$worktrees" -- "$cur"))
                    fi
                    ;;
            esac
            ;;
        open|dir|delete)
            if [[ $cword -eq 2 ]]; then
                # Back-compat: top-level alias
                local worktrees=$(agentdev complete-worktrees 2>/dev/null)
                COMPREPLY=($(compgen -W "$worktrees" -- "$cur"))
            fi
            ;;
        rename)
            if [[ $cword -eq 2 ]]; then
                # Back-compat: top-level alias, complete first argument (old name)
                local worktrees=$(agentdev complete-worktrees 2>/dev/null)
                COMPREPLY=($(compgen -W "$worktrees" -- "$cur"))
            fi
            ;;
        completions)
            if [[ $cword -eq 2 ]]; then
                COMPREPLY=($(compgen -W "bash zsh fish" -- "$cur"))
            fi
            ;;
        sessions)
            if [[ $cword -eq 2 ]]; then
                COMPREPLY=($(compgen -W "list" -- "$cur"))
            fi
            ;;
    esac
}}

complete -F _agentdev agentdev
"#
    );
}

fn print_zsh_completions() {
    println!(
        r#"#compdef agentdev

_agentdev() {{
    local -a commands
    commands=(
        'worktree:Worktree management commands'
        'sessions:Session inspection commands'
        'completions:Generate shell completions'
        'ui:Launch web UI for agent management'
    )

    # Main command completion
    if (( CURRENT == 2 )); then
        _describe 'command' commands
        return
    fi

    # Subcommand argument completion
    case "${{words[2]}}" in
        worktree)
            local -a wt_subs
            wt_subs=(
                'create:Create a new git worktree'
                'open:Open an existing worktree and launch Claude'
                'delete:Delete a worktree and clean up'
                'add:Add current worktree to management'
                'rename:Rename a worktree'
                'list:List all active instances'
                'clean:Clean up invalid worktrees from state'
                'dir:Get the directory path of a worktree'
            )
            if (( CURRENT == 3 )); then
                _describe 'worktree command' wt_subs
                return
            fi
            case "${{words[3]}}" in
                open|dir|delete)
                    if (( CURRENT == 4 )); then
                        __agentdev_worktrees
                    fi
                    ;;
                rename)
                    if (( CURRENT == 4 )); then
                        __agentdev_worktrees
                    elif (( CURRENT == 5 )); then
                        _message "new name"
                    fi
                    ;;
                create|add)
                    if (( CURRENT == 4 )); then
                        _message "worktree name"
                    fi
                    ;;
            esac
            ;;
        sessions)
            if (( CURRENT == 3 )); then
                local -a session_subs
                session_subs=(
                    'list:List recorded sessions'
                )
                _describe 'sessions command' session_subs
            fi
            ;;
        completions)
            if (( CURRENT == 3 )); then
                local -a shells
                shells=(bash zsh fish)
                _describe 'shell' shells
            fi
            ;;
    esac
}}

__agentdev_worktrees() {{
    local -a worktrees
    local IFS=$'\n'
    
    # Get detailed worktree information (sorted by repo, then by name)
    local worktree_data
    worktree_data=($(agentdev complete-worktrees --format=detailed 2>/dev/null))
    
    if [[ -n "$worktree_data" ]]; then
        for line in $worktree_data; do
            # Parse tab-separated values: name<TAB>repo<TAB>path<TAB>sessions
            local name=$(echo "$line" | cut -f1)
            local repo=$(echo "$line" | cut -f2)
            local sessions=$(echo "$line" | cut -f4)
            
            # Add worktree with clear repo marker and session info
            worktrees+=("$name:[$repo] $sessions")
        done
        
        # Use _describe for better presentation
        # -V flag preserves the order (no sorting)
        if (( ${{#worktrees[@]}} > 0 )); then
            _describe -V -t worktrees 'worktree' worktrees
        fi
    else
        # Fallback to simple completion
        local simple_worktrees
        simple_worktrees=($(agentdev complete-worktrees 2>/dev/null))
        if [[ -n "$simple_worktrees" ]]; then
            compadd -a simple_worktrees
        fi
    fi
}}

_agentdev "$@"
"#
    );
}

fn print_fish_completions() {
    println!(
        r#"# Fish completion for agentdev

# Disable file completions by default
complete -c agentdev -f

# Main commands
complete -c agentdev -n "__fish_use_subcommand" -a worktree -d "Worktree management commands"
complete -c agentdev -n "__fish_use_subcommand" -a sessions -d "Session inspection commands"
complete -c agentdev -n "__fish_use_subcommand" -a ui -d "Launch web UI"
complete -c agentdev -n "__fish_use_subcommand" -a completions -d "Generate shell completions"

# Function to get worktree completions with repo markers
function __agentdev_worktrees
    agentdev complete-worktrees --format=detailed 2>/dev/null | while read -l line
        # Split tab-separated values: name<TAB>repo<TAB>path<TAB>sessions
        set -l parts (string split \t $line)
        if test (count $parts) -ge 4
            set -l name $parts[1]
            set -l repo $parts[2]
            set -l sessions $parts[4]
            echo "$name\t[$repo] $sessions"
        end
    end
end

# Simple worktree names (fallback)
function __agentdev_worktrees_simple
    agentdev complete-worktrees 2>/dev/null
end

# Worktree completions for commands
complete -c agentdev -n "__fish_seen_subcommand_from worktree; and __fish_seen_subcommand_from open dir delete" -a "(__agentdev_worktrees)"
complete -c agentdev -n "__fish_seen_subcommand_from worktree; and __fish_seen_subcommand_from rename" -n "not __fish_seen_argument_from (__agentdev_worktrees_simple)" -a "(__agentdev_worktrees)"
complete -c agentdev -n "__fish_seen_subcommand_from sessions" -a list -d "List recorded sessions"

# Shell completions for completions command
complete -c agentdev -n "__fish_seen_subcommand_from completions" -a "bash zsh fish"
"#
    );
}
