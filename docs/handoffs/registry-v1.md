# Handoff: c0-arras-registry, Arras registry v1 (identidad y reputacion de agentes en NEAR)

Rama `0xultravioleta/c0-arras-registry`, desde `origin/main` ddc5121. Worker de la Mac, 2026-11-03.

El pedido del dueno (2026-11-03 22:47Z): "Solo un registro en NEAR" y "identity + reputacion". Un agente con billetera
NEAR ya contrata en Execution Market y paga por Arras en mainnet, pero el worker no lo podia calificar porque la
reputacion de EM vive en Base (ERC-8004, solo EVM). Este contrato es la forma de ERC-8004 en NEAR, con la reputacion
atada a pagos de Arras que de verdad liberaron plata.

## Que quedo

- `contracts/registry-v1` (crate `arras-registry-v1`): un contrato con dos partes.
  - **Identidad.** La identidad es la propia cuenta NEAR (no se acunan NFT ni ids). `register(agent_uri, metadata?)`
    da de alta a la cuenta que llama; `update(agent_uri, metadata?)` reemplaza la ficha. Cada uno paga su storage con
    el deposito adjunto: se mide con `env::storage_usage()` y vuelve el sobrante; un `update` que achica devuelve lo
    liberado.
  - **Reputacion.** `give_feedback(escrow, payment_id, score, tag1?, tag2?, feedback_uri?, feedback_hash?)` llama a
    `escrow.get_payment(payment_id)` y verifica **en el callback**: el pago existe; libero plata (`released > 0`,
    cualquiera sea su estado: tambien un `Refunded` o `Reclaimed` despues de una liberacion parcial); quien llama es el
    payer o el receiver; la calificada es la otra parte (y no es quien llama). Una sola calificacion por `(escrow, payment_id, rater)`. `escrow` tiene que estar
    en la lista fija de `new` (contratos, no personas: el registro no tiene lista de usuarios ni aprueba a nadie; los
    operadores de v1 si los aprueba el owner de v1, ver "Modelo de confianza").
    `revoke_feedback(escrow, payment_id)` la marca (no se borra) y deja de contar.
  - Vistas: `get_config`, `get_agent`, `list_agents`, `get_feedback`, `get_feedback_for_payment`, `list_feedback`
    (las recibidas, paginadas), `get_summary` (cantidad, promedio, la ultima, y separado por rol),
    `get_summary_by_operator` (lo mismo contando solo los pagos de un operador, p. ej. `em`) e `is_feedback_pending`.
  - Eventos NEP-297 `arras-registry` 1.0.0: `agent_registered`, `agent_updated`, `feedback_given`, `feedback_revoked`.
  - Sin `migrate` ni owner: se despliega con `new` privado en la misma tx y se le borran las llaves.
- `contracts/test-escrow` (crate `arras-test-escrow`): escrow de mentira **solo para el sandbox**: `get_payment`
  devuelve el JSON crudo que se le carga (para lo que v1 no puede dar: payer == receiver, JSON roto, mas de 8 KiB,
  respuestas que no son exactamente un `Payment` de v1); `emitir` deja un log y puede fallar despues (WR-10). No se
  despliega nunca.
- `contracts/registry-v1/tests/fixtures/escrow-v1-mainnet-7058319d.wasm` (RONDA 1): la wasm exacta de
  `v1.arras-escrow.near`, que c0der bajo de mainnet con `view_code` (268.839 bytes, sha256 `7058319d...0eada`, que en
  base58 es el code hash `8ZYfMJerz9r1mh4vZH55vyyJb1SLyX7qR2xqpYPxHr5X`). WR-02, WR-03 y WR-04 corren contra ella
  ademas de contra la v1 compilada; el test verifica el sha256 antes de usarla y el code hash despues de desplegarla.
- `Cargo.toml` (miembros nuevos) y `Cargo.lock` (las dos crates nuevas y, desde la RONDA 1, `sha2` 0.10.9, que ya
  estaba en el lock, como dev-dependency del registro; ninguna dependencia cambia de version). `escrow-v1`, `escrow-v2`
  y `escrow-core` no se tocaron.

## Diseno (lo fijado por c0der y lo que decidi dentro)

1. **Verificacion en el callback, que nunca entra en panico.** `on_payment_checked` lee primero el resultado de
   `get_payment` (fuera de un callback de verdad no hay resultado y el host aborta antes de tocar estado o mandar
   plata), y un rechazo devuelve el deposito **entero**, deja el log `arras-registry:rechazo:<CODIGO>`, libera el
   candado y devuelve `null`. Falla cerrado: `get_payment` que falla (`E20`), respuesta de mas de 8 KiB o que no es
   exactamente un `Payment` de v1 (`E21`, decision 5), `null` (`E22`).
2. **Candado.** `give_feedback` escribe `pending[(escrow, payment_id, rater)] = block_height` antes de la llamada
   cruzada; otro `give_feedback` igual mientras vuela es `E19`. El callback solo acepta si el candado sigue siendo el
   suyo (`nonce` = esa altura; si no, `E26`) y vuelve a mirar que no exista la calificacion (`E18`). Un candado que no
   se libero en 1.000 bloques se puede pisar: el callback viejo, si llegara, ve otro `nonce` y rechaza. Nunca hay dos calificaciones del mismo rater y pago (WR-06 lo prueba con dos tx a la vez:
   la segunda cae en `E19`). El candado vence si el callback no llego en 1.000 bloques: porque se quedo sin gas (hoy
   inalcanzable: los 30 TGas se reservan al crear la promesa y el peor caso mide 3,3) o porque se demoro (lo cubre
   `E26`). Si un callback llegara a fallar, el deposito de esa llamada queda en el contrato.
3. **Deposito.** `give_feedback` exige `feedback_deposit` = 1.864 bytes x 10^19 = **0,01864 NEAR** (igual que antes de
   la RONDA 1). Cubre el peor caso con un `operator_id` de 64 bytes; desde la RONDA 1 el callback exige el charset de
   v1 (`[a-z0-9_-]{1,32}`), asi que el peor caso real es **1.800 bytes** (U-ST1 lo mide exacto: cuentas de 64,
   operador de 32, todo al tope, calificada y operador nuevos) y esos 64 bytes de margen vuelven siempre al rater. Se
   dejo 1.864 para no cambiar la interfaz que los SDK ya implementan (`DEPOSIT_OPERATOR_ID_LEN` = 64). El callback
   cobra lo medido (una calificacion tipica con tags, URI y hash, calificada nueva, sobre v1: 745 o 746 bytes segun el
   largo base58 del `payment_id`, ~0,0075 NEAR) y
   devuelve el resto. El resumen por operador nuevo cuesta 189 bytes en el peor caso (operador de 32) y 109 con
   `worker.testnet` y `em` (U-ST5), solo la primera vez por `(calificada, operador)`; lo paga el rater. `register`: `register_deposit` = 1.751 bytes
   (0,01751 NEAR) cubre el peor caso; se cobra lo medido.
4. **Lo que guarda una calificacion:** rater, ratee, `ratee_role` (`payer` o `receiver`: el rol de la calificada en el
   pago), escrow, payment_id, `operator_id` y `released` del pago (leidos en cadena; sirven para filtrar por operador y
   pesar por monto), score 0..=100, tag1, tag2, feedback_uri, feedback_hash (32 bytes), created_at, revoked y
   revoked_at (largo fijo: revocar no cambia el storage ni pide deposito, solo 1 yocto).
5. **Resumen** por cuenta: total, `as_payer` (la califico el receiver: el agente que paga) y `as_receiver` (la califico
   el payer: el worker que cobra). `average_x100` = `floor(score_sum * 100 / count)` de las vigentes (8750 = 87,50),
   `null` sin vigentes. `last_feedback_id`/`last_at`: la ultima recibida, vigente o no. **El mismo resumen por
   `(calificada, operator_id)`** (RESPUESTA de c0der del 2026-11-04 00:06Z): `get_summary_by_operator`; la calificacion
   aceptada y `revoke_feedback` actualizan los dos.
