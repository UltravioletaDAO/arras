# Arras at NEAR HACKATHON

| Fact | Value |
|---|---|
| Event | NEAR HACKATHON, 2026-11-01T00:00:00Z to 2026-11-14T23:59:00Z |
| Window for code ([Q-001](https://nearhackathon.com/HACKATHON.md)) | from the announcement on 2026-10-19 to 2026-11-14 |
| Repository created | 2026-11-01T00:03:11Z (GitHub's initial commit, README only) |
| **Start SHA** | `3a2ebdfc832afd179298fb4f861f23409356fb01` |
| Built during the hackathon | every commit in `git log 3a2ebdf..HEAD`: [3a2ebdf...main](https://github.com/UltravioletaDAO/arras/compare/3a2ebdfc832afd179298fb4f861f23409356fb01...main) |

All the code in this repository was written from 2026-11-01 on, inside the Q-001 window, in the commits after the start
SHA `3a2ebdf`: [3a2ebdf...main](https://github.com/UltravioletaDAO/arras/compare/3a2ebdfc832afd179298fb4f861f23409356fb01...main).

## What already existed before 2026-11-01 (not part of this repo)

Of the rows below, these date from 2026-10-19 or later, so they fall inside the Q-001 window too: x402-rs#129 and
#130, and the two SDK releases.

| Piece | State before 2026-11-01 | Date |
|---|---|---|
| x402-rs facilitator (Ultravioleta DAO) | NEAR `exact` payments through NEP-366 meta-transactions, live in production | live before 2026-10-25; hardened by x402-rs#129 (merged 2026-10-25) and #130 (merged 2026-10-27) |
| Execution Market | marketplace where humans, agents and robots are hired; escrow on EVM networks only, no NEAR | measured 2026-10-25 |
| uvd-x402-sdk-python 0.100.0, uvd-x402-sdk-typescript 2.105.0 | NEAR `exact` only, no escrow | published 2026-10-26 ([PyPI](https://pypi.org/project/uvd-x402-sdk/0.100.0/) 13:24Z, [npm](https://www.npmjs.com/package/uvd-x402-sdk/v/2.105.0) 13:20Z) |

## What was built from 2026-11-01 on (this repo)

Everything after the start SHA: the `arras-escrow-core`, `arras-escrow-v1` and `arras-escrow-v2` crates, their unit
tests, the near-workspaces tests, the reproducible builds, `scripts/testnet.sh` and `scripts/testnet-v2.sh`; the
mainnet deployment script `scripts/mainnet.sh` and its offline tests (2026-11-02/03), and v2's, `scripts/mainnet-v2.sh`
with its smoke test (2026-11-07); the agent registry
(`arras-registry-v1`, 2026-11-03) and the ERC-8004 port (`erc8004-*` crates, from 2026-11-04); and every mainnet
deployment (below). `git log --format='%h %aI %s' 3a2ebdf..HEAD` lists each commit with its timestamp.

## Deployed during the hackathon (testnet)

| Contract | Commit | Wasm sha256 | Role |
|---|---|---|---|
| `arras-v1b-1101.testnet` | `8da7174` (in `main`) | `7058319dff5f8d1e9da91a344ab3c699b709783e4153f3a7da6c2556cde0eada` | **demo contract**, keyless, deployed 2026-11-01 after the money review |
| `v1.arras-dev.testnet` | `0fb7929` | `811e454f6975c97d91cbd5fdb4cf3385acf000ff01f2e46a6a1840b4de5897a0` | first deployment, keyless, before the self-review fixes |
| `arras-v2-1103.testnet` | `c416989` | `8ef765f9f6c3b967cd55501f0eb3c8856b24085acabb479745624b9682f5d66a` | **v2** (agent budgets, disputes, operator tickets), keyless, deployed 2026-11-03 |
| `v2.arras-v2e-1107.testnet` | `ddc5121` (the merge of the reviewed v2) | `1541525c4bf3494ae5025b00df6955698975a6f8c376002f0296ff93b4e4fa77` | first rehearsal of the v2 mainnet script and its smoke test, with the script as it was before its own money review; keyless, deployed 2026-11-07 (docs/MAINNET.md, section 8.6) |
| `v2.arras-v2f-1107.testnet` | `ddc5121` | `1541525c4bf3494ae5025b00df6955698975a6f8c376002f0296ff93b4e4fa77` | second rehearsal, after the two rounds of the script's money review: it found the up-front reserve behind the script's last fix (docs/MAINNET.md 8.3) and finished with the final script; keyless, deployed 2026-11-07 (docs/MAINNET.md, section 8.6) |

Every wasm above is a reproducible build (`sourcescan/cargo-near:0.21.1-rust-1.96.0`; the two rehearsals share the
pinned wasm of `ddc5121`); each carries its commit in the NEP-330 metadata (`contract_source_metadata`). Transactions
and checks: README, "Testnet deployments" (v1) and "Arras v2 on testnet" (v2 and the two rehearsals).

On 2026-11-01 both payment paths ran on the demo contract with Circle's testnet USDC, 1 USDC each:
authorize -> release -> withdraw (0.87 USDC to the worker, 0.13 to the fee recipient) and authorize -> refund ->
withdraw (the 1 USDC back to the payer). The contract ended with 0 USDC and `tvl_global` 0. Transactions, events and
balances: README, "Payments with Circle's testnet USDC".

On 2026-11-03 the v2 walkthrough ran on `arras-v2-1103.testnet` with Circle's testnet USDC: an agent with only a
function-call key paid from a human's budget, one dispute was split by the arbiter, another expired in favour of the
worker, and a payment without the operator's ticket was refused. The contract ended with 0 USDC and `tvl_global` 0.
Transactions, events and balances: README, "Arras v2 on testnet".

On 2026-11-07 the v2 mainnet script ran end to end on testnet over the same layout as mainnet, twice: first on
`v2.arras-v2e-1107.testnet`, then on `v2.arras-v2f-1107.testnet`, which found the up-front reserve behind the script's
last fix and finished with the final script. Each time with its smoke test: an agent with only a function-call key paid
from a human's budget with the operator's tickets, the capturer released it, and a disputed payment went back to the
human through the arbiter. The token was a test NEP-141 (Circle's testnet faucet sits behind a reCAPTCHA).
Transactions: docs/MAINNET.md, section 8.6.

## Deployed during the hackathon (mainnet)

Each one is the reproducible build of a commit in this repository, deployed with a private `new` in the same
transaction and left without access keys. All transactions, with explorer links: [docs/EVIDENCE.md](docs/EVIDENCE.md).

| Contract | Commit | Wasm sha256 | Deployed (UTC) |
|---|---|---|---|
| `v1.arras-escrow.near` (Arras v1 escrow) | `8da7174` | `7058319dff5f8d1e9da91a344ab3c699b709783e4153f3a7da6c2556cde0eada` | 2026-11-03 02:31:45Z; keys deleted 02:32:19Z |
| `registry.arras-escrow.near` (`registry-v1`) | `8cec4e6` | `b7b4593025769e5a12e70ebad7a78faecf523eb834b2f01338727e2833933197` | 2026-11-04 03:47:45Z; key deleted 03:49:43Z |
| `identity-registry.erc-8004.near` (ERC-8004 Identity) | `f18e25c` | `fe284122f04f319117b47234c1daafa11830862312ad138f3463683fb498d3d5` | 2026-11-04 21:47:55Z; keys deleted 21:49:46Z |
| `reputation-registry.erc-8004.near` (ERC-8004 Reputation) | `f18e25c` | `dfe715d087517617811c1155bba9c8b6e71b23cbe63481b5afbf484bcbabd56e` | 2026-11-04 21:48:05Z; keys deleted 21:49:53Z |
| `erc8004-verifier.arras-escrow.near` (Arras proof verifier) | `f18e25c` | `f1b5e29942ca32aea8416a51f3276bd728f5ab804c8853414c5f5128f276e7ba` | 2026-11-04 21:48:16Z; keys deleted 21:50:01Z |
| `v2.arras-escrow.near` (Arras v2 escrow) | `ddc5121` | `1541525c4bf3494ae5025b00df6955698975a6f8c376002f0296ff93b4e4fa77` | 2026-11-08 02:20:36Z; keys deleted 02:21:28Z |

Arras v2 is on mainnet since 2026-11-08: `scripts/mainnet-v2.sh` deployed it
([`FdPJxM9k...`](https://nearblocks.io/txns/FdPJxM9kKsPiXeYBkqZnqTqqHKSkUBM2pM1hUg4uQQ1m)), deleted all its keys
([`Bqc63GgB...`](https://nearblocks.io/txns/Bqc63GgBiTWfrs9nAVHzE9piPLtPsxj6EcPSU7fDmVqt)) and then ran its smoke test,
the security track, with Circle's USDC: the human (the project's funding account) funded a 0.10 USDC budget for an
agent with the operator's ticket; the agent, holding only a function-call key for `authorize_from_budget`, paid 0.05 to
a worker and the capturer released it (0.0435 to the worker, 0.0065 to the fee recipient); a second 0.05 payment,
disputed by the human, went back whole through the arbiter. The agent and the worker are test accounts of the smoke
test, and `get_config` counted 2 payments at its end, both the smoke's. Execution Market and the facilitator still use
v1 only. Transactions: EVIDENCE.md, section 6; the run: docs/MAINNET.md, section 8.8.

Used by the production deployments of Execution Market and of the x402-rs facilitator during the hackathon (their code
lives in their own repositories):

| Date (UTC) | What | Evidence |
|---|---|---|
| 2026-11-03 15:49Z | Execution Market's first NEAR payment through Arras (an end-to-end test): authorize, release by its capturer's function-call key, withdrawals | EVIDENCE.md, section 2 |
| 2026-11-04 08:04Z | Execution Market's golden flow with both ratings in `registry.arras-escrow.near` | EVIDENCE.md, section 3 |
| 2026-11-06 01:17Z to 01:19Z | Execution Market's golden flow on the ERC-8004 registries: the payer's identity, the payment through Arras and a rating in each direction with the payment as proof; from then on ERC-8004 is Execution Market's NEAR registry | EVIDENCE.md, section 5 |
| 2026-11-06 01:42Z to 01:47Z | a new worker's identity registered and transferred by Execution Market's sponsor account, `em-identity.near` | EVIDENCE.md, section 5 |

So far all 13 mainnet payments through v1 (0.10 USDC each) are the team's own end-to-end tests: one test agent paying
the team's test workers. The 2 payments through v2 (0.05 USDC each) are the team's smoke test above.

## License

MIT. See [LICENSE](LICENSE).
