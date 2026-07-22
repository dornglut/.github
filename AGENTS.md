# Agent instructions

Scope: organization-wide GitHub defaults only.

- Do not place product architecture, implementation plans, release state, or generated project data here.
- Keep defaults broadly applicable; repository-local files must be able to override them.
- Keep CI read-only and limited to validating this repository.
- Do not add organization secrets, deployment credentials, or write-enabled automation.
- Update `validation-required-files.txt` whenever a required path changes.
- Run `python scripts/validate.py` before proposing changes.
