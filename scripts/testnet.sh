#!/usr/bin/env bash
# Arras v1 en testnet: los pasos de la sec. 8 de la prescripcion, en orden (near-cli-rs 0.30.1, cargo-near 0.22.0).
#
# Uso: scripts/testnet.sh <paso> | todo
#   1 padre        cuenta padre por el faucet de testnet y su saldo
#   2 hijos        el contrato (si es v1.$P; 5 NEAR), owner y el capturer (0,5), guardian, em-fees, payer-demo y
#                  worker-demo (0,2); las que ya existen se saltan
#   3 usdc         storage_deposit en el USDC de testnet para el contrato, worker-demo, em-fees y payer-demo
#                  (las ya registradas se saltan)
#   4 faucet-usdc  a mano: USDC de https://faucet.circle.com (NEAR Testnet) a payer-demo.$P
#   5 deploy       build reproducible (si falta la wasm) y deploy con init privado (`with-init-call new`)
#   6 operador     operador em (sin listas), patrocinio de storage y NEP-145 del payer
#   7 capturer     FunctionCall key del capturer acotada al contrato y borrado de su FullAccess
#   8 authorize    ft_transfer_call de 1 USDC con salt de un CSPRNG (necesita el paso 4)
#   9 release      release del ultimo pago del paso 8, withdraw_for del worker y del fee_recipient, saldo del worker
#  10 sin-llaves   borra TODAS las llaves del contrato y muestra list-keys vacio
#  11 refund       refund del ultimo pago del paso 8, withdraw_for del payer y su saldo (camino authorize -> refund:
#                  corre el paso 8 otra vez antes)
#     verificar    hash de la wasm desplegada, llaves y get_config del contrato
#
# Variables: ARRAS_PARENT (arras-dev.testnet); ARRAS_CONTRATO (v1.$ARRAS_PARENT; el deploy final usa una cuenta de
# nivel superior del faucet, p. ej. arras-v1b-1101.testnet); ARRAS_CAPTURER (em-capturer.$ARRAS_PARENT; una
# FunctionCall key sirve para un solo contrato, asi que cada contrato lleva su capturer); ARRAS_WASM
# (target/near/arras_escrow_v1/arras_escrow_v1.wasm); ARRAS_REV (la rev NEP-330 que tiene que traer la wasm del paso 5;
# por defecto `git rev-parse HEAD`); ARRAS_KEYCHAIN (keychain | legacy-keychain: el llavero de
# macOS o ~/.near-credentials); ARRAS_ESTADO (donde se guarda el msg del paso 8, fuera del repo); ARRAS_RPC (el RPC de
# testnet para el nonce y el bloque de las firmas del capturer). Las llaves quedan en el llavero de near-cli-rs: este
# script nunca imprime una llave privada. Solo testnet: mainnet es H-3.
set -euo pipefail

NET=testnet
P=${ARRAS_PARENT:-arras-dev.testnet}
C=${ARRAS_CONTRATO:-v1.$P}
CAPT=${ARRAS_CAPTURER:-em-capturer.$P}
USDC=3e2210e1184b45b64c8a434c0a7e7b23cc04ea7eb7a6c3c32520d03d4afcb8af
WASM=${ARRAS_WASM:-target/near/arras_escrow_v1/arras_escrow_v1.wasm}
KC=${ARRAS_KEYCHAIN:-keychain}
ESTADO=${ARRAS_ESTADO:-$HOME/.arras-testnet}
RPC=${ARRAS_RPC:-https://rpc.testnet.fastnear.com}
SAVE="save-to-$KC"
SIGN="sign-with-$KC"

for cuenta in "$P" "$C" "$CAPT"; do
  case "$cuenta" in
    *.testnet) ;;
    *) echo "$cuenta no es una cuenta de testnet: este script no toca mainnet (H-3)" >&2; exit 1 ;;
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

# La rev NEP-330 embebida en la wasm (`source_code_snapshot ...?rev=<sha>`).
rev_de_wasm() {
  LC_ALL=C grep -ao 'rev=[0-9a-f]\{40\}' "$1" | head -1 | cut -d= -f2
}

# El sha256 del codigo desplegado en una cuenta (view-account-summary, en hex).
hash_onchain() {
  near account view-account-summary "$1" network-config "$NET" now 2>/dev/null |
    sed 's/\x1b\[[0-9;]*m//g' | awk '/SHA-256 checksum hex/ { print $NF }'
}

