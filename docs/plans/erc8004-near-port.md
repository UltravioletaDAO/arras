# Plan: port fiel de ERC-8004 a NEAR (Identity, Reputation y Validation Registry) bajo `erc-8004.near`

Rama `0xultravioleta/c0-arras-erc8004`, desde `origin/main` 8cec4e6. Fase 1 del encargo de c0der (2026-11-04), con el
CONTEXTO 1 de c0der (fechado por c0der ~14:00Z) sumado. Este documento es el plan: lo que se porta, como, y cada desviacion que NEAR obliga.
`contracts/registry-v1` y `registry.arras-escrow.near` no se tocan (siguen sirviendo a EM y a los SDK hasta que los
registros nuevos pasen el golden flow en mainnet).

## 0. Fuentes, en un commit fijo

| Fuente | Commit / medicion | Que se leyo |
|---|---|---|
| EIP-8004 (lo que publica eips.ethereum.org) | `ethereum/ERCs`, `ERCS/erc-8004.md`, ultimo commit del archivo `503591a6e80e6e1affdd6403341e25269141f046` (2026-01-25) | la spec entera; es **identica** byte a byte (salvo espacios al final de linea) a `ERC8004SPEC.md` de los contratos oficiales |
| Contratos oficiales | `erc-8004/erc-8004-contracts` `b9e466c250744a7e06b13dff9d3c2844ed64f825` (2026-08-15) | `IdentityRegistryUpgradeable.sol`, `ReputationRegistryUpgradeable.sol`, `ValidationRegistryUpgradeable.sol`, `MinimalUUPS*.sol`, `scripts/addresses.ts`, `README.md` |
| Buenas practicas oficiales (CONTEXTO 1, punto 3) | `erc-8004/best-practices` `8a99b86e9b3b518138187686fa8f757beb01cea8` (2026-01-25) | `Registration.md`, `Reputation.md`, `src/ERC8004SPEC.md` (una copia algo anterior de la spec: sin la mencion de `data:` en el agentURI y con la seccion de feedback en otro orden) |
| Base mainnet (RPC publica `mainnet.base.org`, 2026-11-04 14:13:23Z a 14:13:25Z, `date -u`) | `getVersion()` y `codesize` en las tres direcciones | Identity `0x8004A169FB4a3325136EB29fA0ceB6D2e539a432`, Reputation `0x8004BAa17C55a88189AE136b182e5fdA19dE9b63` y Validation `0x8004Cc8439f36fd5F9F049D9fF86523Df6dAAB58` devuelven `"2.0.0"` y tienen 130 bytes de codigo cada una; Validation (proxy ERC-1967, implementacion `0xdb31f5d9...7f99`) y su `getIdentityRegistry()` es `0x8004A169...` |
| NEAR mainnet (RPC publica, 2026-11-04 14:02:32Z por `date -u`, bloques 223.026.252 a 223.026.285) | `view_account`, `view_access_key_list`, `call_function` | `erc-8004.near` existe (0,0729 NEAR, 1 llave FullAccess); `validation-registry.erc-8004.near` existe (0,02 NEAR, sin codigo, la misma llave); `identity-registry.erc-8004.near` y `reputation-registry.erc-8004.near` **no existen**; `8004.near` es de un tercero; `registry.arras-escrow.near`: `get_config` dice `agents` 0, `feedbacks` 4, sin llaves; `storage_amount_per_byte` = 10^19 yocto (protocolo 87; testnet en 88) |
| CAIP-2 | `ChainAgnostic/namespaces` en `2a9193da0e4f86b4990983e7a9a8781d6965a439` (2026-11-02) | **no hay namespace `near` registrado** (la lista de carpetas no lo trae); `status` de la RPC de NEAR da `chain_id` `mainnet` y `testnet` |

La version vigente es la **2.0.0** de los contratos (la que corre en Base): feedback con `value` (`int128`) y
`valueDecimals` (0..18), `endpoint`, `appendResponse`, sin autorizacion previa del agente. La tabla de equivalencias de
`docs/handoffs/registry-v1.md:348-354` compara contra una version anterior (`score` 0..100 y `feedbackAuth` firmado por
el agente); no se arregla aca porque registry-v1 no se toca, pero la vigente es la de este plan.

### 0.1 Lo que dicen los `.sol` (cada funcion, evento, revert, tope e indice)

**Identity** (`IdentityRegistryUpgradeable`, ERC-721 + URIStorage, nombre `AgentIdentity`, simbolo `AGENT`, EIP-712
`ERC8004IdentityRegistry` version `1`):

| Funcion | Quien | Reverts | Efecto |
|---|---|---|---|
| `register()` / `register(agentURI)` / `register(agentURI, MetadataEntry[])` | cualquiera | `"reserved key"` si una clave es `agentWallet` | `agentId = _lastId++` (**el primero es 0**); `agentWallet = msg.sender`; `_safeMint` (llama `onERC721Received` si el que registra es un contrato); `tokenURI` si se paso; eventos `Transfer`, `Registered(agentId, agentURI, owner)`, `MetadataSet(agentWallet)`, un `MetadataSet` por entrada |
| `setAgentURI(agentId, newURI)` | dueno, operador (`isApprovedForAll`) o aprobado (`getApproved`) | `ERC721NonexistentToken`, `"Not authorized"` | `URIUpdated(agentId, newURI, msg.sender)`; `newURI` puede ser `""` |
| `setMetadata(agentId, key, value)` | idem | `"Not authorized"` (y `ERC721NonexistentToken` via `getApproved`), `"reserved key"` | `MetadataSet(agentId, key, key, value)`; sin tope de largo; un valor vacio deja la clave vacia |
| `getMetadata(agentId, key)` | vista | ninguno | `bytes` (vacio si no hay) |
| `setAgentWallet(agentId, newWallet, deadline, signature)` | dueno/operador/aprobado | `"bad wallet"` (cero), `"expired"` (`now > deadline`), `"deadline too far"` (`deadline > now + 5 min`), `"invalid wallet sig"` | firma EIP-712 `AgentWalletSet(agentId, newWallet, owner, deadline)` de `newWallet` (ECDSA, o ERC-1271 si es contrato); `MetadataSet(agentWallet)` |
| `getAgentWallet(agentId)` | vista | ninguno | la direccion, o cero |
| `unsetAgentWallet(agentId)` | dueno/operador/aprobado | `"Not authorized"` | `agentWallet = ""`, `MetadataSet(agentWallet, "")` |
| `_update` (toda transferencia que no es mint ni burn) | - | - | `agentWallet = ""` y `MetadataSet(agentWallet, "")` **antes** de mover el token |
| `isAuthorizedOrOwner(spender, agentId)` | vista | `ERC721NonexistentToken` | `_isAuthorized`: dueno, operador o aprobado |
| `getVersion()` | vista | - | `"2.0.0"` |

