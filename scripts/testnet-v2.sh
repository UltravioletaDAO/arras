#!/usr/bin/env bash
# Arras v2 en testnet: el recorrido del README ("Arras v2 on testnet"), en orden (near-cli-rs 0.30.1, cargo-near 0.22.0).
# Patron: scripts/testnet.sh (v1). Solo testnet: este script aborta con cualquier cuenta que no sea .testnet.
#
# Uso: scripts/testnet-v2.sh <paso> | todo
#   1 contrato     cuenta nueva de nivel superior por el faucet de testnet ($ARRAS_V2_CONTRATO) y su saldo
#   2 roles        sub-cuentas del contrato, creadas antes de borrarle las llaves: capturer (0,5 NEAR), arbiter y agent
#                  (0,3); las que ya existen se saltan
#   3 usdc         storage_deposit en el USDC de testnet para el contrato (worker-demo, em-fees y payer-demo ya estan
#                  registradas desde v1; se saltan las registradas)
#   4 deploy       build reproducible (si falta la wasm) y deploy con init privado (`with-init-call new`); verifica que
#                  la rev NEP-330 sea HEAD (o $ARRAS_REV) y que el sha256 en la cadena sea el de la wasm
#   5 operadores   `em` (sin ticket) y `emt` (con `ticket_pubkey`: una llave ed25519 local, fuera del repo), los dos con
#                  arbitro y un plazo de 60 s (el minimo, para mostrar el vencimiento); NEP-145 del payer en v2
#   6 llaves       capturer: FunctionCall key (release, refund, withdraw_for); agent: FunctionCall key
#                  (authorize_from_budget). Despues borra la FullAccess de los dos
#   7 sin-llaves   borra TODAS las llaves del contrato y muestra list-keys vacio (el resto corre contra la cuenta sin
#                  llaves)
#   8 presupuesto  payer-demo deposita 2 USDC para el agente (topes: 1 USDC por pago, 2 por dia)
#   9 agente       el agente, con su FunctionCall key, autoriza 1 USDC a worker-demo; el capturer libera y retira por
#                  el worker y por em-fees
#  10 disputa      payer-demo autoriza 0,5 USDC por ft_transfer_call (msg v2 con dispute_deadline), abre la disputa y
#                  el arbitro reparte 0,2 al worker y 0,3 al payer; retiros
#  11 vencida      el agente autoriza 0,5 USDC; payer-demo abre la disputa; pasado el plazo del arbitro (60 s) cualquiera
#                  la cierra y lo vivo va al worker (la parte que no abrio); retiros
#  12 ticket       operador emt: sin ticket el contrato devuelve todo (E53); con el ticket firmado por la llave del paso
#                  5 pasa; el capturer reembolsa y retira por el payer
#  13 cierre       withdraw_budget de lo que queda, revoke_agent y withdraw del payer
#     verificar    hash de la wasm desplegada, llaves y get_config
#
# Variables: ARRAS_PARENT (arras-dev.testnet: owner, guardian, em-fees, payer-demo y worker-demo de v1);
# ARRAS_V2_CONTRATO (arras-v2-1103.testnet); ARRAS_V2_WASM (target/near/arras_escrow_v2/arras_escrow_v2.wasm);
# ARRAS_REV (la rev NEP-330 que tiene que traer la wasm; por defecto `git rev-parse HEAD`); ARRAS_KEYCHAIN (keychain |
# legacy-keychain); ARRAS_ESTADO (fuera del repo: msgs, ids y la llave del ticket, modo 600); ARRAS_RPC (lecturas de
# nonce y bloque). Las llaves de las cuentas quedan en el llavero de near-cli-rs y la del ticket en $ARRAS_ESTADO: este
# script nunca imprime una llave privada. Solo testnet: mainnet es H-11.
set -euo pipefail

