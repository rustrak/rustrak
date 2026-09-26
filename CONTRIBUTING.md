# Contributing to Rustrak

Thank you for your interest in contributing to Rustrak! This guide will help you get started with the project and understand our development workflow.

## Table of Contents
- [Getting Started](#getting-started)
- [Development Workflow](#development-workflow)
- [Project Structure](#project-structure)
- [Code Standards](#code-standards)
- [Submitting Changes](#submitting-changes)
- [Community Guidelines](#community-guidelines)

## Getting Started

### Prerequisites

Before you begin, ensure you have the following installed:

- **Rust** - For the server component; `rustup` installs the version pinned in `rust-toolchain.toml`
- **Node.js** (22.x+) - For the UI and build tools; CI and the images use 24
- **pnpm** (10.x) - Package manager; `corepack enable` picks the version pinned in `package.json`
- **Docker** - For local development and testing

### Local Development Setup

1. **Fork and Clone the Repository**

   ```bash
   git clone https://github.com/YOUR_USERNAME/rustrak.git
   cd rustrak
   ```

2. **Install Dependencies**

   ```bash
   pnpm install
   ```

3. **Start PostgreSQL for Development**

   ```bash
   docker-compose -f docker-compose.dev.yml up -d postgres
   ```

4. **Configure the Server**

   ```bash
   cd apps/server
   cp .env.example .env
   ```

   The example configuration connects to the PostgreSQL container through
   `localhost:5432`. Review `.env` and adjust its values if your local setup is
   different. Without this file, the server may fail with a missing or invalid
   database configuration.

5. **Run the Server (in a new terminal)**

   ```bash
   cd apps/server
   cargo run --no-default-features --features postgres --bin rustrak
   ```

   The server defaults to the `sqlite` feature. Because this development setup
   starts PostgreSQL, disable the default feature and explicitly enable
   `postgres`.

6. **Run the dashboard (in another terminal)**

   ```bash
   cd apps/dashboard
   pnpm dev
   ```

   Vite serves it on `:3000` and proxies `/api`, `/auth`, `/health`, `/docs`
   and `/api-docs` through to the server, so the browser only ever talks to one
   origin — the same arrangement production has, where the server itself hands
   out the compiled bundle.

### Running Tests

```bash
# Run all tests
pnpm test

# Run tests for a specific package
cd apps/server && cargo test
cd apps/dashboard && pnpm test
```

### Linting and Formatting

```bash
# Format all code
pnpm format

# Run linter
pnpm lint

# Everything CI runs: turbo over the JavaScript packages, then cargo for the crates
pnpm run ci
```

## Development Workflow

### Branch Naming Conventions

Use the following prefixes for your branches:

- `feat/` - New features
- `fix/` - Bug fixes
- `docs/` - Documentation changes
- `refactor/` - Code refactoring
- `test/` - Adding or improving tests
- `chore/` - Maintenance tasks

Example: `feat/add-authentication-endpoint` or `fix/login-validation-bug`

### Commit Message Format

Follow the conventional commit format:

```
<type>: <short description>

[optional body]

[optional footer(s)]
```

**Types:**
- `feat`: New features
- `fix`: Bug fixes
- `docs`: Documentation changes
- `refactor`: Code refactoring
- `test`: Adding or improving tests
- `chore`: Maintenance tasks

**Examples:**
```
feat: add user authentication endpoint

fix: resolve memory leak in event processor

docs: update API documentation for events endpoint
```

## Project Structure

Rustrak is a monorepo managed with Turborepo and pnpm:

```
rustrak/
├── apps/
│   ├── server/           # Rust backend (Actix-web)
│   ├── dashboard/        # React SPA, served by the server
│   └── docs/             # Documentation site
├── packages/
│   ├── client/           # TypeScript API client
│   ├── mcp/              # MCP server over the client
│   ├── benchmarks/       # Load and throughput benchmarks
│   └── test-sentry/      # Test utilities for Sentry compatibility
├── .changeset/           # Versioning configuration
├── docker-compose*.yml   # Docker configurations
└── CLAUDE.md            # Project context
```

### Component-Specific Context Files

Each component has its own CLAUDE.md describing its architecture and the rules
that apply inside it. Read the one for the area you are touching:

- Server: `apps/server/CLAUDE.md`
- Dashboard: `apps/dashboard/CLAUDE.md`
- Client Package: `packages/client/CLAUDE.md`
- MCP Server: `packages/mcp/CLAUDE.md`

## Code Standards

### Rust (Server)

- Follow Rust idioms and best practices
- Use `rustfmt` for code formatting
- Run `clippy` to catch common mistakes and improve code quality
- Write tests for new functionality
- Document public APIs with `///` comments

### TypeScript/JavaScript (UI and Packages)

- Use strict TypeScript mode
- Follow ESLint and Prettier configurations
- Write tests for new functionality
- Use Zod for runtime validation where appropriate

### Testing Requirements

- Maintain high test coverage (>90% where possible)
- Write unit tests for pure functions
- Write integration tests for API endpoints
- Include tests for edge cases and error conditions

## Submitting Changes

### Creating Issues

Before submitting a pull request, consider creating an issue first:

1. Search existing issues to avoid duplicates
2. Provide a clear title and detailed description
3. Include reproduction steps for bug reports
4. Explain the motivation for feature requests

### Pull Request Process

1. **Create a branch** for your changes
2. **Make your changes** following the code standards
3. **Write clear commit messages** using the conventional format
4. **Add tests** if your changes affect functionality
5. **Update documentation** as needed
6. **Submit a pull request** with a clear description

### Code Review Process

- All submissions require review before merging
- Address review comments promptly
- Maintainers will merge approved PRs
- Large changes may be broken into smaller PRs

## Community Guidelines

### Code of Conduct

This project follows the [Contributor Covenant Code of Conduct](https://www.contributor-covenant.org/version/2/1/code_of_conduct/). Please be respectful and inclusive when interacting with others.

### Getting Help

- Check the existing documentation and issues
- Ask questions in the issue comments
- Be patient - maintainers are volunteers

### Recognition

Contributors will be acknowledged in release notes and the project's README.

---

Thank you for contributing to Rustrak! Your efforts help make error tracking more accessible and lightweight for everyone.