No hay `burn` expuesto. No hay topes de largo en ningun string ni en `bytes`.

**Reputation** (`ReputationRegistryUpgradeable`): `initialize(identityRegistry_)` (`"bad identity"` si es cero),
`getIdentityRegistry()`.

| Funcion | Quien | Reverts | Efecto |
|---|---|---|---|
| `giveFeedback(agentId, int128 value, uint8 valueDecimals, tag1, tag2, endpoint, feedbackURI, bytes32 feedbackHash)` | cualquiera que **no** sea dueno ni operador/aprobado del agente | `"too many decimals"` (`> 18`), `"value too large"` (`\|value\| > 1e38`), `"Self-feedback not allowed"`, `ERC721NonexistentToken` (el agente no existe) | `feedbackIndex = ++_lastIndex[agentId][client]` (**empieza en 1**, por par agente-cliente); se guardan `value`, `valueDecimals`, `tag1`, `tag2`, `isRevoked`; `endpoint`, `feedbackURI` y `feedbackHash` **solo van en el evento**; el cliente entra a `_clients[agentId]` la primera vez; `NewFeedback(...)` |
| `revokeFeedback(agentId, feedbackIndex)` | el cliente | `"index must be > 0"`, `"index out of bounds"`, `"Already revoked"` | `isRevoked = true`, `FeedbackRevoked` |
| `appendResponse(agentId, clientAddress, feedbackIndex, responseURI, responseHash)` | **cualquiera** | `"index must be > 0"`, `"Empty URI"`, `"index out of bounds"` | cuenta respuestas por respondedor; `responseURI` y `responseHash` solo en el evento `ResponseAppended`; no mira si la calificacion esta revocada |
| `getSummary(agentId, clientAddresses, tag1, tag2)` | vista | `"clientAddresses required"` (lista vacia) | sobre las vigentes de esos clientes y tags (`""` = sin filtro): normaliza a 18 decimales, promedia (division entera de Solidity, trunca hacia cero), y escala a la **moda** de `valueDecimals` (empate: el menor); `(0, 0, 0)` sin calificaciones. Un cliente repetido en la lista **cuenta dos veces** |
| `readFeedback(agentId, client, index)` | vista | `"index must be > 0"`, `"index out of bounds"` | `(value, valueDecimals, tag1, tag2, isRevoked)` |
| `readAllFeedback(agentId, clientAddresses, tag1, tag2, includeRevoked)` | vista | - | lista vacia = todos los clientes; devuelve 7 arreglos paralelos |
| `getResponseCount(agentId, clientAddress, feedbackIndex, responders)` | vista | - | `clientAddress == 0`: todos; `feedbackIndex == 0`: todas las del cliente; `responders` vacio: todos |
| `getClients(agentId)`, `getLastIndex(agentId, client)`, `getVersion()` | vistas | - | |

**Validation** (`ValidationRegistryUpgradeable`): `initialize(identityRegistry_)`, `getIdentityRegistry()`.

| Funcion | Quien | Reverts | Efecto |
|---|---|---|---|
| `validationRequest(validatorAddress, agentId, requestURI, requestHash)` | dueno, operador o aprobado | `"bad validator"` (cero), `"exists"` (el `requestHash` ya se uso: **unico global**), `"Not authorized"` | guarda `{validator, agentId, response 0, responseHash 0, tag "", lastUpdate = now, hasResponse false}`; indices por agente y por validador; `requestURI` solo en el evento |
| `validationResponse(requestHash, response, responseURI, responseHash, tag)` | solo el validador nombrado | `"unknown"`, `"not validator"`, `"resp>100"` | pisa `response`, `responseHash`, `tag`, `lastUpdate`; `hasResponse = true`; **se puede llamar muchas veces** |
| `getValidationStatus(requestHash)` | vista | `"unknown"` | |
| `getSummary(agentId, validatorAddresses, tag)` | vista | - | solo las respondidas; lista vacia = todos; `avg = floor(sum / count)` |
| `getAgentValidations(agentId)`, `getValidatorRequests(validator)`, `getVersion()` | vistas | - | |

**Archivos fuera de cadena** (EIP): el *registration file* (`type`, `name`, `description`, `image`, `services[]`,
`x402Support`, `active`, `registrations[{agentId, agentRegistry}]`, `supportedTrust[]`) y el *feedback file* (MUST:
`agentRegistry`, `agentId`, `clientAddress` en CAIP-10, `createdAt`, `value`, `valueDecimals`; opcionales: `tag1`,
`tag2`, `endpoint`, `mcp`, `a2a`, `oasf`, `proofOfPayment {fromAddress, toAddress, chainId, txHash}` "for x402 proof of
payment"). `feedbackHash` y `responseHash` son KECCAK-256 del archivo (no hace falta para IPFS).

### 0.2 El Validation Registry (CONTEXTO 1, punto 1)

`8004.org/learn` (leido por c0der hoy): "the Validation Registry is still undergoing technical due diligence and is not
yet available". El README de los contratos oficiales dice lo mismo ("still under active update and discussion with the
TEE community") y lista solo Identity y Reputation por cadena, aunque en Base la de Validation ya responde `2.0.0`.
Decision: **Validation queda como diseno** (seccion 4) y como crate `contracts/erc8004-validation` marcado **BORRADOR**,
fuera de lo desplegable: no entra al plan de deploy de ahora. `validation-registry.erc-8004.near` ya existe (tx
`51LwejfbR5fW5ov1edAYjgAtQY7BV6N2JTEHGtYqbcw`, 0,02 NEAR, sin contrato): cuando el equipo de ERC-8004 cierre esa seccion,
el paso es deploy + `new` + borrar llaves, **sin** crear la cuenta (seccion 7).

### 0.3 Un solo despliegue por cadena (CONTEXTO 1, punto 2)

`8004.org/learn`: "it is important for the protocol architecture that there is only one deployment per chain (a
singleton). So if you want to deploy the protocol on your chain, we invite you to coordinate with the ERC-8004 team".
El contacto lo decide el dueno. **El deploy en mainnet puede esperar esa coordinacion; el plan, los crates, los tests y
testnet no la esperan.** Una consecuencia de diseno: si `identity-registry.erc-8004.near` va a ser *el* singleton de
NEAR, el nucleo no deberia llevar nada propio de Arras (ver la decision c y la PREGUNTA 1).

