# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

## Commands

## Architecture

### Monorepo Structure

## Code Quality Rules
- **No duplicate logic**: Never copy-paste code to create variants. If you need a different return type, modify the original function or use composition.

### Logging (tracing)
- **Log aggressively**: Especially at info and above, include any information that could help future debugging. Don't wait until you need to debug to add logs — logs should already be there when you need them.
- **info level**: Should convey what is happening at a glance. Always log success cases too (do NOT follow the Unix "silence is success" convention).
- **warn/error + return Err**: When a function returns an error, always log the error before returning. Do not rely on the caller to log it.
- **Default log level**: info (configured via `RUST_LOG` env var)

### Assertions
- **Assert liberally**: Add assertions even for seemingly obvious invariants. If you would write a comment like "arg must be 0..10", write an `assert!` instead.
- **Don't trust function arguments**: Validate inputs with assertions at function boundaries rather than assuming correctness.

### Keep everything as small as possible
- **Block size**: Keep blocks under ~50 lines; extract meaningful functions even if used only once
- **Nesting**: Keep nesting shallow (max 4 levels); use early return pattern
- **Inner functions**: Avoid unless necessary

## Code Style

- **Comments**: All source code comments must be written in English
- **File naming**: Use kebab-case (e.g., `game-tree-builder.ts`, not `gameTreeBuilder.ts`)

## Pre-commit Checklist (REQUIRED)

**Every commit must pass all unit tests.

## Development Guidelines

- Do not include `Co-Authored-By` in commit messages