6. **Entradas** (nada se normaliza): `agent_uri` y `feedback_uri` de 1 a 512 bytes, ASCII imprimible sin espacios y
   con esquema RFC 3986 (`https:`, `ipfs:`, `ar:`, `data:`, `did:`...); `metadata` de 1 a 1024 bytes de UTF-8 sin
   caracteres de control; tags de 1 a 64 bytes de `[A-Za-z0-9._:/-]` (distinguen mayusculas: `successRate` vale);
   `payment_id` de 1 a 64 bytes imprimibles; `feedback_hash` 64 hex en minusculas sin `0x` (como `terms_hash`).
7. **No se exige** que la calificada (ni el rater) este registrada: la identidad es la cuenta y el worker de EM no
   tiene por que darse de alta para recibir calificaciones.
8. **Quien actua es el `predecessor`, no el `signer`**: `register`, `update`, `give_feedback` y `revoke_feedback` usan
   `env::predecessor_account_id()`. Un contrato intermedio al que el usuario le firma una tx no registra, califica ni
   revoca en su nombre (califica como si mismo: no es parte, `E24`, y el deposito le vuelve a el). Lo fijan
   `el_rater_es_el_predecessor_no_el_signer`, `register_y_update_son_del_predecessor_no_del_signer` y
   `revocar_es_del_predecessor_no_del_signer` (RONDA 1, P2-3).

Decidido por c0der (RESPUESTA del 2026-11-03 23:30Z, a mi PREGUNTA de las 23:29Z):

1. **Lista de `new`: solo `v1.arras-escrow.near`.** v2 no entra mientras tenga una llave FullAccess (seria confiar en
   quien la tiene: podria desplegar un escrow que invente pagos). Cuando v2 este desplegado y sin llaves, sus pagos van
   en un registro nuevo: este es inmutable a proposito.
2. **Vale todo pago con `released > 0`**, cualquiera sea su estado (`Released`, `PartiallyReleased`, y tambien
   `Refunded` o `Reclaimed` despues de una liberacion parcial): una entrega parcial es un trato real. Sigue rechazado
   `released == 0` (`E23`), el tercero, la autocalificacion y la segunda del mismo rater y pago. El `status` no decide
   nada, pero tiene que ser uno de los 5 de v1 (decision 5).
3. **Ventana**: hasta la compactacion del pago en v1 (`reclaim_after + 7 dias`); EM califica apenas se libera el
   pago.

Y en la RESPUESTA del 2026-11-04 00:06Z (al punto 4 de la PREGUNTA, el resumen por operador que sugirio el refutador):

4. **Resumen por `(ratee, operator_id)` en el contrato**, ademas del global, con su vista
   `get_summary_by_operator(account_id, operator_id)`; cada calificacion guarda el `operator_id` y el `released` del
   pago (lo que devuelve `get_payment` al calificar) para filtrar por operador y pesar por plata.

Y en la RONDA 1 (2026-11-04 02:05Z, sobre el informe del refutador de RARRAS-7, que dio MERGEABLE CON RONDA sin P1):

5. **La respuesta de `get_payment` es estricta (P2-1, opcion (a): falla cerrado).** `EscrowPayment` es el espejo
   exacto del `Payment` de v1 (`contracts/escrow-v1/src/lib.rs:82-105`), y todo lo que no lo sea es `E21` con el
   deposito entero de vuelta:
   - los 16 campos con `deny_unknown_fields`: ni uno de mas (los de v2 tampoco) ni uno de menos, ninguno `null`, sin
     claves repetidas;
   - `status`: uno de los 5 strings de v1 (`Authorized`, `PartiallyReleased`, `Released`, `Refunded`, `Reclaimed`),
     solo como string (no la forma `{"Released":null}`, que serde acepta para una variante unitaria);
   - `amount`, `released`, `refunded`, `storage_charged` (`U128`) y `created_at`, `release_deadline`, `reclaim_after`
     (`U64`): string decimal canonico, solo digitos, sin signo ni ceros a la izquierda salvo `"0"`, dentro del rango
     del tipo (`"+1"` y `"01"` pasaban antes: `u128::from_str` los acepta). v1 serializa siempre asi;
   - `fee_bps`/`fee_bps_max` numero `u16`, `sponsored` booleano, `payer`/`receiver`/`fee_recipient` `AccountId`
     validos, `terms_hash` string;
   - `operator_id` con el charset de v1, `[a-z0-9_-]{1,32}` (`is_valid_operator_id` de `escrow-core`, el mismo con el
     que v1 los da de alta).

   Por contenido solo se valida lo que el registro guarda o usa (`operator_id`, `released`, las partes); `terms_hash` y
   `fee_recipient` se exigen con su tipo y nada mas. Lo que prueba que el espejo coincide con v1: WR-02, WR-03 y WR-04
   corren contra la v1 compilada y contra la wasm exacta de mainnet (`tests/fixtures`), pasan por los 5 estados, y en
   cada uno el `get_payment` real tiene exactamente los 16 campos y deserializa en `EscrowPayment` (`assert_espejo`).
   El riesgo de (a) es que un espejo que no coincidiera rechazaria todas las calificaciones para siempre: por eso el
   humo del paso 3b del plan de deploy es **obligatorio** antes de borrar las llaves.

## Modelo de confianza (lo que EM y los SDK tienen que saber)

- La reputacion vale lo que valen los pagos del escrow de la lista. En v1, **el owner de v1**
  (`owner.arras-escrow.near`, FullAccess en SSM) registra operadores, y el capturer de cada operador libera sus pagos.
  Ese owner podria registrar un operador propio con `min_amount` 1 y fee 0, autorizar 1 unidad (1e-6 USDC) a
  cualquier worker, liberarla con su capturer y dejarle un score 0 (o 100): le cuesta el storage de la
  calificacion (~0,0075 NEAR), el del pago en v1 (~1.000 bytes, 0,01 NEAR, que vuelve al compactar) y el gas. Es la
  misma razon por la que v2 no entra con su llave viva (RESPUESTA 1), un poco mas debil: el payer pone USDC real.
- Dentro de `em`, un mismo dueno de agente y de worker se puede inflar el puntaje pagando el 13 % de un pago chico mas
  el storage.
- Por eso `get_summary` (que mezcla operadores y raters) es un numero de referencia, no un puntaje de confianza. **EM
  consulta `get_summary_by_operator(worker, "em")`** (solo los pagos que libero su capturer) y, para el detalle,
  `list_feedback` (o los eventos `feedback_given`) filtrado por `escrow == "v1.arras-escrow.near"` y
  `operator_id == "em"`, pesando por `released` (asi 1e-6 USDC no pesa como un pago real) y por raters distintos
  (ERC-8004 le pide un filtro de clientes a `getSummary` justo por esto).
- **Un indexador cuenta solo los eventos `feedback_given` y `feedback_revoked` de recibos cuyo `executor_id` (el
  receiver del recibo) es `registry.arras-escrow.near` Y cuyo status final es de exito (`SuccessValue` o
  `SuccessReceiptId`).** Los filtros de arriba son campos de `data`: cualquier contrato puede emitir en su propio
  recibo el mismo
  `EVENT_JSON:{"standard":"arras-registry",...,"data":[{"escrow":"v1.arras-escrow.near","operator_id":"em",...}]}` con
  datos inventados. Y un recibo puede emitir su `EVENT_JSON` y fallar despues (un panic, o `LackBalanceForState` al
  cerrar el storage): el estado se revierte pero el log queda en el outcome. Las dos cosas las muestra WR-10 en el
  sandbox (un contrato ajeno emite el mismo `feedback_given` y sale bien; un recibo que lo emite y entra en panico, y
  otro que lo emite y falla con `LackBalanceForState`, quedan con el log en su outcome fallido y el estado revertido).
  Ante la duda, se relee la calificacion del contrato con `get_feedback(feedback_id)` (o
  `get_feedback_for_payment`).