## 1. Mapa general EVM -> NEAR

| EVM | NEAR | Por que |
|---|---|---|
| 3 proxies UUPS por cadena, con owner que puede actualizar | 3 contratos **inmutables**: `new` privado en la misma tx del deploy, sin `migrate`, sin owner, llaves borradas despues | eleccion del dueno ("Inmutables"); un arreglo va en una cuenta nueva con version (`reputation-registry-v2.erc-8004.near`) |
| `address` (20 bytes), con el cero como "no hay" | `AccountId` (2..64 bytes); "no hay" es `null` | NEAR no tiene direccion cero |
| `msg.sender` | `env::predecessor_account_id()` | el que llama directo, no el que firmo la tx (igual que registry-v1, decision 8) |
| ERC-721 + URIStorage | NEP-171 (core) + NEP-177 (metadata) + NEP-178 (aprobaciones) + NEP-181 (enumeracion) | el estandar de NFT de NEAR; `agentURI` es `metadata.reference` (NEP-177: "URL to an off-chain JSON file") |
| `uint256 agentId` | `agent_id`: string decimal **canonico** (`"0"`, `"17"`; sin signo, sin ceros a la izquierda, dentro de `u64`), igual al `token_id` de NEP-171 | NEP-171 usa `token_id: string`; JSON no lleva enteros grandes; `u64` alcanza para un contador |
| `int128 value`, `uint64 feedbackIndex`, `uint256 timestamp` | strings decimales canonicos (`"-32"`, `"1"`); `value_decimals`, `response` y `summary_value_decimals` siguen como numero JSON (`u8`) | regla de NEAR para enteros de mas de 53 bits; canonico para que `"01"` no sea otro indice |
| `bytes` (metadata) | `Base64VecU8` (string base64) | JSON no lleva bytes |
| `bytes32` (hashes) | 64 hex en minusculas sin `0x` (como `terms_hash` y registry-v1); ausente = `null` en vez de `bytes32(0)` | |
| `block.timestamp` (segundos) | `env::block_timestamp()` (**nanosegundos**, string) | el reloj de NEAR |
| eventos indexados de Solidity | NEP-297 `EVENT_JSON:` con `standard` `erc8004`, `version` `1.0.0`, `data` siempre arreglo; y los de NEP-171 (`nep171` `1.0.0`: `nft_mint`, `nft_transfer`) | los logs de NEAR; los `indexed` duplicados (`indexedTag1`, `indexedMetadataKey`) no hacen falta |
| llamada sincrona a otro contrato (`isAuthorizedOrOwner`) | llamada cruzada **asincrona** + callback privado que **falla cerrado** | NEAR no tiene llamadas sincronas entre cuentas |
| gas pagado por quien llama, storage "gratis" | **storage staking**: 1 NEAR por 100 KB; quien llama lo paga con el deposito adjunto, se cobra lo medido y vuelve el sobrante | protocolo de NEAR |
| EIP-712 / ECDSA / ERC-1271 (`setAgentWallet`) | la wallet nueva **confirma llamando ella misma** (`accept_agent_wallet`, el `predecessor` es la prueba) | un contrato de NEAR no puede saber que una llave publica es de una cuenta con nombre (decision a) |
| sobrecargas (`register()` x3) | un metodo con argumentos opcionales | NEAR no tiene sobrecarga |
| `revert` en vistas (`"unknown"`, `"index out of bounds"`) | `null` | una vista de NEAR puede devolver `null`; es mas facil para los SDK |
| sin topes de largo | topes en cada string y en cada lista que se guarda o se emite | storage pagado y el tope de logs de NEAR: **16.384 bytes de logs por recibo** (`max_total_log_length`) y 100 logs |

## 2. Identity Registry: `identity-registry.erc-8004.near` (crate `erc8004-identity`)

### 2.1 Metodos que escriben

Todos usan el `predecessor`. "Autorizado" = el dueno o una cuenta aprobada por NEP-178 para ese token (el `getApproved`
de ERC-721). Codigos de panic `ERC8004_ID_Enn`.