NET=testnet
P=${ARRAS_PARENT:-arras-dev.testnet}
C=${ARRAS_V2_CONTRATO:-arras-v2-1103.testnet}
CAPT=capturer.$C
ARB=arbiter.$C
AGENT=agent.$C
PAYER=payer-demo.$P
WORKER=worker-demo.$P
FEES=em-fees.$P
USDC=3e2210e1184b45b64c8a434c0a7e7b23cc04ea7eb7a6c3c32520d03d4afcb8af
WASM=${ARRAS_V2_WASM:-target/near/arras_escrow_v2/arras_escrow_v2.wasm}
KC=${ARRAS_KEYCHAIN:-keychain}
ESTADO=${ARRAS_ESTADO:-$HOME/.arras-testnet}
RPC=${ARRAS_RPC:-https://rpc.testnet.fastnear.com}
SAVE="save-to-$KC"
SIGN="sign-with-$KC"
VENTANA_NS=60000000000

for cuenta in "$P" "$C"; do
  case "$cuenta" in
    *.testnet) ;;
    *) echo "$cuenta no es una cuenta de testnet: este script no toca mainnet (H-11)" >&2; exit 1 ;;
  esac
done
case "$KC" in
  keychain | legacy-keychain) ;;
  *) echo "ARRAS_KEYCHAIN tiene que ser keychain o legacy-keychain" >&2; exit 1 ;;
esac

run() {
  printf '\n+ %s\n' "$*"
  "$@"
}

existe() {
  near account view-account-summary "$1" network-config "$NET" now >/dev/null 2>&1
}

registrada_usdc() {
  near contract call-function as-read-only "$USDC" storage_balance_of json-args "{\"account_id\":\"$1\"}" network-config "$NET" now 2>/dev/null |
    sed 's/\x1b\[[0-9;]*m//g' | grep -q '"total"'
}

# Llaves publicas de una cuenta (columna "Public Key" de list-keys); con $2 = "full", solo las FullAccess.
llaves() {
  near account list-keys "$1" network-config "$NET" now 2>/dev/null |
    sed 's/\x1b\[[0-9;]*m//g' |
    awk -F'|' -v solo="${2:-}" '$3 ~ /ed25519:/ { gsub(/ /, "", $3); if (solo != "full" || $5 ~ /full access/) print $3 }'
}

rev_de_wasm() {
  LC_ALL=C grep -ao 'rev=[0-9a-f]\{40\}' "$1" | head -1 | cut -d= -f2
}

hash_onchain() {
  near account view-account-summary "$1" network-config "$NET" now 2>/dev/null |
    sed 's/\x1b\[[0-9;]*m//g' | awk '/SHA-256 checksum hex/ { print $NF }'
}

rpc() {
  curl -sf -m 30 -X POST "$RPC" -H 'Content-Type: application/json' \
    -d "$(jq -nc --arg m "$1" --argjson p "$2" '{jsonrpc:"2.0",id:"arras",method:$m,params:$p}')"
}

# Una vista del contrato por RPC (args JSON) -> el resultado ya decodificado (JSON).
vista() {
  rpc query "$(jq -nc --arg c "$C" --arg m "$1" --arg a "$(printf '%s' "$2" | base64 | tr -d '\n')" \
    '{request_type:"call_function",finality:"optimistic",account_id:$c,method_name:$m,args_base64:$a}')" |
    jq -r '.result.result | implode'
}

cola_near() {
  printf '%s\n' "$1" | sed 's/\x1b\[[0-9;]*m//g' | grep -viE 'secret|private|ed25519:[1-9A-HJ-NP-Za-km-z]{60,}' | tail -n 8 >&2 || true
}

# Ultimo nonce mandado por cada cuenta con FunctionCall key (el RPC de lectura puede venir atrasado). Sin arreglos
# asociativos: corre tambien con el bash 3.2 de macOS.
NONCE_CAPT=0
NONCE_AGENT=0

ultimo_nonce() {
  if [ "$1" = "$CAPT" ]; then echo "$NONCE_CAPT"; else echo "$NONCE_AGENT"; fi
}

