#!/usr/bin/env bash
# Arras v1 en mainnet (H-3): cuentas, deploy con `new` privado, operador de EM, capturer acotado y la cuenta del
# contrato sin llaves; el mismo script corre el ensayo en testnet (ARRAS_RED=testnet). near-cli-rs 0.30.1.
#
# EN SECO POR DEFECTO: sin bandera imprime cada comando y no ejecuta nada (ni una consulta de red). El modo real
# exige la bandera de LA MISMA red: --mainnet-ok con ARRAS_RED=mainnet (lo corre c0der) o --testnet-ok con
# ARRAS_RED=testnet; la bandera de la otra red aborta. Este script nunca imprime una llave privada.
#
# Uso: scripts/mainnet.sh [--mainnet-ok | --testnet-ok] <paso>
#
#   wasm        build reproducible de ARRAS_REV en un clon limpio fuera del repo y chequeo del sha256 (local: corre
#               tambien en seco, no toca ninguna red de NEAR)
#   1 padre     el padre, en uno de tres casos segun ARRAS_PADRE:
#               (a) <nombre>.near (o .testnet) con ARRAS_FONDEO: lo crea `near.create_account` (`testnet` en testnet)
#                   firmado por la cuenta implicita de fondeo, cuya llave se lee de SSM (ARRAS_FONDEO_SSM) por un pipe
#                   y nunca toca el disco ni se imprime; en el ensayo de testnet, de ARRAS_FONDEO_ARCHIVO
#               (b) una sub-cuenta (p. ej. arras.ultravioletadao.near): genera la llave del padre en el llavero legacy
#                   e imprime la llave PUBLICA y el comando con el que el dueno lo crea
#               (c) testnet sin ARRAS_FONDEO: por el faucet
#   2 cuentas   v1 (contrato), v2 (reservada), owner, guardian, capturer y fee_recipient, hijas del padre
#   3 usdc      storage_deposit en USDC del contrato y del fee_recipient
#   4 deploy    sha256 y rev de la wasm contra los pines, deploy + `new` privado en una tx y codigo en cadena
#   5 operador  register_operator del operador de EM y fund_operator_storage
#   6 capturer  FunctionCall key del capturer acotada a v1 (release, refund, withdraw_for) y borrado de su FullAccess
#   7 sin-llaves  borra TODAS las llaves de v1 en una tx y exige view_access_key_list vacio
#   8 verificar codigo, llaves, get_config, get_operator y get_storage_health (solo lectura)
#   9 authorize (solo testnet) ft_transfer_call del payer de demo
#  10 release   (solo testnet) release del ultimo authorize, firmado por el capturer con su FunctionCall key
#  11 withdraw  (solo testnet) withdraw_for del worker y del fee_recipient, firmados por el capturer
#     todo      padre, cuentas, usdc, deploy, operador, capturer, sin-llaves, verificar, secretos y enlaces
#     ensayo    (solo testnet) todo, authorize, release, withdraw, verificar, medir y enlaces
#     medir     saldo y storage de cada cuenta y lo que gasto cada paso (solo lectura)
#     secretos  imprime (no corre) los comandos de AWS que suben cada llave nueva por NOMBRE y la borran del disco
#     enlaces   enlaces de NearBlocks de las cuentas y de cada tx anotada, y NEAR_ESCROW_CONTRACTS_<RED>
#     plan      red, cuentas, montos, topes, wasm y presupuesto, sin ejecutar nada
#
# El llavero legacy es el `credentials_home_dir` de la config de near-cli (~/.near-credentials por defecto).
# Variables (todas con defecto; docs/MAINNET.md tiene la tabla):
#   ARRAS_RED (mainnet | testnet), ARRAS_PADRE (mainnet: arras-escrow.near; testnet: obligatoria, una cuenta de
#   desarrollo nueva), ARRAS_FONDEO (mainnet: fe0a5742...1079, la implicita de fondeo del caso a), ARRAS_FONDEO_SSM
#   (/arras/<red>/fondeo), ARRAS_FONDEO_ARCHIVO (solo testnet), ARRAS_CAPTURER (em-capturer.$P), ARRAS_FEE_RECIPIENT (em-fees.$P o una cuenta que ya
#   existe), ARRAS_OPERATOR_ID (em), ARRAS_REV y ARRAS_WASM_SHA256 (los pines de la wasm), ARRAS_WASM, ARRAS_ESTADO
#   (~/.arras-despliegue: txs y saldos, fuera del repo), ARRAS_RPC (tiene que ser de la red: se mira
#   `status.chain_id`), ARRAS_ESPERA (segundos entre reintentos de lectura, 5), los topes ARRAS_TOPE_POR_PAGO (10 USDC) y ARRAS_TOPE_TVL_GLOBAL (1.000 USDC), el
#   operador ARRAS_FEE_BPS, ARRAS_MIN_AMOUNT, ARRAS_TOPE_TVL_OPERADOR, ARRAS_TOPE_TVL_POR_PAYER,
#   ARRAS_SPONSOR_MAX_POR_PAYER (yocto) y ARRAS_POOL, los montos ARRAS_NEAR_<ROL>, ARRAS_ALLOWANCE, el ensayo
#   ARRAS_PAYER, ARRAS_PAYER_KEYCHAIN, ARRAS_WORKER y ARRAS_MONTO, y los secretos ARRAS_SECRETOS (ssm |
#   secretsmanager), ARRAS_SECRETOS_PREFIJO, ARRAS_KMS_KEY (OBLIGATORIA en el modo real: la llave KMS propia de Arras,
#   decision 230) y ARRAS_AWS_REGION (us-east-2).
set -euo pipefail
umask 077

ESC=$(printf '\033')
# `die` corta TODO el script aunque corra dentro de un $(...), un pipe o un <(...) (ahi `exit` solo cierra la subshell
# y el script seguiria con un valor vacio): siempre le manda TERM al proceso principal ($$ es el principal tambien en
# una subshell), cuyo trap sale con 1. No se usa BASH_SUBSHELL: en el /bin/bash 3.2 de macOS vale 0 dentro de <(...).
trap 'exit 1' TERM
die() {
  printf 'ABORTO: %s\n' "$*" >&2
  kill -TERM "$$" 2>/dev/null
  exit 1
}
aviso() { printf 'AVISO: %s\n' "$*" >&2; }
uso() { awk 'NR > 1 && /^#/ { sub(/^# ?/, ""); print; next } NR > 1 { exit }' "$0" >&2; exit 1; }

# --- argumentos -----------------------------------------------------------------------------------------------------
BANDERA="" PASO=""
for arg in "$@"; do
  case "$arg" in
    --mainnet-ok | --testnet-ok)
      b=${arg#--}; b=${b%-ok}
      [ -z "$BANDERA" ] || [ "$BANDERA" = "$b" ] || die "--mainnet-ok y --testnet-ok juntas: una sola red por corrida"
      BANDERA=$b ;;
    -h | --help) uso ;;
    -*) echo "bandera desconocida: $arg" >&2; uso ;;
    *) [ -z "$PASO" ] || { echo "un paso por corrida (vinieron '$PASO' y '$arg')" >&2; uso; }; PASO=$arg ;;
  esac
done
[ -n "$PASO" ] || uso

RED=$(printf '%s' "${ARRAS_RED:-mainnet}" | tr '[:upper:]' '[:lower:]')
case "$RED" in
  mainnet)
    SUF=near
    USDC=17208628f84f5d6ad33f0da3bbbeb27ffcb398eac501a31bd6ad2011e36133a1
    RPC=${ARRAS_RPC:-https://rpc.mainnet.fastnear.com}
    NB=https://nearblocks.io
    P=${ARRAS_PADRE:-arras-escrow.near}
    # La implicita de fondeo del caso (a): la crea y la carga c0der; su llave vive en SSM /arras/mainnet/fondeo.
    FONDEO=${ARRAS_FONDEO-fe0a574258f8277099875e46d4ca0043cdb6274ff2501b375a0d3d64bc211079} ;;
  testnet)
    SUF=testnet
    USDC=3e2210e1184b45b64c8a434c0a7e7b23cc04ea7eb7a6c3c32520d03d4afcb8af
    RPC=${ARRAS_RPC:-https://rpc.testnet.fastnear.com}
    NB=https://testnet.nearblocks.io
    P=${ARRAS_PADRE:-}
    FONDEO=${ARRAS_FONDEO:-} ;;
  *) die "ARRAS_RED tiene que ser mainnet o testnet (vino '${ARRAS_RED:-}')" ;;
esac
NET=$RED

MODO=seco
if [ -n "$BANDERA" ]; then
  [ "$BANDERA" = "$RED" ] || die "--$BANDERA-ok no vale con ARRAS_RED=$RED: el modo real de $RED es --$RED-ok"
  MODO=real
fi
real() { [ "$MODO" = real ]; }

