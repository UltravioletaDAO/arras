# Handoff c0-arras-mainnet (H-3, preparacion): v1 lista para mainnet sin gastar NEAR de mainnet

2026-11-02/03, worker c0-arras-mainnet (task_000b042ab22b). Nada toco mainnet: ni una cuenta creada ni una firma.
Lecturas de mainnet (solo vistas por RPC): `storage_balance_bounds` y `ft_metadata` del USDC, y que
`arras.ultravioletadao.near`, `v1.` y `v2.` no existian (UNKNOWN_ACCOUNT, 2-nov 23:47Z). Aplica la RESPUESTA de c0der
del 2-nov 23:56Z (Q1-Q6) y el dato de la implicita de fondeo (`MENSAJE-20261103T000016Z-c0der.md`).

## Que quedo

| Pieza | Donde | Estado |
|---|---|---|
| Build reproducible de v1 | `scripts/mainnet.sh wasm` (clon limpio de 8da7174 en `~/.arras-despliegue/build-<rev>`) | sha256 `7058319d...0eada`, medido tres veces el 2-nov: el mismo de `arras-v1b-1101.testnet` |
| Script de mainnet | `scripts/mainnet.sh` | en seco por defecto; real solo con la bandera de la red y `ARRAS_KMS_KEY`; idempotente; padre por `ARRAS_PADRE`: (a) `arras-escrow.near` desde la implicita de fondeo (defecto), (b) sub-cuenta del dueno |
| Tests del script | `scripts/test_mainnet.sh` | sin red: dobles de `near`, del RPC y de `aws`; corre con el bash 3.2 de macOS y con el 5.x |
| Ensayo caso (a) | `arras-escrow-1102.testnet` (implicita `904e...ccee`) | un solo comando (`ensayo`), exit 0, 19 tx: create_account desde la implicita, hijas, deploy con 10/1.000, operador 1.000/20, capturer, sin llaves, authorize, release, withdraw |
| Ensayo 1 (faucet) | `arras-ensayo-1102.testnet` | el primero, paso a paso, con los topes viejos (200); la prueba de idempotencia |
| Runbook | `docs/MAINNET.md` | NEAR exactos (medidos; el dueno fondea la implicita con 4,55), quien firma, nombres en SSM, orden con el facilitador y EM |

## Lo que corre c0der (cuando el dueno fondee)

1. `scripts/mainnet.sh wasm` (Docker, ~3 min) y `scripts/mainnet.sh todo` en seco.
2. El dueno manda **4.55 NEAR** a `fe0a574258f8277099875e46d4ca0043cdb6274ff2501b375a0d3d64bc211079`.
3. `ARRAS_KMS_KEY=<la llave KMS de Arras> scripts/mainnet.sh --mainnet-ok todo`: lee la llave de fondeo de SSM
   `/arras/mainnet/fondeo` (us-east-2) por un pipe, crea `arras-escrow.near` con 4.52 y sigue hasta `enlaces`.
4. Las lineas que imprime `secretos`: una por llave viva, `put-parameter && comparacion de sha256 && rm` (si la
   subida o la comparacion fallan, no se borra nada).
5. H-5: `NEAR_ESCROW_CONTRACTS_MAINNET=v1.arras-escrow.near,v2.arras-escrow.near`.

## Decisiones aplicadas (RESPUESTA de c0der)

Q1 v1 tal cual, sin lista de payers. Q2 TVL global 1.000 y por operador 1.000 (su techo queda fijo en
`register_operator`: `contracts/escrow-v1/src/admin.rs:35`, y `update_operator` solo lo baja: `admin.rs:88-96`, `E42`);
por payer 20 y por pago 10. Q3 SSM SecureString; `ARRAS_KMS_KEY` obligatoria en real. Q4 `ARRAS_PADRE`, casos (a) y
(b). Q5 8da7174. Q6 patrocinio 0,02; el payer de EM hace su propio `storage_deposit` (pasado a c0-em-near-pagos).

## Llaves del ensayo (testnet, cuentas de desarrollo)

En el llavero legacy de esta Mac, `~/.near-credentials/testnet/<cuenta>/` (archivos 600): los padres
`arras-ensayo-1102.testnet` y `arras-escrow-1102.testnet` y sus `v2.`, `owner.`, `guardian.`, `em-capturer.` (solo
la FunctionCall key sigue viva) y `em-fees.`; las de `v1.` estan muertas (borradas en cadena). La implicita del ensayo
(`904e...ccee`, 0,37 NEAR) esta en `~/.near-credentials/testnet/fondeo-ensayo/`. `ARRAS_RED=testnet ARRAS_PADRE=<padre>
ARRAS_KMS_KEY=<llave> scripts/mainnet.sh --testnet-ok secretos` imprime los comandos para pasarlas a SSM o borrarlas.
El estado de cada ensayo (txs y saldos por paso) esta en `~/.arras-despliegue/testnet/<padre>/`.

## Refutador (opus/xhigh, ~277K tokens) y lo que cambio

P1: `sin-llaves` corrido solo borraba las llaves de v1 sin mirar el codigo ni la config (irreversible); ahora exige el
sha256 del pin y `get_config`. P2: fee_recipient = capturer (la tarifa quedaba atrapada); los `rm` de `secretos` sin
compuerta (ahora una linea encadenada por llave); ocho guardas sin test (ahora con test). P3 aplicados: allowance > 0 y
comparada en cadena, `chain_id` del RPC, el llavero sale de la config de near-cli, `die` dentro de `<(...)` en bash 3.2,
errores de aws visibles, `rm` comun, `sponsor_max_por_payer` en `operador_ok`, el mensaje de ARRAS_PADRE, la nota de v2
en la lista del facilitador.

## Trampas que encontre

- `die` dentro de `$(...)` solo cierra la subshell: el script seguia con un valor vacio (un RPC caido daba la cuenta por
  registrada en USDC). Arreglado: `die` le manda TERM al proceso principal; test T14b.
- Editar el script mientras corre lo rompe (bash lee por offset): paso con el paso `wasm` en este worktree. Arreglado:
  todo corre dentro de `principal`, ya leida, con el `exit` en la misma linea; test T21.
- `correr` del arnes devolvia su propio estado (0) en vez del exit del script cuando corria en segundo plano: T21 no
  veia el `syntax error` del mutante M31. Arreglado (`return "$RC"`).
- `secretos` listaba para subir la FullAccess muerta del capturer (borrada en cadena) junto a la FunctionCall key, con
  el mismo nombre. Arreglado: en real cada archivo se cruza con `view_access_key_list`; test T15d.
- near-cli 0.30.1 pide confirmacion (y falla sin TTY) al mandar NEAR con `tokens send-near` a una cuenta que no existe,
  como una implicita sin fondos; `transaction construct-transaction ... add-action transfer` no pregunta.
- `cargo near build` (cargo-near 0.22.0) y near-cli-rs 0.30.1 se migran el `config.toml` uno al otro (V5 -> v4 -> V5):
  sin efecto visible.
- Los tests del sandbox sin red necesitan `NEAR_SANDBOX_BIN_PATH` (el build script de near-workspaces descarga el
  binario); anotado en el README.
- La creacion de una sub-cuenta quema ~0,0071 NEAR (0,00703 en el receipt de la cuenta nueva); la del padre por
  `create_account`, 0,00745.
