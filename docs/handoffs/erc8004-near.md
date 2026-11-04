# Handoff: c0-arras-erc8004, el port fiel de ERC-8004 a NEAR (Identity, Reputation y Validation Registry)

Rama `0xultravioleta/c0-arras-erc8004`, desde `origin/main` 8cec4e6. Worker de la Mac, 2026-11-04 (encargo de c0der con
el CONTEXTO 1, las RESPUESTAS 1 y 2 y la RONDA 1 sobre la refutacion de RARRAS-8). El plan (tabla EVM -> NEAR completa, fuentes en commit fijo, cada desviacion con
su porque) es `docs/plans/erc8004-near-port.md`; este documento es lo que quedo, la interfaz para los SDK y EM, el plan
de deploy y lo que corri. **Nada se desplego** (ni testnet ni mainnet). `contracts/registry-v1` y
`registry.arras-escrow.near` no se tocaron: EM y los SDK siguen ahi hasta que esto pase el golden flow en mainnet.

## Que quedo

| Crate | Cuenta | Que es | Tests |
|---|---|---|---|
| `contracts/erc8004-common` | - | sin estado: validaciones (URIs de RFC 3986, claves, tags, hashes, enteros canonicos, `value`), el resumen de `getSummary` en 256 bits, el sobre NEP-297, y los esquemas estrictos de las llamadas cruzadas (`AgentCheck`, `FeedbackProof`) | 21 |
| `contracts/erc8004-identity` | `identity-registry.erc-8004.near` | ERC-8004 Identity: cada agente es un NFT (NEP-171/177/178/181), `agent_id` = `token_id` desde `"0"`, `agentURI` = `metadata.reference`, metadata clave -> bytes, `agentWallet` | 102 |
| `contracts/erc8004-reputation` | `reputation-registry.erc-8004.near` | ERC-8004 Reputation fiel, con la guarda de identidad por llamada cruzada que falla cerrado; con el feature `feedback-proofs` (por defecto), `give_feedback_with_proof` | 90 (65 sin el feature) + 14 del sandbox |
| `contracts/erc8004-arras-verifier` | `erc8004-verifier.arras-escrow.near` | el verificador de pruebas de Arras: un escrow fijo en `new`, espejo estricto del `Payment` de v1 | 19 |
| `contracts/erc8004-validation` | `validation-registry.erc-8004.near` (ya existe) | **BORRADOR, no desplegable**: el equipo de ERC-8004 no cerro la seccion | 27 |
| `contracts/test-nft-receiver` | - | receptor de NFT de mentira, solo para el sandbox | - |

`Cargo.toml` (miembros nuevos) y `Cargo.lock` (los crates nuevos; ninguna dependencia nueva ni cambiada de version:
`near-workspaces`, `tokio`, `sha2`, `serde_json` y `anyhow` ya estaban, como dev-dependencies de registry-v1).

## Decisiones (de quien y donde)

- **Validation no se despliega ahora** (CONTEXTO 1, punto 1): crate BORRADOR sin `reproducible_build` en su
  `Cargo.toml` (el build reproducible falla a proposito) y `get_config.status` = `"BORRADOR"`.
- **Un despliegue por cadena coordinado con el equipo de ERC-8004** (CONTEXTO 1, punto 2): el mainnet puede esperar esa
  coordinacion; testnet no.
- **La prueba de pago de Arras: verificadores aparte, opcion A** (el dueno, RESPUESTA 2): el Reputation Registry no
  tiene lista de escrows ni de verificadores; el de Arras es un contrato aparte con UN escrow fijo; la extension esta
  detras del feature `feedback-proofs` (sin el, el wasm es el port puro).
- **El verificador en `erc8004-verifier.arras-escrow.near`** (el dueno, RESPUESTA 2).
- **El alta del worker de EM por los dos caminos** (el dueno, RESPUESTA 2: "prrimero em paga el alta en el futuro el
  worker firma su alta"): salen con `register` + `nft_transfer` y con `register`; no se agrego ningun metodo (medido
  abajo).
- **`agentRegistry` = `near:mainnet:identity-registry.erc-8004.near`** sin registrar el namespace (el dueno, RESPUESTA 2).
- **Ronda 1 de c0der (refutacion de RARRAS-8):** una revocacion (`nft_revoke`, `nft_revoke_all`) cancela la
  `agentWallet` pendiente, opcion (a), con el evento `agent_wallet_pending` (I-9); cada wasm declara sus estandares en
  NEP-330 (identity `nep171`/`nep177`/`nep178`/`nep181` y `erc8004`; reputation y verificador `erc8004`); **el deploy en
  mainnet espera a que el repo este en `UltravioletaDAO/arras`** (paso 0c: el link NEP-330 queda grabado).
- Mias, con su porque en el plan: `agentWallet` por confirmacion de la wallet (2.3), la reserva por token para que una
  transferencia de 1 yocto nunca haga crecer el storage del contrato sin pagar (2.4), sin `setApprovalForAll` (I-3),
  `summary_value` exacto en 256 bits (R-4: un entero con signo de hasta 56 cifras), topes (6.1).

## Lo que los SDK (Python y TS) y EM tienen que llamar

Todo JSON. Enteros grandes como **string decimal canonico** (`agent_id`, `feedback_index`, `value`, `deadline`, montos):
sin signo `+`, sin ceros a la izquierda (`"01"` es un error), `"-0"` no vale. `value_decimals`, `response` y
`summary_value_decimals` van como numero. Los hashes, 64 hex en minusculas sin `0x` (o `null`). Los depositos
recomendados salen de `get_config` de cada contrato (no los hardcodeen: el contrato cobra lo medido y devuelve el resto).
Las URIs (`agent_uri`, `endpoint`, `feedback_uri`, `response_uri`, `request_uri`) tienen que traer **esquema**
(`https://...`, `ipfs://<cid>`, `data:...`) y solo caracteres de RFC 3986 (G-6): un CID pelado o un `endpoint` como
`"GetPrice"` se rechazan; el SDK los normaliza antes de mandar.

### Identity Registry (`identity-registry.erc-8004.near`)

| Metodo | Args JSON | Quien | Deposito | Gas (recomendado / medido) | Devuelve |
|---|---|---|---|---|---|
| `register` | `{"agent_uri"?: str, "metadata"?: [{"metadata_key": str, "metadata_value": base64}]}` (hasta 4) | cualquiera (queda dueno y `agentWallet`) | `get_config.register_deposit` (peor caso, 0,13279 NEAR); se cobra lo medido + 77 bytes de reserva (~0,0055 NEAR con una URI corta) | 30 / 3,1 TGas | `agent_id` |
| `set_agent_uri` | `{"agent_id", "new_uri"}` (`""` borra) | dueno o aprobado | >= 1 yocto + lo que crezca | 30 TGas | - |
| `set_metadata` | `{"agent_id", "metadata_key", "metadata_value": base64}` (vacio borra) | dueno o aprobado | >= 1 yocto + lo que crezca | 30 TGas | - |
| `set_agent_wallet` | `{"agent_id", "new_wallet", "deadline": ns}` con `now <= deadline <= now + 5 min` | dueno o aprobado | exactamente 1 yocto | 30 / 2,0 TGas | `true` si quedo puesta (la wallet es el que llama), `false` si quedo pendiente (evento `agent_wallet_pending`) |
| `accept_agent_wallet` | `{"agent_id"}` | la wallet pendiente | exactamente 1 yocto | 30 TGas | - |
| `unset_agent_wallet` | `{"agent_id"}` | dueno o aprobado | exactamente 1 yocto | 30 TGas | - |
| `nft_transfer` | `{"receiver_id", "token_id", "approval_id"?, "memo"?}` | dueno o aprobado | exactamente 1 yocto | 30 / 2,8 TGas | - |
| `nft_transfer_call` | `{... , "msg"}` | idem | exactamente 1 yocto | >= 50 / 7,7 TGas | `true` si quedo en el receptor |
| `nft_approve` | `{"token_id", "account_id", "msg"?}` | solo el dueno | >= 1 yocto + lo que crezca | 30 TGas | promesa de `nft_on_approve` si hay `msg` |
| `nft_revoke` / `nft_revoke_all` | `{"token_id", "account_id"}` / `{"token_id"}` | solo el dueno | exactamente 1 yocto | 30 TGas | - ; **cancelan la `agentWallet` pendiente** (I-9), la haya propuesto quien sea |

Vistas: `nft_token({"token_id"})`, `nft_tokens({"from_index"?: U128, "limit"?})`, `nft_supply_for_owner`,
`nft_tokens_for_owner`, `nft_total_supply`, `nft_metadata`, `nft_is_approved`, `token_uri({"agent_id"})`,
`get_metadata({"agent_id", "metadata_key"})` (base64 o `null`; con `agentWallet`, los bytes de la cuenta),
`get_agent_wallet({"agent_id"})`, `get_pending_agent_wallet({"agent_id"})` (`{"wallet","deadline"}` o `null`),
`is_authorized_or_owner({"spender", "agent_id"})` (panic si no existe), `check_agent({"agent_id", "account_id"})`,
`get_version` (`"2.0.0"`), `get_config`.

```json
nft_token   {"token_id":"0","owner_id":"worker.near","metadata":{"title":null,"description":null,"media":null,
             "media_hash":null,"copies":null,"issued_at":null,"expires_at":null,"starts_at":null,"updated_at":null,
             "extra":null,"reference":"https://em.example/workers/w.json","reference_hash":null},
             "approved_account_ids":{}}
check_agent {"owner_id":"worker.near","agent_wallet":null,"authorized":false}
```

### Reputation Registry (`reputation-registry.erc-8004.near`)

| Metodo | Args JSON | Quien | Deposito | Gas (minimo / medido) | Devuelve |
|---|---|---|---|---|---|
| `give_feedback` | `{"agent_id", "value": "-32", "value_decimals": 1, "tag1"?, "tag2"?, "endpoint"?, "feedback_uri"?, "feedback_hash"?}` | cualquiera que no sea dueno ni aprobado | `get_config.feedback_deposit` (0,00591 NEAR); se cobra lo medido (~0,0032), todo de vuelta si se rechaza | >= 40 (recomendado 60) / 7,3 TGas | `feedback_index` o `null` + log `erc8004:rechazo:<CODIGO>` |
| `give_feedback_with_proof` | lo mismo + `"verifier": acc, "proof": str` | idem | `get_config.feedback_proof_deposit` (0,01472 NEAR) | >= 120 (recomendado 150) / 14,0 TGas | idem |
| `revoke_feedback` | `{"agent_id", "feedback_index"}` | el cliente | exactamente 1 yocto | 30 TGas | - |
| `append_response` | `{"agent_id", "client_address", "feedback_index", "response_uri", "response_hash"?}` | cualquiera | >= 1 yocto + lo que crezca (~0,005 NEAR el primero de cada respondedor) | 30 / 3,0 TGas | - |

Vistas: `get_summary({"agent_id", "client_addresses": [..] (no vacia), "tag1"?, "tag2"?})` ->
`{"count":"2","summary_value":"85","summary_value_decimals":0}` (`summary_value` es un entero con signo de hasta 56
cifras: no entra en `int128` ni en un numero JSON; `[(1e38, 0), (1, 1), (1, 1)]` da `"333333333333333333333333333333333333334"`
con 1 decimal, donde EVM da vuelta el signo: R-4); `read_feedback({"agent_id", "client_address",
"feedback_index"})` -> `{"value","value_decimals","tag1","tag2","is_revoked"}` o `null`;
`read_all_feedback({"agent_id", "client_addresses"?, "tag1"?, "tag2"?, "include_revoked"?})` (los 7 arreglos de EVM);
`get_response_count({"agent_id", "client_address"?, "feedback_index"?, "responders"?})`;
`get_clients({"agent_id", "from_index"?, "limit"?})`; `get_last_index({"agent_id", "client_address"})`;
`get_identity_registry`, `get_version`, `get_config`. Con pruebas: `read_feedback_proof({"agent_id",
"client_address", "feedback_index"})`, `get_proof_feedback({"verifier", "proof", "client_address"})`,
`is_proof_pending({...})`, `list_verified_feedback({"agent_id", "verifier", "from_index"?, "limit"?})`,
`get_verified_summary({"agent_id", "verifier", "client_addresses"?, "tag1"?, "tag2"?, "context"?})` ->
`{"count","summary_value","summary_value_decimals","weight_sum"}`.

### Verificador de Arras (`erc8004-verifier.arras-escrow.near`)

`verify_feedback_proof({"client", "proof": <payment_id>})` -> `{"counterparty","role","context","weight"}` o falla.
Lo llama el Reputation Registry; un SDK lo puede llamar para saber de antemano si la prueba va a pasar (es una lectura
con un callback: cuesta gas, no deposito). `get_config` -> `{"contract","escrow",...}`.

### Validation Registry (BORRADOR, no desplegado)

`validation_request({"validator_address", "agent_id", "request_uri", "request_hash"})` (deposito
`get_config.request_deposit`, gas >= 40), `validation_response({"request_hash", "response": 0..100, "response_uri"?,
"response_hash"?, "tag"?})` (>= 1 yocto), vistas `get_validation_status`, `get_summary`, `get_agent_validations`,
`get_validator_requests`.

### Eventos (un elemento en `data`; los strings exactos de los tests)