# --- cuentas --------------------------------------------------------------------------------------------------------
RE_CUENTA='^(([a-z0-9]+[-_])*[a-z0-9]+\.)*([a-z0-9]+[-_])*[a-z0-9]+$'
cuenta_valida() { [[ ${#1} -ge 2 && ${#1} -le 64 && $1 =~ $RE_CUENTA ]]; }
de_la_red() { cuenta_valida "$1" && [[ $1 == *."$SUF" ]]; }
implicita() { [[ $1 =~ ^[0-9a-f]{64}$ ]]; }
# Hija DIRECTA del padre: solo el padre la puede crear.
hija() { cuenta_valida "$1" && [[ $1 == *."$P" && ${1%."$P"} != *.* ]]; }

[ -z "${ARRAS_PARENT:-}" ] || die "ARRAS_PARENT se renombro a ARRAS_PADRE: no se ignora en silencio"
[ -n "$P" ] || die "en testnet ARRAS_PADRE es obligatoria: una cuenta de desarrollo nueva (p. ej. arras-ensayo-1102.testnet)"
de_la_red "$P" || die "ARRAS_PADRE='$P' no es una cuenta valida de $RED (*.$SUF)"
# El caso del padre: (a) fondeo, (b) dueno, (c) faucet (ver la cabecera).
if [[ ${P%."$SUF"} != *.* ]]; then
  if [ -n "$FONDEO" ]; then
    CASO_PADRE=fondeo
  elif [ "$RED" = testnet ]; then
    CASO_PADRE=faucet
  else
    die "ARRAS_PADRE=$P cuelga de near: lo crea near.create_account desde ARRAS_FONDEO, que vino vacia"
  fi
else
  [ "$RED" = mainnet ] || die "en testnet el padre es <nombre>.testnet (faucet o ARRAS_FONDEO)"
  CASO_PADRE=dueno
fi
if [ "$CASO_PADRE" = fondeo ]; then
  implicita "$FONDEO" || die "ARRAS_FONDEO='$FONDEO' tiene que ser una cuenta implicita: 64 hex en minusculas"
fi
FONDEO_SSM=${ARRAS_FONDEO_SSM:-/arras/$RED/fondeo}
FONDEO_ARCHIVO=${ARRAS_FONDEO_ARCHIVO:-}
REGION=${ARRAS_AWS_REGION:-us-east-2}
[ -z "$FONDEO_ARCHIVO" ] || [ "$RED" = testnet ] ||
  die "ARRAS_FONDEO_ARCHIVO es solo del ensayo en testnet: en mainnet la llave de fondeo se lee de SSM ($FONDEO_SSM)"
C1=v1.$P
C2=v2.$P
OWNER=owner.$P
GUARD=guardian.$P
CAPT=${ARRAS_CAPTURER:-em-capturer.$P}
FEES=${ARRAS_FEE_RECIPIENT:-em-fees.$P}
OP=${ARRAS_OPERATOR_ID:-em}
for a in "$C1" "$C2" "$OWNER" "$GUARD"; do
  cuenta_valida "$a" || die "$a no es una cuenta valida (64 caracteres como maximo): acorta ARRAS_PADRE"
done
hija "$CAPT" || die "ARRAS_CAPTURER='$CAPT' tiene que ser hija directa de $P: el script la crea y su FullAccess firma el add-key"
case "$CAPT" in "$C1" | "$C2" | "$OWNER" | "$GUARD") die "el capturer no puede ser $CAPT (otra cuenta del despliegue)" ;; esac
if hija "$FEES"; then
  FEES_NUEVA=1
elif de_la_red "$FEES" || implicita "$FEES"; then
  FEES_NUEVA=0
else
  die "ARRAS_FEE_RECIPIENT='$FEES' no es una cuenta valida de $RED"
fi
case "$FEES" in "$C1" | "$C2") die "el fee_recipient no puede ser un contrato del despliegue ($FEES): ARRAS_E46 / USDC atrapado" ;; esac
# El capturer queda solo con una FunctionCall key a v1: como fee_recipient nunca podria mover su USDC.
[ "$FEES" != "$CAPT" ] || die "el fee_recipient no puede ser el capturer ($FEES): sin FullAccess, la tarifa quedaria atrapada"
[[ $OP =~ ^[a-z0-9_-]{1,32}$ ]] || die "ARRAS_OPERATOR_ID='$OP' fuera de [a-z0-9_-]{1,32} (ARRAS_E45)"

# --- topes y montos -------------------------------------------------------------------------------------------------
entero() { [[ $2 =~ ^[0-9]{1,15}$ ]] || die "$1='$2' tiene que ser un entero de hasta 15 cifras (unidades atomicas)"; }
yocto_valido() { [[ $2 =~ ^[0-9]{1,30}$ ]] || die "$1='$2' tiene que ser un entero de yoctoNEAR"; }
near_valido() { [[ $2 =~ ^[0-9]+(\.[0-9]+)?\ NEAR$ ]] || die "$1='$2' tiene que ser '<numero> NEAR' (p. ej. '0.5 NEAR')"; }

TOPE_POR_PAGO=${ARRAS_TOPE_POR_PAGO:-10000000}          # 10 USDC (P-6)
# Global 1.000 USDC: la decision del dueno del 2026-10-26 ("Tope global de 1.000"), posterior a P-6 (RESPUESTA de
# c0der, Q2). `new` es el techo para siempre (`set_caps` solo baja).
TOPE_TVL_GLOBAL=${ARRAS_TOPE_TVL_GLOBAL:-1000000000}
FEE_BPS=${ARRAS_FEE_BPS:-1300}
MIN_AMOUNT=${ARRAS_MIN_AMOUNT:-10000}                   # 0,01 USDC (F-7); el owner lo mueve en los dos sentidos
# Por operador tambien 1.000: su techo queda fijo en register_operator (admin.rs:35; update_operator solo lo baja,
# admin.rs:88-96, E42), asi que un parametro tecnico no recorta el tope del dueno; el owner lo puede bajar.
TOPE_TVL_OPERADOR=${ARRAS_TOPE_TVL_OPERADOR:-1000000000}
TOPE_TVL_POR_PAYER=${ARRAS_TOPE_TVL_POR_PAYER:-20000000} # 20 USDC
SPONSOR_MAX=${ARRAS_SPONSOR_MAX_POR_PAYER:-20000000000000000000000} # 0,02 NEAR
POOL=${ARRAS_POOL:-0.5 NEAR}
# Medido en el ensayo (docs/MAINNET.md): v1 ocupa 269.674 bytes (2,69674 NEAR) y el deploy quema 0,0022; con 3,0
# quedan 0,30 de reserva libre. Lo que entra a v1 (reserva y pool) queda ahi para siempre: la cuenta no tiene llaves.
NEAR_V1=${ARRAS_NEAR_V1:-3.0 NEAR}
NEAR_V2=${ARRAS_NEAR_V2:-0.1 NEAR}
NEAR_OWNER=${ARRAS_NEAR_OWNER:-0.1 NEAR}
NEAR_GUARD=${ARRAS_NEAR_GUARDIAN:-0.1 NEAR}
NEAR_CAPT=${ARRAS_NEAR_CAPTURER:-0.5 NEAR}
NEAR_FEES=${ARRAS_NEAR_FEE_RECIPIENT:-0.1 NEAR}
ALLOWANCE=${ARRAS_ALLOWANCE:-0.5 NEAR}
USDC_STORAGE='0.00125 NEAR'
MARGEN_GAS='0.1 NEAR'
# Lo que la implicita de fondeo necesita ademas del saldo del padre: near.create_account costo 0,00745 NEAR en el ensayo
# del caso (a) (gas y lo que quema la creacion).
MARGEN_CREACION='0.02 NEAR'
for v in TOPE_POR_PAGO TOPE_TVL_GLOBAL FEE_BPS MIN_AMOUNT TOPE_TVL_OPERADOR TOPE_TVL_POR_PAYER; do entero "$v" "${!v}"; done
yocto_valido ARRAS_SPONSOR_MAX_POR_PAYER "$SPONSOR_MAX"
for v in POOL NEAR_V1 NEAR_V2 NEAR_OWNER NEAR_GUARD NEAR_CAPT NEAR_FEES ALLOWANCE; do near_valido "$v" "${!v}"; done
[[ ${ALLOWANCE%% *} =~ [1-9] ]] || die "ARRAS_ALLOWANCE='$ALLOWANCE': con 0 la FunctionCall key del capturer no puede pagar gas"
[ "$TOPE_POR_PAGO" -gt 0 ] || die "ARRAS_TOPE_POR_PAGO tiene que ser mayor que 0"
[ "$TOPE_TVL_GLOBAL" -ge "$TOPE_POR_PAGO" ] || die "el TVL global ($TOPE_TVL_GLOBAL) no puede ser menor que el tope por pago ($TOPE_POR_PAGO)"
[ "$TOPE_TVL_OPERADOR" -le "$TOPE_TVL_GLOBAL" ] || die "el TVL del operador ($TOPE_TVL_OPERADOR) no puede pasar el global ($TOPE_TVL_GLOBAL)"
[ "$TOPE_TVL_POR_PAYER" -le "$TOPE_TVL_OPERADOR" ] || die "el TVL por payer ($TOPE_TVL_POR_PAYER) no puede pasar el del operador ($TOPE_TVL_OPERADOR)"
[ "$MIN_AMOUNT" -le "$TOPE_POR_PAGO" ] || die "min_amount ($MIN_AMOUNT) por encima del tope por pago ($TOPE_POR_PAGO): ningun pago entraria"
[ "$FEE_BPS" -le 1800 ] || die "ARRAS_FEE_BPS=$FEE_BPS pasa 1800 (MAX_FEE_BPS, ARRAS_E43)"

# --- wasm -----------------------------------------------------------------------------------------------------------
# Los pines: el build reproducible de 8da7174 (RARRAS-1), la misma wasm de arras-v1b-1101.testnet.
REV=$(printf '%s' "${ARRAS_REV:-8da71748b428ceccf9fccd358e61bb4213d54e29}" | tr '[:upper:]' '[:lower:]')
WASM_SHA=$(printf '%s' "${ARRAS_WASM_SHA256:-7058319dff5f8d1e9da91a344ab3c699b709783e4153f3a7da6c2556cde0eada}" | tr '[:upper:]' '[:lower:]')
[[ $REV =~ ^[0-9a-f]{40}$ ]] || die "ARRAS_REV tiene que ser un sha de git de 40 caracteres hex"
[[ $WASM_SHA =~ ^[0-9a-f]{64}$ ]] || die "ARRAS_WASM_SHA256 tiene que ser un sha256 de 64 caracteres hex"
BASE=${ARRAS_ESTADO:-$HOME/.arras-despliegue}
WASM=${ARRAS_WASM:-$BASE/arras_escrow_v1-$REV.wasm}
EST=$BASE/$RED/$P
TXS=$EST/txs.tsv
SALDOS=$EST/saldos.tsv
# El llavero legacy: el `credentials_home_dir` de la config de near-cli (macOS o Linux), que es donde near guarda y de
# donde firma; si no hay config, el defecto de near-cli.
cred_de_near() {
  local c
  for c in "$HOME/Library/Application Support/near-cli/config.toml" "${XDG_CONFIG_HOME:-$HOME/.config}/near-cli/config.toml"; do
    if [ -f "$c" ]; then
      sed -n 's/^credentials_home_dir *= *"\(.*\)"$/\1/p' "$c" | head -1
      return 0
    fi
  done
}
CRED=$(cred_de_near)
CRED=${CRED:-$HOME/.near-credentials}
ESPERA=${ARRAS_ESPERA:-5}
[[ $ESPERA =~ ^[0-9]{1,2}$ ]] || die "ARRAS_ESPERA tiene que ser segundos (0-99)"
SIGN=sign-with-legacy-keychain
SAVE=save-to-legacy-keychain

sha256_de() { shasum -a 256 "$1" | awk '{ print $1 }'; }
rev_de_wasm() { LC_ALL=C grep -ao 'rev=[0-9a-f]\{40\}' "$1" | head -1 | cut -d= -f2; }

# La wasm local tiene que ser la de los pines. En seco solo avisa; en real aborta antes de cualquier tx.
wasm_ok() {
  local h r
  if [ ! -f "$WASM" ]; then
    real && die "falta la wasm $WASM: corre el paso wasm"
    aviso "falta la wasm $WASM (el modo real aborta aca; el paso wasm la construye)"
    return 0
  fi
  h=$(sha256_de "$WASM")
  r=$(rev_de_wasm "$WASM")
  if [ "$h" != "$WASM_SHA" ] || [ "$r" != "$REV" ]; then
    real && die "la wasm $WASM es sha256 $h rev ${r:-?}; los pines son $WASM_SHA rev $REV"
    aviso "la wasm $WASM es sha256 $h rev ${r:-?}; los pines son $WASM_SHA rev $REV (el modo real aborta)"
    return 0
  fi
  echo "wasm $WASM: sha256 $h, rev $r (= pines)"
}

# --- salida y transacciones -----------------------------------------------------------------------------------------
# Toda salida de near pasa por aca: sin colores y sin nada que parezca una llave privada o una frase semilla (near no
# las imprime con save-to-legacy-keychain; esto es la segunda red).
sanitizar() {
  sed "s/${ESC}\[[0-9;]*m//g" | grep -viE 'secret|private|seed|mnemonic|ed25519:[1-9A-HJ-NP-Za-km-z]{60,}' || true
}

# Una transaccion de near. En seco la imprime y sigue; en real la corre sin terminal (si near pidiera algo, falla en
# vez de colgarse), filtra la salida, anota el Transaction ID en $TXS y aborta si near falla.
tx() {
  local etiqueta=$1 salida rc id
  shift
  printf '\n+ near %s\n' "$*"
  if ! real; then
    echo "  [en seco: no se ejecuta]"
    return 0
  fi
  set +e
  salida=$(NO_COLOR=1 near "$@" </dev/null 2>&1)
  rc=$?
  set -e
  printf '%s\n' "$salida" | sanitizar | tail -n 25
  id=$(printf '%s\n' "$salida" | sed "s/${ESC}\[[0-9;]*m//g" | awk '/Transaction ID:/ { print $NF }' | tail -1)
  if [ -n "$id" ]; then
    printf '%s\t%s\t%s\n' "$(date -u +%FT%TZ)" "$etiqueta" "$id" >>"$TXS"
  fi
  [ "$rc" -eq 0 ] || die "near fallo en '$etiqueta' (exit $rc)"
}

# --- lecturas por RPC (solo en real) ----------------------------------------------------------------------------------
rpc_query() {
  local r
  r=$(curl -sf -m 30 -X POST "$RPC" -H 'Content-Type: application/json' \
    -d "$(jq -nc --argjson p "$1" '{jsonrpc:"2.0",id:"arras",method:"query",params:$p}')") ||
    die "el RPC $RPC no respondio"
  printf '%s' "$r"
}
ver_cuenta() { rpc_query "$(jq -nc --arg a "$1" '{request_type:"view_account",finality:"final",account_id:$a}')"; }
existe() {
  local r
  r=$(ver_cuenta "$1")
  jq -e '.result.amount | strings' >/dev/null 2>&1 <<<"$r" && return 0
  jq -e '[.error | .. | strings] | any(test("UNKNOWN_ACCOUNT|does not exist"))' >/dev/null 2>&1 <<<"$r" && return 1
  die "view_account de $1 en $RPC no trajo ni la cuenta ni UNKNOWN_ACCOUNT: $(head -c 300 <<<"$r")"
}
# JSON devuelto por una vista del contrato.
vista() {
  local r b
  b=$(printf '%s' "$3" | base64 | tr -d '\n')
  r=$(rpc_query "$(jq -nc --arg a "$1" --arg m "$2" --arg b "$b" '{request_type:"call_function",finality:"final",account_id:$a,method_name:$m,args_base64:$b}')")
  jq -er '.result.result | implode' 2>/dev/null <<<"$r" || die "la vista $2 de $1 fallo: $(head -c 300 <<<"$r")"
}
llaves_json() {
  local r
  r=$(rpc_query "$(jq -nc --arg a "$1" '{request_type:"view_access_key_list",finality:"final",account_id:$a}')")
  jq -ec '.result.keys | arrays' <<<"$r" 2>/dev/null || die "view_access_key_list de $1 no trajo .result.keys: $(head -c 300 <<<"$r")"
}
llaves_full() { llaves_json "$1" | jq -r '.[] | select(.access_key.permission == "FullAccess") | .public_key'; }
n_llaves() { llaves_json "$1" | jq 'length'; }
# El code_hash (base58) de una cuenta, en hex.
codigo_hex() {
  local h
  h=$(ver_cuenta "$1" | jq -r '.result.code_hash // empty')
  [ -n "$h" ] || die "view_account de $1 no trajo code_hash"
  python3 -I -c '
import sys
a = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"
n = 0
for c in sys.argv[1]:
    n = n * 58 + a.index(c)
print(n.to_bytes(32, "big").hex())' "$h"
}
yocto() { python3 -I -c 'import sys, decimal; print(int(decimal.Decimal(sys.argv[1].split()[0]) * 10**24))' "$1"; }
en_near() { python3 -I -c 'import sys, decimal; print(f"{decimal.Decimal(sys.argv[1]) / 10**24:.5f} NEAR")' "$1"; }
suma_near() { python3 -I -c 'import sys, decimal; print(sum(decimal.Decimal(a.split()[0]) for a in sys.argv[1:]))' "$@"; }

# Archivo del llavero legacy de una llave publica (near-cli: <cred>/<red>/<cuenta>/ed25519_<base58>.json).
archivo_llave() { printf '%s/%s/%s/%s.json' "$CRED" "$NET" "$1" "${2/:/_}"; }

# Saldo y storage de las cuentas del despliegue despues de cada paso real (lo usa `medir`).
# Las cuentas del despliegue; en el caso (a), primero la implicita de fondeo.
cuentas_todas() {
  [ "$CASO_PADRE" = fondeo ] && printf '%s\n' "$FONDEO"
  printf '%s\n' "$P" "$C1" "$C2" "$OWNER" "$GUARD" "$CAPT" "$FEES"
}
anotar_saldos() {
  real || return 0
  local a r
  for a in $(cuentas_todas); do
    r=$(ver_cuenta "$a")
    jq -e '.result.amount' >/dev/null 2>&1 <<<"$r" || continue
    printf '%s\t%s\t%s\t%s\t%s\n' "$(date -u +%FT%TZ)" "$1" "$a" "$(jq -r .result.amount <<<"$r")" "$(jq -r .result.storage_usage <<<"$r")" >>"$SALDOS"
  done
}

solo_testnet() { [ "$RED" = testnet ] || die "el paso $1 es solo del ensayo en testnet: en mainnet el primer pago es de EM (H-8)"; }

# Lo que el padre gasta: las hijas, el pool, el storage de USDC y un margen para el gas (6 creaciones ~0,043 NEAR, el
# resto ~0,001; medido en el ensayo).
presupuesto() {
  local extra=""
  [ "$FEES_NUEVA" = 1 ] && extra=$NEAR_FEES
  suma_near "$NEAR_V1" "$NEAR_V2" "$NEAR_OWNER" "$NEAR_GUARD" "$NEAR_CAPT" ${extra:+"$extra"} "$POOL" "$USDC_STORAGE" "$USDC_STORAGE" "$MARGEN_GAS"
}
# Con lo que se crea el padre: el presupuesto mas el storage propio del padre (182 bytes), redondeado arriba.
presupuesto_padre() {
  python3 -I -c 'import sys, decimal; d = decimal.Decimal(sys.argv[1]) + decimal.Decimal("0.01"); print(d.quantize(decimal.Decimal("0.01"), rounding=decimal.ROUND_UP))' "$(presupuesto)"
}
# Con lo que el dueno fondea la implicita del caso (a): el saldo del padre, el margen de near.create_account y los 182
# bytes de la propia implicita, redondeado arriba.
presupuesto_fondeo() {
  python3 -I -c 'import sys, decimal; d = sum(decimal.Decimal(a.split()[0]) for a in sys.argv[1:]) + decimal.Decimal("0.00182"); print(d.quantize(decimal.Decimal("0.01"), rounding=decimal.ROUND_UP))' "$(presupuesto_padre) NEAR" "$MARGEN_CREACION"
}

ARGS_NEW=$(jq -nc --arg o "$OWNER" --arg g "$GUARD" --arg t "$USDC" --arg p "$TOPE_POR_PAGO" --arg v "$TOPE_TVL_GLOBAL" \
  '{owner:$o,guardian:$g,token:$t,tope_por_pago:$p,tope_tvl_global:$v}')
ARGS_OPERADOR=$(jq -nc --arg id "$OP" --arg o "$OWNER" --arg c "$CAPT" --argjson f "$FEE_BPS" --arg r "$FEES" \
  --arg m "$MIN_AMOUNT" --arg to "$TOPE_TVL_OPERADOR" --arg tp "$TOPE_TVL_POR_PAYER" --arg s "$SPONSOR_MAX" \
  '{operator_id:$id,input:{owner:$o,capturers:[$c],fee_bps:$f,fee_recipient:$r,min_amount:$m,tope_tvl_operador:$to,tope_tvl_por_payer:$tp,sponsor_max_por_payer:$s}}')

# --- pasos ------------------------------------------------------------------------------------------------------------
paso_plan() {
  cat <<PLAN
red $RED ($MODO); RPC $RPC; USDC $USDC
padre $P, caso $CASO_PADRE$( [ "$CASO_PADRE" = fondeo ] && echo " (near.create_account desde $FONDEO, llave en ${FONDEO_ARCHIVO:-SSM $FONDEO_SSM ($REGION)})")
v1 $C1 ($NEAR_V1) | v2 $C2 reservada ($NEAR_V2)
owner $OWNER ($NEAR_OWNER) | guardian $GUARD ($NEAR_GUARD) | capturer $CAPT ($NEAR_CAPT, allowance $ALLOWANCE)
fee_recipient $FEES ($( [ "$FEES_NUEVA" = 1 ] && echo "nueva, $NEAR_FEES" || echo "ya existe, no se crea"))
new: tope_por_pago $TOPE_POR_PAGO, tope_tvl_global $TOPE_TVL_GLOBAL (unidades de 6 decimales)
operador $OP: fee_bps $FEE_BPS, min_amount $MIN_AMOUNT, tope_tvl_operador $TOPE_TVL_OPERADOR, tope_tvl_por_payer $TOPE_TVL_POR_PAYER, sponsor_max_por_payer $SPONSOR_MAX yocto, pool $POOL
wasm $WASM (sha256 $WASM_SHA, rev $REV)
presupuesto del padre (hijas + pool + storage USDC + margen de gas): $(presupuesto) NEAR; el padre se crea con $(presupuesto_padre) NEAR$( [ "$CASO_PADRE" = fondeo ] && echo "; el dueno fondea la implicita $FONDEO con $(presupuesto_fondeo) NEAR")
PLAN
}

paso_wasm() {
  if [ -f "$WASM" ] && [ "$(sha256_de "$WASM")" = "$WASM_SHA" ]; then
    wasm_ok
    return 0
  fi
  local dir repo
  dir=$BASE/build-$REV
  repo=$(git -C "$(dirname "$0")" rev-parse --show-toplevel)
  mkdir -p "$BASE"
  [ -d "$dir/.git" ] || git clone -q "$repo" "$dir"
  git -C "$dir" checkout -q "$REV"
  [ -z "$(git -C "$dir" status --porcelain)" ] || die "$dir tiene cambios: el build reproducible exige git limpio"
  # Sin CARGO_TARGET_DIR: con el seteado, cargo-near 0.22.0 borra el artefacto antes de copiarlo (os error 2).
  (cd "$dir" && env -u CARGO_TARGET_DIR cargo near build reproducible-wasm --manifest-path contracts/escrow-v1/Cargo.toml)
  cp "$dir/target/near/arras_escrow_v1/arras_escrow_v1.wasm" "$WASM"
  MODO=real wasm_ok
}

# real: el padre existe y alguna llave de su carpeta del llavero legacy es FullAccess suya (el script firma con ella).
padre_listo() {
  existe "$P" || return 1
  local f pk full
  full=$(llaves_full "$P")
  for f in "$CRED/$NET/$P"/ed25519_*.json; do
    [ -f "$f" ] || continue
    pk=$(jq -r '.public_key // empty' "$f")
    printf '%s\n' "$full" | grep -qxF -- "$pk" && return 0
  done
  die "$P existe pero ninguna llave de $CRED/$NET/$P/ es FullAccess suya: no es el padre de este llavero"
}
esperar_padre() {
  local _i
  for _i in 1 2 3 4 5 6; do
    padre_listo && return 0
    sleep "$ESPERA"
  done
  die "$P no aparece despues de crearlo (near.create_account le devuelve el deposito a quien firma si falla): mira la tx"
}

# La llave de la implicita de fondeo como archivo de llave de near-cli (JSON con public_key y private_key), por stdout,
# para un pipe (`sign-with-access-key-file <(llave_fondeo)`): viene de SSM o, en el ensayo de testnet, de
# ARRAS_FONDEO_ARCHIVO; se exige que su llave publica sea la de ARRAS_FONDEO (una implicita es el hex de su llave
# publica). Nunca toca el disco, la pantalla ni la linea de comandos.
fuente_fondeo() {
  if [ -n "$FONDEO_ARCHIVO" ]; then
    cat "$FONDEO_ARCHIVO"
  else
    aws ssm get-parameter --region "$REGION" --name "$FONDEO_SSM" --with-decryption --output json | jq -j '.Parameter.Value'
  fi
}
llave_fondeo() {
  if [ -n "$FONDEO_ARCHIVO" ]; then
    [ -f "$FONDEO_ARCHIVO" ] || die "no existe ARRAS_FONDEO_ARCHIVO=$FONDEO_ARCHIVO"
  else
    command -v aws >/dev/null 2>&1 || die "falta aws: la llave de fondeo se lee de SSM ($FONDEO_SSM)"
  fi
  fuente_fondeo | python3 -I -c '
import json, sys
A = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"
def dec(s, n):
    v = 0
    for c in s:
        v = v * 58 + A.index(c)
    return v.to_bytes(n, "big")
def enc(b):
    v, s = int.from_bytes(b, "big"), ""
    while v:
        v, r = divmod(v, 58)
        s = A[r] + s
    return "1" * (len(b) - len(b.lstrip(b"\0"))) + s
try:
    crudo = sys.stdin.read().strip()
    try:
        sk = json.loads(crudo)["private_key"]
    except Exception:
        sk = crudo
    if not sk.startswith("ed25519:"):
        sys.exit(2)
    b = dec(sk[8:], 64)
    if b[32:].hex() != sys.argv[1]:
        sys.exit(3)
    sys.stdout.write(json.dumps({"account_id": sys.argv[1], "public_key": "ed25519:" + enc(b[32:]), "private_key": sk}))
except SystemExit:
    raise
except Exception:
    sys.exit(4)
' "$FONDEO" 2>/dev/null || die "la llave de fondeo (${FONDEO_ARCHIVO:-SSM $FONDEO_SSM}) no se pudo leer o no es la de $FONDEO"
}

padre_faucet() {
  if real && padre_listo; then
    echo "ya existe: $P"
    return 0
  fi
  tx "padre $P (faucet)" account create-account sponsor-by-faucet-service "$P" autogenerate-new-keypair "$SAVE" network-config "$NET" create
  real && esperar_padre
  return 0
}

padre_fondeo() {
  local monto r disponible necesario
  monto="$(presupuesto_padre) NEAR"
  if real && padre_listo; then
    echo "ya existe: $P (con una FullAccess del llavero)"
    return 0
  fi
  if ! real; then
    tx "padre $P (near.create_account desde $FONDEO)" account create-account fund-myself "$P" "$monto" autogenerate-new-keypair "$SAVE" sign-as "$FONDEO" network-config "$NET" sign-with-access-key-file "<la llave de $FONDEO leida de ${FONDEO_ARCHIVO:-SSM $FONDEO_SSM ($REGION)} por un pipe>" send
    echo "  (la implicita $FONDEO necesita $monto + $MARGEN_CREACION de gas libres: el dueno la fondea con $(presupuesto_fondeo) NEAR)"
    return 0
  fi
  existe "$FONDEO" || die "la cuenta de fondeo $FONDEO no existe en $RED (una implicita existe recien al recibir NEAR): la fondea el dueno con $(presupuesto_fondeo) NEAR"
  r=$(ver_cuenta "$FONDEO")
  disponible=$(python3 -I -c 'import sys; print(int(sys.argv[1]) - int(sys.argv[2]) * 10**19)' "$(jq -r .result.amount <<<"$r")" "$(jq -r .result.storage_usage <<<"$r")")
  necesario=$(yocto "$(suma_near "$monto" "$MARGEN_CREACION") NEAR")
  if [ "$(python3 -I -c 'import sys; print(int(int(sys.argv[1]) >= int(sys.argv[2])))' "$disponible" "$necesario")" != 1 ]; then
    die "$FONDEO tiene $(en_near "$disponible") libres y crear $P necesita $monto + $MARGEN_CREACION de gas"
  fi
  # Primero se valida la llave (a /dev/null): si no es la de $FONDEO, se aborta antes de llamar a near.
  llave_fondeo >/dev/null
  tx "padre $P (near.create_account desde $FONDEO)" account create-account fund-myself "$P" "$monto" autogenerate-new-keypair "$SAVE" sign-as "$FONDEO" network-config "$NET" sign-with-access-key-file <(llave_fondeo) send
  esperar_padre
  echo "OK: $P existe y la llave del llavero es FullAccess"
}

padre_dueno() {
  local dir pk f tmp abuelo
  abuelo=${P#*.}
  dir=$CRED/$NET/$P
  pk=""
  if [ -d "$dir" ]; then
    for f in "$dir"/ed25519_*.json; do
      [ -f "$f" ] && pk=$(jq -r '.public_key // empty' "$f") && break
    done
  fi
  if [ -z "$pk" ]; then
    if real; then
      tmp=$(mktemp -d "${TMPDIR:-/tmp}/arras-padre.XXXXXX")
      printf '\n+ near account create-account fund-later use-auto-generation save-to-folder %s\n' "$tmp"
      NO_COLOR=1 near account create-account fund-later use-auto-generation save-to-folder "$tmp" </dev/null 2>&1 | sanitizar
      f=$(find "$tmp" -name '*.json' -type f | head -1)
      [ -n "$f" ] || die "near no genero la llave del padre en $tmp"
      pk=$(jq -r '.public_key' "$f")
      mkdir -p "$dir"
      mv "$f" "$(archivo_llave "$P" "$pk")"
      rmdir "$tmp"
      echo "llave del padre: $(archivo_llave "$P" "$pk") (no se imprime)"
    else
      echo "[en seco] generaria la llave del padre en $dir/ con near account create-account fund-later use-auto-generation"
      pk="<llave publica nueva del padre>"
    fi
  else
    echo "ya hay una llave del padre en el llavero legacy: $pk"
  fi
  cat <<DUENO

El padre $P lo crea el DUENO (firma $abuelo, cuya llave tiene el; c0der no la toca), con la llave PUBLICA de arriba:
  near account create-account fund-myself $P '$(presupuesto_padre) NEAR' use-manually-provided-public-key $pk sign-as $abuelo network-config mainnet sign-with-keychain send
(o desde su wallet: crear la sub-cuenta $P con esa llave publica como FullAccess y ese saldo.)
DUENO
  if real; then
    padre_listo || die "$P todavia no existe: pasale el comando de arriba al dueno y volve a correr este paso"
    echo "OK: $P existe y la llave del llavero es FullAccess"
  fi
}

paso_padre() {
  case "$CASO_PADRE" in
    fondeo) padre_fondeo ;;
    dueno) padre_dueno ;;
    faucet) padre_faucet ;;
  esac
  anotar_saldos padre
}