rpc_query() {
  curl -sf -m 30 -X POST "$RPC" -H 'Content-Type: application/json' \
    -d "$(jq -nc --argjson p "$1" '{jsonrpc:"2.0",id:"arras",method:"query",params:$p}')"
}

# Una llamada del capturer con su FunctionCall key. near-cli-rs 0.30.1 en linea solo firma con llaves FullAccess del
# llavero (sign_with_keychain/mod.rs y sign_with_legacy_keychain/mod.rs filtran AccessKeyPermissionView::FullAccess), y
# el capturer no tiene ninguna desde el paso 7. Con --offline firma con la llave que se le nombra: nonce y bloque se leen
# de la cadena, la tx firmada se manda con send-signed-transaction y la llave no sale del llavero.
capturer() {
  local metodo=$1 args=$2 gas=$3 pk vista nonce hash altura firmada
  pk=$(rpc_query "$(jq -nc --arg a "$CAPT" '{request_type:"view_access_key_list",finality:"optimistic",account_id:$a}')" |
    jq -r --arg c "$C" '[.result.keys[] | select((.access_key.permission | objects | .FunctionCall.receiver_id) == $c)][0].public_key // empty')
  [ -n "$pk" ] || { echo "$CAPT no tiene una FunctionCall key para $C (paso 7)" >&2; return 1; }
  vista=$(rpc_query "$(jq -nc --arg a "$CAPT" --arg k "$pk" '{request_type:"view_access_key",finality:"optimistic",account_id:$a,public_key:$k}')")
  nonce=$(( $(jq -r .result.nonce <<<"$vista") + 1 ))
  hash=$(jq -r .result.block_hash <<<"$vista")
  altura=$(jq -r .result.block_height <<<"$vista")
  printf '\n+ %s %s %s (capturer %s, firma --offline, nonce %s)\n' "$metodo" "$args" "$gas" "$pk" "$nonce"
  firmada=$(near --offline contract call-function as-transaction "$C" "$metodo" json-args "$args" prepaid-gas "$gas" attached-deposit '0 NEAR' sign-as "$CAPT" network-config "$NET" "$SIGN" --signer-public-key "$pk" --nonce "$nonce" --block-hash "$hash" --block-height "$altura" display 2>&1 |
    sed 's/\x1b\[[0-9;]*m//g' | awk 'f { print; f = 0 } /serialized as base64/ { f = 1 }' | tr -d ' ')
  [ -n "$firmada" ] || { echo "no se pudo firmar $metodo con $pk del llavero" >&2; return 1; }
  run near transaction send-signed-transaction base64-signed-transaction "$firmada" network-config "$NET" send
}

# El payment_id del ultimo authorize del paso 8 (un payment_id tiene de 32 a 44 caracteres: base58 de 32 bytes).
pago_actual() {
  local msg
  msg=$(cat "$ESTADO/msg-$C.json")
  near contract call-function as-read-only "$C" compute_payment_id json-args "$(jq -nc --arg s "payer-demo.$P" --arg m "$msg" '{sender_id:$s,amount:"1000000",msg:$m}')" network-config "$NET" now 2>/dev/null | sed 's/\x1b\[[0-9;]*m//g' | grep -oE '"[1-9A-HJ-NP-Za-km-z]{32,44}"' | tail -1 | tr -d '"'
}

crear() {
  if existe "$1"; then
    echo "ya existe: $1"
  else
    run near account create-account fund-myself "$1" "$2" autogenerate-new-keypair "$SAVE" sign-as "$P" network-config "$NET" "$SIGN" send
  fi
}

paso1() {
  run near account create-account sponsor-by-faucet-service "$P" autogenerate-new-keypair "$SAVE" network-config "$NET" create
  run near account view-account-summary "$P" network-config "$NET" now
}

paso2() {
  if [ "$C" = "v1.$P" ]; then
    crear "$C" '5 NEAR'
  else
    existe "$C" || { echo "$C no existe: creala antes (p. ej. por el faucet, paso 1 con ARRAS_PARENT=$C)" >&2; return 1; }
  fi
  crear "owner.$P" '0.5 NEAR'
  crear "$CAPT" '0.5 NEAR'
  for c in guardian em-fees payer-demo worker-demo; do
    crear "$c.$P" '0.2 NEAR'
  done
}

paso3() {
  for a in "$C" "worker-demo.$P" "em-fees.$P" "payer-demo.$P"; do
    if registrada_usdc "$a"; then
      echo "ya registrada en USDC: $a"
    else
      run near contract call-function as-transaction "$USDC" storage_deposit json-args "{\"account_id\":\"$a\",\"registration_only\":true}" prepaid-gas '30.0 Tgas' attached-deposit '0.00125 NEAR' sign-as "$P" network-config "$NET" "$SIGN" send
    fi
  done
}