```
EVENT_JSON:{"standard":"nep171","version":"1.0.0","event":"nft_mint","data":[{"owner_id":"alice.near","token_ids":["0"]}]}
EVENT_JSON:{"standard":"erc8004","version":"1.0.0","event":"registered","data":[{"agent_id":"0","agent_uri":"","owner":"alice.near"}]}
EVENT_JSON:{"standard":"erc8004","version":"1.0.0","event":"metadata_set","data":[{"agent_id":"0","metadata_key":"agentWallet","metadata_value":"YWxpY2UubmVhcg=="}]}
EVENT_JSON:{"standard":"erc8004","version":"1.0.0","event":"uri_updated","data":[{"agent_id":"0","new_uri":"ipfs://bafy","updated_by":"alice.near"}]}
EVENT_JSON:{"standard":"erc8004","version":"1.0.0","event":"metadata_set","data":[{"agent_id":"0","metadata_key":"agentWallet","metadata_value":""}]}
EVENT_JSON:{"standard":"nep171","version":"1.0.0","event":"nft_transfer","data":[{"old_owner_id":"alice.near","new_owner_id":"bob.near","token_ids":["0"],"memo":"venta"}]}
EVENT_JSON:{"standard":"erc8004","version":"1.0.0","event":"agent_wallet_pending","data":[{"agent_id":"0","wallet":"wallet.near","deadline":"1793491500000000000"}]}
EVENT_JSON:{"standard":"erc8004","version":"1.0.0","event":"agent_wallet_pending","data":[{"agent_id":"0","wallet":null,"deadline":"0"}]}
EVENT_JSON:{"standard":"erc8004","version":"1.0.0","event":"new_feedback","data":[{"agent_id":"7","client_address":"cliente.near","feedback_index":"1","value":"87","value_decimals":0,"tag1":"starred","tag2":"","endpoint":"https://agente.example/mcp","feedback_uri":"ipfs://bafyfeedback","feedback_hash":"00ff...6c","proof":null}]}
EVENT_JSON:{"standard":"erc8004","version":"1.0.0","event":"new_feedback","data":[{..., "proof":{"verifier":"erc8004-verifier.arras-escrow.near","proof":"<payment_id>","counterparty":"worker.near","role":"receiver","context":"em","weight":"1000000"}}]}
EVENT_JSON:{"standard":"erc8004","version":"1.0.0","event":"feedback_revoked","data":[{"agent_id":"7","client_address":"cliente.near","feedback_index":"1"}]}
EVENT_JSON:{"standard":"erc8004","version":"1.0.0","event":"response_appended","data":[{"agent_id":"7","client_address":"cliente.near","feedback_index":"1","responder":"dueno.near","response_uri":"ipfs://bafyrespuesta","response_hash":"00ff...6c"}]}
EVENT_JSON:{"standard":"erc8004","version":"1.0.0","event":"validation_request","data":[{"validator_address":"validador.near","agent_id":"7","request_uri":"ipfs://bafypedido","request_hash":"00ff...6c"}]}
EVENT_JSON:{"standard":"erc8004","version":"1.0.0","event":"validation_response","data":[{"validator_address":"validador.near","agent_id":"7","request_hash":"00ff...6c","response":100,"response_uri":"ipfs://bafyprueba","response_hash":"1111...11","tag":"soft"}]}
```

`register` emite, en orden: `nft_mint`, `registered`, `metadata_set` (`agentWallet`) y un `metadata_set` por entrada.
Una transferencia emite `metadata_set` (`agentWallet`, `""`) y despues `nft_transfer` (el orden de `_update` en EVM);
la devolucion de un `nft_transfer_call` emite otro par. `set_agent_wallet` que deja una pendiente emite
`agent_wallet_pending` con la wallet y su `deadline` (ns); `nft_revoke`/`nft_revoke_all` que cancelan una emiten el
mismo evento con `wallet` `null` (I-9). Para un indexador, una pendiente vale hasta su `deadline` salvo que la cancele
un evento posterior del mismo agente: `metadata_set` de `agentWallet`, `nft_transfer` u otro `agent_wallet_pending`.
Como en registry-v1: un indexador cuenta solo los eventos de
recibos cuyo `executor_id` es el contrato y cuyo resultado final es exito (cualquiera puede emitir el mismo
`EVENT_JSON` desde su propio contrato).

### Codigos

Panic (la tx falla, el runtime devuelve el deposito): Identity `ERC8004_ID_E01` agent_uri, `E02` clave de metadata,
`E03` mas de 4 entradas, `E04` `agentWallet` es reservada, `E05` valor de metadata largo, `E06` deposito corto (dice
cuanto), `E07` sin yocto, `E10` agent_id no canonico, `E11` agente inexistente, `E12` `Not authorized`, `E13` deadline
no canonico, `E14` `expired`, `E15` `deadline too far`, `E16` no hay wallet pendiente, `E17` no es la wallet pendiente,
`E18` memo largo, `E19` approval_id, `E20` el receptor ya es el dueno (I-10: en EVM transferirse a si mismo pasa), `E21` gas de `nft_transfer_call`, `E22` solo el
dueno, `E23` mas de 8 aprobaciones. Reputation `ERC8004_REP_E01` agent_id, `E02` value, `E03` `value too large`,
`E04` `too many decimals`, `E05` tag, `E06` endpoint, `E07` feedback_uri, `E08` hash, `E09` deposito corto, `E10` gas,
`E11` feedback_index no canonico, `E12` `index must be > 0`, `E13` `index out of bounds`, `E14` `Already revoked`,
`E15` sin yocto, `E16` response_uri, `E17` identity_registry invalido, `E30` `clientAddresses required`, `E40` proof,
`E41` el registro como verificador, `E42` ya califico con esa prueba, `E43` hay otra en vuelo. Verificador
`ERC8004_ARV_E01`..`E15`. Validation `ERC8004_VAL_E01`..`E15`.

Rechazo de un callback (la tx sale bien, devuelve `null`/`false`, el deposito vuelve entero, log
`erc8004:rechazo:<CODIGO>`): `E20_VISTA_DE_IDENTIDAD_FALLO`, `E21_RESPUESTA_DE_IDENTIDAD_INVALIDA`,
`E22_AGENTE_INEXISTENTE`, `E23_AUTO_CALIFICACION` (Validation: `E23_NO_AUTORIZADO`), y con pruebas
`E42_PRUEBA_YA_USADA`, `E44_REEMPLAZADO`, `E45_VERIFICADOR_FALLO` (el verificador entro en panico: el motivo esta en su
recibo, p. ej. `ERC8004_ARV_E13: el pago no libero plata`), `E46_RESPUESTA_DEL_VERIFICADOR_INVALIDA`,
`E47_NO_ES_EL_AGENTE`, `E48_AGENT_WALLET_CALIFICA`.

## Lo que EM tiene que llamar, camino por camino

Cuentas (mainnet): `identity-registry.erc-8004.near`, `reputation-registry.erc-8004.near`,
`erc8004-verifier.arras-escrow.near` (verificador de `v1.arras-escrow.near`). Hasta que estos pasen el golden flow en
mainnet, EM sigue con `registry.arras-escrow.near` (registry-v1) como hoy.

1. **Alta del worker, hoy (EM paga; RESPUESTA 2 del dueno):** EM firma dos tx, el worker ninguna.
   1. `register({"agent_uri": "<registration file del worker>"})` en el Identity Registry con
      `get_config.register_deposit` adjunto (vuelve el sobrante; se cobra ~0,0055 NEAR con una URI de 40 bytes) y 30
      TGas -> devuelve el `agent_id`.
   2. `nft_transfer({"receiver_id": "<cuenta de cobro del worker>", "token_id": "<agent_id>"})` con 1 yocto y 30 TGas.
      El worker queda dueno; `agentWallet` queda vacia (la transferencia la limpia, como en EVM).
   Medido en el sandbox (WE-01): register 3,1 TGas + nft_transfer 2,8 TGas; EM gasto 0,00601 NEAR en total (storage
   y gas). Si el worker quiere su `agentWallet`, es una tx suya: `set_agent_wallet({"agent_id", "new_wallet": <el
   mismo>, "deadline": <ahora + menos de 5 min, en ns>})` con 1 yocto (2,0 TGas).
2. **Alta del worker, despues (el worker firma):** el worker llama `register` con su cuenta: una tx (3,1 TGas,
   0,00574 NEAR medido). Queda dueno y `agentWallet`.
3. **El agente que paga tambien es un agente** (para que el worker lo califique): `register` con la cuenta NEAR del
   agente.
4. **Calificar despues de un pago liberado en v1.** Las dos partes:
   - el agente (payer) califica al worker: `give_feedback_with_proof({"agent_id": <agente del worker>, "value": "90",
     "value_decimals": 0, "tag1": "starred", "verifier": "erc8004-verifier.arras-escrow.near", "proof": <payment_id>})`
     con `get_config.feedback_proof_deposit` y 150 TGas (minimo 120; mide 14,0);
   - el worker califica al agente: lo mismo con el `agent_id` del agente y la firma del worker.
   La prueba pasa si el pago existe en v1, libero plata (`released > 0`, en cualquier estado), quien califica es el
   payer o el receiver, y la otra parte es el **dueno o la `agentWallet`** del agente calificado. Una calificacion por
   `(verificador, payment_id, cliente)`. Devuelve el `feedback_index` o `null` (el motivo en el log
   `erc8004:rechazo:<CODIGO>`; con `E45`, el motivo del verificador esta en su recibo). La ventana es la misma que en
   registry-v1: hasta que alguien compacte el pago en v1 (`reclaim_after + 7 dias`).
   Una calificacion sin prueba (`give_feedback`) tambien vale para ERC-8004, pero no entra al resumen verificado.
5. **Mostrar reputacion.** `get_verified_summary({"agent_id", "verifier": "erc8004-verifier.arras-escrow.near",
   "context": "em"})` (solo las calificaciones con un pago de `em` liberado, sin revocadas; `weight_sum` = lo liberado,
   en unidades de USDC) y, para el detalle, `list_verified_feedback` + `read_feedback`/`read_feedback_proof`, o el
   indexador de `new_feedback` con `proof.verifier == "erc8004-verifier.arras-escrow.near"` y `proof.context == "em"`
   (contando solo recibos ejecutados por `reputation-registry.erc-8004.near` con exito). Para la reputacion "abierta"
   de ERC-8004, `get_summary` con la lista de clientes en los que EM confie.

## Modelo de confianza

- El nucleo es el de ERC-8004: cualquiera califica a cualquier agente que exista; lo unico que impide es que el dueno
  o un aprobado se califiquen a si mismos. `get_summary` exige la lista de clientes porque sin ella es atacable por
  sybil (la EIP lo dice asi). Un cliente puede crear cuentas a voluntad.
- Una calificacion con prueba vale lo que vale el verificador. No hay lista de verificadores en el registro: cualquier
  contrato puede serlo, y uno malicioso puede "probar" cualquier cosa. Quien lee filtra por el verificador en el que
  confia (EM: `erc8004-verifier.arras-escrow.near`), igual que se filtra por cliente.
- El verificador de Arras hereda el modelo de confianza de registry-v1 (su handoff, "Modelo de confianza"): una
  prueba vale lo que valen los pagos de v1, y en v1 el owner registra los operadores. Por eso EM filtra por
  `context == "em"` (solo los pagos que libero su capturer) y pesa por `weight` (lo liberado) y por clientes
  distintos.
- La identidad es el NFT: quien lo tenga (tambien un comprador) hereda la reputacion recibida. La transferencia limpia
  `agentWallet` y las aprobaciones; no borra el historial.
- **La prueba ata al dueno o la `agentWallet` actual, no al agente** (plan, 3.3): el verificador devuelve la otra parte
  del pago y el registro exige que sea el dueno o la `agentWallet` que tiene el agente cuando corre el callback; la
  unicidad es `(verificador, payment_id, cliente)`, sin el agente. Con un dueno de varios agentes, el cliente elige a
  cual le suma un pago (una vez); despues de una transferencia, los pagos viejos del dueno nuevo cuentan para ese
  agente. Para EM (un agente por worker, el worker es el dueno) no cambia nada; quien muestre la reputacion verificada
  de un agente que cambio de manos lo ve en los `nft_transfer`.
- Una `agentWallet` pendiente no sobrevive a una revocacion (I-9) ni a una transferencia; la cancelacion queda en los
  logs (`agent_wallet_pending` con `wallet` `null`).
- Lo liberado vuelve al dueno actual (S-1), aunque lo haya pagado un dueno anterior o un aprobado; el contrato queda
  solvente igual (solo devuelve storage que se libera).
- `agent_uri`, los tags y las URIs de feedback aceptan lo que la spec permite (dentro de los topes): EM y los SDK
  sanean antes de mostrar (enlaces solo `https:`, `ipfs:`, `ar:`; los tags son UTF-8 sin caracteres de control y
  pueden traer bidi o ancho cero).
## Plan de deploy (lo corre c0der: primero testnet, despues mainnet; yo no desplegue nada)

near-cli-rs 0.30.1, firmando con el llavero legacy como `scripts/mainnet.sh`. Tres cuentas nuevas y una que ya existe:

| Cuenta (mainnet) | Padre | Contrato | `new` | Fondeo |
|---|---|---|---|---|
| `identity-registry.erc-8004.near` | `erc-8004.near` (de c0der; 0,0729 NEAR a las 14:02Z) | `erc8004-identity` | `{}` | 2,8 NEAR (era 2,7: la wasm de 9457b3a mide 1.305 bytes mas) |
| `reputation-registry.erc-8004.near` | `erc-8004.near` | `erc8004-reputation` (con `feedback-proofs`) | `{"identity_registry":"identity-registry.erc-8004.near"}` | 2,9 NEAR |
| `erc8004-verifier.arras-escrow.near` | `arras-escrow.near` (llave en SSM `/arras/mainnet/padre`; 0,1670 NEAR a las 15:00Z) | `erc8004-arras-verifier` | `{"escrow":"v1.arras-escrow.near"}` | 1,3 NEAR |
| `validation-registry.erc-8004.near` | ya existe (0,02 NEAR, la llave del padre) | `erc8004-validation` | **no ahora** (BORRADOR) | - |

El fondeo de cada cuenta es su storage medido (WE-11: wasm reproducible + los bytes de la cuenta y del estado de `new`,
sin la llave: identity 260.240, reputation 274.135, verificador 119.327, con la wasm reproducible de 9457b3a y los
178 / 212 / 164 bytes de WE-11; con la llave son 82 bytes mas cada una) mas 0,1 NEAR, redondeado hacia arriba a la
decena: 2,7024 -> 2,8, 2,84135 -> 2,9, 1,29327 -> 1,3. El gas del deploy y del borrado de llaves lo paga la propia cuenta y es menos de 0,01 NEAR (desplegar ~270 KB
quema ~19 TGas); lo demas es margen para una wasm algo mas grande. Los usuarios pagan su propio storage (lo medido, con
el sobrante de vuelta), asi que el contrato no necesita mas que eso.