crear() {
  if real && existe "$1"; then
    echo "ya existe: $1"
    return 0
  fi
  tx "cuenta $1" account create-account fund-myself "$1" "$2" autogenerate-new-keypair "$SAVE" sign-as "$P" network-config "$NET" "$SIGN" send
}

paso_cuentas() {
  if real; then
    existe "$P" || die "$P no existe: corre el paso padre"
    local r disponible necesario
    r=$(ver_cuenta "$P")
    disponible=$(python3 -I -c 'import sys; print(int(sys.argv[1]) - int(sys.argv[2]) * 10**19)' "$(jq -r .result.amount <<<"$r")" "$(jq -r .result.storage_usage <<<"$r")")
    necesario=$(yocto "$(presupuesto) NEAR")
    if ! existe "$C1" && [ "$(python3 -I -c 'import sys; print(int(int(sys.argv[1]) >= int(sys.argv[2])))' "$disponible" "$necesario")" != 1 ]; then
      die "$P tiene $(en_near "$disponible") libres y el despliegue necesita $(presupuesto) NEAR (paso plan)"
    fi
  fi
  crear "$C1" "$NEAR_V1"
  crear "$C2" "$NEAR_V2"
  crear "$OWNER" "$NEAR_OWNER"
  crear "$GUARD" "$NEAR_GUARD"
  crear "$CAPT" "$NEAR_CAPT"
  if [ "$FEES_NUEVA" = 1 ]; then
    crear "$FEES" "$NEAR_FEES"
  elif real; then
    existe "$FEES" || die "el fee_recipient $FEES no existe y no es hija de $P: creala antes o usa la de por defecto"
  fi
  anotar_saldos cuentas
}

