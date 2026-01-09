# Create Pull Request

Create a GitHub pull request for the current branch with an auto-generated description.

## Pre-flight Checks

**CRITICAL: Never work directly on main/master. All changes require a PR.**

### 1. Check Current Branch

```bash
git branch --show-current
```

**If on `main` or `master`:**
- Check if there are uncommitted changes or unpushed commits
- If yes: Create a new feature branch first, then proceed
- Branch naming: `feature/<short-description>` or `fix/<short-description>`

```bash
# Create and switch to new branch
git checkout -b feature/<description>
```

### 2. Check for Uncommitted Changes

```bash
git status --porcelain
```

**If there are uncommitted changes:**
- Stage all changes: `git add -A`
- Create a commit with a descriptive message
- Use conventional commit format: `type: description`

## Main Flow

### 3. Run Local QA Checks

**CRITICAL: Always run QA checks before pushing to catch issues early.**

```bash
./scripts/qa.sh
```

This script runs:
- `cargo fmt --all -- --check` (formatting)
- `cargo clippy --all-targets -- -D warnings` (lints)
- `cargo test` (tests)
- `cargo doc --no-deps` (documentation)
- `cargo build --release` (release build)

**If any check fails:**
- Run `./scripts/qa.sh --fix` to auto-fix formatting
- Fix other issues manually before proceeding

**Only proceed to push after all checks pass.**

### 4. Gather Information

Run these commands to understand the current state:

```bash
# Get current branch
git branch --show-current

# Check if we're ahead of remote
git status -sb

# Get commits on this branch (not on main/master)
git log main..HEAD --oneline 2>/dev/null || git log master..HEAD --oneline

# Get detailed diff summary
git diff main --stat 2>/dev/null || git diff master --stat
```

### 5. Analyze Changes

Based on the commits and diff:
- Identify the type of change (feature, fix, refactor, docs, etc.)
- List the key modifications
- Note any breaking changes

### 6. Push to Remote

Push the branch to remote (required for PR):

```bash
git push -u origin $(git branch --show-current)
```

### 7. Create the Pull Request

Use `gh pr create` with a well-formatted title and body.

**Title format:** `<type>: <concise description>`

**Body format:**
```bash
gh pr create --title "<type>: <description>" --body "$(cat <<'EOF'
## Summary
<2-3 bullet points describing what this PR does>

## Changes
<List of key changes, grouped by area>

## Test Plan
<How to verify these changes work>

EOF
)"
```

### 8. Report the Result

After creating the PR, display:
- The PR URL (clickable)
- The PR number
- Any CI checks that will run

## Commit & PR Title Types

| Type | Description |
|------|-------------|
| `feat` | New feature |
| `fix` | Bug fix |
| `refactor` | Code refactoring |
| `docs` | Documentation changes |
| `chore` | Maintenance tasks |
| `test` | Test additions/changes |
| `style` | Formatting, no code change |

## Branch Naming

When creating a new branch from main:
- `feature/<description>` - New features
- `fix/<description>` - Bug fixes
- `docs/<description>` - Documentation
- `refactor/<description>` - Refactoring

Use kebab-case: `feature/add-network-monitoring`

## Error Handling

- **On main/master with changes**: Create branch first, then PR
- **On main/master without changes**: Ask user what they want to do
- **No commits ahead of main**: Nothing to create PR for
- **Not authenticated with gh**: Run `gh auth login`
- **No remote**: Add remote first

## Examples

Good PR titles:
- `feat: Add network hop analysis for outage detection`
- `fix: Resolve ping timeout handling on slow connections`
- `docs: Update README with Vigil branding`
- `refactor: Extract traceroute parsing into separate module`