# Una llamada firmada con la FunctionCall key de $1 para $C. near-cli-rs 0.30.1 en linea solo firma con FullAccess
# (ver scripts/testnet.sh): se firma --offline con nonce y bloque leidos de la cadena y se manda la tx firmada.
fc() {
  local cuenta=$1 metodo=$2 args=$3 gas=$4 lista pk vista_k nonce hash altura salida firmada
  lista=$(rpc query "$(jq -nc --arg a "$cuenta" '{request_type:"view_access_key_list",finality:"optimistic",account_id:$a}')") ||
    { echo "$RPC no respondio a view_access_key_list de $cuenta" >&2; return 1; }
  pk=$(jq -r --arg c "$C" '[.result.keys[] | select((.access_key.permission | objects | .FunctionCall.receiver_id) == $c)][0].public_key // empty' <<<"$lista")
  [ -n "$pk" ] || { echo "$cuenta no tiene una FunctionCall key para $C (paso 6)" >&2; return 1; }
  vista_k=$(rpc query "$(jq -nc --arg a "$cuenta" --arg k "$pk" '{request_type:"view_access_key",finality:"optimistic",account_id:$a,public_key:$k}')") ||
    { echo "$RPC no respondio a view_access_key de $cuenta" >&2; return 1; }
  if ! { nonce=$(jq -er '.result.nonce | numbers' <<<"$vista_k" 2>/dev/null) &&
    hash=$(jq -er '.result.block_hash | strings' <<<"$vista_k" 2>/dev/null) &&
    altura=$(jq -er '.result.block_height | numbers' <<<"$vista_k" 2>/dev/null); }; then
    echo "view_access_key de $cuenta ($pk) en $RPC no trajo nonce y bloque: $(head -c 300 <<<"$vista_k")" >&2
    return 1
  fi
  nonce=$(( nonce + 1 ))
  if [ "$nonce" -le "$(ultimo_nonce "$cuenta")" ]; then
    nonce=$(( $(ultimo_nonce "$cuenta") + 1 ))
  fi
  printf '\n+ %s %s %s (%s con su FunctionCall key %s, firma --offline, nonce %s)\n' "$metodo" "$args" "$gas" "$cuenta" "$pk" "$nonce"
  salida=$(near --offline contract call-function as-transaction "$C" "$metodo" json-args "$args" prepaid-gas "$gas" attached-deposit '0 NEAR' sign-as "$cuenta" network-config "$NET" "$SIGN" --signer-public-key "$pk" --nonce "$nonce" --block-hash "$hash" --block-height "$altura" display 2>&1) ||
    { echo "near --offline no pudo firmar $metodo con $pk; sus ultimas lineas:" >&2; cola_near "$salida"; return 1; }
  firmada=$(printf '%s\n' "$salida" | sed 's/\x1b\[[0-9;]*m//g' | awk 'f { print; f = 0 } /serialized as base64/ { f = 1 }' | tr -d ' ')
  [ -n "$firmada" ] || { echo "near --offline no devolvio la tx firmada de $metodo" >&2; cola_near "$salida"; return 1; }
  run near transaction send-signed-transaction base64-signed-transaction "$firmada" network-config "$NET" send
  if [ "$cuenta" = "$CAPT" ]; then NONCE_CAPT=$nonce; else NONCE_AGENT=$nonce; fi
}

# Una llamada con la FullAccess de $1 (del llavero).
llamar() {
  local cuenta=$1 metodo=$2 args=$3 gas=$4 dep=${5:-0 NEAR}
  run near contract call-function as-transaction "$C" "$metodo" json-args "$args" prepaid-gas "$gas" attached-deposit "$dep" sign-as "$cuenta" network-config "$NET" "$SIGN" send
}

# ft_transfer_call de payer-demo a $C.
ft_call() {
  local amount=$1 msg=$2
  run near contract call-function as-transaction "$USDC" ft_transfer_call json-args "$(jq -nc --arg r "$C" --arg a "$amount" --arg m "$msg" '{receiver_id:$r,amount:$a,msg:$m}')" prepaid-gas '100.0 Tgas' attached-deposit '1 yoctoNEAR' sign-as "$PAYER" network-config "$NET" "$SIGN" send
}

saldos() {
  local a
  for a in "$PAYER" "$WORKER" "$FEES" "$C"; do
    printf 'USDC %s: %s\n' "$a" "$(near contract call-function as-read-only "$USDC" ft_balance_of json-args "{\"account_id\":\"$a\"}" network-config "$NET" now 2>/dev/null | sed 's/\x1b\[[0-9;]*m//g' | grep -oE '"[0-9]+"' | tail -1)"
  done
  printf 'get_config: %s\n' "$(vista get_config '{}' | jq -c '{tvl_global,in_flight,budgets_total,payments}')"
}