registrada_usdc() {
  [ "$(vista "$USDC" storage_balance_of "$(jq -nc --arg a "$1" '{account_id:$a}')")" != null ]
}

paso_usdc() {
  local a
  for a in "$C1" "$FEES"; do
    if real && registrada_usdc "$a"; then
      echo "ya registrada en USDC: $a"
      continue
    fi
    tx "usdc $a" contract call-function as-transaction "$USDC" storage_deposit json-args "$(jq -nc --arg a "$a" '{account_id:$a,registration_only:true}')" prepaid-gas '30.0 Tgas' attached-deposit "$USDC_STORAGE" sign-as "$P" network-config "$NET" "$SIGN" send
  done
  anotar_saldos usdc
}

config_ok() {
  local c
  c=$(vista "$C1" get_config '{}')
  jq -e --arg o "$OWNER" --arg g "$GUARD" --arg t "$USDC" --arg p "$TOPE_POR_PAGO" --arg v "$TOPE_TVL_GLOBAL" \
    '.version == "arras-v1" and .owner == $o and .guardian == $g and .token == $t and .tope_por_pago == $p and .tope_tvl_global == $v' \
    >/dev/null <<<"$c" || die "get_config de $C1 no es el esperado: $c"
  echo "OK: get_config de $C1 = owner $OWNER, guardian $GUARD, token $USDC, topes $TOPE_POR_PAGO / $TOPE_TVL_GLOBAL"
}

