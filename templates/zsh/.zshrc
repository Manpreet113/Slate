# History
HISTFILE=~/.zsh_history
HISTSIZE=10000
SAVEHIST=10000

setopt APPEND_HISTORY
setopt INC_APPEND_HISTORY
setopt SHARE_HISTORY
setopt HIST_IGNORE_DUPS
setopt HIST_IGNORE_ALL_DUPS
setopt HIST_REDUCE_BLANKS

# Modern replacements
alias ls='eza --icons --group-directories-first'
alias ll='eza -lh --icons --group-directories-first'
alias la='eza -lah --icons --group-directories-first'
alias cat='bat --paging=never'
alias grep='rg'
alias find='fd'

# Git shortcuts
alias gs='git status'
alias gl='git log --oneline --graph --decorate'
alias gd='git diff'

# Zoxide (if installed)
if command -v zoxide >/dev/null 2>&1; then
    eval "$(zoxide init zsh)"
fi

# Minimal prompt
PS1='%F{cyan}%n%f@%F{white}%m%f %F{blue}%~%f %# '
