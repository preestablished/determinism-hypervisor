# Project Instructions for AI Agents

This file provides instructions and context for AI coding agents working on this project.

<!-- BEGIN BEANS INTEGRATION -->
## Beans Issue Tracker

This project uses **bn** for issue tracking. Run `bn prime` for the authoritative workflow.

### Quick Reference

```bash
bn ready              # Find available work
bn show <id>          # View issue details
bn update <id> --claim  # Claim work
bn close <id> -r "reason"         # Complete work
```

### Rules

- Use `bn` for ALL task tracking — do NOT use TodoWrite, TaskCreate, or markdown TODO lists
- Run `bn prime` for detailed command reference and session close protocol
- Use `bn remember` for persistent knowledge — do NOT use MEMORY.md files
- Mutating `bn` commands commit and push the separate shared hub; never edit or commit hub files by hand.

## Session Completion

**When ending a work session**, you MUST complete ALL steps below. Work is NOT complete until `git push` succeeds.

**MANDATORY WORKFLOW:**

1. **File issues for remaining work** - Create issues for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **Update issue status** - Close finished work, update in-progress items
4. **PUSH TO REMOTE** - This is MANDATORY:
   ```bash
   git pull --rebase
   bn status --json # shared hub must be synchronized
   git push
   git status  # MUST show "up to date with origin"
   bn status --json  # shared hub must be synchronized
   ```
5. **Clean up** - Clear stashes, prune remote branches
6. **Verify** - All changes committed AND pushed
7. **Hand off** - Provide context for next session

**CRITICAL RULES:**
- Work is NOT complete until `git push` succeeds
- NEVER stop before pushing - that leaves work stranded locally
- NEVER say "ready to push when you are" - YOU must push
- If push fails, resolve and retry until it succeeds
<!-- END BEANS INTEGRATION -->


## Build & Test

_Add your build and test commands here_

```bash
# Example:
# npm install
# npm test
```

## Architecture Overview

_Add a brief overview of your project architecture_

## Conventions & Patterns

_Add your project-specific conventions here_