paso_deploy() {
  wasm_ok
  if real; then
    local h _i
    h=$(codigo_hex "$C1")
    if [ "$h" = "$WASM_SHA" ]; then
      echo "ya desplegado: el codigo de $C1 es la wasm $WASM_SHA"
      config_ok
      return 0
    fi
    [ "$h" = "$(printf '0%.0s' {1..64})" ] || die "$C1 ya tiene otro codigo (sha256 $h): no se pisa"
  fi
  tx "deploy + new $C1" contract deploy "$C1" use-file "$WASM" with-init-call new json-args "$ARGS_NEW" prepaid-gas '30.0 Tgas' attached-deposit '0 NEAR' network-config "$NET" "$SIGN" send
  if real; then
    for _i in 1 2 3 4 5 6; do
      h=$(codigo_hex "$C1")
      [ "$h" = "$WASM_SHA" ] && break
      sleep "$ESPERA"
    done
    [ "$h" = "$WASM_SHA" ] || die "el codigo de $C1 tiene sha256 $h y la wasm $WASM_SHA"
    echo "OK: el codigo de $C1 es la wasm $WASM_SHA (rev $REV)"
    config_ok
  fi
  anotar_saldos deploy
}

operador_ok() {
  local o
  o=$(vista "$C1" get_operator "$(jq -nc --arg id "$OP" '{operator_id:$id}')")
  jq -e --arg c "$CAPT" --argjson f "$FEE_BPS" --arg r "$FEES" --arg m "$MIN_AMOUNT" --arg to "$TOPE_TVL_OPERADOR" --arg tp "$TOPE_TVL_POR_PAYER" --arg s "$SPONSOR_MAX" \
    '.capturers == [$c] and .fee_bps == $f and .fee_recipient == $r and .min_amount == $m and .tope_tvl_operador == $to and .tope_tvl_por_payer == $tp and .sponsor_max_por_payer == $s' \
    >/dev/null <<<"$o" || die "get_operator $OP de $C1 no es el esperado: $o"
  printf '%s' "$o"
}

