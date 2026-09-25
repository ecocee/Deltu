# Security Policy

## Supported versions

| Version | Supported |
| --- | --- |
| 0.1.x | Security fixes are applied to the latest release |

## Reporting a vulnerability

**Do not open a public issue for security problems.**

Report privately to ECOCEE through the project's maintained security
contact channel, including:

- A description of the issue and its impact.
- Steps to reproduce (config, payload, version — `deltu --version`).
- Any suggested mitigation.

You will receive an acknowledgement, and a fix will be developed and
released before public disclosure where feasible. Please allow a
reasonable window for coordinated disclosure.

## Scope notes

Deltu's security posture follows its architecture:

- The engine binds loopback by default; binding to a public interface
  is an explicit operator decision.
- **The HTTP API has no authentication built in** (a static bearer key
  is accepted by the SDKs for fronting proxies, but the engine itself
  does not validate it yet). If you expose Deltu beyond localhost, put
  it behind an authenticating reverse proxy or a firewall. Hardened
  access control is tracked as future work — see KNOWN-ISSUES.md.
- MQTT credentials are supported by the adapter configuration; brokers
  should still be treated as part of your trust boundary.
- Persistence snapshots are plain files — protect the snapshot path
  like any other state file (it contains your event values).

## Dependency policy

Dependencies are kept minimal by design (see `Cargo.toml`). Updates
that address published advisories in dependencies are treated as
security fixes.