```sh
# 0. Wasm reproducible (NEP-330), DOS veces desde DOS clones limpios del merge commit de este PR, SIN CARGO_TARGET_DIR
#    (cargo-near 0.22.0 borra el artefacto con CARGO_TARGET_DIR seteado). Imagen sourcescan/cargo-near por digest
#    (la del Cargo.toml de cada crate). Antes, `git fetch` en el checkout de origen: tiene que tener el merge commit.
REV=<MERGE_SHA>
ORIGEN="$(git -C <tu checkout de arras> rev-parse --show-toplevel)"
for n in 1 2; do
  D="$HOME/arras-build/erc8004-$REV-$n"
  git clone -q "$ORIGEN" "$D" && git -C "$D" checkout -q "$REV" && test -z "$(git -C "$D" status --porcelain)"
  for c in erc8004-identity erc8004-reputation erc8004-arras-verifier; do
    ( cd "$D" && env -u CARGO_TARGET_DIR cargo near build reproducible-wasm --manifest-path contracts/$c/Cargo.toml )
  done
done
for c in erc8004_identity erc8004_reputation erc8004_arras_verifier; do
  shasum -a 256 "$HOME"/arras-build/erc8004-"$REV"-{1,2}/target/near/$c/$c.wasm; wc -c < "$HOME/arras-build/erc8004-$REV-1/target/near/$c/$c.wasm"
done
#    -> cada par con el MISMO sha256 (si difieren, se para aca); ese sha256 es el code hash del paso 3. Los largos
#       tienen que andar cerca de los de esta rama (260.062 / 273.923 / 119.163 bytes). Con el fondeo de la tabla y
#       0,01 NEAR para el gas, la wasm entra hasta 278.740 / 288.706 / 128.754 bytes (sin olvidar los 82 bytes de la
#       llave, que sigue puesta durante el deploy y el humo); si alguna mide mas, su fondeo
#       sube a (wasm + 178 / 212 / 164 bytes) / 100.000 + 0,1 NEAR, redondeado hacia arriba a la decena.
B="$HOME/arras-build/erc8004-$REV-1/target/near"
W_ID=$B/erc8004_identity/erc8004_identity.wasm
W_REP=$B/erc8004_reputation/erc8004_reputation.wasm
W_VER=$B/erc8004_arras_verifier/erc8004_arras_verifier.wasm

# 0b. (CONTEXTO 1, punto 2) En mainnet: el dueno decide si espera la coordinacion con el equipo de ERC-8004
#     (team@8004.org) antes de seguir. Testnet no la espera.

# 0c. (Ronda 1 de c0der) En mainnet: NO se sigue hasta que el repo este en UltravioletaDAO/arras. Cada wasm graba en
#     NEP-330 el link https://github.com/UltravioletaDAO/arras/tree/<REV> (el `repository` del Cargo.toml) y un
#     contrato inmutable no lo puede cambiar. c0der se lo pide al dueno. Testnet no lo espera.
gh repo view UltravioletaDAO/arras --json nameWithOwner,visibility
#    -> {"nameWithOwner":"UltravioletaDAO/arras",...}; si da "Could not resolve to a Repository", se para aca. Y el REV
#       del paso 0 tiene que estar en ese repo: gh api repos/UltravioletaDAO/arras/commits/$REV --jq .sha

# 1. Fondear los padres. erc-8004.near necesita 2,8 + 2,9 + 0,03 de gas; arras-escrow.near, 1,3 + 0,03.
near account view-account-summary erc-8004.near network-config mainnet now
near account view-account-summary arras-escrow.near network-config mainnet now

# 2. Las cuentas (la llave nueva de cada una queda en el llavero legacy: la usan los pasos 3 y 6).
near account create-account fund-myself identity-registry.erc-8004.near '2.8 NEAR' autogenerate-new-keypair \
  save-to-legacy-keychain sign-as erc-8004.near network-config mainnet sign-with-legacy-keychain send
near account create-account fund-myself reputation-registry.erc-8004.near '2.9 NEAR' autogenerate-new-keypair \
  save-to-legacy-keychain sign-as erc-8004.near network-config mainnet sign-with-legacy-keychain send
#    (si la llave de erc-8004.near no esta en el llavero legacy, el mismo comando con la firma que use c0der)
near account create-account fund-myself erc8004-verifier.arras-escrow.near '1.3 NEAR' autogenerate-new-keypair \
  save-to-legacy-keychain sign-as arras-escrow.near network-config mainnet \
  sign-with-access-key-file <(aws ssm get-parameter --region us-east-2 --name /arras/mainnet/padre \
    --with-decryption --output json | jq -j '.Parameter.Value') send

# 3. Deploy + `new` privado en UNA tx por cuenta (`new` es #[private]: nadie mas lo puede llamar).
near contract deploy identity-registry.erc-8004.near use-file "$W_ID" with-init-call new json-args '{}' \
  prepaid-gas '30.0 Tgas' attached-deposit '0 NEAR' network-config mainnet sign-with-legacy-keychain send
near contract deploy reputation-registry.erc-8004.near use-file "$W_REP" with-init-call new \
  json-args '{"identity_registry":"identity-registry.erc-8004.near"}' \
  prepaid-gas '30.0 Tgas' attached-deposit '0 NEAR' network-config mainnet sign-with-legacy-keychain send
near contract deploy erc8004-verifier.arras-escrow.near use-file "$W_VER" with-init-call new \
  json-args '{"escrow":"v1.arras-escrow.near"}' \
  prepaid-gas '30.0 Tgas' attached-deposit '0 NEAR' network-config mainnet sign-with-legacy-keychain send

# 4. Verificar ANTES de borrar llaves: code hash == el sha256 del paso 0, la config y NEP-330.
for a in identity-registry.erc-8004.near reputation-registry.erc-8004.near erc8004-verifier.arras-escrow.near; do
  near account view-account-summary $a network-config mainnet now
  near contract call-function as-read-only $a get_config json-args '{}' network-config mainnet now
  near contract call-function as-read-only $a contract_source_metadata json-args '{}' network-config mainnet now
done
#    -> identity: "contract": "erc8004-identity", "erc8004_version": "2.0.0", "total_agents": "0";
#       reputation: "identity_registry": "identity-registry.erc-8004.near", "feedback_deposit": "5910000000000000000000",
#       "feedback_proof_deposit": "14720000000000000000000"; verifier: "escrow": "v1.arras-escrow.near".
#       contract_source_metadata: "link": "https://github.com/UltravioletaDAO/arras/tree/<REV>", "version": "0.1.0",
#       build_info con la imagen por digest, y "standards": identity nep171, nep177, nep178, nep181 y erc8004 en
#       "1.0.0" y nep330 "1.3.0"; reputation y verificador erc8004 "1.0.0" y nep330 "1.3.0" (los mira WE-11).

# 5. Humo OBLIGATORIO antes de borrar llaves (las calificaciones quedan para siempre): el ciclo entero con el pago real
#    de R679 (el mismo del paso 3b de registry-v1). Prueba que el espejo del verificador lee el `Payment` real de v1 y
#    que las tres cuentas se hablan.
PID=EbkEtGaM7ZwY6aHSdHSqxdFx9cJV13KPT7fYBWjxbfd9
PAYER=fe0a574258f8277099875e46d4ca0043cdb6274ff2501b375a0d3d64bc211079      # SSM /arras/mainnet/fondeo
RECEIVER=826e8c640ba0ee77bf255ecb332dfaca73fc1e3aef2dbf370b5ffeb30f9ac7af
near contract call-function as-read-only v1.arras-escrow.near get_payment json-args "{\"payment_id\":\"$PID\"}" \
  network-config mainnet now
#    -> "status": "Released", "released": "100000". Si da null (compactado), otro pago liberado de em sin compactar
#       cuyas dos partes firme c0der; sin uno, no se borran las llaves.
# 5a. El alta del worker como la hace EM hoy: el payer paga el alta y se la transfiere al receiver. Sin `agent_uri`
#     (el `register()` de EVM): el agente queda real y sin un registration file inventado; el worker le pone su URI
#     cuando quiera con `set_agent_uri`.
near contract call-function as-transaction identity-registry.erc-8004.near register json-args '{}' \
  prepaid-gas '30.0 Tgas' attached-deposit '0.13279 NEAR' sign-as "$PAYER" network-config mainnet \
  sign-with-access-key-file <(aws ssm get-parameter --region us-east-2 --name /arras/mainnet/fondeo \
    --with-decryption --output json | jq -j '.Parameter.Value') send
#    -> el agent_id (string). Normalmente "0", pero alguien pudo registrarse antes: se usa el que devolvio.
AGENT=<el agent_id que devolvio register>
near contract call-function as-transaction identity-registry.erc-8004.near nft_transfer \
  json-args "{\"receiver_id\":\"$RECEIVER\",\"token_id\":\"$AGENT\"}" prepaid-gas '30.0 Tgas' attached-deposit '1 yoctoNEAR' \
  sign-as "$PAYER" network-config mainnet sign-with-access-key-file <(...la misma de fondeo...) send
# 5b. El payer califica al agente del worker con la prueba del pago.
near contract call-function as-transaction reputation-registry.erc-8004.near give_feedback_with_proof \
  json-args "{\"agent_id\":\"$AGENT\",\"value\":\"100\",\"value_decimals\":0,\"tag1\":\"starred\",\"verifier\":\"erc8004-verifier.arras-escrow.near\",\"proof\":\"$PID\"}" \
  prepaid-gas '150.0 Tgas' attached-deposit '0.01472 NEAR' sign-as "$PAYER" network-config mainnet \
  sign-with-access-key-file <(...la misma de fondeo...) send
#    -> "1" (el feedback_index). Si da null, el motivo esta en el log `erc8004:rechazo:<CODIGO>` (con E45, en el
#       recibo del verificador: p. ej. ERC8004_ARV_E11 es que el espejo no coincide con lo que serializa v1).
near contract call-function as-read-only reputation-registry.erc-8004.near read_feedback_proof \
  json-args "{\"agent_id\":\"$AGENT\",\"client_address\":\"$PAYER\",\"feedback_index\":\"1\"}" network-config mainnet now
#    -> "counterparty": $RECEIVER, "role": "receiver", "context": "em", "weight": "100000"
#    SI EL HUMO NO DA ESTO, NO SE BORRAN LAS LLAVES Y SE REDESPLIEGA.

# 6. Sin llaves (irreversible: los tres quedan inmutables). Todas las de list-keys, separadas por coma.
for a in identity-registry.erc-8004.near reputation-registry.erc-8004.near erc8004-verifier.arras-escrow.near; do
  near account list-keys $a network-config mainnet now
done
near account delete-keys identity-registry.erc-8004.near public-keys '<las de list-keys>' network-config mainnet \
  sign-with-legacy-keychain send
near account delete-keys reputation-registry.erc-8004.near public-keys '<las de list-keys>' network-config mainnet \
  sign-with-legacy-keychain send
near account delete-keys erc8004-verifier.arras-escrow.near public-keys '<las de list-keys>' network-config mainnet \
  sign-with-legacy-keychain send
for a in identity-registry.erc-8004.near reputation-registry.erc-8004.near erc8004-verifier.arras-escrow.near; do
  near account list-keys $a network-config mainnet now      # -> tabla vacia
done
# 7. Los archivos de las llaves borradas en cadena ya no sirven: se borran del llavero legacy.
```

**Testnet (primero).** Lo mismo con `network-config testnet` y estas cuentas: padre `erc-8004.testnet` (NO EXISTE a las
15:00Z: c0der la crea, p. ej. `near account create-account sponsor-by-faucet-service erc-8004.testnet
autogenerate-new-keypair save-to-legacy-keychain network-config testnet create`, y la fondea con 5,8 NEAR),
hijas `identity-registry.erc-8004.testnet` y `reputation-registry.erc-8004.testnet`, y el verificador
`erc8004-verifier.arras-dev.testnet` (padre `arras-dev.testnet`, 1,3352 NEAR a las 15:00Z: crear la cuenta con 1,3 le
deja 0,035 para el gas, justo; conviene fondearlo con 0,1 NEAR mas antes) con
`new` `{"escrow":"arras-v1b-1101.testnet"}` (el v1 de demo, sin llaves, con el mismo code hash que mainnet,
`8ZYfMJer...`). El humo del paso 5 en testnet va con un pago nuevo: `scripts/testnet.sh 8` y `9` sobre
`arras-v1b-1101.testnet` (authorize de `payer-demo.arras-dev.testnet` a `worker-demo.arras-dev.testnet` y release del
capturer), y el `agentRegistry` del registration file es `near:testnet:identity-registry.erc-8004.testnet`.

**Validation, despues.** Cuando el equipo de ERC-8004 cierre la seccion: revisar el crate contra la spec final, sumarle
el bloque `reproducible_build`, fondear `validation-registry.erc-8004.near` hasta 2,1 NEAR (hoy tiene 0,02), y solo
los pasos 0, 3, 4 y 6 (la cuenta ya existe: no hay paso 2) con `new` `{"identity_registry":"identity-registry.erc-8004.near"}`.

**Un arreglo futuro** va en una cuenta nueva con version (`reputation-registry-v2.erc-8004.near`, etc.): estas no
cambian nunca, y Reputation y Validation no pueden cambiar de Identity Registry (es la raiz de confianza).

## Gas y storage medidos (sandbox nearcore 2.13.4; las lineas `MEDIDA` de `tests/workspaces.rs`)

| Que | Medido | Constante del contrato |
|---|---|---|
| `register` (URI de 38 bytes, dueno nuevo) | 3,08 TGas; 468 bytes + 77 de reserva | `register_deposit` = 13.279 bytes (el peor caso: U-ST1 lo mide exacto) |
| `nft_transfer` | 2,78 TGas; no cambia el storage salvo el contador de un dueno nuevo (+77) o que se vacia (-77) | 1 yocto |
| `set_agent_wallet` (a si mismo) | 1,99 TGas; 0 bytes | 1 yocto |
| `nft_transfer_call` (devuelto / receptor en panico / cuenta sin codigo / se lo queda) | 7,69 / 7,61 / 7,38 / 6,70 TGas | minimo 50 TGas; `nft_resolve_transfer` reserva 15 |
| `give_feedback` aceptada | 7,30 TGas en total: Reputation 2,21, `check_agent` 1,45, callback 2,42; 323 bytes | minimo 40 TGas (`check_agent` 5, callback 20); `feedback_deposit` 591 bytes (U-ST1 exacto) |
| `give_feedback` rechazada (`E22`) | 6,49 TGas; 0 bytes; vuelve todo el deposito | |
| `give_feedback` con el minimo justo (40 TGas) | pasa (7,28 quemados) | |
| `give_feedback_with_proof` aceptada (v1 compilada / de mainnet) | 14,02 / 14,01 TGas: Reputation 2,87, `check_agent` 1,43, verificador 1,89, `get_payment` de v1 1,48 / 1,47, callback del verificador 1,21, callback de Reputation 3,27 | minimo 120 TGas (verificador 50, callback 30); `feedback_proof_deposit` 1.472 bytes (U-ST3 exacto) |
| `give_feedback_with_proof` con el minimo justo (120 TGas) | pasa (13,81 quemados) | |
| `append_response` (respondedor nuevo) | 3,04 TGas | lo medido |
| `validation_request` (BORRADOR) | 7,48 TGas | `request_deposit` 574 bytes (exacto) |
| 6 transferencias a 6 cuentas nuevas (WE-10) | el storage del Identity Registry sube 385 bytes (5 x 77) y su saldo libre no baja | la reserva por token |
| el ciclo de duenos (WE-12): ALICE (1) -> BOB (1), despues BOB (2) -> NUEVA -> BOB seis veces | el saldo libre del Identity Registry sube 96.033.710.506.400.000.002 yocto por ciclo (el 30 % del gas de las dos transferencias y los 2 yocto); un contador son 770.000.000.000.000.000.000 | nada vuelve al dueno que se queda sin agentes |
| peor caso de logs: `register` con todo al tope; `new_feedback` con tags de `"`/`\` y URIs al tope | < 16.384 bytes cada uno (U-ST1) | tope de NEAR por recibo |
| cuentas recien desplegadas, sin llave (WE-11, con la wasm de `compile_project`, 9457b3a) | identity 264.411 bytes (codigo 264.233 + 178), reputation 280.035 (279.823 + 212), verifier 120.389 (120.225 + 164), validation 199.033 (198.846 + 187); WE-11 mira ademas los `standards` de NEP-330 de cada una | el fondeo del plan: wasm reproducible + esos bytes + 0,1 NEAR |
| wasm reproducible (`cargo near build reproducible-wasm`) | dos clones de 8961d49, cada par identico: identity 258.757 bytes, reputation 273.698, verifier 119.123; el codigo final (f1e9a1b): identity 258.757 (sha256 `03850174...6012d3e9`), reputation 273.885 (`c45b546b...b24cbbaa`), verifier 119.123 (`0fa406eb...2794c777`); con la ronda 1 de c0der (9457b3a), dos clones, cada par identico: identity 260.062 (`40638245...61639e3d`), reputation 273.923 (`c0463576...554150f0`), verifier 119.163 (`a07e3a8e...3e6c56c6`) | el merge commit da otro sha256 (NEP-330 embebe el commit) |

## Lo que corri

Sin red de NEAR (`HTTPS_PROXY=http://127.0.0.1:9`, `--offline`), sin credenciales (`AWS_SHARED_CREDENTIALS_FILE=/dev/null`),
`NEAR_SANDBOX_BIN_PATH` = near-sandbox 2.13.4, Rust 1.93.0 de `rust-toolchain.toml`. El repo no tiene
`.github/workflows` ni `scripts/ci/preflight.sh`: no hay CI que reproducir. `origin/main` sigue en 8cec4e6 (re-medido
con `git fetch` antes de cada corrida final, la ultima a las 18:15Z: la union es la rama misma).