paso_operador() {
  local o=null
  real && o=$(vista "$C1" get_operator "$(jq -nc --arg id "$OP" '{operator_id:$id}')")
  if [ "$o" = null ]; then
    tx "register_operator $OP" contract call-function as-transaction "$C1" register_operator json-args "$ARGS_OPERADOR" prepaid-gas '30.0 Tgas' attached-deposit '1 yoctoNEAR' sign-as "$OWNER" network-config "$NET" "$SIGN" send
  else
    echo "ya registrado: operador $OP"
  fi
  real && o=$(operador_ok)
  # fund_operator_storage lo puede firmar cualquiera: lo firma el padre (el owner queda con poco NEAR a proposito).
  if real && [ "$(jq -r .sponsor_pool <<<"$o")" != 0 ]; then
    echo "ya fondeado: sponsor_pool $(jq -r .sponsor_pool <<<"$o") yocto"
  else
    tx "fund_operator_storage $OP" contract call-function as-transaction "$C1" fund_operator_storage json-args "$(jq -nc --arg id "$OP" '{operator_id:$id}')" prepaid-gas '30.0 Tgas' attached-deposit "$POOL" sign-as "$P" network-config "$NET" "$SIGN" send
  fi
  if real; then
    o=$(operador_ok)
    echo "OK: operador $OP = $o"
  fi
  anotar_saldos operador
}

# La FunctionCall key del capturer para v1 (public_key), si ya esta.
llave_fc() {
  llaves_json "$CAPT" | jq -r --arg c "$C1" '[.[] | select((.access_key.permission | objects | .FunctionCall.receiver_id) == $c)][0].public_key // empty'
}

# El capturer libera y reembolsa en mainnet: tiene que quedar con UNA sola llave, la FunctionCall de v1 con release,
# refund y withdraw_for (ni una FullAccess ni otra FunctionCall de mas).
capturer_ok() {
  local l
  l=$(llaves_json "$CAPT")
  jq -e --arg c "$C1" 'length == 1 and ((.[0].access_key.permission | objects | .FunctionCall) as $f | $f != null and $f.receiver_id == $c and ($f.method_names | sort) == ["refund","release","withdraw_for"])' >/dev/null <<<"$l" ||
    die "$CAPT tiene que quedar con UNA sola llave, la FunctionCall de $C1 (release, refund, withdraw_for); tiene: $(jq -c '[.[] | {public_key, permission: (.access_key.permission | if type == "object" then {receiver: .FunctionCall.receiver_id, methods: .FunctionCall.method_names} else . end)}]' <<<"$l")"
}

paso_capturer() {
  local fc full k
  fc=""
  real && fc=$(llave_fc)
  if [ -z "$fc" ]; then
    tx "capturer: FunctionCall key" account add-key "$CAPT" grant-function-call-access --allowance "$ALLOWANCE" --contract-account-id "$C1" --function-names 'release, refund, withdraw_for' autogenerate-new-keypair "$SAVE" network-config "$NET" "$SIGN" send
  else
    echo "ya tiene FunctionCall key para $C1: $fc"
  fi
  if real; then
    fc=$(llave_fc)
    [ -n "$fc" ] || die "$CAPT no quedo con una FunctionCall key para $C1"
    llaves_json "$CAPT" | jq -e --arg k "$fc" '.[] | select(.public_key == $k) | .access_key.permission.FunctionCall.method_names | sort == ["refund","release","withdraw_for"]' >/dev/null ||
      die "la FunctionCall key $fc de $CAPT no tiene exactamente release, refund y withdraw_for"
    llaves_json "$CAPT" | jq -e --arg k "$fc" --arg a "$(yocto "$ALLOWANCE")" '.[] | select(.public_key == $k) | .access_key.permission.FunctionCall.allowance == $a' >/dev/null ||
      die "la FunctionCall key $fc de $CAPT no tiene allowance $ALLOWANCE: no se borra la FullAccess"
    [ -f "$(archivo_llave "$CAPT" "$fc")" ] || die "la FunctionCall key $fc no esta en el llavero legacy ($(archivo_llave "$CAPT" "$fc"))"
    full=$(llaves_full "$CAPT")
    for k in $full; do
      tx "capturer: borra FullAccess $k" account delete-keys "$CAPT" public-keys "$k" network-config "$NET" "$SIGN" send
    done
    [ -z "$(llaves_full "$CAPT")" ] || die "$CAPT todavia tiene una FullAccess"
    capturer_ok
    echo "OK: $CAPT solo tiene la FunctionCall key $fc (release, refund, withdraw_for en $C1, allowance $ALLOWANCE)"
  else
    tx "capturer: borra FullAccess" account delete-keys "$CAPT" public-keys '<la FullAccess de creacion>' network-config "$NET" "$SIGN" send
  fi
  anotar_saldos capturer
}

paso_sin_llaves() {
  local todas n h
  if real; then
    # Sin llaves ya no hay vuelta atras (con una llave, un deploy malo se arregla con delete-account): primero el codigo
    # tiene que ser el del pin y get_config el esperado.
    h=$(codigo_hex "$C1")
    [ "$h" = "$WASM_SHA" ] || die "$C1 tiene codigo sha256 $h y no la wasm $WASM_SHA: no se borran sus llaves (paso deploy)"
    config_ok
    todas=$(llaves_json "$C1" | jq -r '[.[].public_key] | join(",")')
    if [ -n "$todas" ]; then
      tx "sin-llaves $C1" account delete-keys "$C1" public-keys "$todas" network-config "$NET" "$SIGN" send
    else
      echo "ya sin llaves: $C1"
    fi
    n=$(n_llaves "$C1")
    [ "$n" = 0 ] || die "a $C1 le quedan $n llaves: view_access_key_list no esta vacio"
    echo "OK: view_access_key_list de $C1 vacio (nadie puede redesplegar ni tocar la cuenta del contrato)"
  else
    tx "sin-llaves $C1" account delete-keys "$C1" public-keys '<todas las de view_access_key_list, separadas por coma>' network-config "$NET" "$SIGN" send
  fi
  anotar_saldos sin-llaves
}

paso_verificar() {
  wasm_ok
  if ! real; then
    echo "[en seco] lee por RPC: code_hash y view_access_key_list de $C1, get_config, get_operator $OP y get_storage_health"
    return 0
  fi
  local h n
  h=$(codigo_hex "$C1")
  n=$(n_llaves "$C1")
  echo "codigo de $C1: sha256 $h ($( [ "$h" = "$WASM_SHA" ] && echo "= pin" || echo "DISTINTO del pin $WASM_SHA"))"
  echo "llaves de $C1: $n"
  config_ok
  echo "get_operator $OP: $(operador_ok)"
  capturer_ok
  echo "capturer $CAPT: una sola llave, la FunctionCall de $C1"
  echo "get_storage_health: $(vista "$C1" get_storage_health '{}')"
  [ "$h" = "$WASM_SHA" ] && [ "$n" = 0 ] || die "$C1 no esta listo (codigo o llaves)"
}

# --- ensayo en testnet ------------------------------------------------------------------------------------------------
PAYER=${ARRAS_PAYER:-payer-demo.arras-dev.testnet}
PAYER_KC=${ARRAS_PAYER_KEYCHAIN:-keychain}
WORKER=${ARRAS_WORKER:-worker-demo.arras-dev.testnet}
MONTO=${ARRAS_MONTO:-1000000}

ensayo_ok() {
  solo_testnet "$1"
  de_la_red "$PAYER" || die "ARRAS_PAYER='$PAYER' no es de testnet"
  de_la_red "$WORKER" || die "ARRAS_WORKER='$WORKER' no es de testnet"
  [ "$WORKER" != "$PAYER" ] && [ "$WORKER" != "$C1" ] || die "el worker no puede ser el payer ni el contrato (ARRAS_E15)"
  case "$PAYER_KC" in keychain | legacy-keychain) ;; *) die "ARRAS_PAYER_KEYCHAIN tiene que ser keychain o legacy-keychain" ;; esac
  entero ARRAS_MONTO "$MONTO"
  [ "$MONTO" -ge "$MIN_AMOUNT" ] && [ "$MONTO" -le "$TOPE_POR_PAGO" ] || die "ARRAS_MONTO=$MONTO fuera de [min_amount, tope_por_pago]"
}