ahora_ns() {
  echo $(( $(date +%s) * 1000000000 ))
}

nuevo_salt() {
  openssl rand -hex 16
}

# El msg authorize de v2 a worker-demo: release en 1 dia, reclaim en 2, dispute_deadline en 1 dia; $2 = operador;
# $3 = ticket JSON opcional.
msg_authorize() {
  local salt=$1 op=$2 ticket=${3:-} now
  now=$(ahora_ns)
  jq -nc --arg op "$op" --arg r "$WORKER" --arg rel "$(( now + 86400000000000 ))" --arg rec "$(( now + 172800000000000 ))" \
    --arg dd "$(( now + 86400000000000 ))" --arg s "$salt" --argjson t "${ticket:-null}" \
    '{v:1,action:"authorize",operator_id:$op,receiver:$r,release_deadline:$rel,reclaim_after:$rec,fee_bps_max:1300,terms_hash:("0"*64),salt:$s,dispute_deadline:$dd} + (if $t then {operator_ticket:$t} else {} end)'
}

# Los args de authorize_from_budget de un pago a worker-demo con el operador em.
afb_args() {
  local amount=$1 salt=$2 now
  now=$(ahora_ns)
  jq -nc --arg o "$PAYER" --arg r "$WORKER" --arg a "$amount" --arg rel "$(( now + 86400000000000 ))" --arg rec "$(( now + 172800000000000 ))" --arg s "$salt" \
    '{owner:$o,operator_id:"em",receiver:$r,amount:$a,release_deadline:$rel,reclaim_after:$rec,fee_bps_max:1300,terms_hash:("0"*64),salt:$s}'
}

# El payment_id de esos args (la vista del contrato).
id_de_afb() {
  vista compute_budget_payment_id "$(jq -c 'del(.dispute_deadline, .operator_ticket)' <<<"$1")" | tr -d '"'
}

id_de_msg() {
  vista compute_payment_id "$(jq -nc --arg s "$PAYER" --arg a "$1" --arg m "$2" '{sender_id:$s,amount:$a,msg:$m}')" | tr -d '"'
}

guardar() {
  mkdir -p "$ESTADO"
  printf '%s' "$2" > "$ESTADO/v2-$C-$1"
}

leer() {
  cat "$ESTADO/v2-$C-$1"
}

b58() {
  python3 -c 'import sys
A="123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"
b=sys.stdin.buffer.read(); n=int.from_bytes(b,"big"); s=""
while n: n,r=divmod(n,58); s=A[r]+s
print("1"*(len(b)-len(b.lstrip(b"\0")))+s)'
}

LLAVE_TICKET="$ESTADO/v2-$C-ticket.pem"

ticket_pubkey() {
  printf 'ed25519:%s' "$(openssl pkey -in "$LLAVE_TICKET" -pubout -outform DER | tail -c 32 | b58)"
}

# Ticket {expiry, sig} del pago $1 con la llave local: firma los bytes que da la vista `ticket_message_payment`.
ticket_pago() {
  local id=$1 expiry=$2 bytes
  bytes=$(mktemp "${TMPDIR:-/tmp}/arras-ticket.XXXXXX")
  vista ticket_message_payment "$(jq -nc --arg p "$id" --arg e "$expiry" '{payment_id:$p,expiry:$e}')" | tr -d '"' | base64 -d > "$bytes"
  jq -nc --arg e "$expiry" --arg s "$(openssl pkeyutl -sign -rawin -inkey "$LLAVE_TICKET" -in "$bytes" | b58)" '{expiry:$e,sig:$s}'
  rm -f "$bytes"
}

crear_sub() {
  if existe "$1"; then
    echo "ya existe: $1"
  else
    run near account create-account fund-myself "$1" "$2" autogenerate-new-keypair "$SAVE" sign-as "$C" network-config "$NET" "$SIGN" send
  fi
}

paso1() {
  if existe "$C"; then
    echo "ya existe: $C"
  else
    run near account create-account sponsor-by-faucet-service "$C" autogenerate-new-keypair "$SAVE" network-config "$NET" create
  fi
  run near account view-account-summary "$C" network-config "$NET" now
}

