# Arras v1 on NEAR mainnet: the runbook (H-3)

Status: **deployed on 2026-11-03** (02:30:11Z to 02:32:30Z by the script's clock; block times 02:30:39Z to
02:32:19Z), with the one command of section 4, after the testnet rehearsals of section 6. `v1.arras-escrow.near` has
the pinned wasm and no access keys; section 7 has the run and its transactions, and [EVIDENCE.md](EVIDENCE.md) the
mainnet transactions since. The text below is the runbook as it was written before the run (2026-11-02/03).

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
| `new` | owner, guardian, token, `tope_por_pago` **10 USDC**, `tope_tvl_global` **1,000 USDC** (the owner's 2026-10-26 decision "Tope global de 1.000", later than P-6; `new` is the ceiling forever, `set_caps` only lowers) |
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

v1 has **no payer allow-list** (removed in the 2026-10-26 prescription by the owner's 2026-10-22 rule against
allow-lists; v1 is immutable). Who may pay is decided outside the contract. The on-chain limits are the caps above,
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
Execution Market's first payment through it was released on 2026-11-03 at 15:49Z (H-8; EVIDENCE.md, section 2). H-11
has not happened: `v2.arras-escrow.near` still holds no code (its only transaction is its creation), so v2 runs on
testnet only.