| EVM | NEAR (args JSON) | Quien | Deposito | Eventos |
|---|---|---|---|---|
| `register()`, `register(uri)`, `register(uri, metadata[])` | `register({"agent_uri"?: str, "metadata"?: [{"metadata_key": str, "metadata_value": base64}]})` -> `agent_id` (string) | cualquiera | lo medido + la **reserva del token** (2.4); vuelve el sobrante | `nft_mint`, `registered`, `metadata_set` (`agentWallet` = el que registra), un `metadata_set` por entrada |
| `setAgentURI(agentId, newURI)` | `set_agent_uri({"agent_id", "new_uri": str})` (`""` borra) | autorizado | >= 1 yocto y lo que crezca; lo liberado vuelve al **dueno** | `uri_updated` |
| `setMetadata(agentId, key, value)` | `set_metadata({"agent_id", "metadata_key", "metadata_value": base64})` (vacio borra la clave) | autorizado | idem | `metadata_set` |
| `setAgentWallet(agentId, newWallet, deadline, sig)` | `set_agent_wallet({"agent_id", "new_wallet": acc, "deadline": ns})` -> `bool` | autorizado | exactamente 1 yocto | si `new_wallet` es el que llama: `metadata_set` (`agentWallet`) y devuelve `true`; si no, queda **pendiente** y devuelve `false` |
| (la firma de la wallet) | `accept_agent_wallet({"agent_id"})` | **la wallet pendiente** | exactamente 1 yocto | `metadata_set` (`agentWallet`) |
| `unsetAgentWallet(agentId)` | `unset_agent_wallet({"agent_id"})` (tambien borra la pendiente) | autorizado | exactamente 1 yocto | `metadata_set` (`agentWallet`, `""`) |
| `transferFrom` / `safeTransferFrom` | `nft_transfer({"receiver_id", "token_id", "approval_id"?, "memo"?})` | dueno o aprobado (con su `approval_id` si lo pasa) | exactamente 1 yocto | `nft_transfer`, `metadata_set` (`agentWallet`, `""`) |
| `safeTransferFrom(..., data)` | `nft_transfer_call({... , "msg"})` -> `bool` | idem | exactamente 1 yocto | idem; si el receptor devuelve `true`, `nft_resolve_transfer` lo devuelve y emite otro `nft_transfer` |
| `approve(to, tokenId)` | `nft_approve({"token_id", "account_id", "msg"?})` | **solo el dueno** (NEP-178) | lo que crezca | (NEP-178 no tiene eventos) |
| `approve(0, tokenId)` | `nft_revoke({"token_id", "account_id"})`, `nft_revoke_all({"token_id"})` | solo el dueno | exactamente 1 yocto; lo liberado vuelve al dueno | |
| `setApprovalForAll` | **no se porta** (desviacion I-3) | | | |

### 2.2 Vistas

| EVM | NEAR |
|---|---|
| `ownerOf`, `tokenURI`, `getApproved` | `nft_token({"token_id"})` -> `{"token_id","owner_id","metadata":{..."reference": agentURI|null...},"approved_account_ids":{acc: id}}` o `null`; y `token_uri({"agent_id"})` -> string (`""` si no tiene) o `null` |
| `balanceOf`, enumeracion | `nft_total_supply`, `nft_tokens({"from_index"?: U128, "limit"?})`, `nft_supply_for_owner({"account_id"})`, `nft_tokens_for_owner({...})` (NEP-181) |
| `name()`, `symbol()` | `nft_metadata()` -> `{"spec":"nft-1.0.0","name":"AgentIdentity","symbol":"AGENT",...}` |
| `getMetadata(agentId, key)` | `get_metadata({"agent_id","metadata_key"})` -> base64 o `null`; con `agentWallet` devuelve los bytes UTF-8 de la cuenta |
| `getAgentWallet(agentId)` | `get_agent_wallet({"agent_id"})` -> cuenta o `null` |
| (sin equivalente) | `get_pending_agent_wallet({"agent_id"})` -> `{"wallet","deadline"}` o `null` |
| `isAuthorizedOrOwner(spender, agentId)` | `is_authorized_or_owner({"spender","agent_id"})` -> `bool`; **entra en panico** si el agente no existe (como el revert) |
| (para las llamadas cruzadas) | `check_agent({"agent_id","account_id"})` -> `{"owner_id","agent_wallet","authorized"}` o `null` (no entra en panico: asi Reputation y Validation distinguen "no existe" de "fallo la vista") |
| `getVersion()` | `get_version()` -> `"2.0.0"` (la version de la implementacion de referencia que porta) y `get_config()` (topes, depositos, `token_reserve_bytes`) |

### 2.3 Decision a: agentWallet sin EIP-712

Un contrato de NEAR no puede comprobar que una llave publica sea de una cuenta con nombre (las access keys no se leen
desde un contrato). El equivalente: **la wallet nueva lo confirma llamando ella misma** (el `predecessor` es la prueba,
como el `ecrecover` es la prueba en EVM; cubre tambien a los contratos-wallet, que en EVM necesitan ERC-1271):

1. el autorizado llama `set_agent_wallet(agent_id, new_wallet, deadline)` con `now <= deadline <= now + 5 min` (los
   mismos `"expired"` y `"deadline too far"`; en nanosegundos). Si `new_wallet` es el mismo que llama (el dueno que se
   pone a si mismo, o el aprobado a si mismo), queda puesta en el acto: es el caso en que en EVM la firma la hace el
   mismo que manda la tx. Si no, queda **pendiente** `{wallet, deadline}` (una por agente; la nueva pisa la vieja);
2. `new_wallet` llama `accept_agent_wallet(agent_id)` (1 yocto: una llave FullAccess, como una firma) antes del
   `deadline` y queda puesta.

La firma de EVM ata `owner`: aca la pendiente se borra en cada transferencia, asi que una aceptacion nunca vale para
otro dueno. `register` pone `agentWallet` = el que registra; **toda transferencia la limpia** (y la pendiente), como
`_update`, y emite `metadata_set` con `""`. `agentWallet` sigue reservada: `set_metadata` y `register` la rechazan.
Para cuentas implicitas (la cuenta ES la llave ed25519) se podria verificar una firma con `env::ed25519_verify`; no se
hace: el camino de dos pasos cubre a todas.

### 2.4 Storage del NFT: la reserva del token

NEP-171 exige **exactamente 1 yocto** en `nft_transfer` (las wallets y los marketplaces mandan eso): una transferencia
no puede cobrar storage. Si una transferencia pudiera hacer crecer el estado (el indice por dueno de NEP-181 crea un
registro para un dueno nuevo), cualquiera podria vaciar el saldo del contrato transfiriendo a cuentas nuevas, y un
contrato inmutable sin saldo para su storage queda congelado. Por eso:

- lo que cambia en una transferencia es de **largo fijo**: el dueno, `agentWallet` y la pendiente se guardan en 65
  bytes fijos cada uno (largo + 64 bytes), y las claves del indice por dueno usan el sha256 de la cuenta;
- lo unico que puede crecer es la cantidad de indices por dueno (uno por dueno con al menos un agente, nunca mas que
  agentes), y `register` cobra un indice entero de mas por agente (`TOKEN_RESERVE_BYTES`, que no se devuelve). Invariante
  que prueban los tests: `storage_usage <= estado inicial + lo cobrado`;