- **La ventana de calificacion la puede cerrar cualquiera, tambien la calificada.** `compact` de v1
  (`contracts/escrow-v1/src/withdraw.rs:60-66`) no tiene permisos: desde `reclaim_after + 7 dias` cualquiera borra el
  pago, y despues `get_payment` da `null` y toda calificacion es `E22`. Un worker o un agente que espera una mala nota
  puede compactar apenas se cumple el plazo. Por eso EM califica al liberar.
- **El resumen por operador no lleva el escrow en la clave** (`(ratee, operator_id)`, `lib.rs`): vale porque la lista
  de `new` es un solo escrow para siempre. Quien copie el crate para un registro de varios escrows tiene que sumar el
  escrow a la clave (dos escrows pueden tener operadores con el mismo nombre).
- La identidad es la cuenta: una cuenta `.near` de primer nivel borrada la puede volver a crear cualquiera desde el
  registrador `near` (y una sub-cuenta, su padre); el nuevo dueno hereda la ficha y la reputacion recibida, y puede
  hacer `update`.
- `agent_uri` y `feedback_uri` aceptan cualquier esquema (tambien `javascript:`) y `metadata` acepta UTF-8 con
  caracteres bidi o de ancho cero: EM y los SDK sanean antes de mostrar (enlaces solo `https:`, `ipfs:`, `ar:`).

## Codigos

Panic (la tx falla, el runtime devuelve el deposito): `E01` agent_uri, `E02` metadata, `E03` ya registrada, `E04` no
registrada, `E05` deposito de storage corto (el mensaje dice cuanto hace falta), `E06` `update` sin yocto, `E10`
escrow fuera de la lista, `E11` payment_id, `E12` score, `E13` tag, `E14` feedback_uri, `E15` feedback_hash, `E16`
deposito de calificacion corto, `E17` gas menor a 60 TGas, `E18` ya califico, `E19` en vuelo, `E30` calificacion
inexistente (o de otro rater), `E31` ya revocada, `E40` lista de `new` invalida (vacia, mas de 8, repetida o con el
propio registro). Formato: `ARRAS_REG_E16: deposito insuficiente para la calificacion: se necesitan N yocto`.

Rechazo del callback (la tx sale bien, devuelve `null`, el deposito vuelve entero): `E18`, `E20` get_payment fallo,
`E21` respuesta invalida (mas de 8 KiB, JSON roto, o no es exactamente un `Payment` de v1: decision 5), `E22` pago
inexistente, `E23` `released == 0` (no paso plata, en cualquier estado), `E24` no es parte, `E25` se califica a si mismo, `E26` el candado ya no es suyo. Log:
`arras-registry:rechazo:E23_PAGO_SIN_LIBERAR`.

## Plan de deploy en mainnet (lo corre c0der; yo no desplegue nada)

Cuenta `registry.arras-escrow.near`, creada desde `arras-escrow.near`. near-cli-rs 0.30.1, firmando con el llavero
legacy como `scripts/mainnet.sh`. Medido en el sandbox: la wasm de `cargo near build non-reproducible-wasm` ocupa
**231.516 bytes** con la RONDA 1 (2,32 NEAR de storage; antes de la RONDA, 223.893: el espejo estricto suma ~7,6 KB); la
reproducible la construye c0der al desplegar y puede medir distinto (yo no la construi: necesita Docker con red). El
estado inicial es chico (la lista de escrows). Los usuarios pagan su storage, asi que el contrato solo necesita el
codigo y un margen.

```sh
# 0. Wasm reproducible (NEP-330), DOS veces desde DOS clones limpios del merge commit de este PR, SIN CARGO_TARGET_DIR
#    (cargo-near 0.22.0 borra el artefacto con CARGO_TARGET_DIR seteado). Necesita docker (imagen sourcescan/cargo-near
#    del Cargo.toml del crate, por digest). Antes, `git fetch` en el checkout de origen: tiene que tener el merge commit.
#    No hay hash fijado de antemano (NEP-330 embebe el commit): lo que se exige es que las dos construcciones den el
#    MISMO sha256, como se hizo con v1 (docs/MAINNET.md).
REV=<MERGE_SHA>                     # el merge commit de este PR en main
ORIGEN="$(git -C <tu checkout de arras> rev-parse --show-toplevel)"
for n in 1 2; do
  git clone -q "$ORIGEN" "$HOME/arras-build/registry-$REV-$n"
  ( cd "$HOME/arras-build/registry-$REV-$n" && git checkout -q "$REV" && test -z "$(git status --porcelain)" \
    && env -u CARGO_TARGET_DIR cargo near build reproducible-wasm --manifest-path contracts/registry-v1/Cargo.toml )
done
shasum -a 256 "$HOME"/arras-build/registry-"$REV"-{1,2}/target/near/arras_registry_v1/arras_registry_v1.wasm
#    -> los dos sha256 IGUALES; si difieren, se para aca. Ese sha256 es el code hash que se verifica en el paso 3.
WASM="$HOME/arras-build/registry-$REV-1/target/near/arras_registry_v1/arras_registry_v1.wasm"
wc -c < "$WASM"
#    -> por debajo de ~245.000 bytes (la no reproducible mide 231.516): entra en los 2,5 NEAR del paso 1b (2,45 de
#       codigo y ~0,05 para el estado, la llave y el gas del deploy y del borrado de llaves). Si mide mas, el 1b sube a
#       bytes / 100.000 + 0,05 NEAR, redondeado hacia arriba a la decena (p. ej. 251.000 bytes -> 2,6 NEAR), y el 1a
#       pide ese monto + 0,03 libres en el padre.

# 1a. El padre tiene que tener 2,53 NEAR libres (2,5 + el gas de crear la cuenta; mas si el paso 0 subio el monto).
#     Despues del deploy de v1 le quedaron ~0,074 en el ensayo de testnet (docs/MAINNET.md, sec. 3): hay que fondearlo
#     antes (el owner, a arras-escrow.near). Mirar el saldo:
near account view-account-summary arras-escrow.near network-config mainnet now

# 1b. La cuenta del registro (2,5 NEAR, o lo que dijo el paso 0). La llave del padre esta en SSM
#     (/arras/mainnet/padre, us-east-2; `secretos` borro el archivo local): va por un pipe, como en scripts/mainnet.sh,
#     sin tocar el disco. La llave nueva del registro queda en el llavero legacy (la usan los pasos 2 y 4).
near account create-account fund-myself registry.arras-escrow.near '2.5 NEAR' autogenerate-new-keypair \
  save-to-legacy-keychain sign-as arras-escrow.near network-config mainnet \
  sign-with-access-key-file <(aws ssm get-parameter --region us-east-2 --name /arras/mainnet/padre \
    --with-decryption --output json | jq -j '.Parameter.Value') send

# 2. Deploy + `new` privado en UNA transaccion (`new` es #[private]: nadie mas lo puede llamar).
near contract deploy registry.arras-escrow.near use-file "$WASM" \
  with-init-call new json-args '{"escrows":["v1.arras-escrow.near"]}' \
  prepaid-gas '30.0 Tgas' attached-deposit '0 NEAR' network-config mainnet sign-with-legacy-keychain send
#    Solo v1 (RESPUESTA 1): v2, cuando este desplegado y sin llaves, va en un registro nuevo.

# 3. Verificar ANTES de borrar llaves: code hash == el sha256 del paso 0, y la config.
near account view-account-summary registry.arras-escrow.near network-config mainnet now
near contract call-function as-read-only registry.arras-escrow.near get_config json-args '{}' \
  network-config mainnet now
#    -> "version": "arras-registry-v1", "escrows": ["v1.arras-escrow.near"], "agents": 0, "feedbacks": 0,
#       "feedback_deposit": "18640000000000000000000", "register_deposit": "17510000000000000000000",
#       "min_gas_give_feedback": "60000000000000"
near contract call-function as-read-only registry.arras-escrow.near contract_source_metadata json-args '{}' \
  network-config mainnet now        # NEP-330: imagen, digest y commit

# 3b. OBLIGATORIO (RONDA 1), ANTES de borrar llaves: las dos partes del pago real de R679 se califican entre si contra
#     v1 de mainnet. Es la prueba de que el espejo estricto (decision 5) lee el `Payment` real; las calificaciones
#     quedan para siempre. Cada parte necesita ~0,02 NEAR libres (vuelve casi todo: se cobra el storage medido).
PID=EbkEtGaM7ZwY6aHSdHSqxdFx9cJV13KPT7fYBWjxbfd9
PAYER=fe0a574258f8277099875e46d4ca0043cdb6274ff2501b375a0d3d64bc211079      # la implicita de fondeo (SSM /arras/mainnet/fondeo)
RECEIVER=826e8c640ba0ee77bf255ecb332dfaca73fc1e3aef2dbf370b5ffeb30f9ac7af
near contract call-function as-read-only v1.arras-escrow.near get_payment \
  json-args "{\"payment_id\":\"$PID\"}" network-config mainnet now
#    -> "operator_id": "em", "payer": $PAYER, "receiver": $RECEIVER, "released": "100000", "status": "Released".
#       Si da null (el pago ya se compacto: cualquiera puede desde reclaim_after + 7 dias), el humo va con otro pago
#       liberado de em sin compactar cuyas dos partes firme c0der; sin uno, no se borran las llaves.
near contract call-function as-transaction registry.arras-escrow.near give_feedback \
  json-args "{\"escrow\":\"v1.arras-escrow.near\",\"payment_id\":\"$PID\",\"score\":100}" \
  prepaid-gas '100.0 Tgas' attached-deposit '0.01864 NEAR' sign-as "$PAYER" network-config mainnet \
  sign-with-access-key-file <(aws ssm get-parameter --region us-east-2 --name /arras/mainnet/fondeo \
    --with-decryption --output json | jq -j '.Parameter.Value') send
#    -> 0 (el feedback_id: el payer califico al receiver)
near contract call-function as-transaction registry.arras-escrow.near give_feedback \
  json-args "{\"escrow\":\"v1.arras-escrow.near\",\"payment_id\":\"$PID\",\"score\":100}" \
  prepaid-gas '100.0 Tgas' attached-deposit '0.01864 NEAR' sign-as "$RECEIVER" network-config mainnet \
  <la firma de $RECEIVER que uso R679> send
#    -> 1 (el receiver califico al payer)
near contract call-function as-read-only registry.arras-escrow.near get_summary_by_operator \
  json-args "{\"account_id\":\"$RECEIVER\",\"operator_id\":\"em\"}" network-config mainnet now
#    -> "operator_id": "em", "count": 1, "as_receiver": {"count": 1, ...}
#    SI ALGUNA DE LAS DOS DEVUELVE null, NO SE BORRAN LAS LLAVES Y SE REDESPLIEGA. El motivo esta en el log
#    `arras-registry:rechazo:<CODIGO>` de la tx (un E21 es que el espejo no coincide con lo que serializa v1).

# 4. Solo si el 3b dio los dos feedback_id. Sin llaves (irreversible: el registro queda inmutable). Todas las de
#    list-keys, separadas por coma.
near account list-keys registry.arras-escrow.near network-config mainnet now
near account delete-keys registry.arras-escrow.near public-keys '<todas las de list-keys, separadas por coma>' \
  network-config mainnet sign-with-legacy-keychain send
near account list-keys registry.arras-escrow.near network-config mainnet now      # -> tabla vacia
# 5. El archivo de la llave borrada en cadena ya no sirve: se borra del llavero legacy (no va a SSM).
```

