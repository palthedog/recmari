# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

## Commands

## Architecture

### Monorepo Structure

## Code Quality Rules
- **No duplicate logic**: Never copy-paste code to create variants. If you need a different return type, modify the original function or use composition.

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
