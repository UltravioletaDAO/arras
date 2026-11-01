# Arras at NEAR HACKATHON

| Fact | Value |
|---|---|
| Event | NEAR HACKATHON, 2026-11-01T00:00:00Z to 2026-11-14T23:59:00Z |
| Repository created | 2026-11-01T00:03:11Z (GitHub's initial commit, README only) |
| **Start SHA** | `3a2ebdfc832afd179298fb4f861f23409356fb01` |
| Built during the hackathon | every commit in `git log 3a2ebdf..HEAD` |

## Disclosure

"diseno y vectores escritos antes del hackaton en un repo privado (2026-10-26); codigo escrito desde el 2026-11-01
(SHA de inicio `3a2ebdfc832afd179298fb4f861f23409356fb01`)".

## What already existed before 2026-11-01 (not part of this repo)

| Piece | State before the hackathon | Date |
|---|---|---|
| x402-rs facilitator (Ultravioleta DAO) | NEAR `exact` payments through NEP-366 meta-transactions, live in production | live before 2026-10-25; hardened by x402-rs#129 (merged 2026-10-25) and #130 (merged 2026-10-27) |
| Execution Market | marketplace where humans, agents and robots are hired; escrow on EVM networks only, no NEAR | measured 2026-10-25 |
| uvd-x402-sdk-python 0.100.0, uvd-x402-sdk-typescript 2.105.0 | NEAR `exact` only, no escrow | measured 2026-10-25 |
| Arras design | written specification and golden vectors V-1..V-3 in a private planning repo, no contract code | 2026-10-25 / 2026-10-26 |

## What was built from 2026-11-01 on (this repo)

Everything after the start SHA: the `arras-escrow-core` and `arras-escrow-v1` crates, their unit tests, the
near-workspaces tests, the reproducible build and `scripts/testnet.sh`. `git log --format='%h %aI %s' 3a2ebdf..HEAD`
lists each commit with its timestamp.