| Paso (job) | Comando | Resultado |
|---|---|---|
| Linea base, `origin/main` 8cec4e6 (13:50Z-14:09Z) | `cargo test --workspace --offline --no-fail-fast -j 4` | escrow-core 12, escrow-v1 60 + 15 sandbox (+3 ignored, necesitan red), escrow-v2 130 + 11, registry-v1 115 + 14, test-escrow 0, test-ft 0. **Sin rojos** |
| Union con `origin/main` (sin cambios: 8cec4e6), en f1e9a1b (16:03Z-16:20Z) | `cargo test --workspace --offline --no-fail-fast -j 4` | lo mismo que la base (12, 60 + 15 (+3), 130 + 11, 115 + 14) y ademas erc8004-common 19, erc8004-identity 95, erc8004-reputation 90 + **13 sandbox** (WE-00..WE-11, WE-05 contra v1 compilada y contra la wasm de mainnet), erc8004-arras-verifier 19, erc8004-validation 27, test-nft-receiver 0: **todo verde** (la misma suite en 2520dc5, antes del P1, tambien) |
| Ronda 1 de c0der: head y union (la misma: `origin/main` = 8cec4e6 es ancestro), con el codigo de 9457b3a (18:16:21Z-18:43:06Z) | `cargo test --workspace --offline --no-fail-fast -j 4`, red cerrada, `AWS_SHARED_CREDENTIALS_FILE=/dev/null`, `CI=true` | la base igual (12, 60 + 15 (+3 ignored), 130 + 11, 115 + 14) y erc8004-common **21**, erc8004-identity **102**, erc8004-reputation 90 + **14 sandbox** (WE-00..WE-12), erc8004-arras-verifier 19, erc8004-validation 27, test-nft-receiver 0: **todo verde**; la lista de tests contra la de la base: 0 rojos nuevos, 0 tests de la base que falten |
| El port puro (sin la extension) | `cargo test --offline -p erc8004-reputation --no-default-features --lib` | 65 en verde (otra vez en la ronda 1 de c0der) |
| Mediciones del sandbox | `cargo test -p erc8004-reputation --test workspaces -- --nocapture` | las lineas `MEDIDA` de la seccion "Gas y storage medidos" |
| Formato | `cargo fmt --all -- --check` | limpio (tambien en 9457b3a) |
| Lint | `cargo clippy --offline -p erc8004-common -p erc8004-identity -p erc8004-reputation -p erc8004-arras-verifier -p erc8004-validation -p arras-test-nft-receiver --all-targets -- -D warnings`, y `-p erc8004-reputation --no-default-features --all-targets` | limpio (tambien en 9457b3a) |
| Mutaciones | el script del scratchpad (`timeout` por mutante, `git checkout` despues de cada una, arbol limpio entre una y otra) | 212 (199 + los 13 de la ronda 1 de c0der: O1, O1-ws, O2, O4, W1..W8, S1): **210 muertas por un test que falla**, 2 equivalentes (V02, sacada del codigo, y C36) |
| Build reproducible | `cargo near build reproducible-wasm` de los 3 crates desplegables, imagen `sourcescan/cargo-near` por digest `ccb22bb4...` | dos clones limpios de 8961d49: cada par con el mismo sha256 (identity 258.757 bytes, reputation 273.698, verifier 119.123); uno de f1e9a1b, el codigo final: identity 258.757 (`03850174...6012d3e9`), reputation 273.885 (`c45b546b...b24cbbaa`), verifier 119.123 (`0fa406eb...2794c777`); ronda 1 de c0der, dos clones limpios de 9457b3a (17:54Z-18:15Z): cada par identico, identity 260.062 (`406382455fcf488b80d6f84a0825411585459b3c373f38c6318cf3ab61639e3d`), reputation 273.923 (`c0463576acd91692733d60ed4145832c0432d6e52f2ffba3da9cfa3e554150f0`), verifier 119.163 (`a07e3a8e70fdc21046ff654cd84eeb489402472a92e27ed795d8e1b13e6c56c6`); su `contract_source_metadata` trae `link` `.../UltravioletaDAO/arras/tree/9457b3a...`, la imagen por digest y los `standards` del plan (seccion 7) |
| Pre-CI de c0der | `python3 ~/c0der-specs/hooks/preci.py --repo . --base origin/main --ejecutar` | verde por vacio ("ningun paso barato que correr": el repo no tiene workflows) |
| Scripts de mainnet (no cambiaron) | `scripts/test_mainnet.sh` | 347 asserts pasan, 0 fallan, con `/bin/bash` 3.2 y `/opt/homebrew/bin/bash` 5.3.20 (otra vez en la ronda 1 de c0der) |
| Secretos | `gitleaks git --log-opts=origin/main..HEAD` y `gitleaks dir docs/handoffs/erc8004-near.md` | sin hallazgos (12 commits; en la ronda 1 de c0der, 18 commits) |
| No se toco lo desplegado | `git diff origin/main...HEAD -- contracts/registry-v1 contracts/escrow-v1 contracts/escrow-v2 contracts/escrow-core` | vacio |
| Mediciones de cadena (solo lectura) | RPC publica de NEAR y de Base (`view_account`, `view_access_key_list`, `get_config`, `getVersion`) | las de la seccion 0 del plan |

## Auto-refutacion

**Bordes revisados** (cada uno con test): mayusculas (la clave `AgentWallet` no es la reservada, como el `keccak256`
de EVM; un hash en mayusculas es error; `role`/`context` en mayusculas son error; las URIs aceptan esquema en
mayusculas), espacios adelante, atras y en el medio (URI, clave, `proof`, numeros), `0x`/`0X` en hashes, vacios (`""`
en cada opcional = ausente como en EVM, `register()` sin nada, `response_uri` vacia = `E16`, `proof` vacia, lista de
clientes vacia = `E30`, lista de respondedores vacia = todos), nulos (`agent_wallet: null` presente vale y ausente no;
`check_agent` = `null` es `E22`; `verify_feedback_proof` = `null` es `E46`), unicode (tags UTF-8 de 64 bytes justos y
65; control `\n`, `\u{0}`, `\u{7f}`, `\u{85}`; URIs y claves solo ASCII de RFC 3986 / `[A-Za-z0-9._:/-]`), enteros
canonicos (`"01"`, `"00"`, `"+1"`, `"-0"`, `" 1"`, `"1e3"`, `"0x1"`, `"1.0"`, `u64::MAX + 1`, `i128::MIN`/`MAX`,
`+-1e38` justos y uno mas), topes exactos y tope+1 (agent_uri 8.192/8.193, valor de metadata 1.024/1.025, 4/5
entradas, tags 64/65, endpoint 1.024, URIs 4.096, memo 256/257, 8/9 aprobaciones, `proof` 128/129, labels 32/33,
deadline `now`/`now + 5 min`/`+1`, `accept` en el deadline y uno despues, TTL del candado `at + 999`/`at + 1000`,
deposito `need`/`need - 1`, gas minimo justo en el sandbox y menos 1, respuestas de 1.024 bytes justos y 1.025), el
predecessor y no el signer (register, give_feedback, revoke_feedback), y cada forma en que un `Payment` puede no ser el
de v1 (los casos de la RONDA 1 de registry-v1). De la ronda 1 de c0der: revocar a una cuenta que no estaba aprobada
(cancela igual), revocar sin pendiente (no emite nada), la pendiente propuesta por el dueno y por un aprobado, la
pendiente del receptor de un `nft_transfer_call` devuelto, una wallet de 64 bytes (el `Token` no cambia de largo), el
ciclo de duenos de un solo agente, y productos de 256 bits con acarreo entre palabras (`a` desde 2^64 hasta 2^128 - 1,
`b` = 10^k y `u64::MAX`). Parsers: `serde_json` de near-sdk y `bs58` de near-sdk; no se usa
ningun parser de URL. Rutas: no hay (todo en cadena).

**Mutaciones a mano** (cada una aplicada con un reemplazo de texto exacto, corrida con `timeout` (300 s, 900 s las del
sandbox), revertida con `git checkout`, arbol limpio entre una y otra; el script y las listas quedaron en el scratchpad
de la sesion). Solo cuenta como muerta la que hace FALLAR un test (un timeout tambien contaria, regla 14; no hubo).
Primera pasada: 88 (Identity y Common), 83 muertas; I38 y C18 sobrevivieron (no habia test de la wallet que pone el
receptor de un `nft_transfer_call` antes de que se lo devuelvan, ni de `is_canonical_uint` directo) y 3 no aplicaban
(rustfmt habia partido las lineas). Segunda: las 5 rehechas + 107 (Reputation, pruebas, verificador, Validation y los 9
`#[private]` en el sandbox): 110 muertas, P21 sobrevivio (no habia test del TTL de `is_proof_pending`) y V02 era
**equivalente**: volver a codificar el `payment_id` no agrega nada (decodificar base58 a 32 bytes ya es inyectivo), asi
que esa guarda se saco del codigo (8961d49). Tercera: P21 con su test nuevo y la guarda de largo del verificador
reescrita (V01, V01b): 3 muertas. Cuarta, con el arreglo del P1 del refutador: C34 (volver al `Ord` derivado) y C35
(comparar sin `rev`) muertas por el test nuevo; C36 (`>=` por `>` al elegir el signo) es equivalente (con las dos sumas
iguales la magnitud es 0 y el signo no se imprime). Hasta ahi: 199 mutantes, 197 muertas y 2 equivalentes. Quinta,
con la ronda 1 de c0der (sobre 9457b3a): los tres mutantes que el refutador de c0der encontro vivos (O1, O2, O4) con
sus tests nuevos, O1 tambien en el sandbox (WE-12), ocho de la cancelacion de la pendiente (W1..W8) y uno de NEP-330
(S1): 13 muertas; W1, W2, W4 y W5 caen ademas con solo los dos tests de la sonda (`tras_nft_revoke*`). **Total: 212
mutantes, 210 muertas por un test que falla y 2 equivalentes (V02, cuya guarda redundante se saco del codigo, y
C36).**