paso2() {
  crear_sub "$CAPT" '0.5 NEAR'
  crear_sub "$ARB" '0.3 NEAR'
  crear_sub "$AGENT" '0.3 NEAR'
}

paso3() {
  local a
  for a in "$C" "$WORKER" "$FEES" "$PAYER"; do
    if registrada_usdc "$a"; then
      echo "ya registrada en USDC: $a"
    else
      run near contract call-function as-transaction "$USDC" storage_deposit json-args "{\"account_id\":\"$a\",\"registration_only\":true}" prepaid-gas '30.0 Tgas' attached-deposit '0.00125 NEAR' sign-as "$C" network-config "$NET" "$SIGN" send
    fi
  done
}

paso4() {
  if [ ! -f "$WASM" ]; then
    run env -u CARGO_TARGET_DIR cargo near build reproducible-wasm --manifest-path contracts/escrow-v2/Cargo.toml
  fi
  local esperada rev local_hash onchain _i
  esperada=${ARRAS_REV:-$(git rev-parse HEAD)}
  rev=$(rev_de_wasm "$WASM")
  if [ "$rev" != "$esperada" ]; then
    echo "ABORTO: $WASM es de la rev ${rev:-desconocida}, no de $esperada. Borrala y corre el paso 4 o fija ARRAS_REV." >&2
    return 1
  fi
  local_hash=$(shasum -a 256 "$WASM" | awk '{ print $1 }')
  echo "wasm rev $rev, sha256 $local_hash"
  # Los topes de v1 (decision del dueno, 2026-10-26): 10 USDC por pago, 1.000 de TVL global.
  run near contract deploy "$C" use-file "$WASM" with-init-call new json-args "{\"owner\":\"owner.$P\",\"guardian\":\"guardian.$P\",\"token\":\"$USDC\",\"tope_por_pago\":\"10000000\",\"tope_tvl_global\":\"1000000000\"}" prepaid-gas '30.0 Tgas' attached-deposit '0 NEAR' network-config "$NET" "$SIGN" send
  for _i in 1 2 3 4 5 6; do
    onchain=$(hash_onchain "$C")
    [ -n "$onchain" ] && break
    sleep 5
  done
  if [ "$onchain" != "$local_hash" ]; then
    echo "ABORTO: el codigo de $C tiene sha256 ${onchain:-?} y la wasm local $local_hash" >&2
    return 1
  fi
  echo "OK: el codigo de $C es la wasm $local_hash (rev $rev)"
  vista get_config '{}' | jq .
}

