# Enabling GitHub Actions

The OAuth token used to push this repo lacks the `workflow` scope, so
`.github/workflows/*.yml` cannot be created via `git push`. A ready-to-use
workflow lives at [`ci.yml.example`](ci.yml.example).

## One-time setup

1. Re-auth with workflow scope (or use a PAT that includes `workflow`):

   ```powershell
   gh auth refresh -h github.com -s workflow
   # or: gh auth login -h github.com -s repo,workflow
   ```

2. Install the workflow and push:

   ```powershell
   New-Item -ItemType Directory -Force -Path .github/workflows | Out-Null
   Copy-Item docs/ci.yml.example .github/workflows/ci.yml
   git add .github/workflows/ci.yml
   git commit -m "Enable CI workflow"
   git push
   ```

The workflow runs `cargo test --workspace` and `recursa score --k 1` on
`ubuntu-latest` for pushes/PRs to `main`.
