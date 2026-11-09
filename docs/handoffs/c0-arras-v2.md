# Handoff: c0-arras-v2 (H-9), Arras v2

Rama `0xultravioleta/c0-arras-v2`, desde `origin/main` 7312a9d. Worker de la Mac, 2026-11-02/03.

## Que se hizo

- `contracts/escrow-v2`: el contrato v1 (mismo comportamiento) mas lo nuevo del diseno de Arras v2 (README, "Arras v2:
  agent budgets, disputes and operator tickets"):
  - **presupuesto de agente**: `msg` `budget` en `ft_transfer_call`; `authorize_from_budget` sin deposito (lo firma
    una function-call key limitada al contrato y a ese metodo); topes por pago y por dia UTC; `withdraw_budget`,
    `revoke_agent`, `set_budget_caps` (1 yocto). El pago del agente queda en escrow con `payer = owner`: lo libera el
    capturer del operador y un refund o reclaim vuelve al owner, nunca al agente;
  - **disputa**: `open_dispute` (payer o receiver, antes de `dispute_deadline`), `resolve_dispute` (el arbitro actual
    del operador, dentro de su plazo congelado en el pago), `resolve_expired_dispute` (cualquiera, vencido el plazo:
    lo vivo va a la parte que no abrio la disputa, salvo que la haya abierto el payer en o despues de
    `release_deadline`: entonces vuelve al payer); `dispute_deadline >= release_deadline` (`E67`);
  - **`operator_ticket`** ed25519: con `ticket_pubkey` en el operador, todo pago y todo deposito de presupuesto lo
    necesita (F-5: griefing del pool de storage y del TVL).
- Tests: unitarios (los de v1 portados y los nuevos), proptest U-16 de v2 (10.000 secuencias, con presupuestos,
  disputas y retiros intercalables; "lo que entra es lo que sale mas lo retenido"), near-workspaces W2-01..W2-10.
- `docs/vectores.json`: seccion `v2` (diff solo aditivo; los vectores de v1 no cambian).
- `scripts/testnet-v2.sh` y el recorrido en el README ("Arras v2 on testnet").

## Apartes del diseno de Arras v2 (aceptados por c0der en RESPUESTA del 2026-11-03 00:08Z)

1. **Mensaje del ticket con dominio y contrato**: `borsh("arras:v2:operator_ticket", cuenta_del_contrato, sujeto,
   expiry)`; el diseno decia `borsh(payment_id, expiry)`. Sin dominio ni contrato, un ticket valdria en otro contrato
   que confie en la misma llave. Vectores T-1/T-2, recomputados con OpenSSL.
2. **Ticket tambien en el deposito de presupuesto y en `authorize_from_budget`** cuando el operador tiene llave; si
   no, el griefing de F-5 entra por el presupuesto.
3. **Clave del presupuesto sin `token`**: v2 es de un solo token fijo en `new`, como v1.
4. **`dispute_deadline` opcional** (vale `reclaim_after`): un `msg` de v1 es un `msg` valido de v2 con el mismo
   `payment_id`; el SDK de v1 sirve tal cual y EM cambia la direccion por configuracion.

Decidido en lo que el diseno dejaba abierto: codigos `E47`, `E48`, `E50`..`E57`, `E60`..`E67` con nombre; plazo del
arbitro congelado en el pago al autorizar, en `[1 minuto, 30 dias]`; arbitro rotable; storage de la disputa (86
bytes) cobrado al autorizar y devuelto al compactar si no se uso; la pausa no frena disputas, `withdraw_budget` ni
`revoke_agent`.

## Auto-refutacion (antes del PR)

Un refutador adversarial (opus/xhigh) en dos rondas. Ronda 1: 1 P1, 1 P2, 9 P3. Arreglados:

- **P1-1:** el agente elegia los plazos del pago por el owner sin horizonte: con `reclaim_after = u64::MAX` la plata del
  owner quedaba trabada para siempre (solo el capturer o el receiver elegido por el agente la movian). Ahora
  `reclaim_after <= now + 60 dias` en `authorize_from_budget` (`E57`). Fue 30 en la ronda 1; la ronda 2 (P2-N1) mostro
  que EM arma `reclaim_after = max(now, deadline) + 7 d + 7 d` con tareas de hasta 30 dias (hasta 44 dias): 60.
- **P2-1:** faltaba el test del storage exacto del deposito patrocinado.
- **P3-1:** una `ticket_pubkey` de orden chico (el host verifica con `verify`, no `verify_strict`) aceptaba una firma
  fija para cualquier mensaje. Ahora `E48` para los 8 puntos de torsion y sus formas no canonicas.
- **P3-2:** con `min_amount = 0` el agente creaba pagos de 0 que gastaban el storage del owner. Ahora `E24`.
- **P3-5:** `get_storage_health` contaba como libre la reserva de disputa cobrada y sin usar.

Ronda 2: los cinco arreglos cierran; un P2 nuevo (P2-N1, el horizonte de 30 dias dejaba afuera las tareas de EM de mas
de 16 dias: subido a 60) y dos P3 (P3-N1: la reserva de la disputa ahora baja 78 bytes al abrirla y 8 al resolverla;
P3-N2: la llave publica de los vectores, cuya semilla esta publicada, se rechaza como `ticket_pubkey`). El mutante M41
(la firma sin re-codificar) es equivalente: base58 tiene una sola codificacion por arreglo de bytes (400.000 entradas).

Declarados (sin cambio de codigo): P3-3 (con un `msg` de v1 el receiver puede disputar hasta `reclaim_after - 1` y
congelar el reclaim del payer el plazo del arbitro, consecuencia del aparte 4), P3-4 (`dispute_deadline` no esta en el
`payment_id` ni en el ticket), P3-6 (el storage de un presupuesto patrocinado vuelve al pool solo con `revoke_agent`),
P3-7 (la ventana es el dia UTC: alrededor de medianoche el agente gasta hasta 2x el tope diario, como dice el diseno),
P3-8 (la regla del vencimiento es la del diseno; el encargo dice "al ultimo estado acordado"; refinada despues por
RARRAS-6 P2-1, abajo), P3-9 (`ticket_pubkey`
se muestra como `ed25519:<b58>`; se acepta tambien sin prefijo).

## Refutacion de c0der: RARRAS-6, ronda 1 (2026-11-03)

Veredicto sobre `179a843`: MERGEABLE CON RONDA (1 P1, 1 P2, 4 P3). Informe: `ref-rarras-6/ENTREGA-c0der.md`.

- **P1-1:** el PR chocaba con `main` en `README.md` desde el PR #5. Merge de `origin/main` (`ea5869f`) conservando los
  dos lados (Status, "Try it", "Repository").
- **P2-1 (decision de c0der):** vencida la disputa, el receiver cobraba pagos que nadie aprobo si el payer la abria
  despues de `release_deadline` (el capturer ya no podia liberar) o si el owner disputaba un pago de la llave del agente
  filtrada. Ahora, si la abrio el payer con `opened_at >= release_deadline`, lo vivo vuelve al payer. Disputar antes
  de `release_deadline` sigue costando lo vivo. El `reclaim` del owner esta garantizado si no disputa, o si disputa
  desde `release_deadline` (README y `lib.rs`).
- **P3-1 (decision de c0der):** el agente elegia `dispute_deadline = now + 1` y le cerraba la disputa al owner. Ahora
  `dispute_deadline >= release_deadline` en los dos caminos (`E67_DISPUTA_ANTES_DE_RELEASE`, despues de `E11` y antes
  de `E12`). Un `msg` de v1 (sin `dispute_deadline`) no cambia.
- **P3-2:** el ticket de presupuesto vale para cualquier cantidad de depositos del par hasta `expiry`: una linea en el
  README, `operator_ticket.reuso` en `docs/vectores.json` y el test `rarras6_p3_2_...`.
- **P3-3:** "30 dias" -> "60 dias" en `unit_tests/refutacion.rs`. **P3-4:** la fila del pre-CI en la tabla del PR.
- U-16 verifica la regla del vencimiento por rama, con una operacion nueva (el payer disputa desde `release_deadline`);
  la cobertura exige las tres ramas.

Mutantes de los dos arreglos (a mano, en el worktree, revertidos con `git checkout`; proptest a 300 casos): 9 de 9
matados. MA1 (`opened_at >= release_deadline` por `>`) lo mata el borde exacto; MA2 (la regla vieja), MA3 (al
reves), MA4 (la hora de la resolucion en vez de la de apertura) y MA5 (`dispute_deadline` en vez de `release_deadline`)
caen en los tests nuevos y en U-16. MB1 (`E67` rechaza el borde igual), MB2 (sin `E67`), MB3 (acepta
`release_deadline - 1`) y MB4 (`E67` despues de `E12`) caen en los tests de guardas y de orden.

## Leccion que queda en el repo

**Tocar `arras-escrow-core` cambia la wasm de v1 aunque v1 no use nada nuevo.** Medido: agregar un modulo `v2` al
core llevo la build no reproducible de v1 de `b505ec3d...8211` (273.213 bytes) a `623796c9...174a` (272.899): con LTO y
`codegen-units = 1`, mas llamadores de una funcion del core cambian lo que se inlinea. Por eso v2 no toca el core
(`git diff origin/main -- contracts/escrow-core contracts/escrow-v1` = 0 lineas) y lo que v2 suma vive en su crate.
Cualquier cambio futuro al core es un cambio a la wasm de v1 y necesita su refutacion.

## Testnet

- `arras-v2-1103.testnet` (faucet, sin llaves): wasm reproducible de `c416989`, sha256 `8ef765f9...d66a` (dos builds
  iguales). Recorrido completo con USDC de Circle (`scripts/testnet-v2.sh 8`-`13`): el contrato termino en 0. Tablas
  de transacciones y saldos en el README, "Arras v2 on testnet".
- Es el despliegue previo a la refutacion de c0der (H-10). Respecto del HEAD del PR, el codigo del contrato difiere en
  tres cosas: el texto de `E57` (decia "30 dias" con el tope ya en 60), la regla del vencimiento de RARRAS-6 P2-1 y la
  guarda `E67` de P3-1. El recorrido terminaria igual con el codigo nuevo (la disputa vencida del paso 11 se abrio antes
  de `release_deadline`; el `msg` del paso 10 tiene `dispute_deadline == release_deadline`). Como en v1, el commit refutado va a una cuenta nueva y sin
  llaves despues del MERGEABLE (pasos 1-7 de `scripts/testnet-v2.sh` con otro `ARRAS_V2_CONTRATO`).
- La llave de tickets del operador `emt` vive en `~/.arras-testnet/v2-arras-v2-1103.testnet-ticket.pem` de la Mac (modo
  600, fuera del repo). Las sub-cuentas `capturer`, `arbiter` y `agent` de `arras-v2-1103.testnet` tienen sus llaves en
  el llavero de near-cli-rs; el capturer y el agente, solo function-call keys.

## Para despues del merge (no lo hace este worker)

- SDK TS y py: firmar el ticket del operador con T-1/T-2 (`docs/vectores.json` -> `v2.operator_ticket`) y los helpers
  de `msg` `budget`, `authorize_from_budget` (VB-1) y disputa. c0der lo anota en el backlog.
- Facilitador (x402-rs): `validate_budget_authorize` y el ciclo de vida de v2 (`open_dispute`, `resolve_dispute`,
  `resolve_expired_dispute`, `withdraw_budget`) con su gate por rol; gas medido en W2-09.
- H-10 (refutacion de v2) y H-11 (v2 en mainnet en su cuenta, sin llaves; EM apunta a v2 por configuracion).
