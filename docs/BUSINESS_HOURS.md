# Contributor Business Hours

**Audience:** contributors opening issues or pull requests in this repository.

## Canonical Schedule

Credence Contracts contributor support and review hours are:

- **Days:** Monday through Friday
- **Hours:** 09:00 inclusive to 17:00 exclusive
- **Timezone:** UTC

UTC is the canonical timezone for this policy and does not change for daylight
saving time. Unless maintainers post an exception in the repository, weekends
fall outside the business-hours window.

GitHub issues, pull requests, and CI jobs may be submitted or run at any time.
The schedule above describes when contributors should normally expect human
triage, review, and answers to routine project questions. It is not a response
time guarantee.

## Concrete Examples

| Activity time | Inside business hours? | Normal expectation |
| --- | --- | --- |
| Tuesday at 14:30 UTC | Yes | The request is available for triage during Tuesday's window. |
| Friday at 16:30 UTC | Yes | The request arrives inside the window, but review may continue on Monday. |
| Friday at 17:00 UTC | No | The next business-hours window begins Monday at 09:00 UTC. |
| Saturday at 11:00 UTC | No | The request remains queued until Monday's window. |

For example, a contributor who finishes `cargo test --workspace` at 18:15 UTC
on Wednesday can still push a pull request immediately. Routine maintainer
review should be expected from Thursday at 09:00 UTC, not during Wednesday
evening.

## What This Policy Covers

Use these hours when planning:

- issue clarification and assignment follow-up;
- pull request review and reviewer questions;
- help with repository setup, tests, linting, and CI failures; and
- routine support questions about documented contract behaviour.

Automated contract behaviour is not tied to this schedule. Soroban entrypoints,
ledger timestamps, proposal expiry, timelocks, and CI continue to operate
independently of maintainer business hours.

## Security Reports

Do not wait for business hours or open a public issue for a suspected
vulnerability. Follow the private reporting process in
[SECURITY.md](../SECURITY.md). The security process takes precedence over the
routine contributor schedule in this document.