Ensayo de esto en el sandbox: WR-00 (deploy + `new` en una tx, `new` ajeno rechazado, una sola vez, llaves borradas,
sin redeploy y el registro sigue contestando). En testnet no corri nada (la regla: no sin preguntar). El mismo plan en
testnet seria con `network-config testnet` y `{"escrows":["arras-v1b-1101.testnet"]}` (el v1 demo del README).

## Lo que EM y los SDK tienen que llamar

Contrato: `registry.arras-escrow.near` (mainnet). Todo JSON; `U128`/`U64` van como string.

| Metodo | Firma (args JSON) | Deposito | Gas | Devuelve |
|---|---|---|---|---|
| `register` | `{"agent_uri": str, "metadata": str?}` | `register_deposit` de `get_config` (0,01751 NEAR; vuelve el sobrante) | 30 TGas (mide 3) | `AgentView` |
| `update` | `{"agent_uri": str, "metadata": str?}` (reemplaza las dos) | >= 1 yocto, mas lo que crezca (vuelve el sobrante y lo liberado) | 30 TGas | `AgentView` |
| `give_feedback` | `{"escrow": acc, "payment_id": str, "score": 0..100, "tag1": str?, "tag2": str?, "feedback_uri": str?, "feedback_hash": hex64?}` | `feedback_deposit` (0,01864 NEAR; vuelve el sobrante, o todo si se rechaza) | >= 60 TGas (recomendado 100; mide 8,0 total) | `feedback_id` (u32) o `null` + log de rechazo |
| `revoke_feedback` | `{"escrow": acc, "payment_id": str}` (la propia) | exactamente 1 yocto | 30 TGas | `FeedbackView` |

**RONDA 1: la interfaz no cambio.** Metodos, argumentos, depositos (`feedback_deposit` sigue en 0,01864 NEAR), gas,
vistas, eventos y codigos son los mismos; lo unico nuevo son rechazos `E21` cuando la respuesta de `get_payment` no es
exactamente un `Payment` de v1, que con `v1.arras-escrow.near` no pasa (lo prueban WR-02..WR-04 contra su wasm).

Vistas (sin deposito): `get_config()`; `get_agent({"account_id"})` -> `AgentView | null`;
`list_agents({"from_index"?: u32, "limit"?: u32})` (orden de alta, `limit` <= 50); `get_feedback({"feedback_id": u32})`;
`get_feedback_for_payment({"escrow", "payment_id", "rater"})`; `list_feedback({"account_id", "from_index"?,
"limit"?})` (las recibidas, en orden de llegada, revocadas incluidas y marcadas); `get_summary({"account_id"})`;
`get_summary_by_operator({"account_id", "operator_id"})` (el mismo `SummaryView` con `"operator_id"`; en `get_summary`
esa clave no aparece; un operador sin calificaciones da ceros y `null`); `is_feedback_pending({"escrow",
"payment_id", "rater"})`.

```json
AgentView    {"account_id":"agente.near","agent_uri":"https://agente.example/card.json","metadata":null,
              "registered_at":"1793491200000000000","updated_at":"1793491200000000000"}
FeedbackView {"feedback_id":0,"rater":"agente.near","ratee":"worker.near","ratee_role":"receiver",
              "escrow":"v1.arras-escrow.near","payment_id":"DN5e...P38M","operator_id":"em","released":"1000000",
              "score":90,"tag1":"quality","tag2":"successRate","feedback_uri":"https://...","feedback_hash":"00ff...",
              "created_at":"1793491201000000000","revoked":false,"revoked_at":null}
SummaryView  {"account_id":"worker.near","count":2,"revoked":0,"score_sum":"180","average_x100":9000,
              "as_payer":{"count":0,"revoked":0,"score_sum":"0","average_x100":null},
              "as_receiver":{"count":2,"revoked":0,"score_sum":"180","average_x100":9000},
              "last_feedback_id":7,"last_at":"1793491201000000000"}
get_summary_by_operator -> {"account_id":"worker.near","operator_id":"em","count":2, ... (lo mismo)}
```

Eventos (un elemento en `data`):

```
EVENT_JSON:{"standard":"arras-registry","version":"1.0.0","event":"agent_registered","data":[{"account_id":"agente.near","agent_uri":"https://agente.example/card.json","metadata":null,"at":"5"}]}
EVENT_JSON:{"standard":"arras-registry","version":"1.0.0","event":"agent_updated","data":[{"account_id":"agente.near","agent_uri":"ipfs://bafy","metadata":"{\"k\":\"v\"}","at":"6"}]}
EVENT_JSON:{"standard":"arras-registry","version":"1.0.0","event":"feedback_given","data":[{"feedback_id":0,"rater":"payer.near","ratee":"worker.near","ratee_role":"receiver","escrow":"v1.arras-escrow.near","payment_id":"DN5eMA3sNES7xsoy6qvy24cVF6Dmwjxpkvd2b4e8P38M","operator_id":"em","released":"1000000","score":90,"tag1":"quality","tag2":null,"feedback_uri":null,"feedback_hash":null,"created_at":"7"}]}
EVENT_JSON:{"standard":"arras-registry","version":"1.0.0","event":"feedback_revoked","data":[{"feedback_id":0,"rater":"payer.near","ratee":"worker.near","escrow":"v1.arras-escrow.near","payment_id":"x","revoked_at":"8"}]}
```