- `agentWallet`, la pendiente y el dueno nunca cobran ni devuelven nada al cambiar (ya estan pagados);
- las aprobaciones de NEP-178 si se cobran (`nft_approve`, medido) y se devuelven al dueno al revocar o transferir;
- `agent_uri` y la metadata se cobran medidos a quien llama; lo liberado vuelve al **dueno** (desviacion S-1).

## 3. Reputation Registry: `reputation-registry.erc-8004.near` (crate `erc8004-reputation`)

`new({"identity_registry": acc})` privado; no puede ser la propia cuenta. No cambia nunca (decision f).

### 3.1 Metodos que escriben

| EVM | NEAR (args JSON) | Quien | Deposito / gas | Evento |
|---|---|---|---|---|
| `giveFeedback(...)` | `give_feedback({"agent_id", "value": "-32", "value_decimals": 1, "tag1"?, "tag2"?, "endpoint"?, "feedback_uri"?, "feedback_hash"?})` -> `feedback_index` (string) o `null` | cualquiera que no sea dueno ni aprobado | `feedback_deposit` de `get_config`; se cobra lo medido, vuelve el resto (todo si se rechaza); gas minimo de `get_config` | `new_feedback` |
| (extension, decision c) | `give_feedback_with_proof({... lo mismo ..., "verifier": acc, "proof": str})` | idem | `feedback_proof_deposit`; mas gas | `new_feedback` con `"proof": {...}` |
| `revokeFeedback(agentId, feedbackIndex)` | `revoke_feedback({"agent_id","feedback_index"})` | el cliente | exactamente 1 yocto | `feedback_revoked` |
| `appendResponse(...)` | `append_response({"agent_id","client_address","feedback_index","response_uri","response_hash"?})` | cualquiera | >= 1 yocto y lo que crezca | `response_appended` |

### 3.2 Decision b: la guarda de identidad es una llamada cruzada que falla cerrado

`give_feedback` valida todo lo sincronico (panic, el runtime devuelve el deposito), llama a
`identity.check_agent(agent_id, predecessor)` y decide **en el callback**, que nunca entra en panico:

- lee primero el resultado con tope de bytes (`env::promise_result_checked`): fuera de un callback de verdad el host
  aborta antes de tocar nada (el patron de registry-v1, decision 1);
- rechaza si la vista fallo, si la respuesta no es exactamente un `AgentCheck` (`deny_unknown_fields`, sin campos de
  menos), si es `null` (el agente no existe: el `ERC721NonexistentToken` de EVM) o si `authorized` es `true` (el
  dueno o un aprobado: `"Self-feedback not allowed"`);
- un rechazo **no escribe nada**, devuelve **todo lo adjuntado** y deja el log `erc8004:rechazo:<CODIGO>`;
- si acepta, `feedback_index = ++last_index[(agent, cliente)]` se asigna en el callback (atomico dentro del recibo).

El nucleo **no necesita candado**: EVM permite muchas calificaciones del mismo cliente al mismo agente, y el indice se
asigna en el callback. Dos `give_feedback` en vuelo dan los indices 1 y 2, en el orden en que llegan los callbacks. El
candado (registry-v1, decision 2) se usa donde hay unicidad: la prueba de pago (3.3) y el `request_hash` de Validation.

Lo que se comprueba es el estado del Identity Registry **cuando corre la vista**, no cuando se firmo la tx (en EVM es
el mismo instante). Si el agente cambia de dueno entre la vista y el callback, vale lo que vio la vista.

### 3.3 Decision c: el pago de Arras como prueba OPCIONAL

El nucleo acepta feedback de cualquier cliente. Para que EM siga con reputacion respaldada por pagos **encima** del port
fiel, propongo (PREGUNTA 1) que el Reputation Registry acepte pruebas de **verificadores** y que el de Arras sea un
contrato aparte:

- `give_feedback_with_proof(..., verifier, proof)` hace lo mismo que `give_feedback` y ademas, **en paralelo**, llama a
  `verifier.verify_feedback_proof({"client": predecessor, "proof": proof})`. El callback recibe los dos resultados y
  acepta solo si: la identidad pasa (3.2); el verificador devolvio exactamente
  `{"counterparty": acc, "role": str, "context": str, "weight": U128}`; `counterparty` es el **dueno** o la
  **agentWallet** del agente calificado (asi la prueba es de un trato CON ese agente, no un pago cualquiera); el cliente
  no es la `agentWallet` del agente; y nadie califico ya con esa prueba (`(verifier, proof, cliente)` unico, con candado
  en vuelo como registry-v1: `pending` con `nonce` = altura de bloque, TTL de 1.000 bloques).
- La calificacion queda **igual que una del nucleo** (cuenta en `get_summary`, se revoca igual) y ademas guarda
  `{verifier, proof, counterparty, role, context, weight}`. `get_verified_summary(agent_id, verifier, ...)` resume solo
  las de ese verificador, con filtro opcional por `context`, y suma `weight`.
- **No hay lista de verificadores** en el registro: cualquiera puede ser verificador, y quien lee filtra por el
  verificador en el que confia (como filtra por `clientAddress` en ERC-8004). Sin listas de usuarios ni aprobacion
  manual de nadie.
- **El verificador de Arras** (`contracts/erc8004-arras-verifier`): `new({"escrow": acc})` fija UN escrow (contratos, no
  personas; en mainnet `v1.arras-escrow.near`). `verify_feedback_proof(client, proof)`: `proof` es el `payment_id`
  canonico; llama `escrow.get_payment(proof)` y en su callback exige el espejo estricto del `Payment` de v1 (el de
  registry-v1, RONDA 1, decision 5), `released > 0`, `payer != receiver`, que `client` sea el payer o el receiver; y
  devuelve la otra parte como `counterparty`, su rol, `context` = `operator_id` y `weight` = `released`. Si algo falla,
  entra en panico: el verificador no tiene deposito que devolver, y el Reputation Registry ve la promesa fallida y
  rechaza devolviendo todo.

