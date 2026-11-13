# Arras on NEAR mainnet: the runbooks of v1 (H-3) and v2 (H-11)

This file holds the runbooks of the two escrow contracts and, in section 9, the deploy plan and the mainnet run of the
payment-proof verifier that lets a payment of the Arras escrow contract at `v2.arras-escrow.near` serve as the proof of
an ERC-8004 rating:

| Sections | Runbook | Script | On mainnet |
|---|---|---|---|
| [1](#1-what-goes-on-chain) to [7](#7-the-mainnet-run-2026-11-03) | Arras v1 (H-3) | `scripts/mainnet.sh` | `v1.arras-escrow.near`, deployed 2026-11-03 (section 7) |
| [8](#8-arras-v2-on-mainnet-the-runbook-h-11) (8.1 to 8.9) | Arras v2 (H-11) | `scripts/mainnet-v2.sh` | `v2.arras-escrow.near`, rehearsed on testnet and deployed 2026-11-08 (8.8) |
| [9](#9-the-payment-proof-verifier-erc-8004-the-plan) (9.1 to 9.6) | the payment-proof verifier | the commands of 9.3 | `payment-proof.arras-escrow.near`, deployed 2026-11-13 (9.6) |

v1's status: **deployed on 2026-11-03** (02:30:11Z to 02:32:30Z by the script's clock; block times 02:30:39Z to
02:32:19Z), with the one command of section 4, after the testnet rehearsals of section 6. `v1.arras-escrow.near` has
the pinned wasm and no access keys; section 7 has the run and its transactions, and [EVIDENCE.md](EVIDENCE.md) the
mainnet transactions since. Sections 1 to 6 are v1's runbook as it was written before the run (2026-11-02/03).

## 1. What goes on chain

| Fact | Value |
|---|---|
| Wasm | reproducible build of commit `8da71748b428ceccf9fccd358e61bb4213d54e29` (the code reviewed in RARRAS-1, already in `main`) |
| sha256 (hex) | `7058319dff5f8d1e9da91a344ab3c699b709783e4153f3a7da6c2556cde0eada` |
| sha256 (base58, what NearBlocks and `view_account.code_hash` show) | `8ZYfMJerz9r1mh4vZH55vyyJb1SLyX7qR2xqpYPxHr5X` |
| Size | 268,839 bytes |
| Build | `sourcescan/cargo-near:0.21.1-rust-1.96.0@sha256:ccb22bb4...`; re-measured on 2026-11-02 from a scratch clone and twice through `scripts/mainnet.sh wasm`: same sha256 every time |
| Same code as | `arras-v1b-1101.testnet` (the testnet demo) and the two rehearsal contracts of section 6 |
| Token | Circle's native USDC `17208628f84f5d6ad33f0da3bbbeb27ffcb398eac501a31bd6ad2011e36133a1` (6 decimals; `storage_balance_bounds` 0.00125 NEAR, read 2026-11-02 23:47Z) |
| `new` | owner, guardian, token, `tope_por_pago` **10 USDC**, `tope_tvl_global` **1,000 USDC** (the owner's decision, "Tope global de 1.000"; `new` is the ceiling forever, `set_caps` only lowers) |
| Operator `em` | fee 1300 bps, `fee_recipient` frozen per payment, `min_amount` 0.01 USDC, TVL **1,000 USDC per operator** and **20 per payer**, sponsorship 0.02 NEAR per payer, pool 0.5 NEAR |

The per-operator cap is 1,000 because its ceiling is also fixed: `register_operator` sets it
(`contracts/escrow-v1/src/admin.rs:35`, and an existing operator cannot be registered again, `E44`) and
`update_operator` refuses to raise it (`admin.rs:88-96`, `E42`). The owner can lower it.

`main` (`7312a9d`) has the same contract code; it only adds `license = "MIT"` to the `Cargo.toml` files. A build of
`main` embeds another commit in its NEP-330 metadata and so has another sha256; building `8da7174` keeps the mainnet
code byte for byte equal to the one that already moved real testnet USDC.

How anyone checks it after the deploy:

```bash
near account view-account-summary v1.arras-escrow.near network-config mainnet now   # SHA-256 7058319d...0eada, "Account is locked (no access keys)"
near contract call-function as-read-only v1.arras-escrow.near contract_source_metadata json-args '{}' network-config mainnet now   # rev 8da71748...
```

NearBlocks shows the code hash at once; its "verified source" badge (SourceScan) needs the repository public, because
the NEP-330 link is `git+https://github.com/UltravioletaDAO/arras?rev=8da71748...`.

## 2. Accounts, signers and keys

The parent is a new project account, `arras-escrow.near` (case a, the default; free on 2026-11-02 23:50Z). The script
also supports a sub-account created by the owner, such as `arras.ultravioletadao.near` (case b,
`ARRAS_PADRE=arras.ultravioletadao.near`).

| Role | Account | Created by | Funding (NEAR) | Key at the end | Name of the key in AWS |
|---|---|---|---|---|---|
| Funding (case a) | `fe0a574258f8277099875e46d4ca0043cdb6274ff2501b375a0d3d64bc211079` (implicit) | exists once it receives NEAR; **the owner funds it** | **4.55** | c0der's, already in SSM | `/arras/mainnet/fondeo` (us-east-2) |
| Parent | `arras-escrow.near` | `near.create_account`, signed by the funding account | 4.52 | FullAccess | `/arras/mainnet/padre` |
| Contract v1 | `v1.arras-escrow.near` | parent | 3.0 | **none** (`view_access_key_list` empty) | - |
| Contract v2 (reserved) | `v2.arras-escrow.near` | parent | 0.1 | FullAccess until the v2 deploy (D9) | `/arras/mainnet/v2-reserva` |
| Owner | `owner.arras-escrow.near` | parent | 0.1 | FullAccess | `/arras/mainnet/owner` |
| Guardian (pause only) | `guardian.arras-escrow.near` | parent | 0.1 | FullAccess | `/arras/mainnet/guardian` |
| EM capturer | `em-capturer.arras-escrow.near` | parent | 0.5 | **only** a FunctionCall key: receiver v1, `release, refund, withdraw_for`, allowance 0.5 NEAR | `/arras/mainnet/em-capturer-v1` |
| EM `fee_recipient` | `em-fees.arras-escrow.near` (or an existing EM account, `ARRAS_FEE_RECIPIENT`) | parent (only if it is the default) | 0.1 | FullAccess | `/arras/mainnet/em-fees` |

The prize account `ultravioletadao.near` signs nothing in case (a). The funding key never touches disk: the script
reads it from SSM into a pipe that near-cli opens as `sign-with-access-key-file /dev/fd/N`, and first checks that its
public key is the funding account (an implicit account is the hex of its public key). Every new key goes to near-cli's
legacy keychain (the `credentials_home_dir` of near-cli's config, `~/.near-credentials` by default:
`<dir>/mainnet/<account>/`, mode 600) so that `secretos` can hand it to AWS by file name, without printing it.

Who signs each step:

| Step | Signs | What |
|---|---|---|
| `padre` (a) | the funding account (key from SSM) | `create-account fund-myself arras-escrow.near '4.52 NEAR' autogenerate-new-keypair save-to-legacy-keychain sign-as fe0a...1079 ... sign-with-access-key-file <pipe>`: near-cli turns it into `near.create_account` with 4.52 attached |
| `padre` (b) | the owner, as `ultravioletadao.near` | the script generates the parent's key pair and prints the public key and the owner's `create-account fund-myself ... use-manually-provided-public-key` command |
| `cuentas` | parent | six `create-account fund-myself` (v1, v2, owner, guardian, capturer, fee recipient) |
| `usdc` | parent | `storage_deposit` in USDC for v1 and the fee recipient |
| `deploy` | v1 (its creation key) | deploy + private `new` in **one** transaction |
| `operador` | owner; parent | `register_operator em` (1 yocto); `fund_operator_storage` 0.5 NEAR |
| `capturer` | capturer (its creation key) | adds the FunctionCall key, then deletes its own FullAccess |
| `sin-llaves` | v1 | deletes **all** its keys in one transaction (the last signs its own deletion) |
| `verificar`, `medir`, `enlaces` | nobody | RPC reads |
| `secretos` | c0der, AWS CLI | prints one chained line per live key (`put-parameter && verify && rm`); the script runs none of them |

## 3. How much NEAR, exactly

Measured on testnet on 2026-11-02/03 (section 6). Storage costs the same on both networks (10^19 yocto per byte) and
the gas price was 10^8 yocto per gas on both (`gas_price` RPC, 2026-11-02 23:40Z), so the numbers carry over.

| Item | NEAR | Measured as |
|---|---|---|
| creating the parent with `create_account` on the network's account (`testnet` there, `near` on mainnet) | 0.00745 | balance of the funding account: 4.9 before, 0.37255 after, 4.52 of it into the parent |
| creating one sub-account | 0.00711 | 0.0000825 for the transaction + 0.0070325 burnt in the receipt on the new account |
| v1 storage after deploy, `new` and `register_operator` | **2.69674** | 269,674 bytes (`get_storage_health`) |
| v1 gas: deploy + `new` / delete its keys | 0.00213 / 0.00004 | balance of v1 before and after |
| USDC `storage_deposit` | 0.00125 + 0.00023 gas | each |
| `register_operator` (owner) | 0.00025 | |
| `fund_operator_storage` gas (parent) | 0.00020 | plus the 0.5 attached |
| capturer: add the FunctionCall key + delete its FullAccess | 0.00008 | |
| `release` / `withdraw_for` signed by the capturer | 0.00030 / 0.00066 | each |
| storage of one sponsored payment | 0.01003 | 1,003 bytes, paid by the pool, back to the pool only at `compact` |

Funding with the script's defaults (`scripts/mainnet.sh plan` prints it):

| What | NEAR | Why |
|---|---|---|
| v1 | 3.0 | 2.69674 storage + 0.00217 gas + **0.30 free reserve** for later operator edits (measured 0.30109). Everything that enters v1 stays there forever (no keys), so v1 is not over-funded |
| v2 (reserved) | 0.1 | 182 bytes now; the real funding comes with the v2 deploy |
| owner / guardian | 0.1 / 0.1 | admin transactions cost ~0.0003 each |
| capturer | 0.5 | the FunctionCall allowance is 0.5; about 500 `release` + `withdraw_for` pairs if EM signs them itself (none if the facilitator relays them) |
| fee recipient | 0.1 | only if the script creates it |
| sponsorship pool | 0.5 | about 49 first payments without the payer's own NEP-145 deposit; it lives inside v1 (spent or refilled, never withdrawn) |
| USDC storage x2 | 0.0025 | |
| gas margin | 0.1 | covers the 6 creations (0.0427) and the rest (~0.001) |
| **parent budget** | **4.5025** | what `cuentas` checks before creating anything |
| **the parent is created with** | **4.52** | budget + the parent's own 182 bytes, rounded up; **0.0741 stayed in the parent** in the rehearsal, for later top-ups |
| **the owner funds the funding account with** | **4.55** | 4.52 + the 0.02 margin for `create_account` (0.00745 measured) + the implicit account's own 182 bytes, rounded up; `padre` checks it before signing |

So the first mainnet deployment costs the owner **4.55 NEAR**, and about 4.43 of it is locked as storage, reserve and
pool. In case (b) the owner creates the parent directly with 4.52 NEAR (plus ~0.0071 for the creation, from
`ultravioletadao.near`). v2 (D9) needs its own funding later: its wasm storage (v1 is 269 KB, so expect 3 to 4 NEAR)
plus a free reserve and its operator pool; the master plan's 8 to 10 NEAR for both versions still holds.

## 4. The sequence

```bash
scripts/mainnet.sh plan                      # dry run: accounts, amounts, caps, wasm, budget (no network at all)
scripts/mainnet.sh todo                      # dry run of the whole sequence: every command, nothing executed
scripts/mainnet.sh wasm                      # local reproducible build of 8da7174 (Docker, ~3 min), checked against the pin
#   -> the owner sends 4.55 NEAR to fe0a574258f8277099875e46d4ca0043cdb6274ff2501b375a0d3d64bc211079
ARRAS_KMS_KEY=<Arras KMS key> scripts/mainnet.sh --mainnet-ok todo
#   padre (create_account from the funding account), cuentas, usdc, deploy, operador, capturer, sin-llaves,
#   verificar, secretos, enlaces
```

`ARRAS_KMS_KEY` (Arras's own KMS key, decision 230) is mandatory in any real run; without it the script stops before
anything else. Every step is idempotent: a second `todo` sends no transaction (rehearsed: 19 transactions before and
after). If a step fails, fix the cause and run `todo` again; it skips what is done and refuses to overwrite a different
contract. The long part runs inside a function bash has already parsed, so a `git pull` in the middle of a run does not
make bash execute half-read lines.

What the script refuses (each one has a test in `scripts/test_mainnet.sh`):

- a real run without the flag of **that** network (`--mainnet-ok` with `ARRAS_RED=mainnet`, `--testnet-ok` with
  testnet), with both flags, without `ARRAS_KMS_KEY`, or with an `ARRAS_RPC` whose `status.chain_id` is another
  network; without a flag it only prints and makes no network call;
- `ARRAS_PARENT` (renamed to `ARRAS_PADRE`), accounts that are not valid lowercase NEAR accounts of the network, a
  funding account that is not a 64-hex implicit account, a key file instead of SSM on mainnet, a capturer that is not a
  direct child of the parent, a fee recipient equal to v1, v2 or the capturer (the capturer ends with only a
  FunctionCall key, so it could never move the fees), a capturer allowance of 0, a derived account longer than 64;
- incoherent caps (global below per payment, operator above global, per payer above operator, `min_amount` above the
  per-payment cap, fee above 1800 bps) and non-numeric amounts;
- a wasm whose sha256 or embedded commit differs from the pins, near-cli other than 0.30.1, a v1 that already has other
  code, a `get_config` other than the expected owner, guardian, token and caps;
- a funding key whose public key is not the funding account, a funding account without 4.52 + 0.02 free, an existing
  parent with no FullAccess key from this keychain, a parent without the full budget (nothing is created);
- deleting the capturer's FullAccess unless its FunctionCall key for v1 (a FunctionCall key for another receiver does
  not count) is on chain with exactly `release, refund, withdraw_for`, the configured allowance, and its file in the
  keychain; a capturer left with any key but that one (checked at the end of `capturer` and again in `verificar`);
- a `get_config` or a `get_operator` that differs from what was decided in any field it compares (version, owner,
  guardian, token, both caps; capturer, fee, fee recipient, `min_amount`, both operator caps, sponsorship);
- `sin-llaves` (also run alone) on a v1 whose code is not the pinned wasm or whose `get_config` is not the expected
  one: while v1 has a key, a bad deploy can still be undone with `delete-account`; a v1 that still lists any key after
  `sin-llaves`;
- the payment steps (`authorize`, `release`, `withdraw`, `ensayo`) on mainnet: the first mainnet payment is EM's (H-8).

It never prints a private key: the near-cli output goes through a filter, `secretos` reads only `public_key` from the
key files, the funding key only travels through a pipe, and in a real run each key file is checked against
`view_access_key_list` so a key already deleted on chain (the capturer's creation key, v1's keys) is only removed from
disk, never uploaded.

### Keys to AWS

`secretos` in a dry run prints only the inventory (which key files are on disk and the name each would get), never a
command. In a real run (the end of `todo`) it prints **one line per live key**: `aws ssm put-parameter --type SecureString --key-id
$ARRAS_KMS_KEY --value file://<key file>` with the name of section 2 `&&` a check that compares the sha256 of the
stored value and of the file without showing either `&&` `rm` of the file (and of near-cli's `<account>.json` copy when
it holds the same key). If the upload or the check fails, nothing is deleted. Keys already deleted on chain are only
removed. The files of an account that does not exist on chain are not touched, and v1's files are removed only when v1
exists and `view_access_key_list` is empty (after a failure half-way, v1's key must stay on disk to finish
`sin-llaves`). Plain `rm`: on APFS overwriting guarantees nothing (`rm -P` is a no-op on current macOS); FileVault protects
the disk. `ARRAS_SECRETOS=secretsmanager` prints Secrets Manager commands instead (the owner's rule since 2026-10-16 and
2026-10-30 is SSM).

## 5. Order with the facilitator and Execution Market

1. **H-3 (this):** v1 deployed, keyless, verified; v2's account reserved (empty, its key in AWS).
2. **H-5, x402-rs (one batch):** `NEAR_ESCROW_CONTRACTS_MAINNET=v1.arras-escrow.near,v2.arras-escrow.near` and
   `ENABLE_NEAR_ESCROW`. Both names are fixed now, so the facilitator learns v2 in its single batch. Its lifecycle gate
   reads `get_operator("em").capturers`, which is `["em-capturer.arras-escrow.near"]`.
3. **H-8, Execution Market:** network `near`, `escrow_contract` `v1.arras-escrow.near`, `operator_id` `em`, the capturer
   key from `/arras/mainnet/em-capturer-v1`, the fee recipient of section 2. First real payment 0.10 to 1 USDC (D7).
   Until H-11, `v2.arras-escrow.near` is in that list with no code and a FullAccess key (in SSM): a `ft_transfer_call`
   to it fails and the token returns the USDC, but whoever holds that key could deploy anything there. Treat
   `/arras/mainnet/v2-reserva` like the owner key, or add v2 to the list only when it is deployed (one more x402-rs
   batch).
4. **H-11:** v2 deployed into `v2.arras-escrow.near` with the key from `/arras/mainnet/v2-reserva`, then that account's
   keys deleted; v2 gets its own capturer FunctionCall key (a FunctionCall key serves one contract, and the v1 capturer
   no longer has a FullAccess key to add another).

Two operating facts for EM's payers:

- **Sponsored storage is one open payment per payer.** A first payment costs 1,003 bytes (0.01003 NEAR) and the
  sponsorship cap is 0.02 NEAR per payer, returned only at `compact` (7 days after `reclaim_after`). EM's payer makes
  its own `storage_deposit` on v1 (0.05 NEAR covers about five open payments); the owner can also raise
  `sponsor_max_por_payer` with `update_operator` (it moves both ways).
- **The `authorize` reserves 100 TGas at the pessimistic gas price** (about 0.1 NEAR), so a payer that signs its own
  `ft_transfer_call` needs that much free NEAR; through the facilitator (NEP-366) the relayer pays it.

v1 has **no payer allow-list** (a rule of the design, set by the owner: no allow-lists; v1 is immutable). Who may pay
is decided outside the contract. The on-chain limits are the caps above,
the guardian's pause and the storage rules.

## 6. Testnet rehearsals

The same script with `ARRAS_RED=testnet` and fresh development accounts; the payments use the USDC that
`payer-demo.arras-dev.testnet` already had (Circle's testnet faucet).

### 6.1 Case (a), with the mainnet defaults (2026-11-03): `arras-escrow-1102.testnet`

```bash
ARRAS_RED=testnet ARRAS_PADRE=arras-escrow-1102.testnet ARRAS_FONDEO=904e38d03a4706538533301d6b97acb77faaca6310634b8e4717e95d0f39ccee \
  ARRAS_FONDEO_ARCHIVO=<its key file> ARRAS_KMS_KEY=<placeholder> scripts/mainnet.sh --testnet-ok ensayo
```

One command, exit 0. The funding account is an implicit account funded with 4.9 NEAR from the first rehearsal's parent
([`ENfQAji2mvbfUwiANPgipgkXuTmDFHVkCec4kX8CVCdZ`](https://testnet.nearblocks.io/txns/ENfQAji2mvbfUwiANPgipgkXuTmDFHVkCec4kX8CVCdZ));
on testnet its key comes from a file (`ARRAS_FONDEO_ARCHIVO`, refused on mainnet) through the same pipe.

| Step | Transaction |
|---|---|
| parent `arras-escrow-1102.testnet`: `testnet.create_account` with 4.52, signed by the implicit account | [`DLFw9hhFWg37c3XJTFMapbeexTR5GB5C5q5FE81YKtin`](https://testnet.nearblocks.io/txns/DLFw9hhFWg37c3XJTFMapbeexTR5GB5C5q5FE81YKtin) |
| `v1.` (3.0) / `v2.` (0.1) | [`82EaohTYTHpnAXY9SfZ1fxm3pk21iXbMvF45yaipTvoi`](https://testnet.nearblocks.io/txns/82EaohTYTHpnAXY9SfZ1fxm3pk21iXbMvF45yaipTvoi) / [`Gf7EevAbijXQ5Xpfb44m5MRKU7bhAha89q66Jk5BwDuN`](https://testnet.nearblocks.io/txns/Gf7EevAbijXQ5Xpfb44m5MRKU7bhAha89q66Jk5BwDuN) |
| `owner.` (0.1) / `guardian.` (0.1) | [`3Bm7jV94dGX2GzUSb6wHhFwpHYT9BQ7gQhj6CBjD63Wn`](https://testnet.nearblocks.io/txns/3Bm7jV94dGX2GzUSb6wHhFwpHYT9BQ7gQhj6CBjD63Wn) / [`HspDjpggwLAzRxGDEBJmNagwickgGN1rg4TcHrbfiGRy`](https://testnet.nearblocks.io/txns/HspDjpggwLAzRxGDEBJmNagwickgGN1rg4TcHrbfiGRy) |
| `em-capturer.` (0.5) / `em-fees.` (0.1) | [`2KSfb9tgEMGDAbFeLYWCh7N5EpehQLwiD94ERwRNy8Ww`](https://testnet.nearblocks.io/txns/2KSfb9tgEMGDAbFeLYWCh7N5EpehQLwiD94ERwRNy8Ww) / [`GXG9oM8qUj5DGkkZfy79eVdSkW9EXg2TMqc1CwxEG4j3`](https://testnet.nearblocks.io/txns/GXG9oM8qUj5DGkkZfy79eVdSkW9EXg2TMqc1CwxEG4j3) |
| USDC storage for v1 / for `em-fees.` | [`CA2J9L8bTPxrmdibwHiTgtVay6MmpjfxPXNPnCoeoDtD`](https://testnet.nearblocks.io/txns/CA2J9L8bTPxrmdibwHiTgtVay6MmpjfxPXNPnCoeoDtD) / [`D6uwVPRF6ZgsWs5UNbQwadB4178NkDzeRfFUug36pVKp`](https://testnet.nearblocks.io/txns/D6uwVPRF6ZgsWs5UNbQwadB4178NkDzeRfFUug36pVKp) |
| deploy + private `new` (10 / 1,000 USDC) | [`YdCeXi47i24kUcp65D5pN1Wz47eZywYypQQnUexsK98`](https://testnet.nearblocks.io/txns/YdCeXi47i24kUcp65D5pN1Wz47eZywYypQQnUexsK98) |
| `register_operator em` (1,000 / 20 USDC) / `fund_operator_storage` 0.5 | [`Ecx4jjmKJmeKVUAtRz6x9KMddo7LU8DoTGTVuuUQdzAX`](https://testnet.nearblocks.io/txns/Ecx4jjmKJmeKVUAtRz6x9KMddo7LU8DoTGTVuuUQdzAX) / [`FXa7T5wzFVibxKLNmMTCHKUn2iSs6g4FH7ng6kY7si44`](https://testnet.nearblocks.io/txns/FXa7T5wzFVibxKLNmMTCHKUn2iSs6g4FH7ng6kY7si44) |
| capturer: FunctionCall key / its FullAccess deleted | [`Ea4P67bkRDCmRMVpTvraJTKscfZaSknnBdpkQx3nmewg`](https://testnet.nearblocks.io/txns/Ea4P67bkRDCmRMVpTvraJTKscfZaSknnBdpkQx3nmewg) / [`963k5wA5dYBkEjdJdxzTV5guymtNMhxYHCppuTFaJYhK`](https://testnet.nearblocks.io/txns/963k5wA5dYBkEjdJdxzTV5guymtNMhxYHCppuTFaJYhK) |
| **all keys of v1 deleted** | [`4nSGRiKND9mX33kfzvxoJ22S39EyfFwCQp2xnTsamUCz`](https://testnet.nearblocks.io/txns/4nSGRiKND9mX33kfzvxoJ22S39EyfFwCQp2xnTsamUCz) |
| authorize, 1 USDC from `payer-demo.arras-dev.testnet` (sponsored storage) | [`AvTLHxKTVvNDYzx2K2VHMnyE4heH2cyfzeb6cwhWwnsT`](https://testnet.nearblocks.io/txns/AvTLHxKTVvNDYzx2K2VHMnyE4heH2cyfzeb6cwhWwnsT), payment `672ACooU7ZuVtqHSwN4vgaKRCSAQrPFZTaYMPggfu7hj` |
| release by the capturer's FunctionCall key: net 870000, fee 130000 | [`3wqcRzRTRRzrEUPzQRvVaJKrHSiNjT78ddjMYfNcMj5H`](https://testnet.nearblocks.io/txns/3wqcRzRTRRzrEUPzQRvVaJKrHSiNjT78ddjMYfNcMj5H) |
| `withdraw_for` worker (0.87 USDC) / fee recipient (0.13 USDC), both `ok: true` | [`5F5tPzoruDq24eYdDDCGDwsJKxWBSygsb3R5WJuQvRc2`](https://testnet.nearblocks.io/txns/5F5tPzoruDq24eYdDDCGDwsJKxWBSygsb3R5WJuQvRc2) / [`3TiXvRHpajHhBcwx9GA5QvVMQGk8DBpD556HSTocAtzi`](https://testnet.nearblocks.io/txns/3TiXvRHpajHhBcwx9GA5QvVMQGk8DBpD556HSTocAtzi) |

Balances at the end (`scripts/mainnet.sh --testnet-ok medir`): funding account 0.37255 (of 4.9), parent 0.07414 (of
4.52), v1 3.49783 with 270,677 bytes (free reserve 0.30109, pool 0.48997 after one sponsored payment), owner 0.09975,
capturer 0.49829, the rest untouched; the contract holds 0 USDC and `tvl_global` is 0.

| Check | Command | Result |
|---|---|---|
| code = pinned wasm, no keys | `near account view-account-summary v1.arras-escrow-1102.testnet network-config testnet now` | SHA-256 `7058319d...0eada`; `view_access_key_list` empty |
| config | `near contract call-function as-read-only v1.arras-escrow-1102.testnet get_config json-args '{}' network-config testnet now` | `tope_por_pago` 10000000, `tope_tvl_global` 1000000000, `tvl_global` 0, `payments` 1 |
| operator | `... get_operator json-args '{"operator_id":"em"}' ...` | `tope_tvl_operador` 1000000000, `tope_tvl_por_payer` 20000000, capturer `em-capturer.arras-escrow-1102.testnet` |
| the payment | `... get_payment json-args '{"payment_id":"672ACooU7ZuVtqHSwN4vgaKRCSAQrPFZTaYMPggfu7hj"}' ...` | `"status": "Released"` |
| nothing left in the contract | `near contract call-function as-read-only 3e2210e1184b45b64c8a434c0a7e7b23cc04ea7eb7a6c3c32520d03d4afcb8af ft_balance_of json-args '{"account_id":"v1.arras-escrow-1102.testnet"}' network-config testnet now` | `"0"` |
| the capturer can only release, refund, withdraw_for | `near account list-keys em-capturer.arras-escrow-1102.testnet network-config testnet now` | one FunctionCall key, receiver v1, those three methods |

### 6.2 The first rehearsal (2026-11-02): `arras-ensayo-1102.testnet`

Parent from the faucet, the earlier defaults (v1 3.5 NEAR, owner 0.2, caps 10 USDC per payment, 200 global, 200 per operator), run step by step; same
result and the same measurements (v1 269,674 bytes, free reserve 0.80 with 3.5 NEAR), plus the idempotency check: a
second `todo` sent no transaction (19 before and after). Transactions: deploy + `new`
[`9xcYTE6x2dr4nrdBbCJsu4BBGqwa9A1DHRKPziU9joGf`](https://testnet.nearblocks.io/txns/9xcYTE6x2dr4nrdBbCJsu4BBGqwa9A1DHRKPziU9joGf),
keys of `v1.arras-ensayo-1102.testnet` deleted
[`GY5ZvPRjksEkiAdiViyPdB2W89cDhFPCpxmaXaXAtLGg`](https://testnet.nearblocks.io/txns/GY5ZvPRjksEkiAdiViyPdB2W89cDhFPCpxmaXaXAtLGg),
authorize [`AbLh6nZ5bShDDkkLBPPT6XjL4EqxkgRVKBMt2oHSNYwo`](https://testnet.nearblocks.io/txns/AbLh6nZ5bShDDkkLBPPT6XjL4EqxkgRVKBMt2oHSNYwo),
release [`C5MapHzDyN4x8XS5RTU4UumKSL2E6mkmXNpH7GuGdjua`](https://testnet.nearblocks.io/txns/C5MapHzDyN4x8XS5RTU4UumKSL2E6mkmXNpH7GuGdjua),
`withdraw_for` [`HTd8SHpr6a2kw9aEMnndExVvNpc9AQ7fyKCjpdXePX2U`](https://testnet.nearblocks.io/txns/HTd8SHpr6a2kw9aEMnndExVvNpc9AQ7fyKCjpdXePX2U)
and [`HheUe4hg29aA5GwA7uv4Vf5od3ewiQWKro6LLg8ADJdV`](https://testnet.nearblocks.io/txns/HheUe4hg29aA5GwA7uv4Vf5od3ewiQWKro6LLg8ADJdV).

## 7. The mainnet run (2026-11-03)

`ARRAS_RED=mainnet scripts/mainnet.sh --mainnet-ok todo` (with `ARRAS_KMS_KEY`), from a clean checkout of `ea5869f`
(the merge of #5, `main` at the time), 02:30:11Z to 02:32:30Z by the script's clock: 15 transactions. The owner funded the funding
account with 10 NEAR instead of the 4.55 of section 3, so it kept 5.59 NEAR afterwards (for v2). Everything else
followed sections 1 to 4: case (a), the default amounts and caps, the pinned wasm.

What the script checked on chain before it finished, and what anyone can check now (README, "For NEAR HACKATHON
judges"):

- the code of `v1.arras-escrow.near` is the wasm `7058319d...0eada` (rev `8da71748`);
- `get_config` has the expected owner, guardian, native USDC, 10 USDC per payment and 1,000 USDC of global TVL;
  `get_operator("em")` has the capturer `em-capturer.arras-escrow.near`, 1300 bps, `em-fees.arras-escrow.near`, a
  0.01 USDC minimum, 1,000 USDC per operator, 20 per payer and 0.02 NEAR of sponsorship per payer;
- the capturer has exactly one key, the FunctionCall key for `release, refund, withdraw_for` with a 0.5 NEAR allowance;
- `view_access_key_list` of v1 is empty; `get_storage_health`: 269,634 bytes, 0.30 NEAR of free reserve.

The six live keys (parent, owner, guardian, the v2 reserve, the capturer and the fee recipient) went to SSM under the
names of section 2, each checked by sha256 without being shown, and the local files were deleted.

| Step | Transaction |
|---|---|
| parent `arras-escrow.near` | [`6X1ShTfEsUdmEFoneeLPqCSFCDmjytgJvSCis5yTQoNz`](https://nearblocks.io/txns/6X1ShTfEsUdmEFoneeLPqCSFCDmjytgJvSCis5yTQoNz) |
| `v1.` / `v2.` / `owner.` / `guardian.` / `em-capturer.` / `em-fees.` | [`MRGoLQRz...`](https://nearblocks.io/txns/MRGoLQRzPokkPm9HRaCnTPA2sP4BXySAs7he1onY2zC) / [`HcYCRfBP...`](https://nearblocks.io/txns/HcYCRfBPKWLihPQfZQJ6HzwGo3dtDN1CL3mkDzv8HNkE) / [`43eEWHe6...`](https://nearblocks.io/txns/43eEWHe6kGxRFMgmj7GXytoGywv3WQWSCg24cJjf1acS) / [`GqLGSr7J...`](https://nearblocks.io/txns/GqLGSr7JpWp9eHvutv3JgaAc6XaTzuBWSbjKxA2Txe35) / [`12enRGRe...`](https://nearblocks.io/txns/12enRGRe5ZPsRwh5X5UoP5Efm3hUUQe89WhxTRGkjhxf) / [`E17CkZjC...`](https://nearblocks.io/txns/E17CkZjChvRGrZkx4dEDGs7NkEimLuVcaow2L6FMkDjx) |
| USDC storage for v1 / for `em-fees.` | [`Cs9umRAX...`](https://nearblocks.io/txns/Cs9umRAXZNQ3gVynaiaVRSsqyCh8NCsPRiEPJWGjBMkF) / [`8naX3h87...`](https://nearblocks.io/txns/8naX3h87D9kYCWkCnfWVFp4zRQXi7dUuNtBxCP7pvMDA) |
| deploy + private `new` | [`248SUtwuSygSvfYjubUokvDWpAjgbKDQA6zPLXBDKg68`](https://nearblocks.io/txns/248SUtwuSygSvfYjubUokvDWpAjgbKDQA6zPLXBDKg68) |
| `register_operator em` / `fund_operator_storage` 0.5 NEAR | [`7iriz6GN...`](https://nearblocks.io/txns/7iriz6GNsyettoms72Z8SPj8DRocP4ctcKM1S4WxuM3F) / [`9YAfdppV...`](https://nearblocks.io/txns/9YAfdppVg2HYnyGemW6JoFxQ7RGFRdhbcwQzR6fAJder) |
| capturer: FunctionCall key / its FullAccess deleted | [`CazQDx1c...`](https://nearblocks.io/txns/CazQDx1cyYJNvHmL8ZTcJva4hryAXsBVwWFgDUJW5Nq3) / [`DF9cxXNy...`](https://nearblocks.io/txns/DF9cxXNyJjoqSbr5VwFbkdeHg7tR1zf3om1jqzzfVi3w) |
| **all keys of v1 deleted** | [`5gUfctsBnCUoCPoPMKqfbz4ZKVXKtYAnpUyUf2fjYzLj`](https://nearblocks.io/txns/5gUfctsBnCUoCPoPMKqfbz4ZKVXKtYAnpUyUf2fjYzLj) |

After the run, in the order of section 5: the x402-rs facilitator serves `v1.arras-escrow.near` (H-5), and
Execution Market's first payment through it was released on 2026-11-03 at 15:49Z (H-8; EVIDENCE.md, section 2).
`v2.arras-escrow.near` held no code until H-11, on 2026-11-08: v2 was deployed into it
([`FdPJxM9k...`](https://nearblocks.io/txns/FdPJxM9kKsPiXeYBkqZnqTqqHKSkUBM2pM1hUg4uQQ1m)) and left without keys
(section 8.8).

## 8. Arras v2 on mainnet: the runbook (H-11)

Status: **deployed on 2026-11-08** (block times 02:19:52Z to 02:21:28Z), with the smoke test right after (02:22:35Z to
02:25:06Z), from a clean checkout of `5800cb7`, after the testnet rehearsals of 2026-11-07 (8.6).
`v2.arras-escrow.near` has the pinned wasm and no access keys since
[`Bqc63GgB...`](https://nearblocks.io/txns/Bqc63GgBiTWfrs9nAVHzE9piPLtPsxj6EcPSU7fDmVqt); 8.8 has the run and its 31
transactions, and [EVIDENCE.md](EVIDENCE.md), section 6, the same with their signers. 8.1 to 8.7 are the runbook and
its testnet rehearsals.

`scripts/mainnet-v2.sh` keeps every guarantee of `scripts/mainnet.sh`: a dry run by default (no network call at all),
the flag of the same network (`--mainnet-ok` with `ARRAS_RED=mainnet`), no key ever printed, the existing keys read from
SSM through a pipe, the new keys handed to SSM by printed commands (`secretos`), and the contract account without keys
at the end. What both scripts share (arguments, networks, account rules, caps, transactions, RPC reads, the pinned
wasm, keys through a pipe) lives once in `scripts/arras-comun.sh`. v1 runs the same sequence, and its script changed
only around it, for example: `ARRAS_RPC` is no longer printed, the RPC reads retry, and integers with leading zeros are
refused. The 347 asserts of v1's cases in `scripts/test_mainnet.sh` are the same, and the only change to them is in
case t21, which runs a copy of the script and now copies `scripts/arras-comun.sh` next to it.

### 8.1 What goes on chain

| Fact | Value |
|---|---|
| Wasm | reproducible build of commit `ddc51211ed93973ec9eb537a85bac39367f6e763`, the merge of arras#6: the v2 that passed its money review (RARRAS-6). `contracts/escrow-v2` and `contracts/escrow-core` are the same in `main` |
| sha256 (hex) | `1541525c4bf3494ae5025b00df6955698975a6f8c376002f0296ff93b4e4fa77` |
| sha256 (base58, `view_account.code_hash`) | `2RyLHaUjZAXZzwEbnztZeJNB73wJqstjZpY7rtSVx44e` |
| Size | 366,339 bytes |
| Build | `sourcescan/cargo-near:0.21.1-rust-1.96.0@sha256:ccb22bb4...`, built on 2026-11-07 twice from two clean clones (`scripts/mainnet-v2.sh wasm` with two state directories): same sha256. The same environment (Linux under WSL) rebuilt v1's `8da7174` to `7058319d...0eada`, the pin built on a Mac on 2026-11-02: the build reproduces across machines. On 2026-11-08 the mainnet run rebuilt `ddc5121` on a Mac (`scripts/mainnet-v2.sh wasm`, 8.8): the same sha256 and 366,339 bytes |
| Account | `v2.arras-escrow.near`, reserved by `scripts/mainnet.sh` on 2026-11-03 (0.1 NEAR, one full-access key in SSM `/arras/mainnet/v2-reserva`) |
| `new` | the same as v1, from the same function (`comun_topes`): owner, guardian, native USDC, `tope_por_pago` **10 USDC**, `tope_tvl_global` **1,000 USDC** |
| Operator `em` | v1's values (fee 1300 bps, fee recipient `em-fees.arras-escrow.near`, `min_amount` 0.01 USDC, 1,000 USDC per operator, 20 per payer, 0.02 NEAR sponsorship per payer) plus v2's: capturer `em-capturer-v2.arras-escrow.near`, arbiter `em-arbiter-v2.arras-escrow.near`, arbiter window **7 days**, a `ticket_pubkey`, and a sponsorship pool of 0.1 NEAR |

**The operator ticket.** With a `ticket_pubkey`, every payment and every budget deposit in `em` needs Execution Market's
ed25519 signature (README, "Operator ticket"): until Execution Market signs tickets, nobody else can fill `em`'s caps or
drain its pool in v2. The `operador` step generates the key with OpenSSL 3 (outside the repository, mode 600) and
`secretos` hands it to `/arras/mainnet/em-ticket-v2`. When Execution Market points at v2 without signing tickets, the
owner removes it (1 yoctoNEAR):

```bash
near contract call-function as-transaction v2.arras-escrow.near update_operator json-args '{"operator_id":"em","patch":{"quitar_ticket_pubkey":true}}' prepaid-gas '30.0 Tgas' attached-deposit '1 yoctoNEAR' sign-as owner.arras-escrow.near network-config mainnet sign-with-access-key-file <the owner key> send
```

`ARRAS_V2_TICKET=no` registers `em` without a ticket from the start (the smoke then runs without one).

### 8.2 Accounts, signers and keys

| Role | Account | Exists? | NEAR | Key at the end | Name in SSM |
|---|---|---|---|---|---|
| Hub (funding) | `fe0a574258f8277099875e46d4ca0043cdb6274ff2501b375a0d3d64bc211079` | yes | sends what v2 and the parent lack | FullAccess | `/arras/mainnet/fondeo` (already there) |
| Parent | `arras-escrow.near` | yes | receives what the new children need | FullAccess | `/arras/mainnet/padre` (already there) |
| Contract v2 | `v2.arras-escrow.near` | yes, empty | code + state + a 0.25 free reserve | **none** | `/arras/mainnet/v2-reserva` (already there; dead after `sin-llaves`) |
| Owner | `owner.arras-escrow.near` | yes | its own 0.1 | FullAccess | `/arras/mainnet/owner` (already there) |
| Capturer of `em` in v2 | `em-capturer-v2.arras-escrow.near` | **new** | 0.15 | **only** a FunctionCall key: receiver v2, `release, refund, withdraw_for`, allowance 0.15 | `/arras/mainnet/em-capturer-v2` |
| Arbiter of `em` in v2 | `em-arbiter-v2.arras-escrow.near` | **new** | 0.1 | **only** a FunctionCall key: receiver v2, `resolve_dispute`, allowance 0.1 | `/arras/mainnet/em-arbiter-v2` |
| Fee recipient | `em-fees.arras-escrow.near` | yes (v1's, registered in USDC) | - | FullAccess | `/arras/mainnet/em-fees` (already there) |
| Operator ticket | an ed25519 key, not an account | **new** | - | - | `/arras/mainnet/em-ticket-v2` |
| Smoke: the agent | `agent-v2.arras-escrow.near` | **new** (smoke) | 0.06 | **only** a FunctionCall key: receiver v2, `authorize_from_budget`, allowance 0.06 | `/arras/mainnet/humo-agente-v2` |
| Smoke: the worker | `worker-v2.arras-escrow.near` | **new** (smoke) | 0.01 | FullAccess (it holds the USDC it is paid) | `/arras/mainnet/humo-worker-v2` |

Why a new capturer: `em-capturer.arras-escrow.near` has a single FunctionCall key whose receiver is v1, and without a
full-access key it cannot add another (`AddKey` needs one). The facilitator's lifecycle gate reads
`get_operator("em").capturers` of each contract, so each contract has its own capturer.

Each existing key is used from near-cli's legacy keychain when its file is there and its public key is live on chain
with the permission needed; otherwise from SSM. Either way it reaches near-cli through a pipe (`sign-with-access-key-file
/dev/fd/N`) and is checked against `view_access_key_list` before anything is signed. Who signs each step:

| Step | Signs | What |
|---|---|---|
| `fondeo` | hub | `send-near` to v2 (what it lacks for code, state, reserve and gas) and to the parent (what the new children need), only if the hub holds everything `todo` needs; nothing is sent otherwise |
| `cuentas` | parent | `create-account` of the capturer and the arbiter |
| `usdc` | hub | `storage_deposit` of v2 in USDC (and of the fee recipient, if it were missing) |
| `deploy` | v2 | deploy + private `new` in one transaction |
| `operador` | owner; hub | `register_operator em` (1 yocto); `fund_operator_storage` 0.1 NEAR |
| `capturer`, `arbitro` | each account, with its creation key | adds its FunctionCall key, then deletes its own full-access key |
| `sin-llaves` | v2 | deletes **all** its keys in one transaction |
| `verificar`, `plan`, `medir`, `enlaces` | nobody | RPC reads |
| `secretos` | c0der, AWS CLI | one printed line per live key (`put-parameter && compare && rm`); the script runs none |

### 8.3 How much NEAR, exactly

Measured on the testnet rehearsal of 8.6 (storage costs and the gas price are the same on both networks):

| Item | NEAR | Measured as |
|---|---|---|
| v2 storage after deploy, `new` and `register_operator em` (with ticket), keyless | **3.67245** | 367,245 bytes: the wasm (366,339) + 906 of state |
| v2 gas: deploy + `new` / delete its keys | 0.00284 / 0.00004 | balance of v2 before and after |
| creating one sub-account | 0.00711 | parent's balance, as in section 3 |
| a call with a FunctionCall key (`release`, `withdraw_for`, `resolve_dispute`, `authorize_from_budget`) | 0.0003 to 0.0006 | the allowance goes down by the gas burnt only: the gas refund also refunds the allowance |
| what a call reserves **up front** (pessimistic gas price), refunded right after | **0.0309 for 30 TGas** | the second rehearsal: the agent, with 0.0299 free, could not sign its 30 TGas `authorize_from_budget` (near-cli: "does not have enough balance ... to cover TX cost (0.0309...)"). Execution Market measured 0.151 for 150 TGas (EVIDENCE.md, section 5): about 0.00103 NEAR per TGas. Before signing any call the script checks the account's free NEAR, and its FunctionCall key's allowance, against 0.00105 per TGas plus the deposit, and says how to top it up |
| the hub's NEP-145 storage in v2 during the smoke | 0.0243 at the peak | 0.00495 for the budget + 2 payments; `revoke_agent` and `storage_withdraw` return all but the payments' 0.01939, which come back at `compact` |

`scripts/mainnet-v2.sh plan` prints the budget step by step. In a dry run it assumes the worst case (v2 with its 0.1, the
parent with no free NEAR); in a real run it reads the live balances. With the defaults, the dry run for mainnet says:

| Step | What | NEAR from the hub |
|---|---|---|
| `fondeo` | v2: wasm 366,339 + state 1,000 bytes + free reserve 0.25 + gas 0.006, minus the 0.1 it has | 3.83000 |
| `fondeo` | parent: capturer 0.15 + arbiter 0.1 + 0.01 per creation | 0.27000 |
| `fondeo` | gas of the hub's transfers | 0.00100 |
| `usdc` | `storage_deposit` of v2 in USDC | 0.00225 |
| `operador` | sponsorship pool of `em` in v2 | 0.10100 |
| **`todo`** | | **4.20425** |
| `humo` | parent: agent 0.06 (twice what a 30 TGas call reserves) + worker 0.01 + 0.01 per creation, plus the transfer | 0.09100 |
| `humo` | `storage_deposit` of the worker in USDC | 0.00225 |
| `humo` | the hub's NEP-145 deposit in v2 | 0.03100 |
| `humo` | gas of the hub's smoke calls | 0.00500 |
| `humo` | free NEAR at the 100 TGas `ft_transfer_call` (reserves ~0.103; comes back) | 0.15000 |
| **`humo`** | | **0.27925** |
| **total** | | **4.48350** |

The hub held 4.42292 NEAR at first on 2026-11-07, 0.06240 short for `todo` and the smoke together in the worst case;
the owner then sent 0.5 more, and c0der read **4.92292 NEAR** at 23:22:52Z (block 223533439), 4.92110 of them free after
its own 182 bytes: **`todo` and the smoke fit, with 0.43760 NEAR to spare in the worst case** (the parent with no free
NEAR); whatever the parent holds free is spare on top. The real `plan` prints `OK: alcanza` or the exact shortfall
(`FALTA: ...`) from the live balances, and `fondeo` sends nothing unless the hub holds everything `todo` needs. The
smoke also needs 0.10 USDC from the hub (it held 3.957303).

What stays where: the code, the state, the free reserve (0.2597 in the rehearsal) and the pool live in v2 forever (no
keys); the capturer's, arbiter's and agent's NEAR can only pay their gas (FunctionCall keys only); the parent keeps the
unused part of the 0.01 per creation.

### 8.4 The sequence (c0der, on the Mac)

```bash
git checkout <the merge of this PR> && git status --short           # a clean checkout
scripts/mainnet-v2.sh plan                                          # dry run: accounts, caps, wasm, budget (no network)
scripts/mainnet-v2.sh todo                                          # dry run of the whole sequence: every command, nothing executed
scripts/mainnet-v2.sh wasm                                          # local reproducible build of ddc5121 (Docker), checked against the pin
read -rs ARRAS_RPC && export ARRAS_RPC                              # the private mainnet RPC (mandatory, never printed), without echo: not in the history
export ARRAS_KMS_KEY=<Arras KMS key>                                # the KMS key of the SSM uploads (mandatory in any real run)
#   -> point near-cli's mainnet connection at that same private RPC: the transactions go through it
scripts/mainnet-v2.sh --mainnet-ok plan                             # live balances: "OK: alcanza" or the exact shortfall
scripts/mainnet-v2.sh --mainnet-ok todo                             # fondeo, cuentas, usdc, deploy, operador, capturer, arbitro,
                                                                    # sin-llaves, verificar, secretos, enlaces
#   -> run the printed `secretos` lines (each: upload to SSM && compare && rm)
scripts/mainnet-v2.sh --mainnet-ok humo                             # the smoke (8.5); it ends printing the `secretos` lines
#   -> run them (the agent's and the worker's keys to SSM)
scripts/mainnet-v2.sh --mainnet-ok verificar && scripts/mainnet-v2.sh --mainnet-ok enlaces
```

The ticket needs OpenSSL 3 (`/usr/bin/openssl` on macOS is LibreSSL): the script takes `ARRAS_OPENSSL`, the `openssl` of
the `PATH` or Homebrew's `openssl@3`, and checks it before sending anything. Every step is idempotent: run again after a
failure and it skips what is done (rehearsed: a second `todo` and a second `humo` sent no transaction); the pool is
funded once (a marker in the state directory), so a second `todo` does not send another 0.1 to a keyless account if
Execution Market spent it. The steps also run one by one (`fondeo`, `cuentas`, ... `humo-pago`, `humo-disputa`).

`ARRAS_RPC` is for the script's reads: it goes to `curl` through its config on stdin, never as an argument (`ps` would
show it). Do not type it into an `export` line of an interactive shell, which would leave it in the shell history:
`read -rs` above takes it without echo, or it can come from SSM through a pipe. The transactions go through the RPC of
near-cli's own config (`network-config mainnet`), so that connection has to point at the same private RPC before the
run; the script does not check this yet.

What the script refuses (tests in `scripts/test_mainnet.sh`, section v2): the other network's flag; mainnet without
`ARRAS_RPC`; `ARRAS_FONDEO_ARCHIVO` or `ARRAS_USDC_TESTNET` on mainnet; an account in two roles; a capturer, arbiter or
agent that is not a direct child of the parent; incoherent caps (the same checks as v1) and integers with leading zeros
(bash would read them as octal); an arbiter window outside `[60 s, 30 days]`; a free reserve under 0.2 NEAR; an
allowance of 0; smoke amounts outside 0.01 to 0.10 USDC per payment or above 0.20 in total; a key from SSM or the
keychain that is not live on chain with the permission needed; an OpenSSL other than 3 for the ticket; funding unless
the hub holds all of `todo`; a wasm whose sha256 or commit differs from the pins; deploying over other code or into an
unfunded v2; a fee recipient or a smoke worker with no full-access key (what they receive would be stuck); a
`get_config` or `get_operator` that differs in any field compared; deleting a full-access key before the FunctionCall
key is on chain with the exact methods, the configured allowance and its file in the keychain; a capturer or arbiter
left with any key but that one; `sin-llaves` on a v2 without the pinned code, the expected config or the pinned
NEP-330 commit, or with any key left; a storage health in alarm or a v2 not registered in USDC; the smoke before `todo`,
without the hub's USDC or the free NEAR it needs, over a different existing budget, with a ticket key that is not the
operator's, a payment saved without a valid `payment_id`, a `withdraw_for` whose USDC did not arrive, or a dispute that
did not return everything to the payer.

### 8.5 The smoke (security track)

`scripts/mainnet-v2.sh --mainnet-ok humo`, with explicit caps: `ARRAS_HUMO_MONTO` (0.05 USDC per payment, between 0.01
and `ARRAS_HUMO_TOPE_POR_PAGO`, which is 0.10 and cannot go higher) and `ARRAS_HUMO_TOPE_TOTAL` (0.20, the most the
smoke moves; it moves 2 x the amount). Everything goes directly through near-cli: no facilitator, no change to
`NEAR_ESCROW_CONTRACTS_MAINNET`.

| Part | Signs | NEAR from | USDC |
|---|---|---|---|
| `humo-cuentas`: the agent (one FunctionCall key: `authorize_from_budget`) and the worker | parent; each account | hub -> parent (0.06) | the hub pays the worker's USDC storage |
| `humo-presupuesto`: the hub's NEP-145 deposit in v2 (0.03), then `ft_transfer_call` of 0.10 with `msg` `budget` (caps 0.05 per payment, 0.10 per day) and the operator's budget ticket | hub; the ticket key signs the ticket | hub | **0.10 hub -> v2** (the agent's budget) |
| `humo-pago`: the agent authorizes 0.05 to the worker (`authorize_from_budget`, payment ticket); the capturer releases; `withdraw_for` the worker and the fee recipient | agent (FunctionCall key); capturer (FunctionCall key) | their own gas | **0.0435 v2 -> worker, 0.0065 v2 -> em-fees** |
| `humo-disputa`: the agent authorizes 0.05; the hub (the payer) opens a dispute; the arbiter gives everything back; `withdraw_for` the hub | agent; hub; arbiter (FunctionCall key); capturer | their own gas | **0.05 v2 -> hub** |
| `humo-cierre`: `revoke_agent`, `storage_withdraw` of the hub's free NEP-145 balance, the final state | hub | the hub gets its free storage back; the 0.01 to 0.02 NEAR that the two payments keep come back with `compact`, 7 days after their `reclaim_after` | v2 holds 0, `tvl_global` 0 |
| `secretos` (the end of `humo`) | nobody | - | the commands that hand the agent's and the worker's keys to SSM |

Net: the hub spends 0.05 USDC, the worker gets 0.0435 and the fee recipient 0.0065. Every transaction is recorded with its
label (`enlaces` prints the explorer links). The agent's key can only call `authorize_from_budget` on v2, and the
contract bounds it: per payment and per day caps of the budget, the 60-day horizon, `dispute_deadline` no earlier than
`release_deadline`, and the money stays in escrow until the capturer releases it.

### 8.6 Testnet rehearsal (2026-11-07): `arras-v2e-1107.testnet`

The mainnet layout was rebuilt on testnet first, with v1's script and a new implicit funding account (case a), then v2
went on top with `scripts/mainnet-v2.sh --testnet-ok todo` and `humo`, with the mainnet defaults of that run. In the
final script, two defaults come from what the second rehearsal below measured (8.3): the agent's NEAR is 0.06 and the
hub's free NEAR for the `ft_transfer_call` is 0.15. The token was a test
NEP-141 (`usdc-ensayo-1107.testnet`, `contracts/test-ft`, 6 decimals) through `ARRAS_USDC_TESTNET`, which only testnet
accepts: Circle's testnet faucet sits behind a reCAPTCHA. Circle's testnet USDC already went through v1 and v2 on testnet
(README). All runs exited 0; a second `todo` and a second `humo` sent no transaction (31 before and after).

| Step | Transaction |
|---|---|
| the funding account receives 9.95 NEAR from a faucet account | [`9dAgTtyR...`](https://testnet.nearblocks.io/txns/9dAgTtyRg4D7GVgVzT3PxuHrkQYKqbftVsU9Zs3ZVkcB) |
| v1's layout (`scripts/mainnet.sh --testnet-ok todo`): parent / deploy of v1 / all keys of v1 deleted | [`DshDtb18...`](https://testnet.nearblocks.io/txns/DshDtb18PGip8AfTL8Jyzdoc95H9JQRVF4UX2HWYYZ3E) / [`8N1g4oTk...`](https://testnet.nearblocks.io/txns/8N1g4oTkTg84fbk9SqUhiZMhP33sb7DModzik1NgVTBs) / [`89pduPmK...`](https://testnet.nearblocks.io/txns/89pduPmKE8Eynjm6zbN43pMaUvs88Who1LBEWUgaUCfy) |
| `fondeo`: hub -> v2 (3.835) / hub -> parent | [`J7nsXATC...`](https://testnet.nearblocks.io/txns/J7nsXATCRLeP36eXZ4zrM56eEU7QnjPTitvQuC7ciJ3L) / [`6we9jJws...`](https://testnet.nearblocks.io/txns/6we9jJwsfrNr3G4k3zaiMVoCZ34VhZMqyY2Ndemq8yVw) |
| `cuentas`: capturer / arbiter | [`Bhd5ubAm...`](https://testnet.nearblocks.io/txns/Bhd5ubAmFBwjX6V4wiY4M7tCu3MJDDWFM4QsnongnRfY) / [`HVFkBbXa...`](https://testnet.nearblocks.io/txns/HVFkBbXaRbvEzs7dBHCGdZgtr9yXfirNqbVWF4tCK2Fv) |
| `usdc`: v2 | [`HnRdxcjy...`](https://testnet.nearblocks.io/txns/HnRdxcjy4umMLtdxiJwpZ4Sw2jSNn3LUt8G77aGAEEw) |
| **deploy + private `new`** of the pinned wasm | [`Fn6bFEsY...`](https://testnet.nearblocks.io/txns/Fn6bFEsYjGX7VDQgA7zBZULasNaYL6FXTTbvwz9uVci4) |
| `register_operator em` (capturer, arbiter, 7 days, ticket) / pool 0.1 NEAR | [`HYYDCHEG...`](https://testnet.nearblocks.io/txns/HYYDCHEGVmEgjbqkZ7KyWG4oqaNryQGGnCmXMQK2touK) / [`2Zy4F6Ej...`](https://testnet.nearblocks.io/txns/2Zy4F6EjbyS4X7QaMFJ13YMnR4KcJtwecAVfaAXT6bZ4) |
| capturer: FunctionCall key / full-access deleted | [`dKXYo9D9...`](https://testnet.nearblocks.io/txns/dKXYo9D9p1EVX5DndMJeNLHN1ucUSHBAnVTHFWR4F7W) / [`5XSAeAJq...`](https://testnet.nearblocks.io/txns/5XSAeAJqUmr3GMskhn198e3etCdkMJZbrFdzrRTJfNT6) |
| arbiter: FunctionCall key / full-access deleted | [`5zCcr57S...`](https://testnet.nearblocks.io/txns/5zCcr57SLGcmZMEN97X8VbH6uV88jQBdwJUvaMEkYw8N) / [`CsqJBZuk...`](https://testnet.nearblocks.io/txns/CsqJBZukSn82U5JA1KQMc1iNGyqZvgUPJBY4wnwai138) |
| **all keys of v2 deleted** | [`Adc5vT9J...`](https://testnet.nearblocks.io/txns/Adc5vT9JcfDdFtm1Fe3dfgnV7z5Q9Pctwt9vTyxYFHvC) |
| smoke: parent funded / agent / worker / agent's FunctionCall key / its full-access deleted / worker's token storage | [`F45oTAp4...`](https://testnet.nearblocks.io/txns/F45oTAp4TkEG6ZVhb8kiak1URqnxkH7rVuQ9BZpqNdXt) / [`DhUJiPeX...`](https://testnet.nearblocks.io/txns/DhUJiPeXnszDmmc8hSd5ivCAp2jKRfaQAMbkMqUGmzJX) / [`2oKs2E6J...`](https://testnet.nearblocks.io/txns/2oKs2E6JCcJhvdhfbqo4QFuj3ke4KPRP6Hk916HhSghE) / [`HvWXWSwS...`](https://testnet.nearblocks.io/txns/HvWXWSwSzC36yyhNKFztSmPrS1JZWQYKhyYx41Haio7k) / [`DC9zmTdm...`](https://testnet.nearblocks.io/txns/DC9zmTdmEb6eYzPYp4hwj2yh9LKyFadCFGVYXqNjBTRb) / [`3UM963VB...`](https://testnet.nearblocks.io/txns/3UM963VBQeGc1uAvkbGRcbGNqSeTDBzA6gWawSrbg8Ry) |
| smoke: the hub's NEP-145 deposit / **the budget, 0.10 with the operator's ticket** | [`CLC1buDh...`](https://testnet.nearblocks.io/txns/CLC1buDhATBxKQ18PDa5AcLgKN6DV7SDUsKpPDCqcX9f) / [`L6yJ7EpQ...`](https://testnet.nearblocks.io/txns/L6yJ7EpQsWPr29x4dgEkv1Z4ZrQQWCHhecxjcK5Eq5s) |
| smoke, payment `5AkgphxK...`: **the agent's `authorize_from_budget`** / release / `withdraw_for` worker (0.0435) / fee recipient (0.0065) | [`3ejX8nyX...`](https://testnet.nearblocks.io/txns/3ejX8nyXzgpzeYy4ciyMVHgddpZLqSci7pxLctxSD3yk) / [`3YFBHjX3...`](https://testnet.nearblocks.io/txns/3YFBHjX3RYYij1jgFAoVtjKMrH2tUCrKPdku1aCRJ6GU) / [`8jLKWLxm...`](https://testnet.nearblocks.io/txns/8jLKWLxmWSzYvHF8Uy376vPEKGYReHKdWmR2BbU5uzWE) / [`2dYe6JE1...`](https://testnet.nearblocks.io/txns/2dYe6JE1J88hdPfSTgEpPsH2FRCCiAabUhnJ8jNUrYpW) |
| smoke, payment `HfTEaoU9...`: the agent's authorize / **the payer opens a dispute** / **the arbiter returns all of it** / `withdraw_for` the payer (0.05) | [`FR5L8nbj...`](https://testnet.nearblocks.io/txns/FR5L8nbjxipxcinYEtHCT7oZ1wH6CPn2qxDuooM3veYm) / [`HtvwdaFM...`](https://testnet.nearblocks.io/txns/HtvwdaFMbKDMjRNyTQDNfsaTeXFBNtX3i2N8gAR2rWyE) / [`6WgejNRV...`](https://testnet.nearblocks.io/txns/6WgejNRVyrT1Cc4Yugf8p1BNgJyfth8hDaLKT5rbo2Yy) / [`3eKe5Fun...`](https://testnet.nearblocks.io/txns/3eKe5FunRFccYe5dXMkcktGyzXJin28puVQxMPrG7zqa) |
| smoke: `revoke_agent` / `storage_withdraw` | [`Aue8sc1b...`](https://testnet.nearblocks.io/txns/Aue8sc1bqLwk74eYAD5Y1CXr1BNqfuuqS8SksM2HPWjJ) / [`A2zt75wB...`](https://testnet.nearblocks.io/txns/A2zt75wBvkszFRtCjGTddPoCwQ5bDEvTcqU1mhLDgNsE) |

**Second rehearsal, which finished with the final script (2026-11-07): `arras-v2f-1107.testnet`.** The same sequence from scratch,
with a new funding account, after the money review's two rounds. It is the one that found the up-front reserve of 8.3:
the agent's 0.03 (the default at that point) and then its key's 0.03 allowance could not cover a 30 TGas call, and the
script now refuses before signing and says how to top up; the defaults went to 0.06. It also went through two transient
testnet failures (`Transaction has expired` creating the capturer; a nonce clash from a second run started in
parallel by mistake, which also sent the deploy without recording its hash) and every rerun picked up where the chain
was. The smoke then ran with a second agent after `humo-cierre` revoked the first one's budget and returned its tokens.
Final state, read by `verificar`: code = pin, no keys, NEP-330 `ddc5121`, `em` as decided, one FunctionCall key on the
capturer and on the arbiter, v2 registered in the token, storage health `ok`; a second `todo` and a second `humo` sent
no transaction (37 before and after).

| Step | Transaction |
|---|---|
| funding account <- faucet account (9.95 NEAR) | [`9p3fRdtB...`](https://testnet.nearblocks.io/txns/9p3fRdtBwn4Eumqt8QoZ4BGAxCxFfZNvuPXG4Bxq68xK) |
| `register_operator em` / pool | [`Abg5Ck51...`](https://testnet.nearblocks.io/txns/Abg5Ck51qW8JwXKprtb5iwpu4MQayoiArGzYNEv3YbXn) / [`93rwzk15...`](https://testnet.nearblocks.io/txns/93rwzk15vi78uB4qrHuxWrfYFdt2dt5ck3BMgXq34i39) |
| **all keys of v2 deleted** | [`5dCFenxU...`](https://testnet.nearblocks.io/txns/5dCFenxU55zFnfXt7rqPQMw3TZ2Zeq4ZHZjMbLYqCwp4) |
| the first agent's budget revoked (`humo-cierre`) / its tokens back | [`9DSmoHxy...`](https://testnet.nearblocks.io/txns/9DSmoHxyVbCFUFyZbXW3gSszbEoTprN4DnSdExwgqvNT) / [`CyLj4hNX...`](https://testnet.nearblocks.io/txns/CyLj4hNXrYpQMWqBmyie9rPPvKZ4cWxWNXgzEBe1MH2f) |
| smoke with `agent2-v2.`: budget / `authorize_from_budget` / release / `withdraw_for` worker and fee recipient | [`4owzNUGm...`](https://testnet.nearblocks.io/txns/4owzNUGmgLRmzQWbT1rsZXa2goegt4DvmUZwBkKgHMeD) / [`5qdG4RFg...`](https://testnet.nearblocks.io/txns/5qdG4RFgHg7DsRNBbuFodzUmNCC6ycHV4Rz6zhCKXMSB) / [`4pC7pNed...`](https://testnet.nearblocks.io/txns/4pC7pNedUL26GEcKUVr98zmtX2gwKTReDiJ2kbmXJdDS) / [`Ec7myWtq...`](https://testnet.nearblocks.io/txns/Ec7myWtqKeEHVTGEBkvsnSC5LoXZUZLowuRRj3A53xgw), [`24BW5pn1...`](https://testnet.nearblocks.io/txns/24BW5pn14RLg2FNpwcb6n7Z9XZuVkZ9a6kgGmYCsEWGt) |
| smoke: second payment / dispute / the arbiter returns it all / `withdraw_for` the payer / `revoke_agent` | [`58qnhcbg...`](https://testnet.nearblocks.io/txns/58qnhcbgobWjET4VXnNdYezcb1AouTsHkj85mQDYNADv) / [`J92daCvY...`](https://testnet.nearblocks.io/txns/J92daCvYjbadCm3s2Duvooi7nFPPpXsja8nvE7DppkH2) / [`BwGJQvrq...`](https://testnet.nearblocks.io/txns/BwGJQvrqdqVbyJd1iWcfx4om4kzquPViB6hoA4hip4c6) / [`eMvvwgyG...`](https://testnet.nearblocks.io/txns/eMvvwgyGCRDrgfWzetZCEAS7Qhr5xMs8SNCSZ6jjFxD) / [`Ad2m3w9L...`](https://testnet.nearblocks.io/txns/Ad2m3w9LJ1J7s1tiuRbvMUSj4yNQrKde7RSHCKHZihbv) |

What `verificar` read at the end of the first rehearsal: code `1541525c...fa77` (= pin), no keys on `v2.arras-v2e-1107.testnet`, NEP-330
`git+https://github.com/UltravioletaDAO/arras?rev=ddc51211...`, `get_config` `arras-v2` with v1's caps, `get_operator em`
as decided, one FunctionCall key each on the capturer and the arbiter, `get_storage_health` `ok: true` with 0.2597 NEAR
free. After the smoke: `tvl_global` 0, `budgets_total` 0, 2 payments (Released, Resolved), the contract holds 0 tokens;
the worker holds 0.0435, the fee recipient 0.0065, the funding account got its 0.05 back.

### 8.7 If a step fails, and how to undo it

Every step can be run again after fixing the cause; this is what undoes each one:

| Step | Undo (before `sin-llaves`, while v2 still has a key) | After `sin-llaves` |
|---|---|---|
| `fondeo` | the NEAR sent to v2 comes back with `delete-account v2.arras-escrow.near beneficiary <hub>`, signed with `/arras/mainnet/v2-reserva` (the name can be created again); the NEAR sent to the parent stays in the parent | v2's NEAR is locked for good |
| `cuentas` | the capturer and the arbiter can be deleted with their own full-access key (`delete-account ... beneficiary arras-escrow.near`) until `capturer`/`arbitro` remove it | the owner points `em` at other accounts with `update_operator` (`capturers`, `arbiter`) |
| `usdc` | nothing to undo (0.00125 NEAR of storage in USDC) | same |
| `deploy` | `delete-account v2.arras-escrow.near` returns all of v2's NEAR to the hub; deploy again into a new reserved account | none: pause with the guardian, lower the caps with the owner, deploy a new version in a new account (no `migrate`) |
| `operador` | the owner edits `em` with `update_operator` (capturers, arbiter, window, fee, `min_amount`, sponsorship, ticket); caps only go down; the pool cannot be withdrawn | same |
| `capturer` / `arbitro` | a FunctionCall key cannot be widened: replace the account with `update_operator` | same |
| `sin-llaves` | irreversible by design | - |
| `secretos` | the script runs nothing; each printed line deletes a file only after the upload and the comparison succeed | - |
| `humo` | a payment stopped half-way follows its own path: the capturer can `refund` it, the payer can dispute it, `reclaim` after `reclaim_after`, `withdraw_budget`/`revoke_agent` return the budget, `withdraw_for` moves any `claimable`; running `humo` again continues each payment from its state | same |

### 8.8 The mainnet run (2026-11-08)

c0der ran the runbook of 8.4 (as it stood at `5800cb7`) on 2026-11-08 from a clean checkout of `5800cb7` (the merge
of arras#10, `main` at the time), on a Mac, with `ARRAS_RPC` (a private mainnet RPC) and `ARRAS_KMS_KEY`. The
script's reads and the transactions went through that private RPC: c0der pointed near-cli's mainnet connection at it
before the run (the script does not check this yet, 8.4). In this order, every command with exit 0:

```bash
scripts/mainnet-v2.sh wasm                   # reproducible build of ddc5121: sha256 1541525c...fa77, 366,339 bytes = the pin
scripts/mainnet-v2.sh --mainnet-ok plan      # "OK: alcanza; sobran 0.49560 NEAR"
scripts/mainnet-v2.sh --mainnet-ok todo      # 02:19:40Z to 02:21:38Z by the script's clock: 13 transactions
#   -> the printed `secretos` lines (each: upload to SSM && compare && rm)
scripts/mainnet-v2.sh --mainnet-ok humo      # 02:22:29Z to 02:25:16Z by the script's clock: 18 transactions
#   -> the printed `secretos` lines of the smoke (the agent's and the worker's keys)
scripts/mainnet-v2.sh --mainnet-ok verificar && scripts/mainnet-v2.sh --mainnet-ok enlaces
```

31 transactions, block times 02:19:52Z to 02:25:06Z (blocks 223552046 to 223552597), every one with a successful final
status and signed by the expected account (read through the RPC).

What `verificar` read on chain at the end, and what anyone can check now (README, "For NEAR HACKATHON judges"). The
script stops if the code, the NEP-330 commit, v2's keys, the compared fields of `get_config` and `get_operator`, the
capturer's or the arbiter's keys, or the `ok` of the storage health differ; the pool and the free reserve it only
prints, and the `ticket_pubkey` it compares key for key only when the ticket's key file is on disk (otherwise, its
`ed25519:` form):

- the code of `v2.arras-escrow.near` is the pin: `code_hash` `2RyLHaUjZAXZzwEbnztZeJNB73wJqstjZpY7rtSVx44e` (the base58
  of `1541525c...fa77`), NEP-330 `git+https://github.com/UltravioletaDAO/arras?rev=ddc51211ed93973ec9eb537a85bac39367f6e763`;
- `get_config`: `arras-v2`, owner `owner.arras-escrow.near`, guardian `guardian.arras-escrow.near`, the native USDC
  `17208628f84f5d6ad33f0da3bbbeb27ffcb398eac501a31bd6ad2011e36133a1`, 10 USDC per payment, 1,000 USDC of global TVL, not
  paused;
- `view_access_key_list` of v2 is empty (since the 13th transaction, `sin-llaves`);
- `get_operator("em")`: fee 1300 bps, fee recipient `em-fees.arras-escrow.near`, `min_amount` 0.01 USDC, capturer
  `em-capturer-v2.arras-escrow.near`, arbiter `em-arbiter-v2.arras-escrow.near`, a 7-day arbiter window,
  `ticket_pubkey` `ed25519:9zh27mQwBQbKGYixfqUVEU4xNtz93Sad8crhhFH4B9YF`, a sponsorship pool of 0.1 NEAR;
- the capturer and the arbiter have one key each, their FunctionCall key for v2;
- `get_storage_health` `ok`: 0.2546 NEAR of free reserve (the minimum is 0.2).

| Step | Transaction |
|---|---|
| `fondeo`: hub -> v2 / hub -> parent | [`yuVjiZTx...`](https://nearblocks.io/txns/yuVjiZTxbEUfWuqHUKinVsB3rKToUBPgZA1Pdq7Rjwn) / [`HsbGxrzk...`](https://nearblocks.io/txns/HsbGxrzkZbSth6FzzixfQXmgE12kEqBX4mzfVzbGNjJP) |
| `cuentas`: em-capturer-v2 / em-arbiter-v2 | [`HxgECiVj...`](https://nearblocks.io/txns/HxgECiVjU44bzzBt1d4C5Y4vaWb9kunRnbUEfLfx1W6p) / [`54gmUtdY...`](https://nearblocks.io/txns/54gmUtdYBubEV9fNayvmExvq5SLjbJfW1rv35eLhNPUS) |
| `usdc`: v2 | [`9Je5r1PU...`](https://nearblocks.io/txns/9Je5r1PUoSP63CUpw4J7xo6UYqcTcNe8su7Ed2w9jMkc) |
| **deploy + private `new`**, 02:20:36Z | [`FdPJxM9k...`](https://nearblocks.io/txns/FdPJxM9kKsPiXeYBkqZnqTqqHKSkUBM2pM1hUg4uQQ1m) |
| `register_operator em` / pool | [`yAisZKJt...`](https://nearblocks.io/txns/yAisZKJtbNJBW1RFTQpaQLn3tXZXEW3LHQSDtr1KGHf) / [`F8HLKAn5...`](https://nearblocks.io/txns/F8HLKAn5NkZ9rwo1DezHBDTGmUrA3CLwL5tpPmqAiGTM) |
| capturer: FunctionCall key / full-access deleted | [`BzjsSt8V...`](https://nearblocks.io/txns/BzjsSt8VTSziSoU1vQUrnXWZRjwZukBL1yXyUd4c39EQ) / [`4XZ6hcZn...`](https://nearblocks.io/txns/4XZ6hcZnqheABpYQJzHKseoQA2CjgFj55VuTtEXsp1o6) |
| arbiter: FunctionCall key / full-access deleted | [`AAYdP4jS...`](https://nearblocks.io/txns/AAYdP4jS1PpAYdjMHYS9t9rcA3MkYcP8SHAsGFQhHXS4) / [`FTPgCfva...`](https://nearblocks.io/txns/FTPgCfvaYGNVn1UadTmW4mijneuV97oDQF2RtgS7xg6i) |
| **all keys of v2 deleted**, 02:21:28Z | [`Bqc63GgB...`](https://nearblocks.io/txns/Bqc63GgBiTWfrs9nAVHzE9piPLtPsxj6EcPSU7fDmVqt) |
| smoke: parent funded / agent / worker / agent's FunctionCall key / its full-access deleted / worker's USDC storage | [`EQhHFMnd...`](https://nearblocks.io/txns/EQhHFMndwVj1TZfs7R4vWeb8nH8mNavbzABxr5GiLKr6) / [`6bzSzsyB...`](https://nearblocks.io/txns/6bzSzsyB8dGYd7ukT9qm2ZuU27U6tUgXixWupPJFai4W) / [`DCDv7YjH...`](https://nearblocks.io/txns/DCDv7YjH6A94jNLcH7fQ5t19b9SGJhytbzhie1jRYEH5) / [`De7DhA1X...`](https://nearblocks.io/txns/De7DhA1XE4PYTGdE8pj7BKVuN6axueLcsXKc7rAJNMtT) / [`5CQxZ8P1...`](https://nearblocks.io/txns/5CQxZ8P1hiPGXqwziW8pk9RKAQq9DdvFJ4ZNqm24RXKp) / [`2fVqngWP...`](https://nearblocks.io/txns/2fVqngWP4bKx9dPMe8hJH1E2Z4JCG93kne9RVCKyanYY) |
| smoke: the hub's NEP-145 deposit / **the budget, 0.10 USDC with the operator's ticket** | [`4Ncxu59b...`](https://nearblocks.io/txns/4Ncxu59bnuXJHCNZzBtBxMNRZbU1vAWHB8QGJXQqSXNT) / [`8izehMAh...`](https://nearblocks.io/txns/8izehMAhAmaYyJy75mpG884LwCY96C7XHnoS4cJWNZGV) |
| smoke, payment `9nySy6vT...`: **the agent's `authorize_from_budget`** / release / `withdraw_for` worker (0.0435) / fee recipient (0.0065) | [`3Y9cAjyf...`](https://nearblocks.io/txns/3Y9cAjyfgsVEFqeFuwxVMZgxvyAD8AHaDt6LKcy28JTi) / [`52KbQt7q...`](https://nearblocks.io/txns/52KbQt7q9aDJyWJDEcTnLc1uCcVdqaCVnyk7LeZueurw) / [`62tyfGbW...`](https://nearblocks.io/txns/62tyfGbWHhYgLXey5makZhXZLRmFkB2tYhNEcWhJEfpv) / [`9wV2pfNp...`](https://nearblocks.io/txns/9wV2pfNpeQFjwV9vdTYP2cvvTNoW9SLjjNLYGQbT6kwr) |
| smoke, payment `8428jJdx...`: the agent's authorize / **the payer opens a dispute** / **the arbiter returns all of it** / `withdraw_for` the payer (0.05) | [`FrbHGrPR...`](https://nearblocks.io/txns/FrbHGrPRn6ZwYrr4UZcVzpBST73zn5QUi7Yx6qZ79Ty1) / [`2PXjnXi7...`](https://nearblocks.io/txns/2PXjnXi7hQb4MhrjgvPgKDMPx5HBZQLCBoXXUCRqS8wK) / [`6VsgyjUj...`](https://nearblocks.io/txns/6VsgyjUj64hbk4kMFinyniJkyunJ1s3aZj4bdacNouKa) / [`6Kaca22T...`](https://nearblocks.io/txns/6Kaca22TyYbrpJkkcwNYqYb2pUR4pMKfYMVDERVLAS52) |
| smoke: `revoke_agent` / `storage_withdraw` | [`Cc75eHk1...`](https://nearblocks.io/txns/Cc75eHk1vMFAfAs1jsPRwrQCrVwCWkBXgs9ttdi5gch2) / [`3QsUMq9V...`](https://nearblocks.io/txns/3QsUMq9Ve5UhwHPRjCQLYzZk2x94oZKzhZdEbbHXHcWw) |

After the smoke: `get_config` reads `tvl_global` 0, `in_flight` 0, `budgets_total` 0 and `payments` 2, and
`get_payment` the first one `Released` and the second `Resolved` with `refunded` 50000; the worker got 0.0435 USDC, the
fee recipient 0.0065 (the 13 %), and the hub the 0.05 of the disputed payment back, each by `withdraw_for`. Balances at
the end, measured by c0der: the hub 0.67095 NEAR and 3.907303 USDC (the smoke cost it 0.05 USDC net); v2 4.04621 NEAR
(3.69078 of them storage); the capturer 0.14756 NEAR, the arbiter 0.09963, the agent 0.05902, the worker 0.01. The
smoke is the team's: the hub is the project's funding account, and the agent and the worker are test accounts made for
it. Execution Market and the facilitator did not change (8.9).

### 8.9 Order with the facilitator and Execution Market

Status: **all three steps done**, the last two on 2026-11-13, after `payment-proof.arras-escrow.near` was deployed
(9.6). Since then Execution Market pays its NEAR tasks through `v2.arras-escrow.near`
([EVIDENCE.md](EVIDENCE.md), section 8).

1. **H-11 (this):** v2 deployed, keyless, verified, and the smoke run directly with near-cli. The facilitator is not
   needed for any of it. Done on 2026-11-08 (8.8); at that point the facilitator's `NEAR_ESCROW_CONTRACTS_MAINNET`
   still listed v1 only, and Execution Market still used v1.
2. **x402-rs (one batch, when Execution Market is about to use v2):** `NEAR_ESCROW_CONTRACTS_MAINNET=v1.arras-escrow.near,v2.arras-escrow.near`.
   Its lifecycle gate reads `get_operator("em").capturers` on v2: `["em-capturer-v2.arras-escrow.near"]`.
   That capturer starts with 0.15 NEAR of balance and of allowance (v1's started with 0.5): its FunctionCall key
   cannot sign a call that attaches more than about 142 TGas (the up-front reserve of 8.3), and the allowance lasts
   about 200 calls of ~0.0005 NEAR (each call also needs its up-front reserve in the allowance) when the capturer signs
   its own transactions (for v1, section 3 counts none of the allowance through the facilitator's NEP-366 relay, and in
   [EVIDENCE.md](EVIDENCE.md) the relayer `uvd-facilitator.near` sends the `release` and `withdraw_for` of v1's
   capturer and pays their gas: its row in the accounts table, and section 5). Before pointing Execution Market at v2,
   measure the gas that the facilitator attaches to `release` and `withdraw_for` against this key; if the key does not
   fit, the capturer is changed with `update_operator` (8.7).
   **Done on 2026-11-13:** x402-rs 2.57.2, live from about 03:58Z; its `/supported` lists `nearEscrow.contracts`
   `["v1.arras-escrow.near","v2.arras-escrow.near"]`. The capturer of `em` in v2 is still
   `em-capturer-v2.arras-escrow.near`, with its FunctionCall key for `release`, `refund` and `withdraw_for` on v2.
3. **Execution Market:** `escrow_contract` `v2.arras-escrow.near`, `operator_id` `em`, the capturer key from
   `/arras/mainnet/em-capturer-v2`, and either tickets signed with `/arras/mainnet/em-ticket-v2` (SDK, vectors T-1/T-2)
   or the owner's `update_operator` without the ticket (8.1). Removing the ticket does not let anyone rate these
   payments without Execution Market: `payment-proof.arras-escrow.near` (section 9) does not count the one path that
   releases money with neither its capturer nor its arbiter, a dispute that expired in the receiver's favour (9.2,
   `E16`).
   **Done on 2026-11-13, without tickets:** the owner's `update_operator` of 8.1 removed `em`'s ticket key
   ([`7UQvpqRS...`](https://nearblocks.io/txns/7UQvpqRSyUScR94x1vztQznVdLAMG2x7WzyvgK2ko2bK), 04:00:34Z, signed by
   `owner.arras-escrow.near`; `get_operator("em")` answers `"ticket_pubkey": null`), and Execution Market pays its
   NEAR tasks through `v2.arras-escrow.near` and rates them with `payment-proof.arras-escrow.near`, with uvd-x402-sdk
   0.114.0 (Python) and 2.116.0 (TypeScript). Its full cycle there, between the team's test accounts: task
   `a219f4db`, Arras payment `2oxnrLfuZH95KJhcn9sSqjK4hE3BjpJHgB2upwYF7gs6`, 0.10 USDC
   ([EVIDENCE.md](EVIDENCE.md), section 8).

## 9. The payment-proof verifier (ERC-8004): the plan

Status: **deployed on 2026-11-13** from the merge commit `40eba87`, with the sequence of 9.3 (9.6); 9.1 to 9.5 are
the plan as it was written before the run. It is the proof verifier
that lets a task paid through the Arras escrow contract at `v2.arras-escrow.near` be rated on chain with the payment
as the proof. The Reputation Registry takes the verifier as a parameter of `give_feedback_with_proof`, so
nothing changes in `reputation-registry.erc-8004.near`, and `erc8004-verifier.arras-escrow.near` keeps verifying the
payments of `v1.arras-escrow.near`. The interface is the same: `verify_feedback_proof({"client", "proof": <payment_id>})`
returns `{"counterparty", "role", "context", "weight"}` or fails with the same `ERC8004_ARV_*` codes plus one,
`ERC8004_ARV_E16` (9.2), which the other verifier does not need because its escrow has no disputes; `get_config` has
the same fields (`"contract": "erc8004-arras-verifier"` is the name of that interface; `"escrow"` tells the two
verifiers apart).

### 9.1 What goes on chain

| Fact | Value |
|---|---|
| Crate | `contracts/erc8004-payment-proof` (immutable: private `new` in the deploy transaction, no `migrate`, no owner, no state but the escrow) |
| Wasm | reproducible build of the merge commit of its PR (NEP-330 embeds the commit, so the merge gives its own sha256). The code of the PR, built twice from two clean clones of `3b4191e` (the last commit that changes the crate) on 2026-11-12: sha256 `e746a335a157452e3d407b7fee2cc7f49f4506b1a2941b77fe1cbb42e7c0aaf0`, 125,345 bytes, both clones identical |
| Build | `sourcescan/cargo-near:0.21.1-rust-1.96.0@sha256:ccb22bb4...`, the same image as every other crate here |
| Account | `payment-proof.arras-escrow.near`, a new child of `arras-escrow.near` (parent key in SSM `/arras/mainnet/padre`). c0der read on 2026-11-11 (block 224120662) that it does not exist yet |
| `new` | `{"escrow": "v2.arras-escrow.near"}` |
| The escrow it trusts | `v2.arras-escrow.near`: keyless since [`Bqc63GgB...`](https://nearblocks.io/txns/Bqc63GgBiTWfrs9nAVHzE9piPLtPsxj6EcPSU7fDmVqt) (8.8), code hash `2RyLHaUjZAXZzwEbnztZeJNB73wJqstjZpY7rtSVx44e` (8.1); c0der read both again on 2026-11-11 at 21:35Z (block 224120662): 0 keys, the same code hash. A verifier trusts whoever controls its escrow, so step 0 reads them once more |

### 9.2 What counts as a proof

The verifier reads `get_payment(proof)` and fails closed unless the answer is exactly a `Payment` of that escrow: its
19 fields and the 4 of its `dispute` (no field more or less, `dispute` and `resolved_at` present with `null` when
empty, no repeated key), canonical integers, 64-character lowercase hex hashes, one of the 7 statuses, the status
consistent with `dispute` (an open dispute only in `Disputed`, a resolved one only in `Resolved`, none otherwise) and
`released + refunded <= amount`. Then, as for the other verifier: `released > 0` in any status, `payer != receiver`,
and the rater is the payer or the receiver; and, new here, the money was not released by a dispute that expired in the
receiver's favour (`E16`). The order is `E11` (not exactly a `Payment`, or inconsistent), `E13`, `E16`, `E14`,
`E15`. `weight` is `released`, before the fee (as in the other verifier); `context` is the `operator_id`.

**Disputes count for what the operator released; an expired dispute in the receiver's favour does not (`E16`).**
`released` only grows, through three calls: `release` (only a current capturer of the operator, before
`release_deadline`), `resolve_dispute` (only its current arbiter, within the arbiter's window) and
`resolve_expired_dispute` (anyone, once the window is over: what is still in escrow goes to the receiver if the payer
opened the dispute before `release_deadline`, and back to the payer otherwise). Refund, reclaim and opening a dispute
do not touch it, each increase is credited to the receiver (and the fee to the fee recipient) for good, and a payment
has at most one dispute (it can only be opened on a live payment, and `Resolved` is final). So:

- a `Disputed` payment counts for what a capturer released before the dispute was opened;
- a `Resolved` one that the arbiter settled counts for what reached the receiver in total;
- a `Resolved` one whose dispute expired in the receiver's favour fails with `E16`, even for what a capturer had
  released before, because the `Payment` does not tell the two apart. It is the only path that releases money with no
  one at the operator acting: someone paying their own second account opens a dispute before `release_deadline`,
  waits out the window, and anyone closes it;
- a `Resolved` one whose dispute expired in the payer's favour (the receiver opened it, or the payer did at or after
  `release_deadline`) counts for what a capturer released before, or fails with `E13`.

A dispute that returned everything to the payer, with nothing released before, is `released == 0` and fails (`E13`),
like a refund. The verifier reads the expiry off the `Payment`: both calls that close a dispute set `resolved_at` to
the block time, the arbiter only while it is below `opened_at + arbiter_window_ns` (the window frozen in the payment,
with the escrow's saturating sum) and the expiry only from there on. So `resolved_at >= opened_at + arbiter_window_ns`,
with `opened_by` `Payer` and `opened_at < release_deadline`, is exactly the expiry that paid the receiver, the equality
included. The unit tests run the escrow's own code on both sides of that edge, one nanosecond apart, and WP-03 runs the
built escrow in the sandbox.

**The cost, plainly.** A worker whose payer disputed the payment before `release_deadline`, and whose arbiter did not
answer within its window, gets paid but cannot rate with that payment, and neither can the payer, even if a capturer
had released part of it before. That is the price of "verified" meaning that the operator or its arbiter acted; with
an arbiter that answers within its window, there is no cost. While the arbiter does not answer, a payer can use it to
keep a worker from rating them, at the price of the whole payment going to that worker. Disputed payments are not
excluded otherwise: either party can open a dispute while the payment is live, and excluding them would let one party
stop the other from ever rating it.

So a proof with `context` `em` needs Execution Market's capturer or arbiter to have released the money, with or
without the operator's `ticket_pubkey` (8.1): the ticket is not a precondition of this verifier.

How long a payment can serve as proof: until anyone compacts it, 7 days after `reclaim_after` (or after the dispute's
resolution, if later). A rating already given stays: the Reputation Registry keeps the proof it verified.

### 9.3 The sequence (c0der)

The reads go through the private RPC, and near-cli's mainnet connection has to point at it too (as in 8.4). Followed
on 2026-11-13 with `REV` = `40eba87e9d4fb76e75e4292c9ea68feb07a58272` (9.6).

```bash
# 0. The escrow it trusts, once more (mandatory): no keys and the pinned code.
near account list-keys v2.arras-escrow.near network-config mainnet now
#    -> an empty table
near account view-account-summary v2.arras-escrow.near network-config mainnet now
#    -> "Local Contract (SHA-256 checksum hex)" 1541525c4bf3494ae5025b00df6955698975a6f8c376002f0296ff93b4e4fa77
#       (the pin of 8.1; base58 2RyLHaUjZAXZzwEbnztZeJNB73wJqstjZpY7rtSVx44e) and no access keys; anything else: stop
near account view-account-summary payment-proof.arras-escrow.near network-config mainnet now
#    -> the account does not exist (if it does, stop: the name is taken)
near contract call-function as-read-only v2.arras-escrow.near get_operator json-args '{"operator_id":"em"}' \
  network-config mainnet now
#    -> write down in the deploy log its "ticket_pubkey" (ed25519:9zh27mQwBQbKGYixfqUVEU4xNtz93Sad8crhhFH4B9YF in 8.8,
#       or null if the owner removed it), "arbiter" and "arbiter_window_ns". Not a precondition: with or without the
#       ticket, the verifier does not count a dispute that expired in the receiver's favour (9.2).

# 1. Reproducible wasm (NEP-330), TWICE from TWO clean clones of the merge commit, WITHOUT CARGO_TARGET_DIR
#    (cargo-near 0.22.0 deletes the artifact when it is set). Docker, with the image of the crate's Cargo.toml.
REV=<MERGE_SHA>
ORIGEN="$(git -C <your arras checkout> rev-parse --show-toplevel)"
git -C "$ORIGEN" fetch -q origin && git -C "$ORIGEN" cat-file -e "$REV^{commit}"     # the merge commit is there
for n in 1 2; do
  D="$HOME/arras-build/payment-proof-$REV-$n"
  git clone -q "$ORIGEN" "$D" && git -C "$D" checkout -q "$REV" && test -z "$(git -C "$D" status --porcelain)"
  ( cd "$D" && env -u CARGO_TARGET_DIR cargo near build reproducible-wasm \
      --manifest-path contracts/erc8004-payment-proof/Cargo.toml )
done
shasum -a 256 "$HOME"/arras-build/payment-proof-"$REV"-{1,2}/target/near/erc8004_payment_proof/erc8004_payment_proof.wasm
W="$HOME/arras-build/payment-proof-$REV-1/target/near/erc8004_payment_proof/erc8004_payment_proof.wasm"
wc -c < "$W"
#    -> the same sha256 twice (otherwise stop); it is the code hash of step 4. The size should be close to 125,345
#       bytes; up to 134,249 bytes fit in the funding of 9.4.

# 2. The parent pays the account (9.4) plus ~0.01 NEAR of gas.
near account view-account-summary arras-escrow.near network-config mainnet now

# 3. The account; its new key goes to near-cli's legacy keychain (steps 4, 5 and 6 use it).
near account create-account fund-myself payment-proof.arras-escrow.near '1.4 NEAR' autogenerate-new-keypair \
  save-to-legacy-keychain sign-as arras-escrow.near network-config mainnet \
  sign-with-access-key-file <(aws ssm get-parameter --region us-east-2 --name /arras/mainnet/padre \
    --with-decryption --output json | jq -j '.Parameter.Value') send

# 4. Deploy + private `new` in ONE transaction (nobody else can call `new`).
near contract deploy payment-proof.arras-escrow.near use-file "$W" with-init-call new \
  json-args '{"escrow":"v2.arras-escrow.near"}' \
  prepaid-gas '30.0 Tgas' attached-deposit '0 NEAR' network-config mainnet sign-with-legacy-keychain send

# 5. Verify BEFORE deleting the key.
near account view-account-summary payment-proof.arras-escrow.near network-config mainnet now
#    -> "Local Contract (SHA-256 checksum hex)" is the sha256 of step 1, and it has one access key: the one of step 3
near contract call-function as-read-only payment-proof.arras-escrow.near get_config json-args '{}' \
  network-config mainnet now
#    -> {"contract":"erc8004-arras-verifier","escrow":"v2.arras-escrow.near","min_gas_verify":"25000000000000",
#        "gas_get_payment":"10000000000000","gas_on_payment":"10000000000000"}
near contract call-function as-read-only payment-proof.arras-escrow.near contract_source_metadata json-args '{}' \
  network-config mainnet now
#    -> "link": "https://github.com/UltravioletaDAO/arras/tree/<REV>", "version": "0.1.0", build_info with the image
#       by digest and the path contracts/erc8004-payment-proof, standards erc8004 "1.0.0" and nep330 "1.3.0"

# 5b. Smoke, mandatory before deleting the key (no deposit; it writes no state): the two payments of the smoke of 8.8,
#     on chain until their compaction: anyone can compact them from about 2026-11-17 02:23Z (reclaim_after at +2 days,
#     plus the 7 days of retention), so deploy before that or use others. The verifier signs its own calls (its key,
#     its NEAR: ~0.0005 per call). The hub is their payer, and worker-v2 their receiver.
H=fe0a574258f8277099875e46d4ca0043cdb6274ff2501b375a0d3d64bc211079
near contract call-function as-read-only v2.arras-escrow.near get_payment \
  json-args '{"payment_id":"9nySy6vTK95bgPUm5MkdsZtzC9XaGrSCV1EjNnRR6uyC"}' network-config mainnet now
#    -> "status": "Released", "released": "50000", "payer": $H, "receiver": "worker-v2.arras-escrow.near"
#    If it is null (compacted), stop and pick another payment of v2.arras-escrow.near with "released" > 0 (list_payments
#    of em), with its payer as the client; for the E13 check, a "Resolved" (or "Disputed") one with "released": "0",
#    so that the mirror reads a real dispute.
near contract call-function as-transaction payment-proof.arras-escrow.near verify_feedback_proof \
  json-args "{\"client\":\"$H\",\"proof\":\"9nySy6vTK95bgPUm5MkdsZtzC9XaGrSCV1EjNnRR6uyC\"}" \
  prepaid-gas '50.0 Tgas' attached-deposit '0 NEAR' sign-as payment-proof.arras-escrow.near \
  network-config mainnet sign-with-legacy-keychain send
#    -> {"counterparty":"worker-v2.arras-escrow.near","role":"receiver","context":"em","weight":"50000"}
near contract call-function as-transaction payment-proof.arras-escrow.near verify_feedback_proof \
  json-args "{\"client\":\"$H\",\"proof\":\"8428jJdxtdVfJwrMKc7ircLp1Yr6D5nra5e16ZbRNS8w\"}" \
  prepaid-gas '50.0 Tgas' attached-deposit '0 NEAR' sign-as payment-proof.arras-escrow.near \
  network-config mainnet sign-with-legacy-keychain send
#    -> fails with ERC8004_ARV_E13: the Resolved payment whose dispute returned everything to the payer
#    No payment on mainnet exercises E16 (none had a dispute expire in the receiver's favour): the unit tests against
#    the escrow's code and WP-03 in the sandbox do.
#    If either answer differs, the key stays and nothing is deleted. A first payment of the real cycle (an agent
#    rated with `give_feedback_with_proof` and this verifier) is the golden flow of the consumers (9.5); a rating is
#    permanent, so it is not part of this smoke.

# 6. No keys (irreversible: the verifier becomes immutable). Every key that list-keys prints, comma-separated.
near account list-keys payment-proof.arras-escrow.near network-config mainnet now
near account delete-keys payment-proof.arras-escrow.near public-keys '<the keys of list-keys>' \
  network-config mainnet sign-with-legacy-keychain send
near account list-keys payment-proof.arras-escrow.near network-config mainnet now
#    -> an empty table
# 7. The file of the deleted key in near-cli's legacy keychain is useless now: delete it.
```

**Testnet first (optional, the same commands with `network-config testnet`).** The escrow of the testnet rehearsal,
`v2.arras-v2e-1107.testnet`, is keyless and runs the pinned wasm (8.6): `new({"escrow": "v2.arras-v2e-1107.testnet"})`
on a new account under a testnet parent of c0der's, and the smoke of 5b with its two payments,
`5AkgphxKw9ghWReKiJ5j1uUWE9ytPoxm1e6PGUKTzZHe` (Released) and `HfTEaoU9bnXG6CpfxUWvhr1rV8RTEbU5v12ir6CsKwF6` (Resolved
with everything back to the payer), until their compaction (from about 2026-11-16); their payer is the rehearsal's
funding account (README, "Arras v2 on testnet").

### 9.4 How much NEAR

Measured in the sandbox (WP-07, the wasm of `compile_project`): the account without keys holds the wasm plus 164
bytes of account and state, with the sandbox's escrow `arras.test.near` (15 characters); with `v2.arras-escrow.near`
(20) it is 169. With the reproducible wasm of `3b4191e` (125,345 bytes) that is 125,514 bytes, 1.25514 NEAR, and the
funding is that plus 0.1 NEAR, rounded up to the tenth, as for the other verifier: **1.4 NEAR**, from the parent.

The 0.1 covers the key while it is on (82 bytes), the gas burnt by the deploy (~0.003 NEAR), by the two smoke calls
(~0.0005 each: 5.5 TGas measured per `verify_feedback_proof`) and by `delete-keys`, and the up-front reserve of each
50 TGas smoke call (~0.052 NEAR, 8.3; it comes back), which has to fit in what is left free: about 0.14 NEAR with the
wasm of `3b4191e`. That bounds the wasm at (1.4 - 0.003 - 0.052) x 100,000 - 169 - 82 = **134,249 bytes**; above
that, the funding goes up by a tenth. Creating the account costs the parent ~0.0071 more (section 3). What stays: the
storage, for good (the account has no keys). The verifier needs nothing else: it holds no state per call and takes
no deposit.

### 9.5 What the consumers change

- **One verifier per escrow, in this order.** A payment of `v1.arras-escrow.near` keeps being rated with
  `erc8004-verifier.arras-escrow.near`, and one of `v2.arras-escrow.near` is rated with
  `payment-proof.arras-escrow.near`; the other verifier answers `E12` (no such payment) and the Reputation Registry
  returns the deposit, so a rating sent to the wrong one is lost without an error. The SDKs (Python and TypeScript)
  replace their single verifier and escrow per network with that map first; Execution Market moves its escrow to
  `v2.arras-escrow.near` after them, and keeps rating the `v1.arras-escrow.near` payments still within their window
  with the other verifier. The exact lines of each consumer are in this PR's description.
- **One new failure code, `ERC8004_ARV_E16`** (9.2): a dispute that expired in the receiver's favour. Through
  `give_feedback_with_proof` it is one more failed proof (the registry logs its `E45` rejection and returns the
  deposit); a direct `verify_feedback_proof` transaction fails with it. The Python SDK lists the codes it knows
  (`ARV_E01..E15`) and treats any other as an unrecognized failure, so it adds `E16`; the TypeScript SDK accepts any
  two-digit code. The lines are in this PR's description.
- **Trust a verifier by its account, never by its `get_config`.** `contract` is `erc8004-arras-verifier` in both (the
  name of the interface: the same fields and the same `ERC8004_ARV_*` codes, plus `E16` here), so the checks the SDKs
  make on that field accept the new one unchanged; but anyone can deploy a contract that answers the same
  `get_config`. What makes a verifier trustworthy is the account in the consumer's map, keyless, with the code hash of
  9.3.
- **Reading reputation.** `get_verified_summary` and `list_verified_feedback` take one verifier: a view of an agent
  adds the two (with the same client filter and `context`) or shows them apart.
- **Who rates a budget payment.** With `authorize_from_budget` the payer is the budget's owner, not the agent that signs
  it: the agent's account is not a party (`E15`), and the owner or the receiver rates (the agent's key can only call
  `authorize_from_budget` anyway). When the receiver rates the payer's side, the counterparty is that owner, so the
  agent being rated has to be owned by that account or have it as its `agentWallet` (otherwise the registry answers
  `E47`).

### 9.6 The mainnet run (2026-11-13)

The sequence of 9.3, followed on 2026-11-13 with `REV` = `40eba87e9d4fb76e75e4292c9ea68feb07a58272`, the merge of
arras#15 (the PR of `contracts/erc8004-payment-proof`). Read through an RPC node by the deployer:

| Fact | Value |
|---|---|
| Wasm (step 1) | reproducible build of `40eba87`: sha256 `ea430d5fbb3bbe3b59adf5c2cd7010e00f295e6be5a37eee935c560c5431e5a0`, 125,345 bytes, the size of 9.1 (the sha256 differs from that of `3b4191e` because NEP-330 embeds the commit) |
| Account `payment-proof.arras-escrow.near` (step 3) | [`Goz3q7wcLtVBUnXKScKAy7JUM4D8DJLavM4PTXrTC6FU`](https://nearblocks.io/txns/Goz3q7wcLtVBUnXKScKAy7JUM4D8DJLavM4PTXrTC6FU) |
| Deploy + private `new({"escrow":"v2.arras-escrow.near"})` in one transaction (step 4) | [`8MpL3idDcMS9BaKTTnpbEHVGHBQKyyxg3hvu4TU93nyN`](https://nearblocks.io/txns/8MpL3idDcMS9BaKTTnpbEHVGHBQKyyxg3hvu4TU93nyN) |
| Access keys at the end (step 6) | none |

Then, in the order of 8.9, the facilitator listed `v2.arras-escrow.near`, the owner removed `em`'s ticket key in v2,
and Execution Market started paying through v2 and rating with this verifier (8.9, steps 2 and 3;
[EVIDENCE.md](EVIDENCE.md), section 8). The read-only checks of the verifier are in the README ("Check it yourself")
and in EVIDENCE.md, section 8.