(Son los strings exactos de `events::tests::formato_exacto_de_cada_evento`.)

Flujo en EM (payer = la cuenta NEAR del agente, receiver = la cuenta de cobro del worker):

1. Apenas el capturer de EM hace `release` en v1 (entero o parcial), las dos partes pueden calificar (y lo siguen
   pudiendo si despues el resto se devuelve o se reclama: cuenta `released > 0`). El agente firma
   `give_feedback` con su cuenta NEAR (califica al worker: `ratee_role` `receiver`); el worker, con su cuenta de cobro
   (califica al agente: `ratee_role` `payer`). Las dos son llaves FullAccess (hay deposito); una llave FunctionCall no
   alcanza.
2. EM lee el resultado: un numero es el `feedback_id`; `null` es un rechazo y el motivo esta en el log
   `arras-registry:rechazo:<CODIGO>`. Un panic (`ARRAS_REG_Enn`) es un error de entrada o de deposito/gas.
3. Ventana: hasta que el pago se compacte en v1 (cualquiera puede desde `reclaim_after + 7 dias`, tambien la
   calificada, para esquivar una nota tardia); despues `get_payment` da `null` y es `E22`. EM califica apenas se
   libera el pago.
4. Para mostrar reputacion: `get_summary_by_operator(cuenta, "em")`, y para el detalle `list_feedback` (o el indexador
   de `feedback_given`/`feedback_revoked`, contando solo los recibos que ejecuto `registry.arras-escrow.near` y que
   terminaron en `SuccessValue` o `SuccessReceiptId`)
   filtrado por `escrow == "v1.arras-escrow.near"` y `operator_id == "em"`, pesado por `released` y por raters
   distintos; `get_summary` mezcla operadores y raters y sirve solo de referencia (ver "Modelo de confianza").

Equivalencias con ERC-8004: `agentId` (NFT) -> la cuenta NEAR; `register(tokenURI, metadata)` -> `register(agent_uri,
metadata?)`; `giveFeedback(agentId, score, tag1, tag2, feedbackURI, feedbackHash, feedbackAuth)` ->
`give_feedback(escrow, payment_id, score, tag1, tag2, feedback_uri, feedback_hash)`, donde el `feedbackAuth` firmado
por el agente se reemplaza por la prueba en cadena de un pago liberado entre las dos partes; `NewFeedback` ->
`feedback_given`; `revokeFeedback` -> `revoke_feedback`; `getSummary(agentId, clientAddresses, ...)` -> `get_summary`
y `get_summary_by_operator` (el filtro es por operador del pago, no por cliente); `readAllFeedback` ->
`list_feedback`. No hay `appendResponse` (no estaba en el diseno).

## Gas y storage medidos (sandbox nearcore 2.13.4)

Con la RONDA 1 (espejo estricto). Donde hay dos numeros, el primero es contra la v1 compilada y el segundo contra la
wasm de mainnet.

| Que | Medido | Constante del contrato |
|---|---|---|
| `give_feedback`, recibo del registro (WR-02) | 2,392 TGas | `MIN_GAS_GIVE_FEEDBACK` 60 TGas en total |
| `get_payment` de v1 (WR-02) | 1,477 / 1,471 TGas | `GAS_GET_PAYMENT` 10 TGas |
| `on_payment_checked` (aceptada, calificada y operador nuevos; WR-02) | 2,924 TGas | `GAS_ON_PAYMENT_CHECKED` 30 TGas |
| callback en el peor caso (un `Payment` de v1 de 8 KiB justos, todo al tope; WR-05) | 3,305 TGas | 30 TGas |
| tx completa de `give_feedback` aceptada (WR-02) / rechazada con `E22` (WR-03) | 8,008 / 8,001 y 6,844 / 6,838 TGas | se adjuntan >= 60 (recomendado 100) |
| tx completa del peor caso (WR-05) | 8,826 TGas | con 60 TGas justos |
| `give_feedback` con 60 TGas justos sobre v1 real (WR-09) | pasa (8,008 TGas) | |
| `register` con `metadata` de 600 bytes (WR-01) | 798 bytes (0,00798 NEAR), 3,01 TGas | `register_deposit` 1.751 bytes |
| una calificacion con tags, URI y hash, calificada y operador nuevos (WR-02, las dos v1) | 745 o 746 bytes (~0,0075 NEAR): el `payment_id` base58 tiene 43 o 44 caracteres | `feedback_deposit` 1.864 bytes (0,01864 NEAR) |
| peor caso de una calificacion (U-ST1: cuentas de 64, operador de 32, todo al tope, calificada y operador nuevos) | 1.800 bytes exactos (vuelven 64) | `FEEDBACK_MAX_BYTES` 1.864 (cubre un operador de 64) |
| resumen por operador nuevo: peor caso / `worker.testnet` + `em` (U-ST1, U-ST5) | 189 / 109 bytes | |
| peor caso de `register` (U-ST3) | 1.751 bytes exactos | `AGENT_MAX_BYTES` |
| el candado mientras vuela | 81 bytes (se borra en el callback) | |
| gasto del rater en un rechazo (deposito devuelto entero) | 0,00062 NEAR de gas | |
| wasm (`cargo near build non-reproducible-wasm`, la de `compile_project`) | 231.516 bytes (223.893 antes de la RONDA 1) | el 1b del plan, 2,5 NEAR, alcanza hasta ~245.000 |

## Lo que corri

Sin red (`HTTPS_PROXY=http://127.0.0.1:9`, `--offline`), sin credenciales (`AWS_SHARED_CREDENTIALS_FILE=/dev/null`),
`NEAR_SANDBOX_BIN_PATH` = near-sandbox 2.13.4, Rust 1.93.0 de `rust-toolchain.toml`. El repo no tiene
`.github/workflows` ni `scripts/ci/preflight.sh`: no hay CI que reproducir. `origin/main` sigue en ddc5121 (re-medido con
`git fetch` antes de la corrida final; es ancestro de la rama, asi que la union es la rama misma).

| Paso | Comando | Resultado |
|---|---|---|
| Linea base, `origin/main` ddc5121 | `cargo test --workspace --offline` y `scripts/test_mainnet.sh` | escrow-core 12, escrow-v1 60 + 15 sandbox (+3 ignored, necesitan red), escrow-v2 130 + 11 sandbox, test-ft 0; 347 asserts. **Sin rojos** |
| RONDA 1: union con `origin/main` en 2cc2437 (el ultimo commit de codigo; despues solo cambia este handoff) | `cargo test --workspace --offline --no-fail-fast -- --nocapture` | escrow-core 12, escrow-v1 60 + 15 (+3 ignored), escrow-v2 130 + 11, **registry-v1 115 unitarios + 14 sandbox**, test-escrow 0, test-ft 0: todo verde |
| Lo mismo en 60d285c (antes de WR-10) | idem | igual, con registry-v1 115 + 13 |
| Sandbox contra las dos v1 | `cargo test -p arras-registry-v1 --test workspaces -- --nocapture` | WR-02, WR-03 y WR-04 pasan contra la v1 compilada y contra la wasm de mainnet (sha256 `7058319d...0eada` verificado, code hash `8ZYfMJer...PxHr5X` en la cuenta del sandbox); `assert_espejo` en los 5 estados |
| Union | `scripts/test_mainnet.sh` con `/bin/bash` 3.2.57 y `/opt/homebrew/bin/bash` 5.3.20 | 347 asserts pasan, 0 fallan, con los dos |
| Formato | `cargo fmt --all -- --check` | limpio |
| Lint | `cargo clippy --offline -p arras-registry-v1 -p arras-test-escrow --all-targets -- -D warnings` | limpio |
| Pre-CI de c0der | `python3 ~/c0der-specs/hooks/preci.py --repo . --base origin/main --ejecutar` | verde por vacio (0 pasos: no hay workflows) |
| Secretos | `gitleaks git --log-opts=origin/main..HEAD` | sin hallazgos |
| Mutaciones | `mutar.py` (115 mutantes, `timeout` por mutante, revertidos con `git checkout`, arbol limpio entre uno y otro) | **115 de 115 muertas**, todas por un test que falla (0 timeouts, 0 rojos sin test) |
| Wasm | `cargo near build non-reproducible-wasm` (lo hace `compile_project` del sandbox) | 231.516 bytes |