Por que asi y no la extension con la lista de escrows dentro del Reputation Registry (la del encargo): el Reputation
Registry es inmutable y es (o va a ser) el singleton de NEAR. Si la lista de escrows vive adentro, el dia que EM pase a
`escrow-v2` (o haya otro escrow) hay que desplegar `reputation-registry-v2` y **toda** la reputacion de NEAR queda
partida en dos registros. Con verificadores, un escrow nuevo es un verificador nuevo y el registro no cambia. Y si el
equipo de ERC-8004 pide un nucleo sin extensiones (0.3), la extension esta detras del feature de cargo
`feedback-proofs`: sin el, el wasm es el port puro. Las tres opciones, en la PREGUNTA 1.

**Decidido por el dueno (RESPUESTA 2 de c0der, respuesta del dueno de las 14:21:18Z): opcion (A)**, con el verificador
en `erc8004-verifier.arras-escrow.near` (hija de `arras-escrow.near`; ver la seccion 7).

Para EM: el worker que recibe feedback tiene que tener un agente (un NFT) en el Identity Registry, porque ERC-8004
califica `agentId`, no cuentas (registry-v1 no lo pedia). Precedente (verificado por c0der, RESPUESTA 1): EM ya da de
alta a cada worker en el Identity Registry de Base con su primera tarea completada (`mcp_server/api/routers/_helpers.py:947`,
WS-1). **El dueno (RESPUESTA 2, textual): "prrimero em paga el alta en el futuro el worker firma su alta".** Los dos
caminos salen con los metodos del estandar, **sin metodo nuevo**:

- **Hoy, EM paga:** EM llama `register({"agent_uri": ...})` (paga el storage y la reserva; EM queda dueno y
  `agentWallet`) y despues `nft_transfer({"receiver_id": <worker>, "token_id": <agent_id>})` con 1 yocto: el worker queda
  **dueno** de su NFT y la transferencia limpia `agentWallet` (como en EVM). El worker no firma nada. Para la prueba de
  Arras alcanza con que el worker sea el dueno (la otra parte del pago puede ser el dueno o la `agentWallet`), asi que
  cobra con la misma cuenta. Si el worker quiere ademas su `agentWallet`, es una tx suya: `set_agent_wallet` a si mismo
  (1 yocto, queda puesta en el acto). Dos tx de EM (en NEAR no se puede saber el `agent_id` antes de que corra `register`:
  meterlas en un solo batch dependeria de adivinar `nft_total_supply`).
- **Despues, el worker firma:** el worker llama `register` y paga; queda dueno y `agentWallet`. Una tx.

Un `register_for(owner, ...)` (alta a nombre de otro, pagada por quien llama) ahorraria la segunda tx de EM, pero no
hace falta (RESPUESTA 2: "si con register + nft_transfer alcanza, no agregues nada") y no se agrega. Lo que cuesta
cada camino (deposito, gas y tx) lo mide la fase 3 en el sandbox y queda en el handoff.

### 3.4 Vistas

| EVM | NEAR |
|---|---|
| `getIdentityRegistry()` | `get_identity_registry()` |
| `getSummary(agentId, clients, tag1, tag2)` | `get_summary({"agent_id","client_addresses": [acc,...] (no vacia: panic),"tag1"?,"tag2"?})` -> `{"count": "2", "summary_value": "-35", "summary_value_decimals": 1}` |
| `readFeedback(agentId, client, index)` | `read_feedback({"agent_id","client_address","feedback_index"})` -> `{"value","value_decimals","tag1","tag2","is_revoked"}` o `null` |
| `readAllFeedback(...)` | `read_all_feedback({"agent_id","client_addresses"?,"tag1"?,"tag2"?,"include_revoked"?})` -> `{"clients":[],"feedback_indexes":[],"values":[],"value_decimals":[],"tag1s":[],"tag2s":[],"revoked_statuses":[]}` (los 7 arreglos de EVM) |
| `getResponseCount(...)` | `get_response_count({"agent_id","client_address"?,"feedback_index"?,"responders"?})` (`null`/ausente = todos, como el cero de EVM) |
| `getClients(agentId)` | `get_clients({"agent_id","from_index"?,"limit"?})` (sin paginar = todos, como EVM) |
| `getLastIndex(agentId, client)` | `get_last_index({"agent_id","client_address"})` -> string |
| (extension) | `read_feedback_proof`, `get_proof_feedback({"verifier","proof","client_address"})`, `get_verified_summary`, `list_verified_feedback`, `is_proof_pending` |

`summary_value` es el **valor exacto** como string: la cuenta se hace en 256 bits (como el `int256` de Solidity). EVM
hace `int128(avg)` al final, que **trunca en silencio** si el promedio escalado no entra en `int128` (puede pasar: un
valor de 1e38 con 0 decimales y la moda en 18); aca se devuelve el numero correcto (desviacion R-4).

## 4. Validation Registry (BORRADOR): `validation-registry.erc-8004.near` (crate `erc8004-validation`)

Diseno completo; no se despliega hasta que el equipo de ERC-8004 cierre la seccion (0.2).

| EVM | NEAR | Quien | Deposito | Evento |
|---|---|---|---|---|
| `validationRequest(validator, agentId, requestURI, requestHash)` | `validation_request({"validator_address","agent_id","request_uri","request_hash"})` -> `bool` | autorizado del agente (llamada cruzada a `check_agent`, callback que falla cerrado: `null` = no existe, `authorized: false` = `"Not authorized"`) | `request_deposit`, lo medido; todo de vuelta si se rechaza | `validation_request` |
| `validationResponse(requestHash, response, responseURI, responseHash, tag)` | `validation_response({"request_hash","response": 0..100,"response_uri"?,"response_hash"?,"tag"?})` | solo el validador nombrado | >= 1 yocto y lo que crezca (el `tag`); lo liberado vuelve al validador | `validation_response` |
| `getValidationStatus` | `get_validation_status({"request_hash"})` -> `{"validator_address","agent_id","response","response_hash","tag","last_update","has_response"}` o `null` | | | |
| `getSummary(agentId, validators, tag)` | `get_summary({"agent_id","validator_addresses"?,"tag"?})` -> `{"count","average_response"}` | | | |
| `getAgentValidations`, `getValidatorRequests` | `get_agent_validations({"agent_id","from_index"?,"limit"?})`, `get_validator_requests({...})` -> hashes hex | | | |

