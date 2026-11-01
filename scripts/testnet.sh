#!/usr/bin/env bash
# Arras v1 en testnet: los pasos de la sec. 8 de la prescripcion, en orden (near-cli-rs 0.30.1, cargo-near 0.22.0).
#
# Uso: scripts/testnet.sh <paso> | todo
#   1 padre        cuenta padre por el faucet de testnet y su saldo
#   2 hijos        v1 (5 NEAR), owner y em-capturer (0,5), guardian, em-fees, payer-demo y worker-demo (0,2)
#   3 usdc         storage_deposit en el USDC de testnet para v1, worker-demo, em-fees y payer-demo
#   4 faucet-usdc  a mano: USDC de https://faucet.circle.com (NEAR Testnet) a payer-demo.$P
#   5 deploy       build reproducible (si falta la wasm) y deploy con init privado (`with-init-call new`)
#   6 operador     operador em (sin listas), patrocinio de storage y NEP-145 del payer
#   7 capturer     FunctionCall key acotada para em-capturer y borrado de su FullAccess
#   8 authorize    ft_transfer_call de 1 USDC con salt de un CSPRNG (necesita el paso 4)
#   9 release      release, withdraw_for del worker y del fee_recipient, saldo del worker
#  10 sin-llaves   borra TODAS las llaves de v1.$P y muestra list-keys vacio
#
# Variables: ARRAS_PARENT (arras-dev.testnet), ARRAS_WASM (target/near/arras_escrow_v1/arras_escrow_v1.wasm),
# ARRAS_KEYCHAIN (keychain | legacy-keychain: el llavero de macOS o ~/.near-credentials), ARRAS_ESTADO (donde se
# guarda el msg del paso 8, fuera del repo). Las llaves quedan en el llavero de near-cli-rs: este script nunca
# imprime una llave privada. Solo testnet: mainnet es H-3 y no se corre desde aca.
set -euo pipefail

NET=testnet
P=${ARRAS_PARENT:-arras-dev.testnet}
USDC=3e2210e1184b45b64c8a434c0a7e7b23cc04ea7eb7a6c3c32520d03d4afcb8af
WASM=${ARRAS_WASM:-target/near/arras_escrow_v1/arras_escrow_v1.wasm}
KC=${ARRAS_KEYCHAIN:-keychain}
ESTADO=${ARRAS_ESTADO:-$HOME/.arras-testnet}
SAVE="save-to-$KC"
SIGN="sign-with-$KC"

case "$P" in
  *.testnet) ;;
  *) echo "ARRAS_PARENT=$P no es una cuenta de testnet: este script no toca mainnet (H-3)" >&2; exit 1 ;;
esac
case "$KC" in
  keychain | legacy-keychain) ;;
  *) echo "ARRAS_KEYCHAIN tiene que ser keychain o legacy-keychain" >&2; exit 1 ;;
esac

run() {
  printf '\n+ %s\n' "$*"
  "$@"
}

# Llaves publicas de una cuenta (columna "Public Key" de list-keys); con $2 = "full", solo las FullAccess.
llaves() {
  near account list-keys "$1" network-config "$NET" now 2>/dev/null |
    sed 's/\x1b\[[0-9;]*m//g' |
    awk -F'|' -v solo="${2:-}" '$3 ~ /ed25519:/ { gsub(/ /, "", $3); if (solo != "full" || $5 ~ /full access/) print $3 }'
}

paso1() {
  run near account create-account sponsor-by-faucet-service "$P" autogenerate-new-keypair "$SAVE" network-config "$NET" create
  run near account view-account-summary "$P" network-config "$NET" now
}

paso2() {
  run near account create-account fund-myself "v1.$P" '5 NEAR' autogenerate-new-keypair "$SAVE" sign-as "$P" network-config "$NET" "$SIGN" send
  for c in owner em-capturer; do
    run near account create-account fund-myself "$c.$P" '0.5 NEAR' autogenerate-new-keypair "$SAVE" sign-as "$P" network-config "$NET" "$SIGN" send
  done
  for c in guardian em-fees payer-demo worker-demo; do
    run near account create-account fund-myself "$c.$P" '0.2 NEAR' autogenerate-new-keypair "$SAVE" sign-as "$P" network-config "$NET" "$SIGN" send
  done
}

paso3() {
  for c in v1 worker-demo em-fees payer-demo; do
    run near contract call-function as-transaction "$USDC" storage_deposit json-args "{\"account_id\":\"$c.$P\",\"registration_only\":true}" prepaid-gas '30.0 Tgas' attached-deposit '0.00125 NEAR' sign-as "$P" network-config "$NET" "$SIGN" send
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
  shasum -a 256 "$WASM"
  # Tope global de v1: 1.000 USDC (decision del dueno del 2026-10-26, Q3); por pago 10 USDC.
  run near contract deploy "v1.$P" use-file "$WASM" with-init-call new json-args "{\"owner\":\"owner.$P\",\"guardian\":\"guardian.$P\",\"token\":\"$USDC\",\"tope_por_pago\":\"10000000\",\"tope_tvl_global\":\"1000000000\"}" prepaid-gas '30.0 Tgas' attached-deposit '0 NEAR' network-config "$NET" "$SIGN" send
  run near contract call-function as-read-only "v1.$P" get_config json-args '{}' network-config "$NET" now
}