## Auto-refutacion

**Bordes revisados** (cada uno con test): mayusculas (tags las distinguen y no se normalizan; el esquema de una URI
puede ir en mayusculas; un `feedback_hash` en mayusculas es `E15`), espacios adelante, atras y en el medio (URI, tag,
`payment_id`), `0x`/`0X` en el hash, vacios (`""` en cada campo opcional, respuesta vacia de `get_payment`), nulos
(campos opcionales en `null`; `get_payment` = `null` es `E22`; un `Payment` sin `status` es `E21`), unicode (URI, tag
y `payment_id` solo ASCII; `metadata` acepta `ñandú` y rechaza control), topes exactos y tope+1 (URI 512/513, metadata
1024/1025, tag 64/65, `payment_id` 64/65, `operator_id` 32/33, respuesta 8.192 bytes / 9 KiB, score 100/101, deposito
`need`/`need-1`, gas 60 TGas / 60 TGas - 1, TTL `at+999`/`at+1000`, escrows 8/9), `released` como numero JSON en vez
de string (`E21`), `status` como objeto (`E21`). Parser: `serde_json` de near-sdk; no se usa ningun parser de URL.

**La respuesta de `get_payment` (RONDA 1, cada caso `E21` con el deposito entero, por el payer y por el receiver):**
cada uno de los 16 campos ausente y en `null`; campos de mas (los 3 de v2, uno cualquiera, `Status` con mayuscula);
una clave repetida; `status` fuera de los 5 (`Disputed`, `Resolved`, `released`, `RELEASED`, con espacios, vacio,
`{"Released":null}`, `{"Released":{}}`, arreglo, numero, booleano); los 7 numeros con `+1`, `01`, `00`, `-1`, espacios
adelante y atras, vacio, `0x1`, `1e0`, `1.0`, `1_000`, numero JSON y uno mas que el maximo del tipo; `u16` como
string, negativo, 65.536 y 13,5; `sponsored` como string y como numero; cuentas invalidas; `operator_id` vacio, de 33 y
65, con mayusculas, espacios, `.`, `/`, `:`, NUL, no ASCII, numero y `null`. Y lo que vale: `operator_id` de 1 y de 32,
`"0"` y los maximos de cada numero, `fee_bps` 0 y `u16::MAX`, `sponsored: true`.

**Mutaciones a mano** (cada una aplicada, corrida con `timeout`, revertida con `git checkout`, arbol limpio entre una y
otra; script en el scratchpad de la sesion). Ronda 0: 77, 76 muertas; M02 (`colon >= 1` en `is_valid_uri`) sobrevivia
por equivalente (`b[0]` letra lo implica): se saco la guarda redundante. Ronda 1, despues de la RESPUESTA (los mutantes
de `has_released` reescritos para la regla nueva, entre ellos M53, que vuelve a la regla vieja): 79 de 79 muertas.
Ronda 2, despues del resumen por operador (M81..M87 nuevos; M63, M67 y M74..M77 movidos a `Summary::add`/`revoke`):
86 de 86 muertas. **Ronda 3, la RONDA 1 de c0der**: las 86 (M51, M52..M55, M78..M80 reescritos para el codigo nuevo:
M51 vuelve a la regla vieja del `operator_id`, `released` es `u128`, `status` es enum), los tres del refutador (R04,
R05, R11) con sus vecinos (R04b `revoke_feedback`, R05b `update`, R11W en el sandbox) y una por cada guarda nueva del
espejo estricto (N01..N22, N01W en el sandbox). Desde esta ronda solo cuenta como muerta la que hace FALLAR un test
(como mide el refutador de c0der: un timeout o un rojo sin test no cuentan).