El `request_hash` es unico global, como en EVM: `validation_request` pone un candado en vuelo sobre el hash (dos pedidos
iguales a la vez: el segundo entra en panico) y el callback vuelve a mirar que no exista.

## 5. Eventos NEP-297

`EVENT_JSON:{"standard":"erc8004","version":"1.0.0","event":<nombre>,"data":[{...}]}`. Los nombres son los de EVM en
snake_case; los campos, los parametros de EVM en snake_case (sin los `indexed` duplicados). Enteros grandes como string.

| EVM | NEAR `event` | `data[0]` |
|---|---|---|
| `Transfer` (mint) | `nft_mint` (`nep171` 1.0.0) | `{"owner_id","token_ids":["0"]}` |
| `Transfer` | `nft_transfer` (`nep171` 1.0.0) | `{"authorized_id"?,"old_owner_id","new_owner_id","token_ids":["0"],"memo"?}` |
| `Registered` | `registered` | `{"agent_id","agent_uri","owner"}` |
| `MetadataSet` | `metadata_set` | `{"agent_id","metadata_key","metadata_value": base64}` |
| `URIUpdated` | `uri_updated` | `{"agent_id","new_uri","updated_by"}` |
| `NewFeedback` | `new_feedback` | `{"agent_id","client_address","feedback_index","value","value_decimals","tag1","tag2","endpoint","feedback_uri","feedback_hash","proof"}` (`proof` = `null` o `{"verifier","proof","counterparty","role","context","weight"}`) |
| `FeedbackRevoked` | `feedback_revoked` | `{"agent_id","client_address","feedback_index"}` |
| `ResponseAppended` | `response_appended` | `{"agent_id","client_address","feedback_index","responder","response_uri","response_hash"}` |
| `ValidationRequest` | `validation_request` | `{"validator_address","agent_id","request_uri","request_hash"}` |
| `ValidationResponse` | `validation_response` | `{"validator_address","agent_id","request_hash","response","response_uri","response_hash","tag"}` |

Como en registry-v1: un indexador cuenta solo los eventos de recibos cuyo `executor_id` es el registro y cuyo resultado
final es exito; cualquier contrato puede emitir el mismo `EVENT_JSON` en su propio recibo.

## 6. Desviaciones (cada una con su porque)

| # | Desviacion | Porque |
|---|---|---|
| G-1 | inmutables, sin upgrade ni owner | eleccion del dueno; arreglos en cuentas nuevas con version |
| G-2 | `AccountId` y `null` en vez de `address` y cero | NEAR |
| G-3 | enteros grandes y `agent_id` como string decimal canonico; `int128` como string | JSON de NEAR; canonico para que no haya alias |
| G-4 | `bytes` en base64, `bytes32` en hex de 64 | JSON |
| G-5 | tiempos en nanosegundos | `env::block_timestamp()` |
| G-6 | topes de largo y de listas (seccion 6.1) | storage pagado y el tope de 16.384 bytes de logs por recibo; los URIs con el charset de RFC 3986 (sin `"` ni `\`, que JSON escapa al doble) |
| G-7 | las vistas devuelven `null` en vez de revertir | NEAR permite `null`; menos casos de error para los SDK |
| G-8 | eventos NEP-297 | NEAR |
| G-9 | 1 yocto en lo que cambia dueno, wallet o aprobaciones | convencion de NEAR (NEP-171/178): fuerza una llave FullAccess |
| I-1 | `agentWallet` por confirmacion de la wallet (dos pasos) en vez de EIP-712/ERC-1271 | decision a |
| I-2 | `register` unico con opcionales | sin sobrecarga |
| I-3 | sin `setApprovalForAll` / `isApprovedForAll` | NEP-178 no lo tiene; "operador" = cuenta aprobada por NEP-178 para ese agente (el encargo: "NEP-178 aprobaciones como el operador de ERC-721"); menos superficie |
| I-4 | `nft_approve` solo lo hace el dueno | NEP-178 (en ERC-721 tambien un operador puede aprobar) |
| I-5 | un `nft_transfer_call` que el receptor devuelve deja `agentWallet` **limpia** (no se restaura) | en EVM el `safeTransferFrom` revertido no la toca; aca la transferencia ya ocurrio y el resolve la deshace: restaurarla agregaria estado en vuelo; queda del lado seguro y el dueno la vuelve a poner |
| I-6 | `register` no llama `nft_on_transfer` (EVM: `_safeMint` llama `onERC721Received`) | NEP-171 no lo pide en el mint |
| I-7 | `is_authorized_or_owner` entra en panico si no existe (fiel) y `check_agent` no (para las llamadas cruzadas) | 3.2 |
| S-1 | quien llama paga lo que crece; lo liberado vuelve al **dueno** del agente | storage staking; el dueno pago la ficha |
| S-2 | `register` cobra la reserva del token (2.4), que no vuelve | las transferencias no pueden cobrar (1 yocto) y el contrato no puede quedar sin saldo |
| R-1 | `give_feedback` es asincrono: devuelve `feedback_index` o `null` (rechazo con log) | llamada cruzada a Identity |
| R-2 | la guarda de identidad mira el estado cuando corre la vista | 3.2 |
| R-3 | `endpoint`, `feedback_uri`, `feedback_hash` solo en el evento | fiel (EVM no los guarda) |
| R-4 | `summary_value` exacto (256 bits), sin el truncado silencioso de `int128(...)` | 3.4 |
| R-5 | extension opcional `give_feedback_with_proof` (feature `feedback-proofs`) | decision c |
| R-6 | `get_clients` con paginacion opcional | las vistas de NEAR tienen tope de gas |
| V-1 | `validation_request` asincrono, con candado sobre `request_hash` | llamada cruzada + unicidad global |

### 6.1 Topes (propuestos; los fija la fase 2 con el peor caso medido de logs y de storage)

`agent_uri` 8.192 bytes (deja subir un registration file entero como `data:` base64, como sugiere la EIP); clave de
metadata 1..64 bytes `[A-Za-z0-9._:/-]`; valor de metadata 0..1.024 bytes; hasta 4 entradas en `register` (el peor caso
de logs de `register` tiene que entrar en 16.384 bytes); `tag1`/`tag2` 0..64 bytes de UTF-8 sin caracteres de control;
`endpoint` 1.024; `feedback_uri`, `response_uri`, `request_uri` 4.096 (no se guardan); `memo` 256; aprobaciones por
agente 8; `proof` 1..128 bytes ASCII imprimible; `role` y `context` 0..32 `[a-z0-9_-]`.

## 7. Cuentas (decision f) y como se despliega

| Cuenta | Estado (14:02:32Z) | Contrato | `new` |
|---|---|---|---|
| `erc-8004.near` | existe, de c0der (tx `84aGt5kuYiUmsfJSLnajZCq5SH475zrC4R13YsJ28Qki`), 0,0729 NEAR | ninguno (padre) | |
| `identity-registry.erc-8004.near` | no existe: el plan la crea desde el padre | `erc8004-identity` | `{}` |
| `reputation-registry.erc-8004.near` | no existe: el plan la crea desde el padre | `erc8004-reputation` | `{"identity_registry":"identity-registry.erc-8004.near"}` |
| `validation-registry.erc-8004.near` | **ya existe** (0,02 NEAR, la llave del padre) | `erc8004-validation` (BORRADOR: espera a ERC-8004) | `{"identity_registry":"identity-registry.erc-8004.near"}` |
| `erc8004-verifier.arras-escrow.near` (el dueno, RESPUESTA 2) | no existe: la crea c0der desde `arras-escrow.near` | `erc8004-arras-verifier` | `{"escrow":"v1.arras-escrow.near"}` |

Inmutables: cada una se despliega con `new` en la misma tx (`with-init-call`, `#[private]`), se verifica (code hash =
build reproducible, `get_config`, humo), y se le borran **todas** las llaves. Reputation y Validation aprenden cual es
el Identity Registry por el argumento de `new`; **no puede cambiar despues** porque es la raiz de confianza: si se
pudiera cambiar, quien lo cambie decide que agentes existen y quien es dueno, y con eso que feedback vale. Un arreglo
va en una cuenta nueva con version (`reputation-registry-v2.erc-8004.near`, `identity-registry-v2...`), y lo que ya
quedo en la vieja se queda ahi. En testnet, lo mismo bajo un padre de testnet (fase 4).