| # | Mutacion (archivo: lo que habia -> lo que se puso) | Resultado | Un test que la mata |
|---|---|---|---|
| I01 | `erc8004-identity/src/registry.rs`: `if entries.len() > MAX_REGISTER_METADATA {` -> `if entries.len() > MAX_REGISTER_METADATA + 1 {` | muerta | `unit_tests::identidad::register_con_cinco_entradas_es_e03` |
| I02 | `erc8004-identity/src/registry.rs`: `if key == AGENT_WALLET_KEY {` -> `if false {` | muerta | `unit_tests::identidad::register_con_agent_wallet_es_e04` |
| I03 | `erc8004-identity/src/registry.rs`: `if value.len() > MAX_METADATA_VALUE_LEN {` -> `if value.len() > MAX_METADATA_VALUE_LEN + 1 {` | muerta | `unit_tests::identidad::register_con_valor_largo_es_e05` |
| I04 | `erc8004-identity/src/registry.rs`: `if !is_valid_metadata_key(key) {` -> `if false {` | muerta | `unit_tests::identidad::register_con_clave_invalida_es_e02` |
| I05 | `erc8004-identity/src/registry.rs`: `if !is_valid_optional_uri(uri, MAX_AGENT_URI_LEN) {` -> `if false {` | muerta | `unit_tests::identidad::register_uri_de_mas_es_e01` |
| I06 | `erc8004-identity/src/lib.rs`: `if d == 0 {` -> `if false {` | muerta | `unit_tests::identidad::register_sin_deposito_es_e07` |
| I07 | `erc8004-identity/src/registry.rs`: `self.settle(before, after, TOKEN_RESERVE_BYTES, &owner, &owner, deposit);` -> `self.settle(before, after, 0, &owner, &owner, deposit);` | muerta | `unit_tests::identidad::register_con_un_yocto_menos_es_e06` |
| I08 | `erc8004-identity/src/registry.rs`: `agent_wallet: FixedAccount::of(&owner),` -> `agent_wallet: FixedAccount::NONE,` | muerta | `unit_tests::identidad::register_sin_argumentos_acuna_el_0_y_pone_agent_wallet` |
| I09 | `erc8004-identity/src/lib.rs`: `if deposit < cost {` -> `if deposit + 1 < cost {` | muerta | `unit_tests::identidad::register_con_un_yocto_menos_es_e06` |
| I10 | `erc8004-identity/src/lib.rs`: `refund(owner, freed);` -> `refund(payer, freed);` | muerta | `unit_tests::identidad::lo_liberado_vuelve_al_dueno_y_el_deposito_al_que_llama` |
| I11 | `erc8004-identity/src/lib.rs`: `refund(payer, deposit - cost);` -> `refund(payer, deposit);` | muerta | `unit_tests::identidad::register_cobra_lo_medido_mas_la_reserva_y_devuelve_el_resto` |
| I12 | `erc8004-identity/src/registry.rs`: `self.require_authorized(id, &token, &caller); check_uri(&new_uri);` -> `check_uri(&new_uri);` | muerta | `unit_tests::identidad::set_agent_uri_de_otro_es_e12` |
| I13 | `erc8004-identity/src/registry.rs`: `self.require_authorized(id, &token, &caller); check_entry(&metadata_key` -> `check_entry(&metadata_key` | muerta | `unit_tests::identidad::set_metadata_de_otro_es_e12` |
| I14 | `erc8004-identity/src/registry.rs`: `self.require_authorized(id, &token, &caller); let deadline =` -> `let deadline =` | muerta | `unit_tests::wallet::set_agent_wallet_de_otro_es_e12` |
| I15 | `erc8004-identity/src/registry.rs`: `if now > deadline {` -> `if now > deadline + 1 {` | muerta | `unit_tests::wallet::deadline_vencido_es_e14` |
| I16 | `erc8004-identity/src/registry.rs`: `if deadline > now.saturating_add(MAX_DEADLINE_DELAY_NS) {` -> `if deadline > now.saturating_add(MAX_DEADLINE_DELAY_NS + 1) {` | muerta | `unit_tests::wallet::deadline_mas_alla_de_5_min_es_e15` |
| I17 | `erc8004-identity/src/registry.rs`: `let set_now = caller == new_wallet;` -> `let set_now = true;` | muerta | `unit_tests::wallet::aceptar_otra_cuenta_es_e17` |
| I18 | `erc8004-identity/src/registry.rs`: `if !token.pending_wallet.is(&caller) {` -> `if false {` | muerta | `unit_tests::wallet::aceptar_otra_cuenta_es_e17` |
| I19 | `erc8004-identity/src/registry.rs`: `if env::block_timestamp() > token.pending_deadline {` -> `if env::block_timestamp() > token.pending_deadline + 1 {` | muerta | `unit_tests::wallet::aceptar_despues_del_deadline_es_e14` |
| I20 | `erc8004-identity/src/registry.rs`: `if token.pending_wallet.is_none() {` -> `if false {` | muerta | `unit_tests::wallet::aceptar_sin_pendiente_es_e16` |
| I21 | `erc8004-identity/src/registry.rs`: `self.require_authorized(id, &token, &caller); token.agent_wallet = Fix...` -> `token.agent_wallet = FixedAccount::NONE;` | muerta | `unit_tests::wallet::unset_de_otro_es_e12` |
| I22 | `erc8004-identity/src/registry.rs`: `if set_now { token.agent_wallet = FixedAccount::of(&new_wallet); token...` -> `if set_now { token.agent_wallet = FixedAccount::of(&new_wallet);` | muerta | `unit_tests::wallet::un_aprobado_tambien_y_una_nueva_pisa_la_vieja` |
| I23 | `erc8004-identity/src/nft.rs`: `if sender != &owner {` -> `if false {` | muerta | `unit_tests::nft::la_aprobacion_no_sobrevive_a_la_transferencia` |
| I24 | `erc8004-identity/src/nft.rs`: `if approval_id.is_some_and(|expected| expected != actual) {` -> `if false {` | muerta | `unit_tests::nft::approval_id_viejo_es_e19` |
| I25 | `erc8004-identity/src/nft.rs`: `if receiver == &owner {` -> `if false {` | muerta | `unit_tests::nft::transferir_al_mismo_dueno_es_e20` |
| I26 | `erc8004-identity/src/nft.rs`: `token.owner = FixedAccount::of(receiver); token.agent_wallet = FixedAc...` -> `token.owner = FixedAccount::of(receiver);` | muerta | `unit_tests::nft::transferir_limpia_wallet_pendiente_y_aprobaciones` |
| I27 | `erc8004-identity/src/nft.rs`: `token.agent_wallet = FixedAccount::NONE; token.pending_wallet = FixedA...` -> `token.agent_wallet = FixedAccount::NONE; self.tokens.insert(id, token)...` | muerta | `unit_tests::nft::transferir_limpia_wallet_pendiente_y_aprobaciones` |
| I28 | `erc8004-identity/src/nft.rs`: `let approvals = self.approvals.remove(&id);` -> `let approvals = self.approvals.get(&id).cloned();` | muerta | `unit_tests::nft::la_aprobacion_no_sobrevive_a_la_transferencia` |
| I29 | `erc8004-identity/src/nft.rs`: `refund(&t.previous_owner, bytes_cost(t.freed_bytes));` -> `let _ = &t;` | muerta | `unit_tests::nft::transferir_limpia_wallet_pendiente_y_aprobaciones` |
| I30 | `erc8004-identity/src/nft.rs`: `is_some_and(|m| m.len() > MAX_MEMO_LEN)` -> `is_some_and(|m| m.len() > MAX_MEMO_LEN + 1)` | muerta | `unit_tests::nft::memo_largo_es_e18` |
| I31 | `erc8004-identity/src/nft.rs`: `if env::prepaid_gas() < MIN_GAS_NFT_TRANSFER_CALL {` -> `if false {` | muerta | `unit_tests::nft::transfer_call_con_poco_gas_es_e21` |
| I32 | `erc8004-identity/src/nft.rs`: `serde_json::from_slice::<bool>(&v).unwrap_or(true)` -> `serde_json::from_slice::<bool>(&v).unwrap_or(false)` | muerta | `unit_tests::nft::resolve_con_receptor_que_fallo_o_respuesta_rara_lo_devuelve` |
| I33 | `erc8004-identity/src/nft.rs`: `Err(_) => true,` -> `Err(_) => false,` | muerta | `unit_tests::nft::resolve_con_receptor_que_fallo_o_respuesta_rara_lo_devuelve` |
| I34 | `erc8004-identity/src/nft.rs`: `if !must_revert || !token.owner.is(&receiver_id) {` -> `if !must_revert {` | muerta | `unit_tests::nft::resolve_si_el_receptor_ya_lo_paso_queda_donde_esta` |
| I35 | `erc8004-identity/src/nft.rs`: `self.approvals.insert(id, a); } self.remove_from_owner(&receiver_id` -> `let _ = a; } self.remove_from_owner(&receiver_id` | muerta | `unit_tests::nft::resolve_true_lo_devuelve_con_sus_aprobaciones` |
| I36 | `erc8004-identity/src/nft.rs`: `refund(&previous_owner_id, approvals_refund.0);` -> `let _ = &approvals_refund;` | muerta | `unit_tests::nft::resolve_si_el_receptor_ya_lo_paso_queda_donde_esta` |
| I37 | `erc8004-identity/src/nft.rs`: `refund(&receiver_id, bytes_cost(freed));` -> `let _ = freed;` | muerta | `unit_tests::nft::resolve_true_lo_devuelve_con_sus_aprobaciones` |
| I38 | `erc8004-identity/src/nft.rs`: `token.owner = FixedAccount::of(&previous_owner_id); token.agent_wallet...` -> `token.owner = FixedAccount::of(&previous_owner_id);` | muerta (test nuevo) | `unit_tests::nft::resolve_que_lo_devuelve_limpia_la_wallet_que_puso_el_receptor` |
| I39 | `erc8004-identity/src/nft.rs`: `if !token.owner.is(&caller) { IdentityError::E22SoloElDueno.panic(); }...` -> `let mut map` | muerta | `unit_tests::nft::approve_de_un_aprobado_es_e22` |
| I40 | `erc8004-identity/src/nft.rs`: `map.len() >= MAX_APPROVALS {` -> `map.len() > MAX_APPROVALS {` | muerta | `unit_tests::nft::la_novena_aprobacion_es_e23` |
| I41 | `erc8004-identity/src/nft.rs`: `token.next_approval_id = approval_id.checked_add(1).unwrap_or_else(|| ...` -> `token.next_approval_id = approval_id;` | muerta | `unit_tests::nft::approval_id_viejo_es_e19` |
| I42 | `erc8004-identity/src/nft.rs`: `if !token.owner.is(&caller) { IdentityError::E22SoloElDueno.panic(); }...` -> `let Some(mut map)` | muerta | `unit_tests::nft::revoke_de_otro_es_e22` |
| I43 | `erc8004-identity/src/nft.rs`: `if !token.owner.is(&caller) { IdentityError::E22SoloElDueno.panic(); }...` -> `let before = self.usage(); self.approvals.remove(&id);` | muerta | `unit_tests::nft::revoke_all_de_otro_es_e22` |
| I44 | `erc8004-identity/src/nft.rs`: `refund(&caller, bytes_cost(freed)); }  /// NEP-178: solo el dueno, exa...` -> `let _ = freed; }  /// NEP-178: solo el dueno, exactamente 1 yocto.` | muerta | `unit_tests::nft::revoke_y_revoke_all_devuelven_lo_liberado_al_dueno` |
| I45 | `erc8004-identity/src/nft.rs`: `self.settle(before, after, 0, &caller, &caller, deposit);` -> `let _ = (before, after, deposit);` | muerta | `unit_tests::nft::approve_con_poco_deposito_es_e06` |
| I46 | `erc8004-identity/src/lib.rs`: `self.approvals.get(&id).is_some_and(|m| m.contains_key(who))` -> `self.approvals.get(&id).is_some_and(|_| false)` | muerta | `unit_tests::identidad::lo_liberado_vuelve_al_dueno_y_el_deposito_al_que_llama` |
| I47 | `erc8004-identity/src/views.rs`: `authorized: self.is_authorized(id, t, &account_id),` -> `authorized: false,` | muerta | `unit_tests::vistas::check_agent_para_las_llamadas_cruzadas` |
| I48 | `erc8004-identity/src/views.rs`: `agent_wallet: t.agent_wallet.get(),` -> `agent_wallet: None,` | muerta | `unit_tests::vistas::check_agent_para_las_llamadas_cruzadas` |
| I49 | `erc8004-identity/src/lib.rs`: `t.owner_pos = pos;` -> `let _ = (t, pos);` | muerta | `unit_tests::storage::ust3_una_transferencia_solo_mueve_el_contador` |
| I50 | `erc8004-identity/src/lib.rs`: `if last == 0 {` -> `if false {` | muerta | `unit_tests::storage::ust3_una_transferencia_solo_mueve_el_contador` |
| I51 | `erc8004-identity/src/views.rs`: `let limit = limit.unwrap_or(MAX_LIST_LIMIT).min(MAX_LIST_LIMIT);` -> `let limit = limit.unwrap_or(MAX_LIST_LIMIT);` | muerta | `unit_tests::vistas::limit_tope_100` |
| I52 | `erc8004-identity/src/views.rs`: `if metadata_key == AGENT_WALLET_KEY {` -> `if false {` | muerta | `unit_tests::wallet::el_dueno_se_pone_a_si_mismo_en_el_acto` |
| I53 | `erc8004-identity/src/lib.rs`: `erc8004_common::validate::parse_u64(agent_id)` -> `agent_id.parse::<u64>().ok()` | muerta | `unit_tests::identidad::set_agent_uri_con_id_no_canonico_es_e10` |
| I54 | `erc8004-identity/src/views.rs`: `let id = parse_u64(agent_id)?; self.tokens.get(&id).map(|t| (id, t))` -> `let id: u64 = agent_id.parse().ok()?; self.tokens.get(&id).map(|t| (id...` | muerta | `unit_tests::vistas::check_agent_para_las_llamadas_cruzadas` |
| I55 | `erc8004-identity/src/views.rs`: `Some(&actual) => approval_id.is_none_or(|expected| expected == actual),` -> `Some(_) => true,` | muerta | `unit_tests::nft::approve_cobra_lo_medido_y_con_msg_llama_nft_on_approve` |
| C01 | `erc8004-common/src/validate.rs`: `b"-._~:/?#[]@!$&'()*+,;=%".contains(&b)` -> `b"-._~:/?#[]@!$&'()*+,;=%\"".contains(&b)` | muerta | `validate::tests::uris` |
| C02 | `erc8004-common/src/validate.rs`: `if b.is_empty() || b.len() > max || !b.iter().all(|&c| is_uri_byte(c)) {` -> `if b.is_empty() || b.len() > max + 1 || !b.iter().all(|&c| is_uri_byte...` | muerta | `validate::tests::uris` |
| C03 | `erc8004-common/src/validate.rs`: `colon + 1 < b.len()` -> `colon < b.len()` | muerta | `validate::tests::uris` |
| C04 | `erc8004-common/src/validate.rs`: `&& b[0].is_ascii_alphabetic()` -> `&& b[0] != b':'` | muerta | `validate::tests::uris` |
| C05 | `erc8004-common/src/validate.rs`: `.all(|&c| c.is_ascii_alphanumeric() || c == b'+' || c == b'-' || c == ...` -> `.all(|_| true)` | muerta | `validate::tests::uris` |
| C06 | `erc8004-common/src/validate.rs`: `s.is_empty() || is_valid_uri(s, max)` -> `is_valid_uri(s, max)` | muerta | `validate::tests::uris` |
| C07 | `erc8004-common/src/validate.rs`: `!k.is_empty() && k.len() <= MAX_METADATA_KEY_LEN` -> `k.len() <= MAX_METADATA_KEY_LEN` | muerta | `validate::tests::claves_tags_labels_y_proof` |
| C08 | `erc8004-common/src/validate.rs`: `&& k.len() <= MAX_METADATA_KEY_LEN` -> `&& k.len() <= MAX_METADATA_KEY_LEN + 1` | muerta | `validate::tests::claves_tags_labels_y_proof` |
| C09 | `erc8004-common/src/validate.rs`: `c.is_ascii_alphanumeric() || c == b'.' || c == b'_' || c == b':' || c ...` -> `c.is_ascii_graphic() || c == b'.' || c == b'_' || c == b':' || c == b'/'` | muerta | `validate::tests::claves_tags_labels_y_proof` |
| C10 | `erc8004-common/src/validate.rs`: `t.len() <= MAX_TAG_LEN && !t.chars().any(char::is_control)` -> `t.len() <= MAX_TAG_LEN + 1 && !t.chars().any(char::is_control)` | muerta | `validate::tests::claves_tags_labels_y_proof` |
| C11 | `erc8004-common/src/validate.rs`: `t.len() <= MAX_TAG_LEN && !t.chars().any(char::is_control)` -> `t.len() <= MAX_TAG_LEN` | muerta | `validate::tests::claves_tags_labels_y_proof` |
| C12 | `erc8004-common/src/validate.rs`: `.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()` -> `.all(|c| c.is_ascii_alphabetic() || c.is_ascii_digit()` | muerta | `validate::tests::claves_tags_labels_y_proof` |
| C13 | `erc8004-common/src/validate.rs`: `s.len() <= MAX_LABEL_LEN` -> `s.len() <= MAX_LABEL_LEN + 1` | muerta | `validate::tests::claves_tags_labels_y_proof` |
| C14 | `erc8004-common/src/validate.rs`: `!p.is_empty() && p.len() <= MAX_PROOF_LEN` -> `p.len() <= MAX_PROOF_LEN` | muerta | `validate::tests::claves_tags_labels_y_proof` |
| C15 | `erc8004-common/src/validate.rs`: `!p.is_empty() && p.len() <= MAX_PROOF_LEN` -> `!p.is_empty() && p.len() <= MAX_PROOF_LEN + 1` | muerta | `validate::tests::claves_tags_labels_y_proof` |
| C16 | `erc8004-common/src/validate.rs`: `p.bytes().all(|c| (0x21..=0x7e).contains(&c))` -> `p.bytes().all(|c| (0x20..=0x7e).contains(&c))` | muerta | `validate::tests::claves_tags_labels_y_proof` |
| C17 | `erc8004-common/src/validate.rs`: `&& (b[0] != b'0' || b.len() == 1)` -> `&& true` | muerta | `validate::tests::enteros_canonicos` |
| C18 | `erc8004-common/src/validate.rs`: `!b.is_empty() && b.iter().all(u8::is_ascii_digit)` -> `!b.is_empty() && b.iter().all(|c| c.is_ascii_alphanumeric())` | muerta (test nuevo) | `validate::tests::is_canonical_uint_directo` |
| C19 | `erc8004-common/src/validate.rs`: `if !is_canonical_uint(digits) || (neg && digits == "0") {` -> `if !is_canonical_uint(digits) {` | muerta | `validate::tests::values` |
| C20 | `erc8004-common/src/validate.rs`: `b'a'..=b'f' => Some(c - b'a' + 10),` -> `b'a'..=b'f' => Some(c - b'a' + 10), b'A'..=b'F' => Some(c - b'A' + 10),` | muerta | `validate::tests::hashes` |
| C21 | `erc8004-common/src/validate.rs`: `if b.len() != 64 {` -> `if b.len() < 64 {` | muerta | `validate::tests::hashes` |
| C22 | `erc8004-common/src/summary.rs`: `for (d, &c) in self.decimal_counts.iter().enumerate() { if c > max {` -> `for (d, &c) in self.decimal_counts.iter().enumerate() { if c >= max {` | muerta | `summary::tests::coincide_con_la_referencia_en_casos_chicos` |
| C23 | `erc8004-common/src/summary.rs`: `let value = if negative && !scaled.is_zero() {` -> `let value = if negative {` | muerta | `summary::tests::trunca_hacia_cero_como_solidity` |
| C24 | `erc8004-common/src/summary.rs`: `let (scaled, _) = avg.div_rem_u64(pow10(18 - mode));` -> `let (scaled, _) = avg.div_rem_u64(pow10(18 - mode) * 10);` | muerta | `summary::tests::ejemplos_de_la_eip` |
| C25 | `erc8004-common/src/summary.rs`: `let slot = if value < 0 { &mut self.neg } else { &mut self.pos };` -> `let slot = if value < 0 { &mut self.pos } else { &mut self.neg };` | muerta | `summary::tests::exacto_donde_evm_trunca_int128` |
| C26 | `erc8004-common/src/summary.rs`: `let (avg, _) = magnitude.div_rem_u64(self.count);` -> `let (avg, _) = magnitude.div_rem_u64(self.count + 1);` | muerta | `summary::tests::exacto_donde_evm_trunca_int128` |
| C27 | `erc8004-common/src/summary.rs`: `let n = U256::mul_u128_u64(value.unsigned_abs(), pow10(18 - d));` -> `let n = U256::mul_u128_u64(value.unsigned_abs(), pow10(18 - d) / 10 + 1);` | muerta | `summary::tests::ejemplos_de_la_eip` |
| C28 | `erc8004-common/src/types.rs`: `#[serde(deny_unknown_fields)] #[derive(Debug, Clone, PartialEq, Eq)] p...` -> `#[derive(Debug, Clone, PartialEq, Eq)] pub struct AgentCheck` | muerta | `types::tests::agent_check_estricto` |
| C29 | `erc8004-common/src/types.rs`: `#[serde(deserialize_with = "required_nullable")]` -> `#[serde(default)]` | muerta | `types::tests::agent_check_estricto` |
| C30 | `erc8004-common/src/types.rs`: `#[serde(deserialize_with = "label")] pub role: String,` -> `pub role: String,` | muerta | `types::tests::feedback_proof_estricto` |
| C31 | `erc8004-common/src/types.rs`: `#[serde(deserialize_with = "label")] pub context: String,` -> `pub context: String,` | muerta | `types::tests::feedback_proof_estricto` |
| C32 | `erc8004-common/src/types.rs`: `#[serde(deserialize_with = "canonical_u128")]` -> (nada: el atributo se saca) | muerta | `types::tests::feedback_proof_estricto` |
| C33 | `erc8004-common/src/types.rs`: `#[serde(deny_unknown_fields)] #[derive(Debug, Clone, PartialEq, Eq)] p...` -> `#[derive(Debug, Clone, PartialEq, Eq)] pub struct FeedbackProof` | muerta | `types::tests::feedback_proof_estricto` |
| C34 | `erc8004-common/src/summary.rs`: `#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)] pub struct U256(...` -> `#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]...` | muerta (test nuevo, P1 del refutador) | `summary::tests::u256_ordena_desde_la_palabra_mas_significativa` |
| C35 | `erc8004-common/src/summary.rs`: `self.0.iter().rev().cmp(o.0.iter().rev())` -> `self.0.iter().cmp(o.0.iter())` | muerta (test nuevo, P1 del refutador) | `summary::tests::u256_ordena_desde_la_palabra_mas_significativa` |
| C36 | `erc8004-common/src/summary.rs`: `let (negative, magnitude) = if self.pos >= self.neg {` -> `let (negative, magnitude) = if self.pos > self.neg {` | equivalente (el codigo queda) | `con pos == neg la magnitud es 0 y el signo no se imprime` |
| R01 | `erc8004-reputation/src/feedback.rs`: `parse_agent_id(&agent_id); let v = parse_value` -> `let v = parse_value` | muerta | `unit_tests::nucleo::agent_id_no_canonico_es_e01` |
| R02 | `erc8004-reputation/src/feedback.rs`: `if value_decimals > MAX_VALUE_DECIMALS {` -> `if value_decimals > MAX_VALUE_DECIMALS + 1 {` | muerta | `unit_tests::nucleo::diecinueve_decimales_es_e04` |
| R03 | `erc8004-reputation/src/feedback.rs`: `if v.unsigned_abs() > MAX_ABS_VALUE {` -> `if v.unsigned_abs() > MAX_ABS_VALUE + 1 {` | muerta | `unit_tests::nucleo::value_menor_a_menos_1e38_es_e03` |
| R04 | `erc8004-reputation/src/feedback.rs`: `if !is_valid_tag(&tag1) || !is_valid_tag(&tag2) {` -> `if !is_valid_tag(&tag1) {` | muerta | `unit_tests::nucleo::tag2_largo_es_e05` |
| R05 | `erc8004-reputation/src/feedback.rs`: `if !is_valid_tag(&tag1) || !is_valid_tag(&tag2) {` -> `if !is_valid_tag(&tag2) {` | muerta | `unit_tests::nucleo::tag1_con_control_es_e05` |
| R06 | `erc8004-reputation/src/feedback.rs`: `if !is_valid_optional_uri(&endpoint, MAX_ENDPOINT_LEN) {` -> `if false {` | muerta | `unit_tests::nucleo::endpoint_invalido_es_e06` |
| R07 | `erc8004-reputation/src/feedback.rs`: `if !is_valid_optional_uri(&feedback_uri, MAX_FEEDBACK_URI_LEN) {` -> `if false {` | muerta | `unit_tests::nucleo::feedback_uri_invalida_es_e07` |
| R08 | `erc8004-reputation/src/feedback.rs`: `if let Some(h) = &feedback_hash && parse_hash(h).is_none()` -> `if let Some(h) = &feedback_hash && parse_hash(h).is_none() && false` | muerta | `unit_tests::nucleo::hash_en_mayusculas_es_e08` |
| R09 | `erc8004-reputation/src/feedback.rs`: `if env::attached_deposit().as_yoctonear() < need {` -> `if env::attached_deposit().as_yoctonear() + 1 < need {` | muerta | `unit_tests::nucleo::un_yocto_menos_de_deposito_es_e09` |
| R10 | `erc8004-reputation/src/feedback.rs`: `if env::prepaid_gas() < min_gas {` -> `if false {` | muerta | `unit_tests::nucleo::gas_de_menos_es_e10` |
| R11 | `erc8004-reputation/src/feedback.rs`: `Err(PromiseError::Failed) => return Err(ReputationError::E20VistaDeIde...` -> `Err(PromiseError::Failed) => return Err(ReputationError::E21RespuestaD...` | muerta | `unit_tests::nucleo::la_vista_que_falla_es_e20` |
| R12 | `erc8004-reputation/src/feedback.rs`: `.ok_or(ReputationError::E22AgenteInexistente)?;` -> `.ok_or(ReputationError::E21RespuestaDeIdentidadInvalida)?;` | muerta | `unit_tests::nucleo::agente_inexistente_es_e22` |
| R13 | `erc8004-reputation/src/feedback.rs`: `if check.authorized {` -> `if false {` | muerta | `unit_tests::nucleo::el_dueno_o_un_aprobado_es_e23` |
| R14 | `erc8004-reputation/src/feedback.rs`: `refund(&p.client, p.deposit.0); None` -> `None` | muerta | `unit_tests::nucleo::el_dueno_o_un_aprobado_es_e23` |
| R15 | `erc8004-reputation/src/feedback.rs`: `let cost = bytes_cost(after.saturating_sub(before)).min(p.deposit.0);` -> `let cost = bytes_cost(after.saturating_sub(before));` | muerta | `unit_tests::nucleo::el_cobro_no_pasa_del_deposito` |
| R16 | `erc8004-reputation/src/feedback.rs`: `.checked_add(1)` -> `.checked_add(0)` | muerta | `unit_tests::nucleo::aceptada_da_el_indice_y_emite_new_feedback` |
| R17 | `erc8004-reputation/src/feedback.rs`: `if index == 1 {` -> `if index == 2 {` | muerta | `unit_tests::nucleo::aceptada_da_el_indice_y_emite_new_feedback` |
| R18 | `erc8004-reputation/src/feedback.rs`: `refund(&p.client, p.deposit.0 - cost);` -> `let _ = cost;` | muerta | `unit_tests::nucleo::aceptada_cobra_lo_medido_y_devuelve_el_resto` |
| R19 | `erc8004-reputation/src/feedback.rs`: `let client = env::predecessor_account_id(); if index == 0 {` -> `let client = env::signer_account_id(); if index == 0 {` | muerta | `unit_tests::nucleo::revocar_es_del_predecessor` |
| R20 | `erc8004-reputation/src/feedback.rs`: `let client = env::predecessor_account_id(); if index == 0 { Reputation...` -> `let client = env::predecessor_account_id(); if false { ReputationError...` | muerta | `unit_tests::nucleo::revocar_el_cero_es_e12` |
| R21 | `erc8004-reputation/src/feedback.rs`: `if index > self.last(id, &client) {` -> `if index > self.last(id, &client) + 1 {` | muerta | `unit_tests::nucleo::revocar_la_de_otro_cliente_es_e13` |
| R22 | `erc8004-reputation/src/feedback.rs`: `if fb.is_revoked {` -> `if false {` | muerta | `unit_tests::nucleo::revocar_dos_veces_es_e14` |
| R23 | `erc8004-reputation/src/feedback.rs`: `client: env::predecessor_account_id(),` -> `client: env::signer_account_id(),` | muerta | `unit_tests::nucleo::el_cliente_es_el_predecessor_no_el_signer` |
| R24 | `erc8004-reputation/src/feedback.rs`: `if deposit == 0 { ReputationError::E15RequiereDeposito.panic();` -> `if false { ReputationError::E15RequiereDeposito.panic();` | muerta | `unit_tests::respuestas::sin_yocto_es_e15` |
| R25 | `erc8004-reputation/src/feedback.rs`: `let index = parse_index(&feedback_index); if index == 0 { ReputationEr...` -> `let index = parse_index(&feedback_index); if false { ReputationError::...` | muerta | `unit_tests::respuestas::indice_cero_es_e12` |
| R26 | `erc8004-reputation/src/feedback.rs`: `if !is_valid_uri(&response_uri, MAX_RESPONSE_URI_LEN) {` -> `if false {` | muerta | `unit_tests::respuestas::uri_larga_es_e16` |
| R27 | `erc8004-reputation/src/feedback.rs`: `if index > self.last(id, &client_address) {` -> `if index > self.last(id, &client_address) + 1 {` | muerta | `unit_tests::respuestas::indice_de_mas_es_e13` |
| R28 | `erc8004-reputation/src/feedback.rs`: `if let Some(h) = &response_hash && parse_hash(h).is_none()` -> `if let Some(h) = &response_hash && parse_hash(h).is_none() && false` | muerta | `unit_tests::respuestas::hash_invalido_es_e08` |
| R29 | `erc8004-reputation/src/feedback.rs`: `if deposit < cost {` -> `if false {` | muerta | `unit_tests::respuestas::respondedor_nuevo_sin_deposito_suficiente_es_e09` |
| R30 | `erc8004-reputation/src/feedback.rs`: `if n == 0 {` -> `if n <= 1 {` | muerta | `unit_tests::respuestas::cualquiera_responde_y_se_cuenta_por_respondedor` |
| R31 | `erc8004-reputation/src/feedback.rs`: `refund(&responder, deposit - cost);` -> `let _ = cost;` | muerta | `unit_tests::respuestas::el_respondedor_nuevo_paga_y_el_repetido_no` |
| R32 | `erc8004-reputation/src/views.rs`: `if client_addresses.is_empty() {` -> `if false {` | muerta | `unit_tests::vistas::get_summary_sin_clientes_es_e30` |
| R33 | `erc8004-reputation/src/views.rs`: `if fb.is_revoked || !tag_matches(&tag1, &fb.tag1) || !tag_matches(&tag...` -> `if !tag_matches(&tag1, &fb.tag1) || !tag_matches(&tag2, &fb.tag2) {` | muerta | `unit_tests::nucleo::revocar_marca_y_deja_de_contar` |
| R34 | `erc8004-reputation/src/views.rs`: `None | Some("") => true,` -> `None => true,` | muerta | `unit_tests::vistas::get_summary_como_evm` |
| R35 | `erc8004-reputation/src/views.rs`: `let include_revoked = include_revoked.unwrap_or(false);` -> `let include_revoked = include_revoked.unwrap_or(true);` | muerta | `unit_tests::vistas::read_all_feedback_como_evm` |
| R36 | `erc8004-reputation/src/views.rs`: `Some(client) if index == 0 => {` -> `Some(client) if false => {` | muerta | `unit_tests::respuestas::get_response_count_por_cliente_y_por_agente` |
| R37 | `erc8004-reputation/src/views.rs`: `Some(list) if !list.is_empty() => list.iter()` -> `Some(list) => list.iter()` | muerta | `unit_tests::respuestas::cualquiera_responde_y_se_cuenta_por_respondedor` |
| R38 | `erc8004-reputation/src/views.rs`: `let limit = limit.unwrap_or(MAX_LIST_LIMIT).min(MAX_LIST_LIMIT); let e...` -> `let limit = limit.unwrap_or(MAX_LIST_LIMIT); let end = u64::from(list....` | muerta | `unit_tests::vistas::get_clients_pagina_hasta_100` |
| R39 | `erc8004-reputation/src/lib.rs`: `if identity_registry == env::current_account_id() {` -> `if false {` | muerta | `unit_tests::nucleo::init_con_el_propio_registro_es_e17` |
| R40 | `erc8004-reputation/src/views.rs`: `Some(c) if !c.is_empty() => c,` -> `Some(c) => c,` | muerta | `unit_tests::vistas::read_all_feedback_como_evm` |
| P01 | `erc8004-reputation/src/proof.rs`: `if !is_valid_proof(&proof) {` -> `if false {` | muerta | `unit_tests::pruebas::proof_con_espacio_es_e40` |
| P02 | `erc8004-reputation/src/proof.rs`: `if verifier == env::current_account_id() {` -> `if false {` | muerta | `unit_tests::pruebas::el_registro_como_verificador_es_e41` |
| P03 | `erc8004-reputation/src/proof.rs`: `Self::feedback_proof_deposit(),` -> `Self::feedback_deposit(),` | muerta | `unit_tests::pruebas::con_el_deposito_del_nucleo_no_alcanza_e09` |
| P04 | `erc8004-reputation/src/proof.rs`: `MIN_GAS_GIVE_FEEDBACK_WITH_PROOF,` -> `MIN_GAS_GIVE_FEEDBACK,` | muerta | `unit_tests::pruebas::con_el_gas_del_nucleo_no_alcanza_e10` |
| P05 | `erc8004-reputation/src/proof.rs`: `if self.proof_used.contains_key(&key) {` -> `if false {` | muerta | `unit_tests::pruebas::una_prueba_usada_es_e42_sincronico` |
| P06 | `erc8004-reputation/src/proof.rs`: `&& height < at.saturating_add(PENDING_TTL_BLOCKS)` -> `&& height <= at.saturating_add(PENDING_TTL_BLOCKS)` | muerta | `unit_tests::pruebas::un_candado_ajeno_es_e44_y_no_lo_suelta` |
| P07 | `erc8004-reputation/src/proof.rs`: `self.pending.insert(key, height);` -> `let _ = height;` | muerta | `unit_tests::pruebas::aceptada_guarda_la_prueba_y_la_cuenta_en_los_dos_resumenes` |
| P08 | `erc8004-reputation/src/proof.rs`: `let outcome = if self.pending.get(&key) == Some(&nonce.0) {` -> `let outcome = if true {` | muerta | `unit_tests::pruebas::un_candado_ajeno_es_e44_y_no_lo_suelta` |
| P09 | `erc8004-reputation/src/proof.rs`: `self.pending.remove(&key); self.check_proof` -> `self.check_proof` | muerta | `unit_tests::pruebas::aceptada_guarda_la_prueba_y_la_cuenta_en_los_dos_resumenes` |
| P10 | `erc8004-reputation/src/proof.rs`: `Err(PromiseError::Failed) => return Err(ReputationError::E45Verificado...` -> `Err(PromiseError::Failed) => return Err(ReputationError::E46RespuestaD...` | muerta | `unit_tests::pruebas::el_verificador_que_falla_es_e45` |
| P11 | `erc8004-reputation/src/proof.rs`: `if !is_agent {` -> `if false {` | muerta | `unit_tests::pruebas::una_contraparte_que_no_es_el_agente_es_e47` |
| P12 | `erc8004-reputation/src/proof.rs`: `let is_agent = fp.counterparty == agent.owner_id || agent.agent_wallet...` -> `let is_agent = fp.counterparty == agent.owner_id;` | muerta | `unit_tests::pruebas::la_contraparte_puede_ser_la_agent_wallet` |
| P13 | `erc8004-reputation/src/proof.rs`: `if agent.agent_wallet.as_ref() == Some(&p.client) {` -> `if false {` | muerta | `unit_tests::pruebas::la_agent_wallet_no_califica_a_su_agente_e48` |
| P14 | `erc8004-reputation/src/proof.rs`: `if self.proof_used.contains_key(key) {` -> `if false {` | muerta | `unit_tests::pruebas::la_prueba_usada_se_vuelve_a_mirar_en_el_callback_e42` |
| P15 | `erc8004-reputation/src/proof.rs`: `self.proof_used.insert(p.key, (id, index));` -> `let _ = p.key;` | muerta | `unit_tests::pruebas::aceptada_guarda_la_prueba_y_la_cuenta_en_los_dos_resumenes` |
| P16 | `erc8004-reputation/src/proof.rs`: `list.push((client.clone(), index));` -> `let _ = (&list, index);` | muerta | `unit_tests::pruebas::aceptada_guarda_la_prueba_y_la_cuenta_en_los_dos_resumenes` |
| P17 | `erc8004-reputation/src/views.rs`: `if fb.is_revoked || !tag_matches(&tag1, &fb.tag1)` -> `if !tag_matches(&tag1, &fb.tag1)` | muerta | `unit_tests::pruebas::revocar_no_libera_la_prueba` |
| P18 | `erc8004-reputation/src/views.rs`: `if context.as_ref().is_some_and(|c| c != &pr.context) {` -> `if false {` | muerta | `unit_tests::pruebas::get_verified_summary_filtra_por_clientes_tags_y_context` |
| P19 | `erc8004-reputation/src/views.rs`: `if clients.as_ref().is_some_and(|c| !c.contains(client)) {` -> `if false {` | muerta | `unit_tests::pruebas::get_verified_summary_filtra_por_clientes_tags_y_context` |
| P20 | `erc8004-reputation/src/views.rs`: `.checked_add(erc8004_common::summary::U256::from_u128(pr.weight))` -> `.checked_add(erc8004_common::summary::U256::from_u128(1))` | muerta | `unit_tests::pruebas::aceptada_guarda_la_prueba_y_la_cuenta_en_los_dos_resumenes` |
| P21 | `erc8004-reputation/src/views.rs`: `.is_some_and(|&at| env::block_height() < at.saturating_add(PENDING_TTL...` -> `.is_some_and(|&at| env::block_height() <= at.saturating_add(PENDING_TT...` | muerta (test nuevo) | `unit_tests::pruebas::is_proof_pending_vence_con_el_ttl` |
| V01 | `erc8004-arras-verifier/src/lib.rs`: `bs58::decode(s).into_vec().is_ok_and(|b| b.len() == 32)` -> `bs58::decode(s).into_vec().is_ok_and(|b| b.len() >= 32)` | muerta | `unit_tests::payment_id_canonico` |
| V01b | `erc8004-arras-verifier/src/lib.rs`: `bs58::decode(s).into_vec().is_ok_and(|b| b.len() == 32)` -> `bs58::decode(s).into_vec().is_ok_and(|b| b.len() <= 32)` | muerta | `unit_tests::payment_id_canonico` |
| V02 | `erc8004-arras-verifier/src/lib.rs`: `Ok(b) => b.len() == 32 && bs58::encode(&b).into_string() == s,` -> `Ok(b) => b.len() == 32,` | equivalente: la guarda se saco del codigo (8961d49) | `decodificar base58 a 32 bytes ya es inyectivo` |
| V03 | `erc8004-arras-verifier/src/lib.rs`: `if env::prepaid_gas() < MIN_GAS_VERIFY {` -> `if false {` | muerta | `unit_tests::gas_de_menos_es_e03` |
| V04 | `erc8004-arras-verifier/src/lib.rs`: `Err(PromiseError::Failed) => return Err(VerifierError::E10GetPaymentFa...` -> `Err(PromiseError::Failed) => return Err(VerifierError::E11RespuestaInv...` | muerta | `unit_tests::null_es_e12_y_fallo_es_e10` |
| V05 | `erc8004-arras-verifier/src/lib.rs`: `let pay = payment.ok_or(VerifierError::E12PagoInexistente)?;` -> `let pay = payment.ok_or(VerifierError::E11RespuestaInvalida)?;` | muerta | `unit_tests::null_es_e12_y_fallo_es_e10` |
| V06 | `erc8004-arras-verifier/src/lib.rs`: `if pay.released == 0 {` -> `if false {` | muerta | `unit_tests::vale_con_plata_liberada_en_cualquier_estado` |
| V07 | `erc8004-arras-verifier/src/lib.rs`: `if pay.payer == pay.receiver {` -> `if false {` | muerta | `unit_tests::payer_igual_a_receiver_es_e14` |
| V08 | `erc8004-arras-verifier/src/lib.rs`: `(pay.receiver, "receiver")` -> `(pay.receiver, "payer")` | muerta | `unit_tests::la_otra_parte_y_su_rol` |
| V09 | `erc8004-arras-verifier/src/lib.rs`: `} else if client == &pay.receiver {` -> `} else if true {` | muerta | `unit_tests::la_otra_parte_y_su_rol` |
| V10 | `erc8004-arras-verifier/src/lib.rs`: `#[serde(crate = "near_sdk::serde", deny_unknown_fields)] pub struct Es...` -> `#[serde(crate = "near_sdk::serde")] pub struct EscrowPayment` | muerta | `unit_tests::un_campo_de_mas_es_e11` |
| V11 | `erc8004-arras-verifier/src/lib.rs`: `#[serde(crate = "near_sdk::serde", try_from = "String")]` -> `#[serde(crate = "near_sdk::serde")]` | muerta | `unit_tests::un_status_que_no_es_de_v1_es_e11` |
| V12 | `erc8004-arras-verifier/src/lib.rs`: `if !is_canonical_uint(&s) {` -> `if false {` | muerta | `unit_tests::un_numero_no_canonico_es_e11` |
| V13 | `erc8004-arras-verifier/src/lib.rs`: `if !is_valid_operator_id(&s) {` -> `if false {` | muerta | `unit_tests::operator_id_fuera_del_charset_de_v1_es_e11` |
| V14 | `erc8004-arras-verifier/src/lib.rs`: `if escrow == env::current_account_id() {` -> `if false {` | muerta | `unit_tests::el_propio_verificador_como_escrow_es_e01` |
| V15 | `erc8004-arras-verifier/src/lib.rs`: `if !is_canonical_payment_id(&proof) {` -> `if false {` | muerta | `unit_tests::proof_no_canonica_es_e02` |
| L01 | `erc8004-validation/src/lib.rs`: `if parse_u64(&agent_id).is_none() {` -> `if false {` | muerta | `unit_tests::agent_id_invalido_e01` |
| L02 | `erc8004-validation/src/lib.rs`: `if !is_valid_uri(&request_uri, MAX_REQUEST_URI_LEN) {` -> `if false {` | muerta | `unit_tests::request_uri_vacia_e02` |
| L03 | `erc8004-validation/src/lib.rs`: `if self.validations.contains_key(&hash) {` -> `if false {` | muerta | `unit_tests::hash_repetido_e04` |
| L04 | `erc8004-validation/src/lib.rs`: `if validator_address == env::current_account_id() {` -> `if false {` | muerta | `unit_tests::el_registro_como_validador_e06` |
| L05 | `erc8004-validation/src/lib.rs`: `if deposit < need {` -> `if false {` | muerta | `unit_tests::deposito_corto_e07` |
| L06 | `erc8004-validation/src/lib.rs`: `if env::prepaid_gas() < MIN_GAS_VALIDATION_REQUEST {` -> `if false {` | muerta | `unit_tests::gas_corto_e08` |
| L07 | `erc8004-validation/src/lib.rs`: `&& height < at.saturating_add(PENDING_TTL_BLOCKS)` -> `&& height <= at.saturating_add(PENDING_TTL_BLOCKS)` | muerta | `unit_tests::candado_ajeno_es_e30` |
| L08 | `erc8004-validation/src/lib.rs`: `let outcome = if self.pending.get(&hash) == Some(&nonce.0) {` -> `let outcome = if true {` | muerta | `unit_tests::candado_ajeno_es_e30` |
| L09 | `erc8004-validation/src/lib.rs`: `if !c.authorized {` -> `if false {` | muerta | `unit_tests::rechazos_del_callback` |
| L10 | `erc8004-validation/src/lib.rs`: `.ok_or(ValidationError::E22AgenteInexistente)?;` -> `.ok_or(ValidationError::E21RespuestaDeIdentidadInvalida)?;` | muerta | `unit_tests::rechazos_del_callback` |
| L11 | `erc8004-validation/src/lib.rs`: `if self.validations.contains_key(hash) {` -> `if false {` | muerta | `unit_tests::el_callback_vuelve_a_mirar_si_existe` |
| L12 | `erc8004-validation/src/lib.rs`: `if validator != s.validator {` -> `if false {` | muerta | `unit_tests::respuesta_de_otro_e11` |
| L13 | `erc8004-validation/src/lib.rs`: `if response > MAX_RESPONSE {` -> `if response > MAX_RESPONSE + 1 {` | muerta | `unit_tests::respuesta_101_e12` |
| L14 | `erc8004-validation/src/lib.rs`: `if deposit == 0 {` -> `if false {` | muerta | `unit_tests::respuesta_sin_yocto_e09` |
| L15 | `erc8004-validation/src/lib.rs`: `if !response_uri.is_empty() && !is_valid_uri(&response_uri, MAX_RESPON...` -> `if false {` | muerta | `unit_tests::response_uri_invalida_e13` |
| L16 | `erc8004-validation/src/lib.rs`: `if !is_valid_tag(&tag) {` -> `if false {` | muerta | `unit_tests::tag_invalido_e14` |
| L17 | `erc8004-validation/src/lib.rs`: `if deposit < cost {` -> `if false {` | muerta | `unit_tests::tag_que_crece_sin_deposito_e07` |
| L18 | `erc8004-validation/src/lib.rs`: `if !s.has_response || validators` -> `if validators` | muerta | `unit_tests::sin_respuesta_no_cuenta_en_el_resumen` |
| L19 | `erc8004-validation/src/lib.rs`: `refund(&p.requester, p.deposit.0);` -> `let _ = &p;` | muerta | `unit_tests::rechazos_del_callback` |
| L20 | `erc8004-validation/src/lib.rs`: `let cost = bytes_cost(after.saturating_sub(before)).min(p.deposit.0);` -> `let cost = bytes_cost(after.saturating_sub(before));` | muerta | `unit_tests::el_cobro_no_pasa_del_deposito` |
| L21 | `erc8004-validation/src/lib.rs`: `refund(&validator, deposit + bytes_cost(before - after));` -> `refund(&validator, deposit);` | muerta | `unit_tests::respuesta_del_validador_varias_veces` |
| L22 | `erc8004-validation/src/lib.rs`: `if identity_registry == env::current_account_id() {` -> `if false {` | muerta | `unit_tests::init_con_el_propio_e15` |
| S01 | `erc8004-identity/src/lib.rs`: `#[init] #[private] pub fn new() -> Self {` -> `#[init] pub fn new() -> Self {` | muerta | `we00_new_privado_una_vez_y_sin_llaves` |
| S02 | `erc8004-identity/src/nft.rs`: `#[private] pub fn nft_resolve_transfer(` -> `pub fn nft_resolve_transfer(` | muerta | `we07_callbacks_privados` |
| S03 | `erc8004-reputation/src/lib.rs`: `#[init] #[private] pub fn new(identity_registry` -> `#[init] pub fn new(identity_registry` | muerta | `we00_new_privado_una_vez_y_sin_llaves` |
| S04 | `erc8004-reputation/src/feedback.rs`: `#[private] pub fn on_feedback_checked(` -> `pub fn on_feedback_checked(` | muerta | `we07_callbacks_privados` |
| S05 | `erc8004-reputation/src/proof.rs`: `#[private] pub fn on_feedback_proof_checked(` -> `pub fn on_feedback_proof_checked(` | muerta | `we07_callbacks_privados` |
| S06 | `erc8004-arras-verifier/src/lib.rs`: `#[init] #[private] pub fn new(escrow` -> `#[init] pub fn new(escrow` | muerta | `we00_new_privado_una_vez_y_sin_llaves` |
| S07 | `erc8004-arras-verifier/src/lib.rs`: `#[private] pub fn on_payment(` -> `pub fn on_payment(` | muerta | `we07_callbacks_privados` |
| S08 | `erc8004-validation/src/lib.rs`: `#[init] #[private] pub fn new(identity_registry` -> `#[init] pub fn new(identity_registry` | muerta | `we00_new_privado_una_vez_y_sin_llaves` |
| S09 | `erc8004-validation/src/lib.rs`: `#[private] pub fn on_request_checked(` -> `pub fn on_request_checked(` | muerta | `we07_callbacks_privados` |
| O1 | `erc8004-identity/src/nft.rs`: `let freed_bytes = before.saturating_sub(self.usage());` medido despues de `remove_from_owner`/`add_to_owner` en vez de antes | muerta | `unit_tests::storage::ust5_quedarse_sin_agentes_no_devuelve_el_contador` |
| O1-ws | lo mismo, contra el sandbox | muerta | `we12_el_ciclo_de_duenos_no_baja_el_saldo_libre` |
| O2 | `erc8004-identity/src/nft.rs`: el resolve que deshace sin `token.pending_wallet = FixedAccount::NONE; token.pending_deadline = 0;` | muerta | `unit_tests::nft::resolve_que_lo_devuelve_la_pendiente_del_receptor_no_acepta` |
| O4 | `erc8004-common/src/summary.rs`: `let w2 = ((carry >> 64) + (hi >> 64)) as u64;` -> `let w2 = (hi >> 64) as u64;` | muerta | `summary::tests::mul_u128_u64_contra_python` |
| W1 | `erc8004-identity/src/nft.rs`: `nft_revoke` sin `self.cancel_pending_wallet(id, token);` | muerta | `unit_tests::wallet::tras_nft_revoke_la_wallet_que_propuso_el_revocado_no_acepta` |
| W2 | `erc8004-identity/src/nft.rs`: `nft_revoke_all` sin `self.cancel_pending_wallet(id, token);` | muerta | `unit_tests::wallet::tras_nft_revoke_all_la_wallet_que_propuso_el_revocado_no_acepta` |
| W3 | `erc8004-identity/src/nft.rs`: `cancel_pending_wallet` sin `emit_agent_wallet_pending(id, None, 0);` | muerta | `unit_tests::wallet::revocar_cancela_tambien_la_del_dueno_y_aunque_la_cuenta_no_estuviera` |
| W4 | `erc8004-identity/src/nft.rs`: `cancel_pending_wallet` sin `token.pending_wallet = FixedAccount::NONE;` | muerta | `unit_tests::wallet::tras_nft_revoke_la_wallet_que_propuso_el_revocado_no_acepta` |
| W5 | `erc8004-identity/src/nft.rs`: `if token.pending_wallet.is_none() {` -> `if !token.pending_wallet.is_none() {` | muerta | `unit_tests::wallet::tras_nft_revoke_all_la_wallet_que_propuso_el_revocado_no_acepta` |
| W6 | `erc8004-identity/src/nft.rs`: en `nft_revoke`, cancelar solo si la cuenta estaba aprobada (despues de los `return`) | muerta | `unit_tests::wallet::revocar_cancela_tambien_la_del_dueno_y_aunque_la_cuenta_no_estuviera` |
| W7 | `erc8004-identity/src/registry.rs`: `set_agent_wallet` sin `emit_agent_wallet_pending(id, Some(&new_wallet), deadline);` | muerta | `unit_tests::wallet::otra_wallet_queda_pendiente_hasta_que_acepta` |
| W8 | `erc8004-identity/src/registry.rs`: `emit_agent_wallet_pending(id, Some(&new_wallet), deadline);` -> `emit_agent_wallet_pending(id, Some(&new_wallet), 0);` | muerta | `unit_tests::wallet::otra_wallet_queda_pendiente_hasta_que_acepta` |
| S1 | `erc8004-identity/src/lib.rs`: sin `standard(standard = "nep181", version = "1.0.0"),` | muerta | `we11_storage_de_cada_cuenta_recien_desplegada` |

