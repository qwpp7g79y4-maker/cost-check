# Security policy

## Supported versions

| Version | Supported |
| --- | --- |
| 0.1.x | Yes |

Security fixes go into the latest version only.

## Reporting a vulnerability

Please report security problems privately, not in a public Issue. Send an e-mail to pep_ai@icloud.com. Examples are a value from an export that ends up unescaped in the report, a way to make `serve` accept a request from another origin or listen beyond loopback, an uploaded file that is not deleted, or a way to make cost-check read, write or hang on something it should refuse.

Please include:

- the version (`cost-check --version`) and your operating system;
- the exact command;
- a minimal input that reproduces the problem, with made-up values instead of real data;
- what cost-check printed and what you expected.

We aim to respond within 7 days (best effort).