El `agentRegistry` del registration file: CAIP-2 no tiene namespace `near` registrado (0); `near:<chain_id>` con el
`chain_id` de nearcore: **`near:mainnet:identity-registry.erc-8004.near`** (y `near:testnet:<cuenta>` en testnet).
**Decidido por el dueno (RESPUESTA 2): asi, sin registrar el namespace en ChainAgnostic por ahora.**

## 8. Archivos fuera de cadena en NEAR (siguiendo best-practices 8a99b86)

Registration file (Rule 1..4 de `Registration.md`):

```json
{
  "type": "https://eips.ethereum.org/EIPS/eip-8004#registration-v1",
  "name": "Mi Agente",
  "description": "Que hace, como se usa, precios.",
  "image": "https://example.com/agente.png",
  "services": [
    {"name": "MCP", "endpoint": "https://agente.example/mcp", "version": "2025-06-18"},
    {"name": "A2A", "endpoint": "https://agente.example/.well-known/agent-card.json", "version": "0.3.0"},
    {"name": "agentWallet", "endpoint": "near:mainnet:agente.near"}
  ],
  "x402Support": true,
  "active": true,
  "registrations": [{"agentId": 0, "agentRegistry": "near:mainnet:identity-registry.erc-8004.near"}],
  "supportedTrust": ["reputation"]
}
```

`agentId` va como numero JSON (como en la EIP; un `u64` de NEAR hasta 2^53 entra). Dos diferencias con
best-practices, a favor de la EIP y del contrato: `Registration.md` sugiere pasar `agentWallet` en el `register(...)`
como metadata, pero el contrato oficial lo rechaza (`"reserved key"`) y este tambien; y la tabla de `Reputation.md`
pone `tradingYield` `-3.2%` como `32` con 1 decimal (pierde el signo), la EIP dice `-32`.

Feedback file (las MUST de la EIP; `clientAddress` en CAIP-10 con el mismo namespace):

```json
{
  "agentRegistry": "near:mainnet:identity-registry.erc-8004.near",
  "agentId": 0,
  "clientAddress": "near:mainnet:cliente.near",
  "createdAt": "2026-11-04T12:00:00Z",
  "value": 87,
  "valueDecimals": 0,
  "tag1": "starred",
  "endpoint": "https://agente.example/mcp",
  "proofOfPayment": {"fromAddress": "cliente.near", "toAddress": "agente.near", "chainId": "mainnet", "txHash": "<hash base58 de la tx>"}
}
```

`proofOfPayment` en el archivo es una afirmacion fuera de cadena (nadie la verifica); la prueba **en cadena** de un pago
de Arras es `give_feedback_with_proof` (3.3), y el archivo puede repetirla como
`"arras": {"escrow": "v1.arras-escrow.near", "paymentId": "..."}`. `feedbackHash` es KECCAK-256 del archivo, como en
la EIP (el contrato no lo verifica).

## 9. Fases que siguen

2. Crates: `contracts/erc8004-common` (sin estado: validaciones, enteros canonicos, la cuenta de 256 bits, eventos, el
   esquema de `check_agent`), `erc8004-identity`, `erc8004-reputation` (feature `feedback-proofs`),
   `erc8004-arras-verifier`, `erc8004-validation` (BORRADOR). near-sdk 5.29.1 como registry-v1, NEP-330 con
   `repository = "https://github.com/UltravioletaDAO/arras"` (el repo hoy es `0xultravioleta/arras` y va a pasar ahi).
3. Tests: unitarios (cada guarda, cada resultado de cada callback, el storage del peor caso, la invariante de la
   reserva) y sandbox (el ciclo entero, con un pago `Released` real de escrow-v1 compilado y de la wasm de mainnet que
   ya esta en `contracts/registry-v1/tests/fixtures`), gas y storage medidos, una mutacion por guarda.
4. Builds reproducibles, bytes de cada wasm, NEAR por cuenta y el plan de deploy exacto (testnet y mainnet).
5. Handoff `docs/handoffs/erc8004-near.md`.