**Refutador** (un subagente opus/xhigh sobre el diff, el encargo, el CONTEXTO, las RESPUESTAS, el plan, este handoff y
el borrador del PR; regla 5c). Ronda 1: **NO MERGEABLE por un P1**, sin P2.
- **P1** (verificado por el en una copia aislada del workspace, con 3 tests que fallaban sobre el contrato real):
  `U256` derivaba `PartialOrd`/`Ord` sobre palabras en little-endian, asi que comparaba primero la menos significativa
  y `get_summary`/`get_verified_summary` elegian mal el signo con calificaciones positivas y negativas mezcladas de
  mas de 2^64 normalizadas (19 y -1 daba un numero de 59 cifras en vez de 9). Arreglado en f76f664: `Ord` a mano desde
  la palabra mas significativa; tests de signos mezclados (en `summary` y sobre las dos vistas del contrato), uno
  diferencial de 2.000 casos contra la referencia en i128, y los mutantes C34 y C35 (muertos) y C36 (equivalente).
- **P3** arreglados: la formula y los margenes del fondeo; el humo toma el `agent_id` que devuelve `register` y da de
  alta sin un `agent_uri` inventado; la desviacion I-8 (`reference_hash` en `null`); el `msg` sin tope, documentado
  (6.1 del plan); el README; el fondeo de `arras-dev.testnet`; la nota del gas fijo de `nft_resolve_transfer`.
