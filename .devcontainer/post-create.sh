#!/bin/sh
# Installs what the container needs beyond its image. Runs again on every
# rebuild, against a home directory that is kept, so every step has to be
# repeatable.
set -eu

# CI=1 only buys one line of output per step, instead of a progress display
# that the devcontainer CLI would relay frame by frame. It is not exported:
# with it, `mise trust` records nothing, because mise trusts every config
# while it is set.
quiet_mise() {
    CI=1 mise "$@"
}

# The toolchain that mise.toml pins: Rust and Typst.
mise trust --quiet
quiet_mise install

# What an agent session needs around the toolchain: node to run its server,
# with python3 for the native module that the server builds on first start,
# gh for git over HTTPS, and Claude Code.
# From the home directory, so that the project's settings do not apply to the
# global config.
sudo apt-get update -qq
sudo apt-get install -y -qq python3 >/dev/null
(cd "$HOME" && quiet_mise use --global node@lts gh claude-code)

# `mise use` installs but does not write shims, and the shims are what is on
# PATH.
mise reshim

# Interactive shells get the tools and the [env] of mise.toml without shims.
if ! grep -q 'mise activate' "$HOME/.bashrc" 2>/dev/null; then
    # shellcheck disable=SC2016  # expands when the shell starts
    printf '\n# Added by .devcontainer/post-create.sh\neval "$(mise activate bash)"\n' >>"$HOME/.bashrc"
fi