paso4() {
  echo "A mano: https://faucet.circle.com -> NEAR Testnet -> payer-demo.$P (1 USDC alcanza para el paso 8)."
  run near contract call-function as-read-only "$USDC" ft_balance_of json-args "{\"account_id\":\"payer-demo.$P\"}" network-config "$NET" now
}

paso5() {
  if [ ! -f "$WASM" ]; then
    # Sin CARGO_TARGET_DIR: con el seteado, cargo-near 0.22.0 resuelve el artefacto del contenedor y el destino al mismo
    # archivo, lo borra como "previous artifact" y falla al copiarlo (os error 2). Sale en target/near/ del repo.
    run env -u CARGO_TARGET_DIR cargo near build reproducible-wasm --manifest-path contracts/escrow-v1/Cargo.toml
  fi
  local esperada rev local_hash onchain _intento
  esperada=${ARRAS_REV:-$(git rev-parse HEAD)}
  rev=$(rev_de_wasm "$WASM")
  if [ "$rev" != "$esperada" ]; then
    echo "ABORTO: $WASM es de la rev ${rev:-desconocida}, no de $esperada. Borrala y corre el paso 5 (reconstruye) o fija ARRAS_REV." >&2
    return 1
  fi
  local_hash=$(shasum -a 256 "$WASM" | awk '{ print $1 }')
  echo "wasm rev $rev, sha256 $local_hash"
  # Tope global de v1: 1.000 USDC (decision del dueno del 2026-10-26, Q3); por pago 10 USDC.
  run near contract deploy "$C" use-file "$WASM" with-init-call new json-args "{\"owner\":\"owner.$P\",\"guardian\":\"guardian.$P\",\"token\":\"$USDC\",\"tope_por_pago\":\"10000000\",\"tope_tvl_global\":\"1000000000\"}" prepaid-gas '30.0 Tgas' attached-deposit '0 NEAR' network-config "$NET" "$SIGN" send
  for _intento in 1 2 3 4 5 6; do
    onchain=$(hash_onchain "$C")
    [ -n "$onchain" ] && break
    sleep 5
  done
  if [ "$onchain" != "$local_hash" ]; then
    echo "ABORTO: el codigo de $C tiene sha256 ${onchain:-?} y la wasm local $local_hash" >&2
    return 1
  fi
  echo "OK: el codigo de $C es la wasm $local_hash (rev $rev)"
  run near contract call-function as-read-only "$C" get_config json-args '{}' network-config "$NET" now
}

paso6() {
  run near contract call-function as-transaction "$C" register_operator json-args "{\"operator_id\":\"em\",\"input\":{\"owner\":\"owner.$P\",\"capturers\":[\"$CAPT\"],\"fee_bps\":1300,\"fee_recipient\":\"em-fees.$P\",\"min_amount\":\"10000\",\"tope_tvl_operador\":\"200000000\",\"tope_tvl_por_payer\":\"20000000\",\"sponsor_max_por_payer\":\"20000000000000000000000\"}}" prepaid-gas '30.0 Tgas' attached-deposit '1 yoctoNEAR' sign-as "owner.$P" network-config "$NET" "$SIGN" send
  # fund_operator_storage lo puede firmar cualquiera: lo firma el padre, porque owner tiene 0,5 NEAR y no le alcanzan
  # para adjuntar 0,5 NEAR y pagar el gas (la sec. 8 lo firmaba owner).
  run near contract call-function as-transaction "$C" fund_operator_storage json-args '{"operator_id":"em"}' prepaid-gas '30.0 Tgas' attached-deposit '0.5 NEAR' sign-as "$P" network-config "$NET" "$SIGN" send
  run near contract call-function as-transaction "$C" storage_deposit json-args "{\"account_id\":\"payer-demo.$P\"}" prepaid-gas '30.0 Tgas' attached-deposit '0.05 NEAR' sign-as "payer-demo.$P" network-config "$NET" "$SIGN" send
  run near contract call-function as-read-only "$C" get_operator json-args '{"operator_id":"em"}' network-config "$NET" now
}