- Lo que encontro bien (resumen): autorizacion, callbacks que fallan cerrado y devuelven todo, candado y unicidad,
  la prueba atada al dueno o la `agentWallet`, la reserva por token, NEP-171/178/181 y los eventos, la fidelidad a EVM
  salvo el P1, los conteos de tests, el `Cargo.lock`, los comandos de near-cli-rs (con `--help`, sin red) y los
  depositos del paso 4.
Ronda 2 (sobre f76f664 y f1e9a1b): **MERGEABLE CON RONDA**, sin P1 ni P2. Verifico el arreglo con su propio
diferencial (20.000 casos por operacion de `U256`, y 3.000 acumulados con valores de hasta 10^38 contra la semantica
de EVM): 0 discrepancias, y la misma prueba sobre el codigo viejo encuentra el bug (5.415 comparaciones mal). Confirmo
C35 muerto y C36 equivalente, y que los P3 de la ronda 1 quedaron bien. Pidio tres P3 de documentacion, arreglados
aca: 65 tests (no 64) sin el feature, la tabla de lo que corri al dia con f1e9a1b, y el sha256 y el largo nuevos de la
wasm de Reputation (f76f664 la cambia).

**Refutador de c0der (RARRAS-8, ronda 1 de c0der, sobre 49bbcb4): MERGEABLE CON RONDA**, sin P1. Lo que pidio y lo que
quedo (codigo y tests en 9457b3a, plan en e85d58d, este handoff despues):
- **P2-1** (codigo): una `agentWallet` pendiente sobrevivia a la revocacion del aprobado que la propuso (en EVM
  `setAgentWallet` es atomico). Con la opcion (a), que eligio c0der: `nft_revoke` y `nft_revoke_all` cancelan la
  pendiente, y el evento `agent_wallet_pending` la deja en los logs al ponerla y al cancelarla (I-9). Busque otro
  camino por el que un aprobado revocado deje una pendiente aceptable y no encontre ninguno: una aprobacion solo se va por
  `nft_revoke`, `nft_revoke_all` o una transferencia (que ya la cancelaba, tambien el resolve que deshace), asi que no
  hizo falta la (b). Tests: `revocar_al_aprobado_cancela_la_pendiente_que_propuso`, los dos `tras_nft_revoke*` (la
  wallet acepta -> `E16`), `revocar_cancela_tambien_la_del_dueno_y_aunque_la_cuenta_no_estuviera`,
  `revocar_no_cambia_el_storage_del_token`; mutantes W1..W8.
