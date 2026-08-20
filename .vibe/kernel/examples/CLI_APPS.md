# CLI Apps

Use this file only for command-line applications that users install and run from a shell.

Do not read it for libraries, services, bots, web apps, or non-installable tools.

## Required CLI contract

Every CLI app must provide:

- GitHub Release CI/CD from `.vibe/kernel/examples/GITHUB_RELEASES.md`;
- one-command install or update from the release/source;
- README documentation for that exact install command;
- `-V` and `--version` flags that print the installed version;
- a self-update command named `update`;
- all installed binaries and runtime artifacts under `~/.x-cli-<project-name>`;
- PATH setup that makes the installed binary available after install/update.
- `GH_INSTALLER_TOKEN` as the default environment variable name for a user-supplied private GitHub installer token.

Use the repository directory basename as `<project-name>` unless the project has one stable CLI name documented in its Makefile and README.

## Home layout

All CLI app install/update commands must use this layout:

```make
PROJECT_NAME := $(notdir $(CURDIR))
X_CLI_HOME := $(HOME)/.x-cli-$(PROJECT_NAME)
X_CLI_BIN_DIR := $(X_CLI_HOME)
X_CLI_RUNTIME_DIR := $(X_CLI_HOME)/runtime
X_CLI_CACHE_DIR := $(X_CLI_HOME)/cache
X_CLI_TOKEN_FILE := $(X_CLI_HOME)/gh-token
```

Rules:

- executable binaries live in `$(X_CLI_BIN_DIR)`;
- single-binary CLIs may use `$(X_CLI_HOME)` as `$(X_CLI_BIN_DIR)`;
- multi-binary CLIs may use `$(X_CLI_HOME)/bin` as `$(X_CLI_BIN_DIR)`;
- runtime state, caches, temporary update downloads, and logs live under `$(X_CLI_HOME)`;
- install/update commands create needed directories with private permissions where possible;
- install/update commands must not write routine runtime artifacts into the project repo;
- update downloads should be staged under `$(X_CLI_HOME)` and moved into place atomically when possible.

## PATH rule

Install and update commands must ensure `$(X_CLI_BIN_DIR)` is on the user's PATH.

Use an idempotent shell-profile block, for example:

```sh
# x-cli-<project-name>
export PATH="$HOME/.x-cli-<project-name>:$PATH"
# /x-cli-<project-name>
```

Rules:

- point PATH at the directory that actually contains the executable;
- do not duplicate PATH entries;
- update the likely shell profile for the user's shell (`.zshrc`, `.bashrc`, or equivalent);
- if the running process cannot update the current parent shell, print the exact one-line command the user can run for the current session;
- repeat this check after updates, because users may move machines or edit profiles.

## Version and update commands

CLI apps must support:

```text
<command> -V
<command> --version
<command> update
```

`-V` and `--version` must print the CLI name and semantic version.

`update` must reinstall from the same source channel used by the installer:

- public GitHub repo: latest suitable GitHub Release or documented source install path;
- private GitHub repo: authenticated GitHub Release or source path using the token stored in `$(X_CLI_TOKEN_FILE)`.

If the app cannot update safely, `update` must fail with a clear error and leave the old binary in place.

## Private repository token

If a CLI app is installed or updated from a private GitHub repo, store the GitHub token at:

```text
~/.x-cli-<project-name>/gh-token
```

Rules:

- create the token file with mode `0600`;
- never commit the token;
- never write the token to README examples, shell profiles, logs, release assets, or error output;
- the installer must accept `GH_INSTALLER_TOKEN` as the default environment variable for a user-supplied private GitHub token;
- the installer may also read the token from a prompt or GitHub CLI, but the updater must read it from `$(X_CLI_TOKEN_FILE)`;
- if the token is missing or invalid, the update command must explain how to refresh it without printing token contents.

## README install docs

The README must include an exact one-command install example with no placeholders.

Public repo shape:

```sh
curl -fsSL https://raw.githubusercontent.com/<owner>/<repo>/main/install.sh | sh
```

Private repo shape:

```sh
GH_INSTALLER_TOKEN="$(gh auth token)" sh -c 'curl -fsSL -H "Authorization: Bearer $GH_INSTALLER_TOKEN" https://raw.githubusercontent.com/<owner>/<repo>/main/install.sh | GH_INSTALLER_TOKEN="$GH_INSTALLER_TOKEN" sh'
```

Replace `<owner>` and `<repo>` before documenting. If the project uses a different installer URL, document the exact real command.

The README must also document:

- the installed binary path;
- the version check command;
- the self-update command;
- where private repo update tokens are stored when applicable.

## Standardization checklist

For an existing CLI child project:

1. Read `.vibe/kernel/examples/GITHUB_RELEASES.md` and this file.
2. Add or repair release CI/CD.
3. Ensure install/update writes under `~/.x-cli-<project-name>` and documents the executable directory.
4. Ensure PATH setup is idempotent.
5. Ensure `-V`, `--version`, and `update` work.
6. Update README install/update docs with exact commands.
7. Run `make check` or the closest real verification command.
