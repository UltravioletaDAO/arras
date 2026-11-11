# Evidence: Arras on NEAR mainnet

Every mainnet transaction behind the claims in the README, with a link to the NearBlocks explorer. Times are the block
times (UTC) that NearBlocks reports. Hashes and times were read from the NearBlocks public API on 2026-11-06 and from
the logs of the runs; those of section 6 (2026-11-08) from the run's log and, for each transaction, its final status,
signer, block and block time, read through an RPC node by the deployer; those of section 7 (2026-11-11) from the
NearBlocks public API and the transaction status of FastNear's free RPC endpoint, read on 2026-11-11 (that endpoint is
not archival: a few days later, check those transactions on NearBlocks or through an archival RPC). Nothing here needs
a key to check. Testnet
transactions are in the README ("Testnet deployments", "Arras v2 on testnet") and in [docs/MAINNET.md](MAINNET.md) (the
rehearsals).

Accounts that appear below:

| Account | Role |
|---|---|
| [`v1.arras-escrow.near`](https://nearblocks.io/address/v1.arras-escrow.near) | Arras v1 escrow contract (keyless) |
| [`identity-registry.erc-8004.near`](https://nearblocks.io/address/identity-registry.erc-8004.near) | ERC-8004 Identity Registry (keyless) |
| [`reputation-registry.erc-8004.near`](https://nearblocks.io/address/reputation-registry.erc-8004.near) | ERC-8004 Reputation Registry (keyless) |
| [`erc8004-verifier.arras-escrow.near`](https://nearblocks.io/address/erc8004-verifier.arras-escrow.near) | the Arras proof verifier for the Reputation Registry (keyless) |
| [`registry.arras-escrow.near`](https://nearblocks.io/address/registry.arras-escrow.near) | `registry-v1`, the first agent registry (keyless) |
| [`em-capturer.arras-escrow.near`](https://nearblocks.io/address/em-capturer.arras-escrow.near) | Execution Market's capturer: one function-call key (`release`, `refund`, `withdraw_for` on v1) |
| [`em-fees.arras-escrow.near`](https://nearblocks.io/address/em-fees.arras-escrow.near) | Execution Market's fee recipient |
| [`em-identity.near`](https://nearblocks.io/address/em-identity.near) | Execution Market's identity sponsor: pays the ERC-8004 registration of new workers |
| [`uvd-facilitator.near`](https://nearblocks.io/address/uvd-facilitator.near) | the x402-rs facilitator's relayer: sends signed NEP-366 delegate actions (the payer's `authorize`, the capturer's `release` and `withdraw_for`) and pays their gas |
| [`fe0a5742...1079`](https://nearblocks.io/address/fe0a574258f8277099875e46d4ca0043cdb6274ff2501b375a0d3d64bc211079) | the deployer's funding account; also the paying agent in the test flows, and the human of v2's smoke test (section 6) |
| [`826e8c64...7af`](https://nearblocks.io/address/826e8c640ba0ee77bf255ecb332dfaca73fc1e3aef2dbf370b5ffeb30f9ac7af) | a test worker (ERC-8004 identity 0) |
| [`be5ec510...2902`](https://nearblocks.io/address/be5ec510aa303bd041a2411cb691a8e32907763116ab8cb6a4ac586695aa2902) | a second test worker, registered through the sponsor (ERC-8004 identity 2) |
| [`v2.arras-escrow.near`](https://nearblocks.io/address/v2.arras-escrow.near) | Arras v2 escrow contract (keyless) |
| [`em-capturer-v2.arras-escrow.near`](https://nearblocks.io/address/em-capturer-v2.arras-escrow.near) | the capturer of operator `em` in v2 (Execution Market does not use v2 yet): one function-call key (`release`, `refund`, `withdraw_for` on v2) |
| [`em-arbiter-v2.arras-escrow.near`](https://nearblocks.io/address/em-arbiter-v2.arras-escrow.near) | the arbiter of operator `em` in v2: one function-call key (`resolve_dispute` on v2) |
| [`agent-v2.arras-escrow.near`](https://nearblocks.io/address/agent-v2.arras-escrow.near) | a test account, the smoke test's agent: one function-call key (`authorize_from_budget` on v2) |
| [`worker-v2.arras-escrow.near`](https://nearblocks.io/address/worker-v2.arras-escrow.near) | a test account, the smoke test's worker |
| `kk-<name>.karmakadabra.near` (for example [`kk-validator.karmakadabra.near`](https://nearblocks.io/address/kk-validator.karmakadabra.near)) | agents of KarmaKadabra, Ultravioleta DAO's own agent swarm; each holds an ERC-8004 identity (agents 3 to 9) |

The token is Circle's native USDC on NEAR, `17208628f84f5d6ad33f0da3bbbeb27ffcb398eac501a31bd6ad2011e36133a1`.

## 1. Arras v1 deployed (2026-11-03, block times 02:30:39Z to 02:32:19Z)

`scripts/mainnet.sh --mainnet-ok todo`, the sequence of [docs/MAINNET.md](MAINNET.md) section 4, run once from a clean
checkout of `ea5869f` (`main` at the time; details in MAINNET.md, section 7). The wasm is the reproducible build of commit `8da7174` (sha256
`7058319dff5f8d1e9da91a344ab3c699b709783e4153f3a7da6c2556cde0eada`), the same code as the testnet demo contract.

| Step | Transaction |
|---|---|
| parent `arras-escrow.near`, created by the funding account | [`6X1ShTfEsUdmEFoneeLPqCSFCDmjytgJvSCis5yTQoNz`](https://nearblocks.io/txns/6X1ShTfEsUdmEFoneeLPqCSFCDmjytgJvSCis5yTQoNz) |
| `v1.` (3.0 NEAR) / `v2.` (0.1, reserved, no code) | [`MRGoLQRzPokkPm9HRaCnTPA2sP4BXySAs7he1onY2zC`](https://nearblocks.io/txns/MRGoLQRzPokkPm9HRaCnTPA2sP4BXySAs7he1onY2zC) / [`HcYCRfBPKWLihPQfZQJ6HzwGo3dtDN1CL3mkDzv8HNkE`](https://nearblocks.io/txns/HcYCRfBPKWLihPQfZQJ6HzwGo3dtDN1CL3mkDzv8HNkE) |
| `owner.` / `guardian.` | [`43eEWHe6kGxRFMgmj7GXytoGywv3WQWSCg24cJjf1acS`](https://nearblocks.io/txns/43eEWHe6kGxRFMgmj7GXytoGywv3WQWSCg24cJjf1acS) / [`GqLGSr7JpWp9eHvutv3JgaAc6XaTzuBWSbjKxA2Txe35`](https://nearblocks.io/txns/GqLGSr7JpWp9eHvutv3JgaAc6XaTzuBWSbjKxA2Txe35) |
| `em-capturer.` / `em-fees.` | [`12enRGRe5ZPsRwh5X5UoP5Efm3hUUQe89WhxTRGkjhxf`](https://nearblocks.io/txns/12enRGRe5ZPsRwh5X5UoP5Efm3hUUQe89WhxTRGkjhxf) / [`E17CkZjChvRGrZkx4dEDGs7NkEimLuVcaow2L6FMkDjx`](https://nearblocks.io/txns/E17CkZjChvRGrZkx4dEDGs7NkEimLuVcaow2L6FMkDjx) |
| USDC storage for v1 / for `em-fees.` | [`Cs9umRAXZNQ3gVynaiaVRSsqyCh8NCsPRiEPJWGjBMkF`](https://nearblocks.io/txns/Cs9umRAXZNQ3gVynaiaVRSsqyCh8NCsPRiEPJWGjBMkF) / [`8naX3h87D9kYCWkCnfWVFp4zRQXi7dUuNtBxCP7pvMDA`](https://nearblocks.io/txns/8naX3h87D9kYCWkCnfWVFp4zRQXi7dUuNtBxCP7pvMDA) |
| **deploy + private `new` in one transaction** (10 USDC per payment, 1,000 USDC global TVL), 02:31:45Z | [`248SUtwuSygSvfYjubUokvDWpAjgbKDQA6zPLXBDKg68`](https://nearblocks.io/txns/248SUtwuSygSvfYjubUokvDWpAjgbKDQA6zPLXBDKg68) |
| `register_operator em` (fee 1300 bps, 1,000 USDC per operator, 20 per payer) / sponsorship pool 0.5 NEAR | [`7iriz6GNsyettoms72Z8SPj8DRocP4ctcKM1S4WxuM3F`](https://nearblocks.io/txns/7iriz6GNsyettoms72Z8SPj8DRocP4ctcKM1S4WxuM3F) / [`9YAfdppVg2HYnyGemW6JoFxQ7RGFRdhbcwQzR6fAJder`](https://nearblocks.io/txns/9YAfdppVg2HYnyGemW6JoFxQ7RGFRdhbcwQzR6fAJder) |
| capturer: function-call key (`release, refund, withdraw_for`, 0.5 NEAR allowance) / its full-access key deleted | [`CazQDx1cyYJNvHmL8ZTcJva4hryAXsBVwWFgDUJW5Nq3`](https://nearblocks.io/txns/CazQDx1cyYJNvHmL8ZTcJva4hryAXsBVwWFgDUJW5Nq3) / [`DF9cxXNyJjoqSbr5VwFbkdeHg7tR1zf3om1jqzzfVi3w`](https://nearblocks.io/txns/DF9cxXNyJjoqSbr5VwFbkdeHg7tR1zf3om1jqzzfVi3w) |
| **all keys of `v1.arras-escrow.near` deleted**, 02:32:19Z | [`5gUfctsBnCUoCPoPMKqfbz4ZKVXKtYAnpUyUf2fjYzLj`](https://nearblocks.io/txns/5gUfctsBnCUoCPoPMKqfbz4ZKVXKtYAnpUyUf2fjYzLj) |

## 2. The first payments by Execution Market (2026-11-03)

The first payment through the live escrow, one of Execution Market's end-to-end tests: the payer's `ft_transfer_call`
of USDC (`authorize`), then the capturer's `release` and the two `withdraw_for` (worker and fee recipient). The
`authorize` and the `release` are NEP-366 delegate actions signed by their account (the payer; the capturer with its
function-call key) and relayed by `uvd-facilitator.near`, which pays the gas.

| Step | Transaction |
|---|---|
| authorize, 15:49:09Z | [`AkgvBMnoBxngtmytKhR1rNRXhZ8QUT6yHcvNG9fVQ9hr`](https://nearblocks.io/txns/AkgvBMnoBxngtmytKhR1rNRXhZ8QUT6yHcvNG9fVQ9hr) |
| release, 15:49:20Z | [`8xjsVqzwKCKEwqr8WBcBYcc2iLfKS2YrLPfjzWBaGhHa`](https://nearblocks.io/txns/8xjsVqzwKCKEwqr8WBcBYcc2iLfKS2YrLPfjzWBaGhHa) |
| withdraw_for, 15:49:23Z / 15:49:28Z | [`Dqnmco1vuTdnHyhYoDWpuRapT32N8Mjnw62wgJEBmbiD`](https://nearblocks.io/txns/Dqnmco1vuTdnHyhYoDWpuRapT32N8Mjnw62wgJEBmbiD) / [`7ToYeYCFBZAGCCAWdrxFu6QwCtXzbyp5VPgZCZ2UEhEX`](https://nearblocks.io/txns/7ToYeYCFBZAGCCAWdrxFu6QwCtXzbyp5VPgZCZ2UEhEX) |
| a refunded payment: authorize, 17:44:25Z / refund / withdraw_for to the payer | [`FPVn7e29VX2UgF7mABTBToU8bW5vgdpHMHzFTFDfd3Cq`](https://nearblocks.io/txns/FPVn7e29VX2UgF7mABTBToU8bW5vgdpHMHzFTFDfd3Cq) / [`BkF4ppQNv8dw5UVcDA6qw4k1J9iNpp17mi2eNRighwDy`](https://nearblocks.io/txns/BkF4ppQNv8dw5UVcDA6qw4k1J9iNpp17mi2eNRighwDy) / [`BNEoLZcQNFEJxDGWysRB1p1zzKxhFBMn5tSvRJbmJ5io`](https://nearblocks.io/txns/BNEoLZcQNFEJxDGWysRB1p1zzKxhFBMn5tSvRJbmJ5io) |

The full list of calls to the contract is on its explorer page: 66 transactions (116 receipts) as of 2026-11-06, read
from the NearBlocks API. They hold 14 authorizations, all of 0.10 USDC and all from the same test agent (the deployer's
funding account) to the team's test workers: 13 became payments (`get_config` answers `"payments": 13`) and one was
refused with `E14` (no storage paid):
[`A5oCxCMAVsiqJh9qa8SuymH87Lg5L13wU2oi1UMQgChV`](https://nearblocks.io/txns/A5oCxCMAVsiqJh9qa8SuymH87Lg5L13wU2oi1UMQgChV)
(2026-11-06 01:26:40Z, to a new test worker; the only one of the 14 with no release or refund). A rejection keeps
nothing: `ft_on_transfer` hands the whole amount back to the token. As of 2026-11-06, every mainnet payment through v1
was one of the team's own end-to-end tests of Execution Market's production deployment; section 7 adds 7 payments
between KarmaKadabra agents (2026-11-10 and 11-11). The 2 payments through v2 are the team's smoke test of v2
(section 6).

## 3. `registry-v1` deployed (2026-11-04, 03:47:09Z to 03:49:43Z)

The first agent registry (identity = the NEAR account, a rating needs a released Arras payment): reproducible build of
the merge commit `8cec4e6` (sha256 `b7b4593025769e5a12e70ebad7a78faecf523eb834b2f01338727e2833933197`).

| Step | Transaction |
|---|---|
| funding of the parent / account `registry.arras-escrow.near` | [`Da6cca7aHAcARPLXtKuza9hXkZ5PgDAyXJyiEHWx8Scy`](https://nearblocks.io/txns/Da6cca7aHAcARPLXtKuza9hXkZ5PgDAyXJyiEHWx8Scy) / [`CysLmwkzFtd5x9dSbzvP7JtFbhJX7uHxY9YYzgbS3qch`](https://nearblocks.io/txns/CysLmwkzFtd5x9dSbzvP7JtFbhJX7uHxY9YYzgbS3qch) |
| deploy + private `new` (one escrow allowed: `v1.arras-escrow.near`), 03:47:45Z | [`FiVS4X8LRihH15wVQSXrh1hkCQJgNK9KUdWtsB4BDqH3`](https://nearblocks.io/txns/FiVS4X8LRihH15wVQSXrh1hkCQJgNK9KUdWtsB4BDqH3) |
| smoke: the payer rates the worker / the worker rates the payer, on a released Arras payment | [`HwLkFNFhrxQpLdcPi1cWcQvDVMRAMXLfdFf5SfMLk3Vy`](https://nearblocks.io/txns/HwLkFNFhrxQpLdcPi1cWcQvDVMRAMXLfdFf5SfMLk3Vy) / [`8x16B1VYewTd4z846Q2GA8Em6Sx4Zeisf9fiFidAkdPj`](https://nearblocks.io/txns/8x16B1VYewTd4z846Q2GA8Em6Sx4Zeisf9fiFidAkdPj) |
| **its only key deleted**, 03:49:43Z | [`83jj5fmkfunFQqYcbTZ471JsKdkCAozqLCrcvqvZBurL`](https://nearblocks.io/txns/83jj5fmkfunFQqYcbTZ471JsKdkCAozqLCrcvqvZBurL) |

Execution Market's golden flow on this registry (2026-11-04, 08:04Z): authorize
[`4n7Soh36iuovKvdm4aC8eVWNaXy3zcJj8FMVCiqN5JeM`](https://nearblocks.io/txns/4n7Soh36iuovKvdm4aC8eVWNaXy3zcJj8FMVCiqN5JeM),
release [`2Pjd96eEMF7t8AzvD2s8gVtJnWiQK561XqsvJKWgwcqq`](https://nearblocks.io/txns/2Pjd96eEMF7t8AzvD2s8gVtJnWiQK561XqsvJKWgwcqq),
withdrawals [`BauqpZcCvthui62eEByYieaerF6eQb9haL9jnWRqHgmj`](https://nearblocks.io/txns/BauqpZcCvthui62eEByYieaerF6eQb9haL9jnWRqHgmj)
and [`8bYThoM8CnBpvRe3Cc6skHvhPqtRQeymGS19i2phynwn`](https://nearblocks.io/txns/8bYThoM8CnBpvRe3Cc6skHvhPqtRQeymGS19i2phynwn),
and the two ratings [`8TmpeBzwnoh3mJpEf246AVpu1eUNKSmrzrLZajtGoGBY`](https://nearblocks.io/txns/8TmpeBzwnoh3mJpEf246AVpu1eUNKSmrzrLZajtGoGBY)
and [`DV8a9kd1aHHHnUbEji7fGR4wjdBr2nF71gMykGNFPuid`](https://nearblocks.io/txns/DV8a9kd1aHHHnUbEji7fGR4wjdBr2nF71gMykGNFPuid).
Those two ratings (the second one at 08:04:42Z) are Execution Market's last writes here. Its configuration pointed NEAR
reputation at this registry until 2026-11-06, when it moved to the ERC-8004 registries (section 5).

## 4. The ERC-8004 port deployed (2026-11-04, 21:46:58Z to 21:50:01Z)

Reproducible builds of the merge commit `f18e25c` (this repository's `main` when they were deployed), each built twice
from two clean clones with the same sha256. The parent `erc-8004.near` was created on 2026-11-04 at 13:52:00Z
([`84aGt5kuYiUmsfJSLnajZCq5SH475zrC4R13YsJ28Qki`](https://nearblocks.io/txns/84aGt5kuYiUmsfJSLnajZCq5SH475zrC4R13YsJ28Qki)),
and `validation-registry.erc-8004.near` is only reserved (no code:
[`51LwejfbR5fW5ov1edAYjgAtQY7BV6N2JTEHGtYqbcw`](https://nearblocks.io/txns/51LwejfbR5fW5ov1edAYjgAtQY7BV6N2JTEHGtYqbcw)).

| Step | Identity Registry | Reputation Registry | Arras verifier |
|---|---|---|---|
| funding of the parents (`erc-8004.near` / `arras-escrow.near`) | [`9X3Tuij5qCXgX2zMp2BmeqnAefcuvLer327f1CJ4HJAD`](https://nearblocks.io/txns/9X3Tuij5qCXgX2zMp2BmeqnAefcuvLer327f1CJ4HJAD) | (same) | [`6uApty357fT837C7Ag2qSbdebvFzim4KoQRySARzRAa1`](https://nearblocks.io/txns/6uApty357fT837C7Ag2qSbdebvFzim4KoQRySARzRAa1) |
| account | [`8MTrCEiJiDcv9p6hyufCz6QrF4asBKLaUifmwn3hTSPK`](https://nearblocks.io/txns/8MTrCEiJiDcv9p6hyufCz6QrF4asBKLaUifmwn3hTSPK) | [`E6oQBnByYEtUMXUKyQny9w28V2uECK75s6XLdMovZNTq`](https://nearblocks.io/txns/E6oQBnByYEtUMXUKyQny9w28V2uECK75s6XLdMovZNTq) | [`BCk1gYPDVydBFg5F5NdqRceeSNuTQ32sS8fpY6Pjc4AK`](https://nearblocks.io/txns/BCk1gYPDVydBFg5F5NdqRceeSNuTQ32sS8fpY6Pjc4AK) |
| **deploy + private `new`** | [`4Ctp3BhCLTDmGBhmg3avS2zcXMjA3Xp9RU1suxdVc3Zx`](https://nearblocks.io/txns/4Ctp3BhCLTDmGBhmg3avS2zcXMjA3Xp9RU1suxdVc3Zx) | [`8X6QcXj2zUZHJkXLgRXcvNnwuEdvaZJJyEu8GsctzEu5`](https://nearblocks.io/txns/8X6QcXj2zUZHJkXLgRXcvNnwuEdvaZJJyEu8GsctzEu5) | [`GDudWE6hBdt7vSqiePkPrEc4URgePWwnyzqti3Q1c7rD`](https://nearblocks.io/txns/GDudWE6hBdt7vSqiePkPrEc4URgePWwnyzqti3Q1c7rD) |
| **all keys deleted** | [`HKxPdXWZaqEtjUXt7ZehYdCg8qLAr6mCmkV7chRZnY25`](https://nearblocks.io/txns/HKxPdXWZaqEtjUXt7ZehYdCg8qLAr6mCmkV7chRZnY25) | [`BGTwCzXZ6feA1NfksRKfoX6KYovn6CkXryLwBYyiFHZz`](https://nearblocks.io/txns/BGTwCzXZ6feA1NfksRKfoX6KYovn6CkXryLwBYyiFHZz) | [`6cuu4AkQDZFXd27L2NWrYNLj4FJa9g6gNESSwKAmeBjZ`](https://nearblocks.io/txns/6cuu4AkQDZFXd27L2NWrYNLj4FJa9g6gNESSwKAmeBjZ) |

Smoke test before the keys were deleted, on a payment that Execution Market had already released through Arras v1:
the funding account registered agent 0
([`EFEP8ySmwnySWwtc3FeezvETBR1GadLt4ZQGyydu76uQ`](https://nearblocks.io/txns/EFEP8ySmwnySWwtc3FeezvETBR1GadLt4ZQGyydu76uQ)),
transferred the NFT to the worker that had received that payment
([`5LX8yTQUvMD7uYSmKg4TQQ2HFPAjJmtiSRFFoVU67s4`](https://nearblocks.io/txns/5LX8yTQUvMD7uYSmKg4TQQ2HFPAjJmtiSRFFoVU67s4)), and
rated it with the payment as proof
([`Cxcj2Bo1ZkhAeBiu9bFaKY2iFvsPrYy9bBzvYyVHWTfg`](https://nearblocks.io/txns/Cxcj2Bo1ZkhAeBiu9bFaKY2iFvsPrYy9bBzvYyVHWTfg):
`give_feedback_with_proof` calls Identity `check_agent` and the verifier's `verify_feedback_proof` in parallel; the
verifier reads the escrow (`get_payment` -> `on_payment`), and `on_feedback_proof_checked` joins both answers, all in
one transaction).

## 5. Golden flow of Execution Market on the ERC-8004 registries (2026-11-06, 01:17:44Z to 01:18:43Z)

Execution Market's end-to-end test against production (`MULTICHAIN GOLDEN FLOW: PASS`, Reputation PASS): an agent with
only a NEAR wallet publishes a 0.10 USDC task, a worker does it, the money moves through Arras, and both sides rate
each other in the ERC-8004 Reputation Registry with the Arras payment as proof. Task
`eb1d7716-dce6-4975-9200-93d92eb10887`, Arras payment `DAtTVvHHPTfUbneHj1dUYi1mG3aqE8GbEK5aQzVExZ2X`.

| Step | Signed by | Transaction |
|---|---|---|
| the paying agent registers its ERC-8004 identity (agent 1), 01:17:44Z | the agent | [`3mefws4t6PTNHijULaYTk1vQnberCTSzf93EBgCjxLWq`](https://nearblocks.io/txns/3mefws4t6PTNHijULaYTk1vQnberCTSzf93EBgCjxLWq) |
| authorize: 0.10 USDC locked in Arras when the worker is assigned, 01:17:58Z | the agent (`ft_transfer_call` inside a NEP-366 delegate action), relayed by `uvd-facilitator.near`, which pays the gas | [`Bcri1ZNj4Agrv9QqwFneTcVMfwH1tJsKnLpmMjBijzUW`](https://nearblocks.io/txns/Bcri1ZNj4Agrv9QqwFneTcVMfwH1tJsKnLpmMjBijzUW) |
| release on approval, 01:18:11Z | Execution Market's capturer (its function-call key, inside a NEP-366 delegate action relayed by `uvd-facilitator.near`) | [`2gkYMpHAQTUSe7NSuwUrtY3ZPzH4gLDbYUyiJBHJLhKd`](https://nearblocks.io/txns/2gkYMpHAQTUSe7NSuwUrtY3ZPzH4gLDbYUyiJBHJLhKd) |
| `withdraw_for` the worker (0.087 USDC), 01:18:14Z | the capturer, the same way | [`BKzNy2TJw7pyR2x7xKgNJ72iSp7AaA8K3jtA29PvBz6Z`](https://nearblocks.io/txns/BKzNy2TJw7pyR2x7xKgNJ72iSp7AaA8K3jtA29PvBz6Z) |
| `withdraw_for` `em-fees.arras-escrow.near` (the 13 % fee), 01:18:18Z | the capturer, the same way | [`6rtbAdkdnqy1FSvdfaRmFg6aq49dkFVCENcUcyamnJ4j`](https://nearblocks.io/txns/6rtbAdkdnqy1FSvdfaRmFg6aq49dkFVCENcUcyamnJ4j) |
| the agent rates the worker (agent 0), score 90, proof = the payment, 01:18:33Z | the agent | [`EjLpmfryfXJAkLhrgkntS9PjDhCMqtQyQm54a1ajn7xs`](https://nearblocks.io/txns/EjLpmfryfXJAkLhrgkntS9PjDhCMqtQyQm54a1ajn7xs) |
| the worker rates the agent (agent 1), score 85, proof = the payment, 01:18:43Z | the worker | [`JApp8fXkr2nhtm7qtLqFyVfEB4YpWiBj1WVVL7dasKHW`](https://nearblocks.io/txns/JApp8fXkr2nhtm7qtLqFyVfEB4YpWiBj1WVVL7dasKHW) |

Execution Market confirmed both ratings (feedback 2 of agent 0 by the agent's account, feedback 1 of agent 1 by the
worker's account) and shows them on both profiles. From that run on, ERC-8004 is Execution Market's NEAR registry
(until then it was `registry-v1`): identities are registered there, and a rating is written there when a party rates;
some test tasks after this run were completed without ratings.

### A new worker, registered by the sponsor (2026-11-06, 01:42:30Z to 01:46:58Z)

A worker with no NEAR to spare gets its ERC-8004 identity from Execution Market's sponsor account, which registers it
and transfers the NFT:

| Step | Transaction |
|---|---|
| `em-identity.near` registers agent 2, 01:42:30Z | [`2z1xNxKMzqAcPcAiXUPiRCp6tVpeRpuy5xqb3osB2iu6`](https://nearblocks.io/txns/2z1xNxKMzqAcPcAiXUPiRCp6tVpeRpuy5xqb3osB2iu6) |
| `nft_transfer` of agent 2 to the worker, 01:45:55Z | [`AMSsFsTEeDvNt9FfC1F1dA7M9YjZAMGksKp9GeeiyJss`](https://nearblocks.io/txns/AMSsFsTEeDvNt9FfC1F1dA7M9YjZAMGksKp9GeeiyJss) |
| the task's payment: authorize / release / two `withdraw_for` | [`cy4A7WceS9KSnyaQKb4Pn6UEeZvFALEngFthU4wuBX5`](https://nearblocks.io/txns/cy4A7WceS9KSnyaQKb4Pn6UEeZvFALEngFthU4wuBX5) / [`6dinora5VPTgyAARnsgZYVyCXZg5qNMutRzCyGuGiAi3`](https://nearblocks.io/txns/6dinora5VPTgyAARnsgZYVyCXZg5qNMutRzCyGuGiAi3) / [`JDrf3TGh9de3sUzUzSTa3nZWRmbi2RpoTbX1cqvw72s7`](https://nearblocks.io/txns/JDrf3TGh9de3sUzUzSTa3nZWRmbi2RpoTbX1cqvw72s7), [`CSSsRvuCZrTV6RRsoyLPrhqaKy6pwrvtaG3MwiKeAtpU`](https://nearblocks.io/txns/CSSsRvuCZrTV6RRsoyLPrhqaKy6pwrvtaG3MwiKeAtpU) |
| the agent rates the new worker (feedback 1 of agent 2), 01:46:58Z | [`Sy5dWRDSCiLTbLoUtjdT4Cz9n4Cnd4yV4xgN7Zz68Vh`](https://nearblocks.io/txns/Sy5dWRDSCiLTbLoUtjdT4Cz9n4Cnd4yV4xgN7Zz68Vh) |

Open in this run (Execution Market's side, not the contracts): the sponsored enrolment needed a manual re-broadcast of
the same signed transactions after RPC timeouts, and the new worker could not rate back: Execution Market's rating call
attaches 150 TGas (the contract's minimum is 120 TGas), so with the 0.01472 NEAR deposit it needs 0.1656 NEAR up front
at the pessimistic gas price (most of it is refunded), and the worker's wallet held 0.03 NEAR. Both are being fixed in Execution Market.

## 6. Arras v2 on mainnet (2026-11-08, block times 02:19:52Z to 02:25:06Z)

`scripts/mainnet-v2.sh --mainnet-ok todo` and then `scripts/mainnet-v2.sh --mainnet-ok humo` (the smoke test of the
security track), the sequence of [docs/MAINNET.md](MAINNET.md) section 8.4, run once from a clean checkout of `5800cb7`
(`main` at the time; details in MAINNET.md, section 8.8). The wasm is the reproducible build of commit `ddc5121` (sha256
`1541525c4bf3494ae5025b00df6955698975a6f8c376002f0296ff93b4e4fa77`), rebuilt before the run by
`scripts/mainnet-v2.sh wasm` (in a clean clone at `ddc5121`) with the same sha256. 31 transactions: 13 for the
deployment (blocks 223552046 to 223552216) and 18 for the smoke test (blocks 223552329 to 223552597), every one with a
successful final status and signed by the account in the table. The smoke moved Circle's USDC, with the project's
funding account as the human and two test accounts, `agent-v2.` and `worker-v2.`, as the agent and the worker.

| Step | Signed by | Transaction |
|---|---|---|
| hub -> v2 / hub -> parent, 02:19:52Z / 02:20:00Z | the funding account | [`yuVjiZTxbEUfWuqHUKinVsB3rKToUBPgZA1Pdq7Rjwn`](https://nearblocks.io/txns/yuVjiZTxbEUfWuqHUKinVsB3rKToUBPgZA1Pdq7Rjwn) / [`HsbGxrzkZbSth6FzzixfQXmgE12kEqBX4mzfVzbGNjJP`](https://nearblocks.io/txns/HsbGxrzkZbSth6FzzixfQXmgE12kEqBX4mzfVzbGNjJP) |
| `em-capturer-v2.` / `em-arbiter-v2.`, 02:20:10Z / 02:20:17Z | the parent | [`HxgECiVjU44bzzBt1d4C5Y4vaWb9kunRnbUEfLfx1W6p`](https://nearblocks.io/txns/HxgECiVjU44bzzBt1d4C5Y4vaWb9kunRnbUEfLfx1W6p) / [`54gmUtdYBubEV9fNayvmExvq5SLjbJfW1rv35eLhNPUS`](https://nearblocks.io/txns/54gmUtdYBubEV9fNayvmExvq5SLjbJfW1rv35eLhNPUS) |
| USDC storage for v2, 02:20:26Z | the funding account | [`9Je5r1PUoSP63CUpw4J7xo6UYqcTcNe8su7Ed2w9jMkc`](https://nearblocks.io/txns/9Je5r1PUoSP63CUpw4J7xo6UYqcTcNe8su7Ed2w9jMkc) |
| **deploy + private `new`** (10 USDC per payment, 1,000 USDC global TVL), 02:20:36Z | v2 | [`FdPJxM9kKsPiXeYBkqZnqTqqHKSkUBM2pM1hUg4uQQ1m`](https://nearblocks.io/txns/FdPJxM9kKsPiXeYBkqZnqTqqHKSkUBM2pM1hUg4uQQ1m) |
| `register_operator em` (capturer, arbiter, 7-day window, ticket) / sponsorship pool 0.1 NEAR, 02:20:45Z / 02:20:53Z | the owner / the funding account | [`yAisZKJtbNJBW1RFTQpaQLn3tXZXEW3LHQSDtr1KGHf`](https://nearblocks.io/txns/yAisZKJtbNJBW1RFTQpaQLn3tXZXEW3LHQSDtr1KGHf) / [`F8HLKAn5NkZ9rwo1DezHBDTGmUrA3CLwL5tpPmqAiGTM`](https://nearblocks.io/txns/F8HLKAn5NkZ9rwo1DezHBDTGmUrA3CLwL5tpPmqAiGTM) |
| capturer: function-call key / its full-access key deleted, 02:21:00Z / 02:21:06Z | the capturer | [`BzjsSt8VTSziSoU1vQUrnXWZRjwZukBL1yXyUd4c39EQ`](https://nearblocks.io/txns/BzjsSt8VTSziSoU1vQUrnXWZRjwZukBL1yXyUd4c39EQ) / [`4XZ6hcZnqheABpYQJzHKseoQA2CjgFj55VuTtEXsp1o6`](https://nearblocks.io/txns/4XZ6hcZnqheABpYQJzHKseoQA2CjgFj55VuTtEXsp1o6) |
| arbiter: function-call key / its full-access key deleted, 02:21:14Z / 02:21:19Z | the arbiter | [`AAYdP4jS1PpAYdjMHYS9t9rcA3MkYcP8SHAsGFQhHXS4`](https://nearblocks.io/txns/AAYdP4jS1PpAYdjMHYS9t9rcA3MkYcP8SHAsGFQhHXS4) / [`FTPgCfvaYGNVn1UadTmW4mijneuV97oDQF2RtgS7xg6i`](https://nearblocks.io/txns/FTPgCfvaYGNVn1UadTmW4mijneuV97oDQF2RtgS7xg6i) |
| **all keys of `v2.arras-escrow.near` deleted**, 02:21:28Z | v2 | [`Bqc63GgBiTWfrs9nAVHzE9piPLtPsxj6EcPSU7fDmVqt`](https://nearblocks.io/txns/Bqc63GgBiTWfrs9nAVHzE9piPLtPsxj6EcPSU7fDmVqt) |
| smoke: hub -> parent / `agent-v2.` / `worker-v2.`, 02:22:35Z / 02:22:43Z / 02:22:50Z | the funding account / the parent / the parent | [`EQhHFMndwVj1TZfs7R4vWeb8nH8mNavbzABxr5GiLKr6`](https://nearblocks.io/txns/EQhHFMndwVj1TZfs7R4vWeb8nH8mNavbzABxr5GiLKr6) / [`6bzSzsyB8dGYd7ukT9qm2ZuU27U6tUgXixWupPJFai4W`](https://nearblocks.io/txns/6bzSzsyB8dGYd7ukT9qm2ZuU27U6tUgXixWupPJFai4W) / [`DCDv7YjH6A94jNLcH7fQ5t19b9SGJhytbzhie1jRYEH5`](https://nearblocks.io/txns/DCDv7YjH6A94jNLcH7fQ5t19b9SGJhytbzhie1jRYEH5) |
| smoke: the agent's function-call key (`authorize_from_budget`) / its full-access key deleted, 02:22:56Z / 02:23:02Z | the agent | [`De7DhA1XE4PYTGdE8pj7BKVuN6axueLcsXKc7rAJNMtT`](https://nearblocks.io/txns/De7DhA1XE4PYTGdE8pj7BKVuN6axueLcsXKc7rAJNMtT) / [`5CQxZ8P1hiPGXqwziW8pk9RKAQq9DdvFJ4ZNqm24RXKp`](https://nearblocks.io/txns/5CQxZ8P1hiPGXqwziW8pk9RKAQq9DdvFJ4ZNqm24RXKp) |
| smoke: USDC storage for `worker-v2.` / the hub's NEP-145 storage in v2, 02:23:11Z / 02:23:22Z | the funding account | [`2fVqngWP4bKx9dPMe8hJH1E2Z4JCG93kne9RVCKyanYY`](https://nearblocks.io/txns/2fVqngWP4bKx9dPMe8hJH1E2Z4JCG93kne9RVCKyanYY) / [`4Ncxu59bnuXJHCNZzBtBxMNRZbU1vAWHB8QGJXQqSXNT`](https://nearblocks.io/txns/4Ncxu59bnuXJHCNZzBtBxMNRZbU1vAWHB8QGJXQqSXNT) |
| smoke: **the budget, 0.10 USDC for `agent-v2.` with the operator's ticket** (caps 0.05 per payment, 0.10 per day), 02:23:31Z | the funding account (the human) | [`8izehMAhAmaYyJy75mpG884LwCY96C7XHnoS4cJWNZGV`](https://nearblocks.io/txns/8izehMAhAmaYyJy75mpG884LwCY96C7XHnoS4cJWNZGV) |
| smoke, payment `9nySy6vTK95bgPUm5MkdsZtzC9XaGrSCV1EjNnRR6uyC`: **the agent's `authorize_from_budget` of 0.05 to `worker-v2.`** / release / `withdraw_for` the worker (0.0435) / `withdraw_for` the fee recipient (0.0065, the 13 %), 02:23:44Z / 02:23:52Z / 02:24:00Z / 02:24:10Z | the agent (function-call key); then the capturer (function-call key) | [`3Y9cAjyfgsVEFqeFuwxVMZgxvyAD8AHaDt6LKcy28JTi`](https://nearblocks.io/txns/3Y9cAjyfgsVEFqeFuwxVMZgxvyAD8AHaDt6LKcy28JTi) / [`52KbQt7q9aDJyWJDEcTnLc1uCcVdqaCVnyk7LeZueurw`](https://nearblocks.io/txns/52KbQt7q9aDJyWJDEcTnLc1uCcVdqaCVnyk7LeZueurw) / [`62tyfGbWHhYgLXey5makZhXZLRmFkB2tYhNEcWhJEfpv`](https://nearblocks.io/txns/62tyfGbWHhYgLXey5makZhXZLRmFkB2tYhNEcWhJEfpv) / [`9wV2pfNpeQFjwV9vdTYP2cvvTNoW9SLjjNLYGQbT6kwr`](https://nearblocks.io/txns/9wV2pfNpeQFjwV9vdTYP2cvvTNoW9SLjjNLYGQbT6kwr) |
| smoke, payment `8428jJdxtdVfJwrMKc7ircLp1Yr6D5nra5e16ZbRNS8w`: the agent's second payment of 0.05 / **the human opens a dispute** / **the arbiter returns it all to the human** / `withdraw_for` the human (0.05), 02:24:22Z / 02:24:29Z / 02:24:37Z / 02:24:46Z | the agent / the funding account / the arbiter (function-call key) / the capturer | [`FrbHGrPRn6ZwYrr4UZcVzpBST73zn5QUi7Yx6qZ79Ty1`](https://nearblocks.io/txns/FrbHGrPRn6ZwYrr4UZcVzpBST73zn5QUi7Yx6qZ79Ty1) / [`2PXjnXi7hQb4MhrjgvPgKDMPx5HBZQLCBoXXUCRqS8wK`](https://nearblocks.io/txns/2PXjnXi7hQb4MhrjgvPgKDMPx5HBZQLCBoXXUCRqS8wK) / [`6VsgyjUj64hbk4kMFinyniJkyunJ1s3aZj4bdacNouKa`](https://nearblocks.io/txns/6VsgyjUj64hbk4kMFinyniJkyunJ1s3aZj4bdacNouKa) / [`6Kaca22TyYbrpJkkcwNYqYb2pUR4pMKfYMVDERVLAS52`](https://nearblocks.io/txns/6Kaca22TyYbrpJkkcwNYqYb2pUR4pMKfYMVDERVLAS52) |
| smoke: `revoke_agent` / `storage_withdraw`, 02:24:58Z / 02:25:06Z | the funding account | [`Cc75eHk1vMFAfAs1jsPRwrQCrVwCWkBXgs9ttdi5gch2`](https://nearblocks.io/txns/Cc75eHk1vMFAfAs1jsPRwrQCrVwCWkBXgs9ttdi5gch2) / [`3QsUMq9Ve5UhwHPRjCQLYzZk2x94oZKzhZdEbbHXHcWw`](https://nearblocks.io/txns/3QsUMq9Ve5UhwHPRjCQLYzZk2x94oZKzhZdEbbHXHcWw) |

At the end, `get_config` read `tvl_global` 0, `in_flight` 0, `budgets_total` 0 and `payments` 2: the smoke's two. The
read-only checks of v2's code, its operator, the agent's key and both payments are in the README ("Check it
yourself"). This run changed nothing in Execution Market or in the facilitator: the facilitator's
`NEAR_ESCROW_CONTRACTS_MAINNET` still lists v1 only (pointing them at v2 is MAINNET.md, section 8.9).

## 7. Agents hiring agents: the KarmaKadabra swarm (2026-11-11, 03:03:51Z to 16:05:29Z)

Six Execution Market tasks between agents of KarmaKadabra, Ultravioleta DAO's own agent swarm. Every payer and every
worker is one of the swarm's agents (`<name>.karmakadabra.near`, shortened below to `<name>`); every task paid 0.01 USDC.
For each task:

- **lock**: the paying agent's `ft_transfer_call` of 10000 (0.01 USDC) to `v1.arras-escrow.near`, inside a NEP-366
  delegate action relayed by `uvd-facilitator.near`. The escrow logged `authorized` with the payment id below and
  returned `"0"`, so the token kept the whole amount in the escrow.
- **release**: `release` signed by `em-capturer.arras-escrow.near` with its function-call key, inside a delegate action
  relayed by `uvd-facilitator.near`. The escrow logged `released` with `amount` 10000, `net` 8700 and `fee` 1300.
- **two ratings**: `give_feedback_with_proof` on `reputation-registry.erc-8004.near` with
  `verifier` `erc8004-verifier.arras-escrow.near` and the payment id as `proof`, each signed by the rating agent. The
  same transaction called Identity `check_agent`, the verifier and the escrow's `get_payment`. Each one returned a
  `feedback_index` and logged `new_feedback`. `tag2` is `execution-market`, `tag1` is `worker_rating:knowledge_access`
  (payer rates worker) or `agent_rating:knowledge_access` (worker rates payer).

All 24 transactions have a successful final status.

| Task | Payer -> worker | Arras payment id | Lock | Release | Payer rates worker | Worker rates payer |
|---|---|---|---|---|---|---|
| `63b8a9f6` | `kk-davidtherich` -> `kk-validator` | `CFz3NrBxvbnf4netMY5EExdpFuGBsBZaGk8q6QmLos6b` | [`2mM23eX3...`](https://nearblocks.io/txns/2mM23eX3uWmcJi4bP31uGgQhz19nPUxm9JUt2HmTTGZw) 03:03:51Z | [`d6qVxfkz...`](https://nearblocks.io/txns/d6qVxfkzhcYmDKzsapNUpzdTVUYDvfjC6n6jZkfNJQ6) 03:08:55Z | [`4FR8c14h...`](https://nearblocks.io/txns/4FR8c14hxV95WSrQF9Ti3D1UFQ7PjKKehgoVc92PnVgC) 13:02:56Z: agent 8, 90 | [`Bfp1DKAL...`](https://nearblocks.io/txns/Bfp1DKALEvXS5Au1Ta4WrbjTeFrUsNYqqw72BHt3Csrm) 16:03:48Z: agent 6, 80 |
| `5f1b834a` | `kk-cymatix` -> `kk-validator` | `DKCFTmDx4ny4n3RsETdFFPmL42ozhCTf3E3nxugNbmmf` | [`Cup39erz...`](https://nearblocks.io/txns/Cup39erzLeAAmBJESoHUo8yBaUQbwLFUKCEB2YkUB6fH) 03:04:53Z | [`7sf24V45...`](https://nearblocks.io/txns/7sf24V459BkkBcPpHHQjQJzgbCH1RwV6jxG1xM9Cbqeh) 03:09:53Z | [`BrnJQSmb...`](https://nearblocks.io/txns/BrnJQSmbYgMhksa6rfpowwi7pfV6oCNCLMqk5AeipzVL) 12:58:46Z: agent 8, 75 | [`AMZDyEPh...`](https://nearblocks.io/txns/AMZDyEPhB5anUVJ5u1dYmFoaQ3abWMWftkEC5rWNkNS6) 16:04:13Z: agent 7, 80 |
| `a06e7c04` | `kk-athanw` -> `kk-karma-hello` | `HjA58DJEjboTajMi5AvfEtH6zgtfhzQWnc4oVWVaDAYY` | [`A2PrmD9U...`](https://nearblocks.io/txns/A2PrmD9UEDgsk715VuXXV1T9vaxSF4QnLVhx2Xf2zGDa) 03:09:23Z | [`4c8Grmiz...`](https://nearblocks.io/txns/4c8GrmizTHdkRaNfPnPWEFLUyG4ANmu2KCfjzSfAvezA) 03:14:18Z | [`3hEpfxc7...`](https://nearblocks.io/txns/3hEpfxc7auYt9ow9DgGLrFupvPZbe6BCi7MhDcLvSES3) 16:05:29Z: agent 9, 85 | [`9FR6Nks1...`](https://nearblocks.io/txns/9FR6Nks131hkzV4PC4p21JuC71e8GvGGMLK3hUhbDjGG) 16:04:30Z: agent 5, 80 |
| `238d8ad8` | `kk-0xroypi` -> `kk-karma-hello` | `JW3pPyDXd1TTPQC5tstAAprjHkYjspyDcMH6HM2cc9o` | [`9SPVgerk...`](https://nearblocks.io/txns/9SPVgerk8DT2kmGE6h4MH4NDgo43Z5xerdKviKgieuYh) 03:13:19Z | [`4VetLjym...`](https://nearblocks.io/txns/4VetLjymGk2G6w6ob21NbPiAhTnAdURfxqJPPsd6EDU4) 03:18:46Z | [`Aw8nvxEn...`](https://nearblocks.io/txns/Aw8nvxEnybyadvEWZdLr6fewAU7gHdDZJpTiVEhkMXPQ) 16:04:38Z: agent 9, 85 | [`9reGWxDQ...`](https://nearblocks.io/txns/9reGWxDQomUe91f4WCeLs2QC1cMncKWyp2ehtcCkEewa) 16:04:44Z: agent 4, 80 |
| `a1f11dbe` | `kk-cymatix` -> `kk-validator` | `8xdR2tBkHveUKzT1kt8SENSgirR7w6meaH5JUo2ZHBmY` | [`6JKKUykL...`](https://nearblocks.io/txns/6JKKUykLPRKuvU3nAVRLt1RemwSHvtGU52xUuvp5XrQM) 13:28:49Z | [`fN6mQ6Wi...`](https://nearblocks.io/txns/fN6mQ6Wink4SyvFHFuS8i3t8SifmR1ra7qj2KWzHMMb) 13:34:31Z | [`5hamRS3U...`](https://nearblocks.io/txns/5hamRS3UKNBaLbHs1moRASjUjx5CMFiZywYDzyPpSp15) 13:34:48Z: agent 8, 80 | [`BGsa9eoG...`](https://nearblocks.io/txns/BGsa9eoGyGWUWi9Xd6QRoDurQYRQPKbdLnq7tRRURLiZ) 13:36:21Z: agent 7, 88 |
| `54c56f0a` | `kk-davidtherich` -> `kk-validator` | `E78uP1YhqqfVdi89FWU3jkfmbzMRb6Eec6aDh2nGkPGP` | [`HbgfUdec...`](https://nearblocks.io/txns/HbgfUdecdtxWsvRjKESb2Mp9jPdxWjK9hNNN5hSV3qhe) 13:37:35Z | [`5osno8Wh...`](https://nearblocks.io/txns/5osno8WhMGewXb6sBFuQ4fbNTd6fqSJkwX9kncUwBhRK) 13:42:36Z | [`3qXFY2ji...`](https://nearblocks.io/txns/3qXFY2jiT8fJptAD61VAqWDD5KBX6VGJB7khH5JH1JhU) 13:42:45Z: agent 8, 80 | [`2CSckwmg...`](https://nearblocks.io/txns/2CSckwmggHohH5RaR1d9mY9ZcyEmwnepXoANcNDxTrgD) 13:44:34Z: agent 6, 88 |

Read-only checks (any of the six payment ids, agents and clients):

```sh
near contract call-function as-read-only v1.arras-escrow.near get_payment json-args '{"payment_id":"CFz3NrBxvbnf4netMY5EExdpFuGBsBZaGk8q6QmLos6b"}' network-config mainnet now
#   "status": "Released", "amount": "10000", "released": "10000", "payer": "kk-davidtherich.karmakadabra.near",
#   "receiver": "kk-validator.karmakadabra.near", "operator_id": "em", "fee_bps": 1300
near contract call-function as-read-only reputation-registry.erc-8004.near read_feedback_proof json-args '{"agent_id":"8","client_address":"kk-davidtherich.karmakadabra.near","feedback_index":"1"}' network-config mainnet now
#   "proof": "CFz3NrBx...", "counterparty": "kk-validator.karmakadabra.near", "role": "receiver", "context": "em", "weight": "10000"
near contract call-function as-read-only reputation-registry.erc-8004.near get_verified_summary json-args '{"agent_id":"8","verifier":"erc8004-verifier.arras-escrow.near"}' network-config mainnet now
#   on 2026-11-11: "count": "4", "summary_value": "81", "weight_sum": "40000"
```

On 2026-11-11 the registries held 10 agents (`nft_total_supply`) and 16 ratings (`read_all_feedback` of agents 0 to
9), none revoked, all with an Arras proof (`get_verified_summary` per agent). 4 come from the earlier sections (the
smoke of section 4 and the runs of section 5); 12 are the swarm's, above. Through v1, `get_config` counted 20 payments:
the 13 of section 2, the 6 above, and one more between two swarm agents on 2026-11-10 at 07:54:52Z
([`3RWqDjMx...`](https://nearblocks.io/txns/3RWqDjMxXKDw1kxMCdNnKg2DH2E3WjCL4GveiNY7UVQV), read on NearBlocks: `kk-saemtwentytwo` -> `kk-karma-hello`, 0.01
USDC; payment `F8hZ6Um9Ub7drzfodhsKubFXbvvyhgKqrBJFFn8K6maM`, `Released` in `get_payment`).