- **P2-2** (tests): O1 con U-ST5 (ALICE (1) -> BOB (1) no le devuelve nada a ALICE; despues el ciclo BOB (2) -> cuenta
  sin agentes -> BOB seis veces, con la cota de U-ST4 en cada paso) y WE-12 (el saldo libre de identity no baja en
  ningun ciclo); O2 con el resolve que deshace con una pendiente del receptor (`get_pending_agent_wallet` = `null`, y
  la wallet no acepta: `E16`); O4 con 80 vectores de `mul_u128_u64` (33 que acarrean de la palabra 1 a la 2) y 200
  resumenes con `|value|` hasta 1e38 y decimales 0..18 (72 que el mutante cambia), calculados con los enteros de
  Python (`contracts/erc8004-common/testdata/vectores_256.py`). Los cuatro mutantes, muertos.
- **P3** 1 a 8: G-6 (URIs con esquema), I-10 a I-12 (`E20` de autotransferencia, hasta 8 aprobaciones, sin
  `MetadataUpdate`), R-4 con el cambio de signo y las 56 cifras, la prueba atada al dueno o la `agentWallet` actual
  (3.3 del plan y "Modelo de confianza"), S-1, los `standards` de NEP-330 en las tres wasm (decision de c0der), las
  lineas 35, 37 y 128 del plan y V-2 a V-4 de Validation, y el paso 0c del deploy (mainnet despues de
  `UltravioletaDAO/arras`).
- Esta ronda no tuvo un refutador mio aparte: la regla del encargo es un subagente como mucho y ya lo use en las dos
  rondas de arriba; la verificacion de esta ronda la hace el refutador de c0der.

## Trampas que encontre

- `cargo near build` genera el ABI: todo tipo que un metodo recibe o devuelve necesita `JsonSchema`. Con
  `#[derive(Serialize)]` solo, los unitarios pasan y el build de la wasm falla; va `#[near(serializers = [json])]`
  (como las vistas de registry-v1).
- `#[private]` lo pone el wrapper de la wasm, no el metodo de Rust: en un unitario se puede llamar un callback
  privado desde cualquiera. Se prueba en el sandbox (WE-00, WE-07), y por eso sus mutantes son del sandbox.
- El mock de near-sdk arranca `env::storage_usage()` en el valor del `VMContext` de cada `testing_env!`; para medir
  entre llamadas, los contextos de los tests le pasan el uso del contexto anterior (y se llama `flush_all` antes),
  asi U-ST4 mide la invariante de la reserva a lo largo de 400 pasos.
- El mock cobra el gas distinto que la wasm: "con el gas minimo justo pasa" no se puede probar en un unitario (da
  `GasExceeded`); va en el sandbox (WE-09).
- `nft_transfer` exige exactamente 1 yocto (NEP-171): una transferencia no puede cobrar storage, y aun asi puede crear
  el contador de un dueno nuevo. Por eso la reserva por token (seccion 2.4 del plan): el storage del contrato queda
  siempre pagado (WE-10, WE-12, U-ST4 y U-ST5 lo cubren).
- NEAR corta en 16.384 bytes de logs por recibo: con una URI de 8 KB y metadata al tope, `register` tiene que entrar
  ahi (U-ST1). Por eso las URIs son de RFC 3986 estricto (sin `"` ni `\`, que JSON escaparia al doble).
- Sacar un caracter de un `payment_id` de 44 caracteres puede dar otro `payment_id` valido de 32 bytes: no hay largo fijo
  (como ya decia `escrow-core`).
- **Un `Ord` derivado sobre palabras en little-endian compara al reves** (refutador, P1): `U256` decidia mal el signo
  del resumen con calificaciones positivas y negativas de mas de 2^64 normalizadas (un `value` de 19 con 0 decimales ya
  pasa), y `get_summary` daba numeros de 58 cifras. Ahora `Ord` compara desde la palabra mas significativa, con tests de
  signos mezclados, uno diferencial de 2.000 casos contra la referencia en i128, y los mutantes C34 y C35.
- **El orden en que se mide lo liberado importa** (refutador de c0der, O1): en una transferencia, lo que vuelve al
  dueno anterior se mide sobre las aprobaciones **antes** de mover el indice por dueno. Medido despues, le devolveria
  el contador de 77 bytes del dueno que se queda sin agentes, que es justo la reserva que va a necesitar el proximo
  dueno nuevo. Ningun test lo fijaba (U-ST4 es al azar y casi nunca arma el ciclo); ahora U-ST5 y WE-12.
- **Un diferencial contra i128 no ve el acarreo de 256 bits** (O4): la referencia en i128 nunca llega a productos de
  mas de 2^128, y con `b = 10^k` el acarreo de la palabra 1 a la 2 de `mul_u128_u64` pasa en ~5 % de los `a` al azar.
  Los vectores de `contracts/erc8004-common/testdata/vectores_256.py` (enteros de Python) lo buscan a proposito.
- `nft_resolve_transfer` reserva 15 TGas fijos (peso 0), como la implementacion de referencia: un receptor que devuelva
  una respuesta enorme podria dejarlo sin gas; entonces el agente queda en el receptor (que eligio quien transfirio) y
  el storage de las aprobaciones queda en el contrato (el contrato no pierde saldo).
