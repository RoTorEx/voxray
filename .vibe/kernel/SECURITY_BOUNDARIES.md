# Security Boundaries

## Default rules

- Do not commit secrets.
- Do not log raw secrets, tokens, private prompts, or sensitive payloads.
- Treat network access as an explicit project boundary.
- Treat local-first constraints as product truth.
- Manifest, environment, deployment, and auth configuration are child project truth.
- For web apps, name environment variables as `SERVICE__SETTING_NAME`: use a double underscore between the service/integration namespace and the setting purpose, and single underscores within the setting purpose.
- Keep env var naming consistent within a service. Do not mix styles such as `GOOGLE_CLIENT_ID` and `GOOGLE__CLIENT_ID` for the same service.
- For CLI apps installed from private GitHub repos, use `GH_INSTALLER_TOKEN` as the default installer token input name, then store update tokens only under `~/.x-cli-<project-name>/gh-token` with mode `0600`; never commit them, put them in shell profiles, or print them in logs/errors.

## Parent vs child

The kernel defines safety principles.
Child projects define concrete boundaries.

Local overrides may strengthen boundaries.
Local overrides must not silently weaken boundaries.

## Agent behavior

Before changing security, environment, network, storage, logging, or deployment boundaries:

1. read local overrides;
2. read relevant contracts/docs;
3. explain the boundary change;
4. ask for explicit approval if weakening anything.