| # | Mutacion (guarda) | Resultado | Un test que la mata |
|---|---|---|---|
| M01 | URI: sin el chequeo de ASCII imprimible sin espacios | muerta | `identidad::update_con_uri_invalida_es_e01` |
| M03 | URI: `colon + 1 < len` -> `colon < len` (nada despues de `:`) | muerta | `validate::tests::uri_invalidas` |
| M04 | URI: sin `b[0]` letra | muerta | `validate::tests::uri_invalidas` |
| M05 | URI: caracteres del esquema sin chequear | muerta | `validate::tests::uri_invalidas` |
| M06 | URI: tope de largo +1 | muerta | `identidad::register_uri_larga_es_e01` |
| M07 | metadata: vacia aceptada | muerta | `identidad::register_metadata_vacia_es_e02` |
| M08 | metadata: tope +1 | muerta | `identidad::register_metadata_larga_es_e02` |
| M09 | metadata: caracteres de control aceptados | muerta | `identidad::update_con_metadata_invalida_es_e02` |
| M10 | tag: vacio aceptado | muerta | `reputacion::tag2_invalido_es_e13` |
| M11 | tag: tope +1 | muerta | `validate::tests::tags` |
| M12 | tag: cualquier ASCII grafico | muerta | `validate::tests::tags` |
| M13 | payment_id: vacio aceptado | muerta | `reputacion::payment_id_vacio_es_e11` |
| M14 | payment_id: tope +1 | muerta | `reputacion::payment_id_largo_es_e11` |
| M15 | payment_id: espacios/no imprimibles aceptados | muerta | `reputacion::payment_id_con_espacio_es_e11` |
| M16 | hash: largo `< 64` en vez de `!= 64` | muerta | `validate::tests::hash_hex` |
| M17 | hash: mayusculas aceptadas | muerta | `reputacion::feedback_hash_en_mayusculas_es_e15` |
| M18 | register: sin E03 (doble alta) | muerta | `identidad::register_dos_veces_es_e03` |
| M19 | register: deposito minimo a la mitad | muerta | `identidad::register_con_un_yocto_menos_es_e05` |
| M20 | register: cobra la mitad | muerta | `identidad::register_cobra_lo_medido_y_devuelve_el_resto` |
| M21 | update: sin E06 (sin yocto) | muerta | `identidad::update_sin_yocto_es_e06` |
| M22 | update: no devuelve lo liberado | muerta | `identidad::update_que_achica_devuelve_lo_liberado_y_el_deposito` |
| M23 | update: sin E05 | muerta | `identidad::update_que_crece_sin_deposito_suficiente_es_e05` |
| M24 | update: pisa `registered_at` | muerta | `identidad::update_que_crece_cobra_la_diferencia` |
| M25 | update: sin validar agent_uri | muerta | `identidad::update_con_uri_invalida_es_e01` |
| M26 | new: lista vacia aceptada | muerta | `init::new_sin_escrows` |
| M27 | new: 9 escrows | muerta | `init::new_con_nueve_escrows` |
| M28 | new: repetidos aceptados | muerta | `init::new_con_escrow_repetido` |
| M29 | new: el propio registro aceptado | muerta | `init::new_con_el_propio_registro` |
| M30 | give: sin E10 (escrow fuera de la lista) | muerta | `reputacion::el_propio_registro_como_escrow_es_e10` |
| M31 | give: sin E11 | muerta | `reputacion::payment_id_con_espacio_es_e11` |
| M32 | give: score 101 aceptado | muerta | `reputacion::score_101_es_e12` |
| M33 | give: tag1 sin validar | muerta | `reputacion::tag1_invalido_es_e13` |
| M34 | give: tag2 sin validar | muerta | `reputacion::tag2_invalido_es_e13` |
| M35 | give: feedback_uri sin validar | muerta | `reputacion::feedback_uri_invalida_es_e14` |
| M36 | give: feedback_hash sin validar | muerta | `reputacion::feedback_hash_con_0x_es_e15` |
| M37 | give: deposito minimo -1 | muerta | `reputacion::un_yocto_menos_de_deposito_es_e16` |
| M38 | give: gas minimo 1 TGas | muerta | `reputacion::gas_de_menos_es_e17` |
| M39 | give: sin E18 sincronico | muerta | `reputacion::la_segunda_calificacion_del_mismo_rater_y_pago_es_e18` |
| M40 | give: TTL `<` -> `<=` | muerta | `reputacion::candado_vencido_se_pisa_y_el_callback_viejo_es_e26` |
| M41 | give: candado que nunca frena (E19) | muerta | `reputacion::dos_give_feedback_en_vuelo_es_e19` |
| M42 | give: sin escribir el candado | muerta | `reputacion::calificarse_a_si_mismo_es_e25` |
| M43 | give: nonce distinto del candado | muerta | `reputacion::el_callback_vuelve_a_mirar_si_ya_califico_e18` |
| M44 | give: gas de get_payment = el del callback | muerta | `reputacion::give_feedback_llama_get_payment_y_el_callback_con_su_gas` |
| M45 | callback: sin chequear el nonce (E26) | muerta | `reputacion::callback_sin_candado_es_e26` |
| M46 | callback: no libera el candado | muerta | `reputacion::calificarse_a_si_mismo_es_e25` |
| M47 | callback: Failed -> E21 | muerta | `reputacion::get_payment_que_falla_es_e20` |
| M48 | callback: tope de respuesta 16 KiB | muerta | `reputacion::respuesta_de_mas_de_8_kib_es_e21` |
| M49 | callback: JSON roto como `null` | muerta | `reputacion::los_demas_tipos_de_v1_son_estrictos` |
| M50 | callback: `null` -> E21 | muerta | `reputacion::pago_inexistente_es_e22` |
| M51 | operator_id: vuelve la regla vieja (largo <= 64, sin charset) | muerta | `reputacion::operator_id_fuera_del_charset_de_v1_es_e21` |
| M52 | has_released: `true` (released == 0 vale) | muerta | `reputacion::pago_sin_plata_liberada_es_e23` |
| M53 | has_released: regla vieja (solo Released/PartiallyReleased) | muerta | `reputacion::con_plata_liberada_vale_en_cualquier_estado` |
| M54 | has_released: `> 1` | muerta | `reputacion::con_plata_liberada_vale_en_cualquier_estado` |
| M55 | has_released: Refunded rechazado | muerta | `reputacion::con_plata_liberada_vale_en_cualquier_estado` |
| M56 | callback: rol de la calificada cambiado | muerta | `reputacion::payer_califica_al_receiver_de_un_pago_released` |
| M57 | callback: tercero tratado como payer (sin E24) | muerta | `reputacion::el_rater_es_el_predecessor_no_el_signer` |
| M58 | callback: sin E25 | muerta | `reputacion::calificarse_a_si_mismo_es_e25` |
| M59 | callback: sin E18 | muerta | `reputacion::el_callback_vuelve_a_mirar_si_ya_califico_e18` |
| M60 | accept: cobro sin tope en el deposito | muerta | `reputacion::el_cobro_no_pasa_del_deposito` |
| M61 | accept: cobra la mitad | muerta | `reputacion::el_cobro_no_pasa_del_deposito` |
| M62 | rechazo: no devuelve el deposito | muerta | `reputacion::calificarse_a_si_mismo_es_e25` |
| M63 | Summary::add: no suma score_sum | muerta | `reputacion::payer_califica_al_receiver_de_un_pago_released` |
| M64 | accept: no indexa en received | muerta | `reputacion::revocar_marca_y_descuenta_del_resumen` |
| M65 | revoke: sin 1 yocto | muerta | `reputacion::revocar_sin_yocto` |
| M66 | revoke: sin E31 | muerta | `reputacion::revocar_dos_veces_es_e31` |
| M67 | Summary::revoke: no descuenta count | muerta | `reputacion::revocar_la_del_payer_descuenta_del_rol_payer` |
| M68 | revoke: no marca revoked_at | muerta | `reputacion::revocar_marca_y_descuenta_del_resumen` |
| M69 | vistas: limit sin tope 50 | muerta | `identidad::list_agents_pagina_en_orden_de_alta` |
| M70 | resumen: promedio x10 | muerta | `reputacion::payer_califica_al_receiver_de_un_pago_released` |
| M71 | is_feedback_pending: TTL `<=` | muerta | `reputacion::candado_vencido_se_pisa_y_el_callback_viejo_es_e26` |
| M72 | callback sin `#[private]` (sandbox) | muerta | `wr07_callback_privado` |
| M73 | new sin `#[private]` (sandbox) | muerta | `wr00_new_privado_y_sin_llaves` |
| M74 | Summary::add: no suma count | muerta | `reputacion::payer_califica_al_receiver_de_un_pago_released` |
| M75 | Summary::add: no actualiza last_feedback_id | muerta | `reputacion::revocar_marca_y_descuenta_del_resumen` |
| M76 | Summary::revoke: no suma revoked | muerta | `reputacion::revocar_la_del_payer_descuenta_del_rol_payer` |
| M77 | Summary::revoke: no resta score_sum | muerta | `reputacion::revocar_la_del_payer_descuenta_del_rol_payer` |
| M78 | EscrowPayment: `status` opcional (`Option`) | muerta | `reputacion::un_campo_de_menos_es_e21` |
| M79 | has_released: Reclaimed rechazado (sandbox, v1 compilada y de mainnet) | muerta | `wr04m_reclaimed_v1_de_mainnet` |
| M80 | has_released: Refunded rechazado (sandbox, v1 compilada y de mainnet) | muerta | `wr03m_rechazos_con_v1_de_mainnet` |
| M81 | accept: no suma al resumen por operador | muerta | `storage::ust1_peor_caso_de_una_calificacion_y_el_margen_de_feedback_max_bytes` |
| M82 | revoke: no descuenta del resumen por operador | muerta | `vistas::resumen_por_operador` |
| M83 | accept: resumen por operador con la clave equivocada | muerta | `storage::ust1_peor_caso_de_una_calificacion_y_el_margen_de_feedback_max_bytes` |
| M84 | get_summary_by_operator: lee otra clave | muerta | `vistas::resumen_por_operador_en_json` |
| M85 | FEEDBACK_MAX_BYTES sin el resumen por operador | muerta | `storage::ust1_peor_caso_de_una_calificacion_y_el_margen_de_feedback_max_bytes` |
| M86 | accept: no suma al resumen global | muerta | `reputacion::payer_califica_al_receiver_de_un_pago_released` |
| M87 | revoke: no descuenta del resumen global | muerta | `reputacion::revocar_la_del_payer_descuenta_del_rol_payer` |
| R04 | give_feedback: rater = signer | muerta | `reputacion::el_rater_es_el_predecessor_no_el_signer` |
| R04b | revoke_feedback: rater = signer | muerta | `reputacion::revocar_es_del_predecessor_no_del_signer` |
| R05 | register: cuenta = signer | muerta | `identidad::register_y_update_son_del_predecessor_no_del_signer` |
| R05b | update: cuenta = signer | muerta | `identidad::register_y_update_son_del_predecessor_no_del_signer` |
| R11 | rechazo: devuelve `feedback_deposit`, no lo adjuntado | muerta | `reputacion::un_rechazo_devuelve_todo_lo_adjuntado` |
| R11W | lo mismo, en el sandbox | muerta | `wr05_get_payment_que_falla_y_respuestas_raras` |
| N01 | EscrowPayment sin `deny_unknown_fields` | muerta | `reputacion::un_campo_de_mas_es_e21` |
| N01W | lo mismo, en el sandbox (los campos de v2 del mock) | muerta | `wr05_get_payment_que_falla_y_respuestas_raras` |
| N02 | status: cualquier string vale | muerta | `reputacion::un_rechazo_devuelve_todo_lo_adjuntado` |
| N03 | status: tambien la forma objeto (`{"Released":null}`) | muerta | `reputacion::un_status_que_no_es_de_v1_es_e21` |
| N04 | operator_id: sin charset (solo no vacio) | muerta | `reputacion::operator_id_fuera_del_charset_de_v1_es_e21` |
| N05 | numero: ceros a la izquierda | muerta | `reputacion::un_numero_no_canonico_es_e21` |
| N06 | numero: sin chequeo canonico | muerta | `reputacion::un_numero_no_canonico_es_e21` |
| N07 | numero: sin exigir solo digitos | muerta | `reputacion::un_numero_no_canonico_es_e21` |
| N08 | numero: vacio (sin `!is_empty`) | muerta | `validate::tests::enteros_canonicos` |
| N09 | amount: `U128` laxo | muerta | `reputacion::un_numero_no_canonico_es_e21` |
| N10 | released: `U128` laxo | muerta | `reputacion::un_numero_no_canonico_es_e21` |
| N11 | refunded: `U128` laxo | muerta | `reputacion::un_numero_no_canonico_es_e21` |
| N12 | created_at: `U64` laxo | muerta | `reputacion::un_numero_no_canonico_es_e21` |
| N13 | release_deadline: `U64` laxo | muerta | `reputacion::un_numero_no_canonico_es_e21` |
| N14 | reclaim_after: `U64` laxo | muerta | `reputacion::un_numero_no_canonico_es_e21` |
| N15 | storage_charged: `U128` laxo | muerta | `reputacion::un_numero_no_canonico_es_e21` |
| N16 | fee_bps: opcional (`serde(default)`) | muerta | `reputacion::un_campo_de_menos_es_e21` |
| N17 | sponsored: opcional | muerta | `reputacion::un_campo_de_menos_es_e21` |
| N18 | terms_hash: opcional | muerta | `reputacion::un_campo_de_menos_es_e21` |
| N19 | fee_recipient: `String` (sin validar la cuenta) | muerta | `reputacion::los_demas_tipos_de_v1_son_estrictos` |
| N20 | fee_bps: `u32` | muerta | `reputacion::los_demas_tipos_de_v1_son_estrictos` |
| N21 | sponsored: cualquier JSON | muerta | `reputacion::los_demas_tipos_de_v1_son_estrictos` |
| N22 | fee_bps_max: cualquier JSON | muerta | `reputacion::los_demas_tipos_de_v1_son_estrictos` |