paso6() {
  run near contract call-function as-transaction "v1.$P" register_operator json-args "{\"operator_id\":\"em\",\"input\":{\"owner\":\"owner.$P\",\"capturers\":[\"em-capturer.$P\"],\"fee_bps\":1300,\"fee_recipient\":\"em-fees.$P\",\"min_amount\":\"10000\",\"tope_tvl_operador\":\"200000000\",\"tope_tvl_por_payer\":\"20000000\",\"sponsor_max_por_payer\":\"20000000000000000000000\"}}" prepaid-gas '30.0 Tgas' attached-deposit '1 yoctoNEAR' sign-as "owner.$P" network-config "$NET" "$SIGN" send
  # fund_operator_storage lo puede firmar cualquiera: lo firma el padre, porque owner tiene 0,5 NEAR y no le alcanzan
  # para adjuntar 0,5 NEAR y pagar el gas (la sec. 8 lo firmaba owner).
  run near contract call-function as-transaction "v1.$P" fund_operator_storage json-args '{"operator_id":"em"}' prepaid-gas '30.0 Tgas' attached-deposit '0.5 NEAR' sign-as "$P" network-config "$NET" "$SIGN" send
  run near contract call-function as-transaction "v1.$P" storage_deposit json-args "{\"account_id\":\"payer-demo.$P\"}" prepaid-gas '30.0 Tgas' attached-deposit '0.05 NEAR' sign-as "payer-demo.$P" network-config "$NET" "$SIGN" send
  run near contract call-function as-read-only "v1.$P" get_operator json-args '{"operator_id":"em"}' network-config "$NET" now
}

paso7() {
  local full
  full=$(llaves "em-capturer.$P" full)
  [ -n "$full" ] || { echo "em-capturer.$P no tiene FullAccess (ya se borro?)" >&2; return 1; }
  run near account add-key "em-capturer.$P" grant-function-call-access --allowance '0.5 NEAR' --contract-account-id "v1.$P" --function-names 'release, refund, withdraw_for' autogenerate-new-keypair "$SAVE" network-config "$NET" "$SIGN" send
  run near account list-keys "em-capturer.$P" network-config "$NET" now
  for k in $full; do
    run near account delete-keys "em-capturer.$P" public-keys "$k" network-config "$NET" "$SIGN" send
  done
  run near account list-keys "em-capturer.$P" network-config "$NET" now
}

paso8() {
  mkdir -p "$ESTADO"
  local now salt msg
  now=$(date +%s)
  salt=$(openssl rand -hex 16)
  msg="{\"v\":1,\"action\":\"authorize\",\"operator_id\":\"em\",\"receiver\":\"worker-demo.$P\",\"release_deadline\":\"$(( (now + 86400) * 1000000000 ))\",\"reclaim_after\":\"$(( (now + 172800) * 1000000000 ))\",\"fee_bps_max\":1300,\"terms_hash\":\"0000000000000000000000000000000000000000000000000000000000000000\",\"salt\":\"$salt\"}"
  printf '%s' "$msg" > "$ESTADO/msg.json"
  run near contract call-function as-transaction "$USDC" ft_transfer_call json-args "$(jq -nc --arg r "v1.$P" --arg m "$msg" '{receiver_id:$r,amount:"1000000",msg:$m}')" prepaid-gas '100.0 Tgas' attached-deposit '1 yoctoNEAR' sign-as "payer-demo.$P" network-config "$NET" "$SIGN" send
}

paso9() {
  local msg id
  msg=$(cat "$ESTADO/msg.json")
  id=$(near contract call-function as-read-only "v1.$P" compute_payment_id json-args "$(jq -nc --arg s "payer-demo.$P" --arg m "$msg" '{sender_id:$s,amount:"1000000",msg:$m}')" network-config "$NET" now 2>/dev/null | sed 's/\x1b\[[0-9;]*m//g' | grep -oE '"[1-9A-HJ-NP-Za-km-z]{43,44}"' | tail -1 | tr -d '"')
  echo "payment_id: $id"
  run near contract call-function as-read-only "v1.$P" get_payment json-args "{\"payment_id\":\"$id\"}" network-config "$NET" now
  run near contract call-function as-transaction "v1.$P" release json-args "{\"payment_id\":\"$id\"}" prepaid-gas '30.0 Tgas' attached-deposit '0 NEAR' sign-as "em-capturer.$P" network-config "$NET" "$SIGN" send
  for a in worker-demo em-fees; do
    run near contract call-function as-transaction "v1.$P" withdraw_for json-args "{\"account\":\"$a.$P\"}" prepaid-gas '50.0 Tgas' attached-deposit '0 NEAR' sign-as "em-capturer.$P" network-config "$NET" "$SIGN" send
  done
  run near contract call-function as-read-only "$USDC" ft_balance_of json-args "{\"account_id\":\"worker-demo.$P\"}" network-config "$NET" now
  run near contract call-function as-read-only "v1.$P" get_config json-args '{}' network-config "$NET" now
}

paso10() {
  local todas
  todas=$(llaves "v1.$P")
  run near account list-keys "v1.$P" network-config "$NET" now
  for k in $todas; do
    run near account delete-keys "v1.$P" public-keys "$k" network-config "$NET" "$SIGN" send
  done
  run near account list-keys "v1.$P" network-config "$NET" now
  [ -z "$(llaves "v1.$P")" ] && echo "OK: v1.$P sin llaves"
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
  todo) paso1; paso2; paso3; paso4; paso5; paso6; paso7; paso8; paso9; paso10 ;;
  *) sed -n '2,24p' "$0"; exit 1 ;;
esac
