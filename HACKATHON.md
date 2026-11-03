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

Everything after the start SHA: the `arras-escrow-core`, `arras-escrow-v1` and `arras-escrow-v2` crates, their unit
tests, the near-workspaces tests, the reproducible builds, `scripts/testnet.sh` and `scripts/testnet-v2.sh`. `git log --format='%h %aI %s' 3a2ebdf..HEAD`
lists each commit with its timestamp.

## Deployed during the hackathon (testnet)

| Contract | Commit | Wasm sha256 | Role |
|---|---|---|---|
| `arras-v1b-1101.testnet` | `8da7174` (in `main`) | `7058319dff5f8d1e9da91a344ab3c699b709783e4153f3a7da6c2556cde0eada` | **demo contract**, keyless, deployed 2026-11-01 after the money review |
| `v1.arras-dev.testnet` | `0fb7929` | `811e454f6975c97d91cbd5fdb4cf3385acf000ff01f2e46a6a1840b4de5897a0` | first deployment, keyless, before the self-review fixes |
| `arras-v2-1103.testnet` | `c416989` | `8ef765f9f6c3b967cd55501f0eb3c8856b24085acabb479745624b9682f5d66a` | **v2** (agent budgets, disputes, operator tickets), keyless, deployed 2026-11-03 |

All three wasm files are reproducible builds (`sourcescan/cargo-near:0.21.1-rust-1.96.0`); each carries its commit in the
NEP-330 metadata (`contract_source_metadata`). Transactions and checks: README, "Testnet deployments".

On 2026-11-01 both payment paths ran on the demo contract with Circle's testnet USDC, 1 USDC each:
authorize -> release -> withdraw (0.87 USDC to the worker, 0.13 to the fee recipient) and authorize -> refund ->
withdraw (the 1 USDC back to the payer). The contract ended with 0 USDC and `tvl_global` 0. Transactions, events and
balances: README, "Payments with Circle's testnet USDC".

On 2026-11-03 the v2 walkthrough ran on `arras-v2-1103.testnet` with Circle's testnet USDC: an agent with only a
function-call key paid from a human's budget, one dispute was split by the arbiter, another expired in favour of the
worker, and a payment without the operator's ticket was refused. The contract ended with 0 USDC and `tvl_global` 0.
Transactions, events and balances: README, "Arras v2 on testnet".

## License

MIT. See [LICENSE](LICENSE).