paso5() {
  mkdir -p "$ESTADO"
  chmod 700 "$ESTADO"
  if [ ! -f "$LLAVE_TICKET" ]; then
    (umask 077 && openssl genpkey -algorithm ed25519 -out "$LLAVE_TICKET")
  fi
  local op ticket input
  for op in em emt; do
    if [ "$(vista get_operator "{\"operator_id\":\"$op\"}")" != "null" ]; then
      echo "ya registrado: $op"
      continue
    fi
    ticket=null
    [ "$op" = emt ] && ticket="\"$(ticket_pubkey)\""
    input=$(jq -nc --arg o "owner.$P" --arg c "$CAPT" --arg f "$FEES" --arg a "$ARB" --arg w "$VENTANA_NS" --argjson t "$ticket" \
      '{owner:$o,capturers:[$c],fee_bps:1300,fee_recipient:$f,min_amount:"10000",tope_tvl_operador:"200000000",tope_tvl_por_payer:"20000000",sponsor_max_por_payer:"20000000000000000000000",arbiter:$a,arbiter_window_ns:$w} + (if $t then {ticket_pubkey:$t} else {} end)')
    llamar "owner.$P" register_operator "$(jq -nc --arg op "$op" --argjson i "$input" '{operator_id:$op,input:$i}')" '30.0 Tgas' '1 yoctoNEAR'
  done
  if [ "$(vista storage_balance_of "{\"account_id\":\"$PAYER\"}")" = "null" ]; then
    llamar "$PAYER" storage_deposit "{\"account_id\":\"$PAYER\"}" '30.0 Tgas' '0.05 NEAR'
  fi
  vista get_operator '{"operator_id":"emt"}' | jq -c '{arbiter,arbiter_window_ns,ticket_pubkey}'
}

paso6() {
  local cuenta metodos allowance full
  for cuenta in "$CAPT" "$AGENT"; do
    full=$(llaves "$cuenta" full)
    if [ -z "$full" ]; then
      echo "$cuenta ya no tiene FullAccess"
      continue
    fi
    if [ "$cuenta" = "$CAPT" ]; then
      metodos='release, refund, withdraw_for'
      allowance='0.4 NEAR'
    else
      metodos='authorize_from_budget'
      allowance='0.25 NEAR'
    fi
    run near account add-key "$cuenta" grant-function-call-access --allowance "$allowance" --contract-account-id "$C" --function-names "$metodos" autogenerate-new-keypair "$SAVE" network-config "$NET" "$SIGN" send
    for k in $full; do
      run near account delete-keys "$cuenta" public-keys "$k" network-config "$NET" "$SIGN" send
    done
    run near account list-keys "$cuenta" network-config "$NET" now
  done
}

paso7() {
  local todas
  todas=$(llaves "$C")
  run near account list-keys "$C" network-config "$NET" now
  for k in $todas; do
    run near account delete-keys "$C" public-keys "$k" network-config "$NET" "$SIGN" send
  done
  run near account list-keys "$C" network-config "$NET" now
  [ -z "$(llaves "$C")" ] && echo "OK: $C sin llaves"
}

paso8() {
  saldos
  ft_call 2000000 "$(jq -nc --arg a "$AGENT" '{v:1,action:"budget",operator_id:"em",agent:$a,tope_por_pago:"1000000",tope_diario:"2000000"}')"
  vista get_budget "$(jq -nc --arg o "$PAYER" --arg a "$AGENT" '{owner:$o,agent:$a,operator_id:"em"}')" | jq -c .
  saldos
}

paso9() {
  local args id
  args=$(afb_args 1000000 "$(nuevo_salt)")
  id=$(id_de_afb "$args")
  guardar afb1 "$id"
  echo "payment_id: $id"
  fc "$AGENT" authorize_from_budget "$args" '30.0 Tgas'
  vista get_payment "{\"payment_id\":\"$id\"}" | jq -c '{payer,receiver,amount,status}'
  fc "$CAPT" release "{\"payment_id\":\"$id\"}" '30.0 Tgas'
  fc "$CAPT" withdraw_for "{\"account\":\"$WORKER\"}" '50.0 Tgas'
  fc "$CAPT" withdraw_for "{\"account\":\"$FEES\"}" '50.0 Tgas'
  saldos
}

paso10() {
  local msg id
  msg=$(msg_authorize "$(nuevo_salt)" em)
  id=$(id_de_msg 500000 "$msg")
  guardar disputa "$id"
  echo "payment_id: $id"
  ft_call 500000 "$msg"
  llamar "$PAYER" open_dispute "{\"payment_id\":\"$id\",\"reason_hash\":\"$(printf 'trabajo incompleto' | shasum -a 256 | awk '{ print $1 }')\"}" '30.0 Tgas'
  llamar "$ARB" resolve_dispute "{\"payment_id\":\"$id\",\"to_receiver\":\"200000\",\"to_payer\":\"300000\"}" '30.0 Tgas'
  vista get_payment "{\"payment_id\":\"$id\"}" | jq -c '{status,released,refunded,dispute}'
  fc "$CAPT" withdraw_for "{\"account\":\"$WORKER\"}" '50.0 Tgas'
  fc "$CAPT" withdraw_for "{\"account\":\"$FEES\"}" '50.0 Tgas'
  fc "$CAPT" withdraw_for "{\"account\":\"$PAYER\"}" '50.0 Tgas'
  saldos
}

paso11() {
  local args id resolve_by
  args=$(afb_args 500000 "$(nuevo_salt)")
  id=$(id_de_afb "$args")
  guardar vencida "$id"
  echo "payment_id: $id"
  fc "$AGENT" authorize_from_budget "$args" '30.0 Tgas'
  llamar "$PAYER" open_dispute "{\"payment_id\":\"$id\",\"reason_hash\":\"$(printf 'no hubo trabajo' | shasum -a 256 | awk '{ print $1 }')\"}" '30.0 Tgas'
  resolve_by=$(( $(vista get_payment "{\"payment_id\":\"$id\"}" | jq -r '.dispute.opened_at') + VENTANA_NS ))
  echo "el arbitro tiene hasta $resolve_by ns; esperando"
  while [ "$(ahora_ns)" -le $(( resolve_by + 5000000000 )) ]; do
    sleep 5
  done
  llamar "$PAYER" resolve_expired_dispute "{\"payment_id\":\"$id\"}" '30.0 Tgas'
  vista get_payment "{\"payment_id\":\"$id\"}" | jq -c '{status,released,refunded,dispute}'
  fc "$CAPT" withdraw_for "{\"account\":\"$WORKER\"}" '50.0 Tgas'
  fc "$CAPT" withdraw_for "{\"account\":\"$FEES\"}" '50.0 Tgas'
  saldos
}

paso12() {
  local salt sin id ticket con
  salt=$(nuevo_salt)
  sin=$(msg_authorize "$salt" emt)
  id=$(id_de_msg 100000 "$sin")
  echo "payment_id: $id"
  echo "sin ticket: el contrato devuelve todo (arras:rechazo:E53_TICKET_INVALIDO)"
  ft_call 100000 "$sin"
  ticket=$(ticket_pago "$id" "$(( $(ahora_ns) + 3600000000000 ))")
  con=$(jq -c --argjson t "$ticket" '. + {operator_ticket:$t}' <<<"$sin")
  [ "$(id_de_msg 100000 "$con")" = "$id" ] || { echo "el ticket cambio el payment_id" >&2; return 1; }
  guardar ticket "$id"
  ft_call 100000 "$con"
  fc "$CAPT" refund "{\"payment_id\":\"$id\"}" '30.0 Tgas'
  fc "$CAPT" withdraw_for "{\"account\":\"$PAYER\"}" '50.0 Tgas'
  saldos
}

paso13() {
  local saldo
  saldo=$(vista get_budget "$(jq -nc --arg o "$PAYER" --arg a "$AGENT" '{owner:$o,agent:$a,operator_id:"em"}')" | jq -r '.saldo // "0"')
  if [ "$saldo" != "0" ]; then
    llamar "$PAYER" withdraw_budget "{\"agent\":\"$AGENT\",\"operator_id\":\"em\"}" '30.0 Tgas'
  fi
  llamar "$PAYER" revoke_agent "{\"agent\":\"$AGENT\",\"operator_id\":\"em\"}" '30.0 Tgas'
  if [ "$(vista get_claimable "{\"account\":\"$PAYER\"}" | tr -d '"')" != "0" ]; then
    llamar "$PAYER" withdraw '{}' '50.0 Tgas'
  fi
  saldos
}

verificar() {
  if [ -f "$WASM" ]; then
    echo "wasm local: rev $(rev_de_wasm "$WASM"), sha256 $(shasum -a 256 "$WASM" | awk '{ print $1 }')"
  fi
  echo "codigo en $C: sha256 $(hash_onchain "$C")"
  run near account list-keys "$C" network-config "$NET" now
  vista get_config '{}' | jq .
}

case "${1:-}" in
  1 | contrato) paso1 ;;
  2 | roles) paso2 ;;
  3 | usdc) paso3 ;;
  4 | deploy) paso4 ;;
  5 | operadores) paso5 ;;
  6 | llaves) paso6 ;;
  7 | sin-llaves) paso7 ;;
  8 | presupuesto) paso8 ;;
  9 | agente) paso9 ;;
  10 | disputa) paso10 ;;
  11 | vencida) paso11 ;;
  12 | ticket) paso12 ;;
  13 | cierre) paso13 ;;
  verificar) verificar ;;
  todo) paso1; paso2; paso3; paso4; paso5; paso6; paso7; paso8; paso9; paso10; paso11; paso12; paso13 ;;
  *) sed -n '2,40p' "$0"; exit 1 ;;
esac