# Firma con la FunctionCall key del capturer (near-cli 0.30.1 en linea solo elige FullAccess del llavero; con el
# archivo de la llave firma con esa y lee el nonce de la cadena).
capturer_firma() {
  local fc
  fc=$(llave_fc)
  [ -n "$fc" ] || die "$CAPT no tiene FunctionCall key para $C1 (paso capturer)"
  printf '%s' "$(archivo_llave "$CAPT" "$fc")"
}

paso_authorize() {
  ensayo_ok authorize
  local now salt msg id disponible r p
  now=$(date +%s)
  salt=$(openssl rand -hex 16)
  msg=$(printf '{"v":1,"action":"authorize","operator_id":"%s","receiver":"%s","release_deadline":"%s","reclaim_after":"%s","fee_bps_max":%s,"terms_hash":"%s","salt":"%s"}' \
    "$OP" "$WORKER" "$(((now + 86400) * 1000000000))" "$(((now + 172800) * 1000000000))" "$FEE_BPS" "$(printf '0%.0s' {1..64})" "$salt")
  if real; then
    mkdir -p "$EST"
    printf '%s' "$msg" >"$EST/msg.json"
    [ "$(vista "$USDC" ft_balance_of "$(jq -nc --arg a "$PAYER" '{account_id:$a}')" | tr -d '"')" -ge "$MONTO" ] ||
      die "$PAYER no tiene $MONTO de USDC de testnet (https://faucet.circle.com)"
    # El authorize reserva 100 TGas al precio pesimista (~0,1 NEAR): el payer necesita ese NEAR libre.
    r=$(ver_cuenta "$PAYER")
    disponible=$(python3 -I -c 'import sys; print(int(sys.argv[1]) - int(sys.argv[2]) * 10**19)' "$(jq -r .result.amount <<<"$r")" "$(jq -r .result.storage_usage <<<"$r")")
    if [ "$(python3 -I -c 'import sys; print(int(int(sys.argv[1]) < 15 * 10**22))' "$disponible")" = 1 ]; then
      tx "0,3 NEAR al payer" tokens "$P" send-near "$PAYER" '0.3 NEAR' network-config "$NET" "$SIGN" send
    fi
  fi
  tx "authorize" contract call-function as-transaction "$USDC" ft_transfer_call json-args "$(jq -nc --arg r "$C1" --arg a "$MONTO" --arg m "$msg" '{receiver_id:$r,amount:$a,msg:$m}')" prepaid-gas '100.0 Tgas' attached-deposit '1 yoctoNEAR' sign-as "$PAYER" network-config "$NET" "sign-with-$PAYER_KC" send
  if real; then
    id=$(vista "$C1" compute_payment_id "$(jq -nc --arg s "$PAYER" --arg a "$MONTO" --arg m "$msg" '{sender_id:$s,amount:$a,msg:$m}')" | tr -d '"')
    p=$(vista "$C1" get_payment "$(jq -nc --arg id "$id" '{payment_id:$id}')")
    jq -e '.status == "Authorized"' >/dev/null <<<"$p" || die "el authorize no creo el pago $id (rechazo: mira el log arras:rechazo de la tx): $p"
    printf '%s' "$id" >"$EST/payment_id"
    echo "OK: pago $id Authorized"
  fi
  anotar_saldos authorize
}

paso_release() {
  ensayo_ok release
  local id='<payment_id del paso authorize>' firma='<archivo de la FunctionCall key del capturer>' p
  if real; then
    id=$(cat "$EST/payment_id")
    firma=$(capturer_firma)
  fi
  tx "release" contract call-function as-transaction "$C1" release json-args "$(jq -nc --arg id "$id" '{payment_id:$id}')" prepaid-gas '30.0 Tgas' attached-deposit '0 NEAR' sign-as "$CAPT" network-config "$NET" sign-with-access-key-file "$firma" send
  if real; then
    p=$(vista "$C1" get_payment "$(jq -nc --arg id "$id" '{payment_id:$id}')")
    jq -e '.status == "Released"' >/dev/null <<<"$p" || die "el pago $id no quedo Released: $p"
    echo "OK: pago $id Released; claimable worker $(vista "$C1" get_claimable "$(jq -nc --arg a "$WORKER" '{account:$a}')"), fee_recipient $(vista "$C1" get_claimable "$(jq -nc --arg a "$FEES" '{account:$a}')")"
  fi
  anotar_saldos release
}

paso_withdraw() {
  ensayo_ok withdraw
  local firma='<archivo de la FunctionCall key del capturer>' a c
  real && firma=$(capturer_firma)
  for a in "$WORKER" "$FEES"; do
    tx "withdraw_for $a" contract call-function as-transaction "$C1" withdraw_for json-args "$(jq -nc --arg a "$a" '{account:$a}')" prepaid-gas '50.0 Tgas' attached-deposit '0 NEAR' sign-as "$CAPT" network-config "$NET" sign-with-access-key-file "$firma" send
    if real; then
      c=$(vista "$C1" get_claimable "$(jq -nc --arg a "$a" '{account:$a}')" | tr -d '"')
      [ "$c" = 0 ] || die "a $a le quedo claimable $c despues del withdraw_for (el ft_transfer fallo?)"
      echo "OK: $a cobro; USDC $(vista "$USDC" ft_balance_of "$(jq -nc --arg a "$a" '{account_id:$a}')")"
    fi
  done
  if real; then
    echo "get_config: $(vista "$C1" get_config '{}')"
    echo "USDC del contrato: $(vista "$USDC" ft_balance_of "$(jq -nc --arg a "$C1" '{account_id:$a}')")"
  fi
  anotar_saldos withdraw
}

# --- medir, secretos, enlaces -----------------------------------------------------------------------------------------
paso_medir() {
  if ! real; then
    echo "[en seco] lee por RPC el saldo y el storage de $(cuentas_todas | tr '\n' ' ')y $SALDOS"
    return 0
  fi
  local a r
  printf '%-48s %18s %10s %18s\n' cuenta saldo storage 'storage en NEAR'
  for a in $(cuentas_todas); do
    r=$(ver_cuenta "$a")
    jq -e '.result.amount' >/dev/null 2>&1 <<<"$r" || { printf '%-48s %18s\n' "$a" '(no existe)'; continue; }
    printf '%-48s %18s %10s %18s\n' "$a" "$(en_near "$(jq -r .result.amount <<<"$r")")" "$(jq -r .result.storage_usage <<<"$r")" \
      "$(en_near "$(python3 -I -c 'import sys; print(int(sys.argv[1]) * 10**19)' "$(jq -r .result.storage_usage <<<"$r")")")"
  done
  existe "$C1" && [ "$(codigo_hex "$C1")" = "$WASM_SHA" ] && echo "get_storage_health de $C1: $(vista "$C1" get_storage_health '{}')"
  if [ -f "$SALDOS" ]; then
    echo
    echo "saldo del padre${FONDEO:+ y de la de fondeo} despues de cada paso ($SALDOS):"
    awk -F'\t' -v p="$P" -v f="$FONDEO" '$3 == p || ($3 == f && f != "") { print "  " $2 "\t" $3 "\t" $4 }' "$SALDOS"
  fi
}

paso_secretos() {
  local modo=${ARRAS_SECRETOS:-ssm} prefijo kms region
  prefijo=${ARRAS_SECRETOS_PREFIJO:-/arras/$RED}
  kms=${ARRAS_KMS_KEY:-<ARRAS_KMS_KEY: la llave KMS propia de Arras (decision 230)>}
  region=$REGION
  case "$modo" in ssm | secretsmanager) ;; *) die "ARRAS_SECRETOS tiene que ser ssm o secretsmanager" ;; esac
  local rol cuenta f pk nombre vivas estado copia pk_copia copia_hecha subir verificar borrar
  if ! real; then
    # En seco no sale NINGUNA linea ejecutable: sin la cadena no se sabe que llave sigue viva, y un rm pegado despues
    # de un fallo a mitad (v1 con su llave todavia en cadena) no tiene vuelta atras.
    echo "EN SECO: solo las llaves que hay en $CRED/$NET/ y el nombre que llevarian en $modo; ningun comando."
    echo "Los comandos (subir, comparar, borrar) los da el modo real, que mira cada llave en cadena: scripts/mainnet.sh --$RED-ok secretos"
    while read -r rol cuenta; do
      [ -n "$rol" ] || continue
      if [ "$rol" = em-fees ] && [ "$FEES_NUEVA" != 1 ]; then continue; fi
      nombre=$prefijo/$rol
      [ "$rol" = em-capturer ] && nombre=$prefijo/em-capturer-v1
      [ "$rol" = v1 ] && nombre="(ninguno: se borran en cadena en sin-llaves)"
      for f in "$CRED/$NET/$cuenta"/ed25519_*.json "$CRED/$NET/$cuenta.json"; do
        [ -f "$f" ] || continue
        echo "  $rol ($cuenta): $(jq -r '.public_key // empty' "$f") en $f; nombre: $nombre"
      done
    done <<ROLES