paso7() {
  local full
  full=$(llaves "$CAPT" full)
  [ -n "$full" ] || { echo "$CAPT no tiene FullAccess (ya se borro?)" >&2; return 1; }
  run near account add-key "$CAPT" grant-function-call-access --allowance '0.5 NEAR' --contract-account-id "$C" --function-names 'release, refund, withdraw_for' autogenerate-new-keypair "$SAVE" network-config "$NET" "$SIGN" send
  run near account list-keys "$CAPT" network-config "$NET" now
  for k in $full; do
    run near account delete-keys "$CAPT" public-keys "$k" network-config "$NET" "$SIGN" send
  done
  run near account list-keys "$CAPT" network-config "$NET" now
}

paso8() {
  mkdir -p "$ESTADO"
  local now salt msg
  now=$(date +%s)
  salt=$(openssl rand -hex 16)
  msg="{\"v\":1,\"action\":\"authorize\",\"operator_id\":\"em\",\"receiver\":\"worker-demo.$P\",\"release_deadline\":\"$(( (now + 86400) * 1000000000 ))\",\"reclaim_after\":\"$(( (now + 172800) * 1000000000 ))\",\"fee_bps_max\":1300,\"terms_hash\":\"0000000000000000000000000000000000000000000000000000000000000000\",\"salt\":\"$salt\"}"
  printf '%s' "$msg" > "$ESTADO/msg-$C.json"
  run near contract call-function as-transaction "$USDC" ft_transfer_call json-args "$(jq -nc --arg r "$C" --arg m "$msg" '{receiver_id:$r,amount:"1000000",msg:$m}')" prepaid-gas '100.0 Tgas' attached-deposit '1 yoctoNEAR' sign-as "payer-demo.$P" network-config "$NET" "$SIGN" send
}

paso9() {
  local id
  id=$(pago_actual)
  echo "payment_id: $id"
  run near contract call-function as-read-only "$C" get_payment json-args "{\"payment_id\":\"$id\"}" network-config "$NET" now
  capturer release "{\"payment_id\":\"$id\"}" '30.0 Tgas'
  for a in worker-demo em-fees; do
    capturer withdraw_for "{\"account\":\"$a.$P\"}" '50.0 Tgas'
  done
  run near contract call-function as-read-only "$USDC" ft_balance_of json-args "{\"account_id\":\"worker-demo.$P\"}" network-config "$NET" now
  run near contract call-function as-read-only "$C" get_config json-args '{}' network-config "$NET" now
}

paso10() {
  local todas
  todas=$(llaves "$C")
  run near account list-keys "$C" network-config "$NET" now
  for k in $todas; do
    run near account delete-keys "$C" public-keys "$k" network-config "$NET" "$SIGN" send
  done
  run near account list-keys "$C" network-config "$NET" now
  [ -z "$(llaves "$C")" ] && echo "OK: $C sin llaves"
}

paso11() {
  local id
  id=$(pago_actual)
  echo "payment_id: $id"
  run near contract call-function as-read-only "$C" get_payment json-args "{\"payment_id\":\"$id\"}" network-config "$NET" now
  capturer refund "{\"payment_id\":\"$id\"}" '30.0 Tgas'
  capturer withdraw_for "{\"account\":\"payer-demo.$P\"}" '50.0 Tgas'
  run near contract call-function as-read-only "$USDC" ft_balance_of json-args "{\"account_id\":\"payer-demo.$P\"}" network-config "$NET" now
  run near contract call-function as-read-only "$C" get_config json-args '{}' network-config "$NET" now
}

verificar() {
  if [ -f "$WASM" ]; then
    echo "wasm local: rev $(rev_de_wasm "$WASM"), sha256 $(shasum -a 256 "$WASM" | awk '{ print $1 }')"
  fi
  echo "codigo en $C: sha256 $(hash_onchain "$C")"
  run near account view-account-summary "$C" network-config "$NET" now
  run near account list-keys "$C" network-config "$NET" now
  run near contract call-function as-read-only "$C" get_config json-args '{}' network-config "$NET" now
}

case "${1:-}" in
  1 | padre) paso1 ;;
  2 | hijos) paso2 ;;
  3 | usdc) paso3 ;;
  4 | faucet-usdc) paso4 ;;
  5 | deploy) paso5 ;;
  6 | operador) paso6 ;;
  7 | capturer) paso7 ;;
  8 | authorize) paso8 ;;
  9 | release) paso9 ;;
  10 | sin-llaves) paso10 ;;
  11 | refund) paso11 ;;
  verificar) verificar ;;
  todo) paso1; paso2; paso3; paso4; paso5; paso6; paso7; paso8; paso9; paso10; paso8; paso11 ;;
  *) sed -n '2,28p' "$0"; exit 1 ;;
esac