**Refutador** (un subagente opus/xhigh sobre el diff, el encargo, la RESPUESTA y estos borradores). Ronda 1: sin P1;
corrio la suite (101 + 10 en verde), una sonda en WR-06 (la segunda tx cae en `E19`, no por un `E18` secuencial) y
comparo `contracts/escrow-v1/src` y `escrow-core/src` entre 8da7174 (el v1 de mainnet) y main: iguales.
- P2-1, el paso 1 del deploy no corria tal cual (el padre tiene ~0,074 NEAR libres y su llave esta en SSM): arreglado
  (pasos 1a y 1b, la llave por un pipe como en `mainnet.sh`).
- P2-2, el handoff no decia de quien depende la reputacion (el owner de v1 registra operadores; `get_summary` mezcla
  operadores): arreglado en documentacion ("Modelo de confianza", el flujo 4 de EM y el README). El resumen por
  `(ratee, operator_id)` en el contrato queda como opcion para c0der (PREGUNTA 4), sin implementar.
- P3: el candado (tambien vence por demora; el deposito de un callback fallido queda en el contrato), el tamano de la
  wasm (era el de 36b6a02), `javascript:` y bidi en URI/metadata (EM sanea), la cuenta recreada hereda la reputacion,
  el `git fetch` del paso 0, y el humo opcional antes de borrar llaves: todos al handoff.
Ronda 2 (solo la documentacion): P2-1 y P2-2 cerrados, sin P1 ni P2 nuevos; los P3 que quedaban (la frase vieja del
candado, el costo del ataque del owner, "a cualquier receiver", el saldo del padre como medida del ensayo, el
`sign-with-keychain` del payer en el 3b, "must filter" en el README) estan arreglados. **El resumen por
`(ratee, operator_id)` (RESPUESTA de 00:06Z, la respuesta a su P2-2) se hizo despues de sus dos rondas**: lo cubren
U-ST1, U-ST5, `resumen_por_operador*`, WR-02, WR-05 y los mutantes M81..M87, no el refutador.

**RONDA 1 de c0der** (refutador de RARRAS-7 sobre 651fa341: MERGEABLE CON RONDA, sin P1). Lo que pidio y donde quedo:
- P2-1, opcion (a) de c0der: espejo estricto del `Payment` de v1 (decision 5; tests `un_campo_de_menos_es_e21`,
  `un_campo_de_mas_es_e21`, `una_clave_repetida_es_e21`, `un_status_que_no_es_de_v1_es_e21`,
  `un_numero_no_canonico_es_e21`, `los_demas_tipos_de_v1_son_estrictos`, `operator_id_fuera_del_charset_de_v1_es_e21`,
  `los_numeros_canonicos_al_borde_valen`; mutantes N01..N22). WR-02..WR-04 contra la wasm exacta de mainnet, con
  `assert_espejo` en los 5 estados: el `get_payment` real de las dos v1 tiene los 16 campos y entra en el espejo.
- P2-2: los eventos cuentan solo si el recibo lo ejecuto el registro y termino con exito (con el MENSAJE de c0der de
  02:28Z); WR-10 lo muestra en el sandbox ("Modelo de confianza", flujo 4, README).
- P2-3: `el_rater_es_el_predecessor_no_el_signer`, `register_y_update_son_del_predecessor_no_del_signer`,
  `revocar_es_del_predecessor_no_del_signer` y `un_rechazo_devuelve_todo_lo_adjuntado` (y WR-05 con 3 depositos):
  R04, R05 y R11 mueren, y sus vecinos R04b, R05b y R11W tambien.
- P3-1 (`compact` lo puede llamar la calificada): "Modelo de confianza" y flujo 3. P3-2 (dos builds reproducibles,
  mismo sha256, `wc -c` con el umbral de ~245.000 bytes): paso 0 y 1b. P3-3 (humo obligatorio): paso 3b con el pago
  de R679. P3-4 (el resumen por operador sin el escrow en la clave): `lib.rs` y "Modelo de confianza". P3-5: README.
- En esta ronda no use un subagente refutador propio: la regla es como mucho uno y ya lo use en las rondas 1 y 2. Lo
  que respalda lo nuevo es mecanico: las dos wasm de v1 en el sandbox, WR-10 y los 115 mutantes.

## Trampas que encontre

- serde acepta una variante unitaria de un enum tambien como objeto (`{"Released":null}`), y `u128::from_str` acepta
  `+1` y `01`: por eso `EscrowStatus` se lee con `try_from = "String"` y los numeros con `canonical` (RONDA 1).

- El mock de near-sdk (5.29.1) vuelve `env::storage_usage()` a su valor inicial (307.200) en cada `testing_env!`: un
  test que mide storage antes y despues tiene que hacerlo dentro del mismo contexto (anotado en
  `unit_tests/common.rs`).
- `store::IterableMap` usa `Sha256` como hasher por defecto: la ficha se guarda con clave de 32 bytes y la cuenta
  aparece una sola vez en el storage (mi primera cuenta del peor caso de `register` daba 1.789; lo medido es 1.751).
- En el sandbox, los reembolsos de gas y el deposito de una tx que fallo llegan en recibos que se ejecutan despues de
  que `transact()` vuelve (el reembolso del deploy + `new` del registro, +0,0434 NEAR, aparece un bloque despues): los
  tests que miran saldos esperan unos bloques (`settle`).
- `compile_project` de near-workspaces relee el `Cargo.toml` del workspace en cada test: cambiar los miembros mientras
  corre la suite rompe los tests del sandbox de los otros crates (me paso en la linea base; se repitio limpia).
