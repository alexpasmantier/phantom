# Security Policy

## Reporting a Vulnerability

If you discover a security vulnerability in phantom, please report it responsibly.

**Do not open a public GitHub issue for security vulnerabilities.**

Instead, email [alex.pasmant@gmail.com](mailto:alex.pasmant@gmail.com) with:

- A description of the vulnerability
- Steps to reproduce
- Potential impact

You should receive a response within 72 hours.

## Scope

Phantom creates Unix domain sockets for daemon communication. The socket is created with default permissions in `$XDG_RUNTIME_DIR` or `$HOME/.phantom/`. On shared systems, ensure these directories have appropriate permissions.