padre $P
owner $OWNER
guardian $GUARD
v2-reserva $C2
em-capturer $CAPT
em-fees $FEES
v1 $C1
ROLES
    return 0
  fi
  echo "Comandos para c0der (este script NO los corre y nunca imprime una llave). Cada llave VIVA es UNA linea:"
  echo "sube a $modo por NOMBRE && compara sha256 sin mostrar nada && recien entonces borra el archivo (y la copia"
  echo "<cuenta>.json de near-cli si es la misma llave). Si la subida o la comparacion fallan, no se borra nada."
  echo "Una llave que ya no esta en cadena (la FullAccess de creacion del capturer, las de v1) solo se borra."
  while read -r rol cuenta; do
    [ -n "$rol" ] || continue
    if [ "$rol" = em-fees ] && [ "$FEES_NUEVA" != 1 ]; then continue; fi
    nombre=$prefijo/$rol
    [ "$rol" = em-capturer ] && nombre=$prefijo/em-capturer-v1
    if ! existe "$cuenta"; then
      echo
      echo "# $rol: $cuenta no existe en cadena: sus archivos no se tocan"
      continue
    fi
    vivas=$(llaves_json "$cuenta" | jq -r '.[].public_key')
    copia=$CRED/$NET/$cuenta.json
    pk_copia=""
    [ -f "$copia" ] && pk_copia=$(jq -r '.public_key // empty' "$copia")
    copia_hecha=0
    for f in "$CRED/$NET/$cuenta"/ed25519_*.json; do
      if [ ! -f "$f" ]; then
        echo
        echo "# $rol: $cuenta, sin llave en $CRED/$NET/$cuenta/"
        continue
      fi
      pk=$(jq -r '.public_key // empty' "$f")
      if printf '%s\n' "$vivas" | grep -qxF -- "$pk"; then
        estado=viva
      else
        estado=muerta
      fi
      borrar="'$f'"
      if [ -n "$pk_copia" ] && [ "$pk_copia" = "$pk" ]; then
        borrar="$borrar '$copia'"
        copia_hecha=1
      fi
      echo
      echo "# $rol: $cuenta, llave publica $pk ($estado)"
      if [ "$estado" = muerta ]; then
        echo "rm $borrar   # ya no esta en view_access_key_list de $cuenta"
        continue
      fi
      if [ "$modo" = ssm ]; then
        subir="aws ssm put-parameter --region $region --name '$nombre' --type SecureString --key-id '$kms' --value 'file://$f' --description 'Arras $RED $rol $cuenta $pk'"
        verificar="[ \"\$(aws ssm get-parameter --region $region --name '$nombre' --with-decryption --output json | jq -j .Parameter.Value | shasum -a 256)\" = \"\$(shasum -a 256 < '$f')\" ]"
      else
        subir="aws secretsmanager create-secret --region $region --name '${nombre#/}' --kms-key-id '$kms' --secret-string 'file://$f' --description 'Arras $RED $rol $cuenta $pk'"
        verificar="[ \"\$(aws secretsmanager get-secret-value --region $region --secret-id '${nombre#/}' --output json | jq -j .SecretString | shasum -a 256)\" = \"\$(shasum -a 256 < '$f')\" ]"
      fi
      echo "$subir && $verificar && rm $borrar && echo 'OK $rol en $nombre'"
    done
    if [ -n "$pk_copia" ] && [ "$copia_hecha" = 0 ]; then
      if ! printf '%s\n' "$vivas" | grep -qxF -- "$pk_copia"; then
        echo "rm '$copia'   # copia de near-cli con la llave $pk_copia, que ya no esta en cadena"
      else
        echo "# $copia tiene la llave $pk_copia, viva y sin archivo en $CRED/$NET/$cuenta/: revisarla a mano antes de borrarla"
      fi
    fi
  done <<ROLES
padre $P
owner $OWNER
guardian $GUARD
v2-reserva $C2
em-capturer $CAPT
em-fees $FEES
ROLES
  echo
  # v1: sus archivos se borran solo si v1 existe y ya no tiene ninguna llave en cadena.
  if real && existe "$C1" && [ "$(n_llaves "$C1")" != 0 ]; then
    echo "# OJO: $C1 todavia tiene llaves en cadena: corre el paso sin-llaves; sus archivos no se tocan."
  elif ! existe "$C1"; then
    echo "# v1: $C1 no existe en cadena: sus archivos no se tocan."
  else
    echo "# v1 ($C1): sus llaves se borraron en cadena (view_access_key_list vacio); los archivos solo se borran del disco."
    for f in "$CRED/$NET/$C1"/ed25519_*.json "$CRED/$NET/$C1.json"; do
      if [ -f "$f" ]; then
        echo "rm '$f'"
      fi
    done
  fi
  echo "# (rm comun: en APFS sobrescribir no garantiza nada; el disco lo protege FileVault.)"
}

paso_enlaces() {
  local a
  echo "Cuentas:"
  for a in $(cuentas_todas); do
    echo "  $NB/address/$a"
  done
  echo "Codigo verificado del contrato: $NB/address/$C1?tab=contract (sha256 $WASM_SHA, rev $REV)"
  if [ -f "$TXS" ]; then
    echo "Transacciones ($TXS):"
    awk -F'\t' -v nb="$NB" '{ printf "  %-40s %s/txns/%s\n", $2, nb, $3 }' "$TXS"
  fi
  echo "Facilitador (H-5): NEAR_ESCROW_CONTRACTS_$(printf '%s' "$RED" | tr '[:lower:]' '[:upper:]')=$C1,$C2"
}

# --- principal --------------------------------------------------------------------------------------------------------
# Todo lo que tarda corre dentro de `principal`, que bash ya leyo entera antes de empezar; el `exit` va en la misma linea.
# Asi, si el archivo cambia mientras corre (un git pull o un merge en medio de un deploy de minutos), bash no sigue leyendo
# lineas nuevas desde un offset viejo.
principal() {
  command -v near >/dev/null 2>&1 || PATH=$HOME/.cargo/bin:$PATH
  for herramienta in jq shasum python3; do
    command -v "$herramienta" >/dev/null 2>&1 || die "falta $herramienta"
  done
  if real; then
    command -v near >/dev/null 2>&1 || die "falta near (near-cli-rs 0.30.1)"
    v=$(NO_COLOR=1 near --version 2>/dev/null | awk '/near-cli-rs/ { print $2 }' | tail -1)
    [ "$v" = 0.30.1 ] || die "near-cli-rs es '${v:-?}' y la sintaxis de este script es la de 0.30.1"
    cadena=$(curl -sf -m 30 -X POST "$RPC" -H 'Content-Type: application/json' -d '{"jsonrpc":"2.0","id":"arras","method":"status","params":[]}' | jq -r '.result.chain_id // empty') ||
      die "el RPC $RPC no respondio a status"
    [ "$cadena" = "$RED" ] || die "ARRAS_RPC=$RPC es de la red '${cadena:-?}', no de $RED"
    [ -n "${ARRAS_KMS_KEY:-}" ] || die "ARRAS_KMS_KEY es obligatoria en el modo real: la llave KMS propia de Arras (decision 230) con la que van las llaves a SSM"
    mkdir -p "$EST"
  fi
  echo "== Arras $RED, modo $MODO, padre $P, paso $PASO"
  [ "$MODO" = seco ] && echo "== EN SECO: nada se ejecuta. Modo real: --$RED-ok"

  case "$PASO" in
    plan) paso_plan ;;
    wasm) paso_wasm ;;
    1 | padre) paso_padre ;;
    2 | cuentas) paso_cuentas ;;
    3 | usdc) paso_usdc ;;
    4 | deploy) paso_deploy ;;
    5 | operador) paso_operador ;;
    6 | capturer) paso_capturer ;;
    7 | sin-llaves) paso_sin_llaves ;;
    8 | verificar) paso_verificar ;;
    9 | authorize) paso_authorize ;;
    10 | release) paso_release ;;
    11 | withdraw) paso_withdraw ;;
    medir) paso_medir ;;
    secretos) paso_secretos ;;
    enlaces) paso_enlaces ;;
    todo)
      paso_plan; paso_padre; paso_cuentas; paso_usdc; paso_deploy; paso_operador; paso_capturer; paso_sin_llaves
      paso_verificar; paso_secretos; paso_enlaces ;;
    ensayo)
      solo_testnet ensayo
      ensayo_ok ensayo
      paso_plan; paso_padre; paso_cuentas; paso_usdc; paso_deploy; paso_operador; paso_capturer; paso_sin_llaves
      paso_verificar; paso_authorize; paso_release; paso_withdraw; paso_verificar; paso_medir; paso_enlaces ;;
    *) echo "paso desconocido: $PASO" >&2; uso ;;
  esac
}
principal; exit $?
