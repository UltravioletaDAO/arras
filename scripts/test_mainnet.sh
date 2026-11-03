#!/usr/bin/env bash
# Tests de scripts/mainnet.sh sin red y sin llaves: `near` y `curl` son dobles (un near que anota y un RPC de mentira
# con respuestas de archivo), HOME es temporal y el proxy apunta a un puerto cerrado. Ninguno firma ni consulta nada
# real. Uso: scripts/test_mainnet.sh (sale 1 si falla alguno).
set -uo pipefail
# Corre la suite con cada bash que haya: el /bin/bash 3.2 de macOS y el de Homebrew (5.x).
if [ -z "${ARRAS_TEST_SH:-}" ]; then
  rc=0
  for sh in /bin/bash /opt/homebrew/bin/bash /usr/local/bin/bash; do
    [ -x "$sh" ] || continue
    echo "== $sh ($("$sh" -c 'echo $BASH_VERSION'))"
    ARRAS_TEST_SH=$sh "$sh" "$0" || rc=1
  done
  exit "$rc"
fi
SH=$ARRAS_TEST_SH
RAIZ=$(cd "$(dirname "$0")/.." && pwd -P)
SCRIPT=$RAIZ/scripts/mainnet.sh
TMP=$(mktemp -d "${TMPDIR:-/tmp}/arras-test-mainnet.XXXXXX")
trap 'rm -rf "$TMP"' EXIT
FALLOS=0
PASAN=0
P_MAIN=arras-escrow.near
P_DUENO=arras.ultravioletadao.near
FONDEO=fe0a574258f8277099875e46d4ca0043cdb6274ff2501b375a0d3d64bc211079
USDC_MAIN=17208628f84f5d6ad33f0da3bbbeb27ffcb398eac501a31bd6ad2011e36133a1
USDC_TEST=3e2210e1184b45b64c8a434c0a7e7b23cc04ea7eb7a6c3c32520d03d4afcb8af
# get_config de mentira: owner guardian tope_tvl_global.
config_falsa() { jq -nc --arg o "$1" --arg g "$2" --arg t "$USDC_MAIN" --arg v "$3" '{version:"arras-v1",owner:$o,guardian:$g,token:$t,tope_por_pago:"10000000",tope_tvl_global:$v}'; }
P_TEST=arras-ensayo.testnet
REV=8da71748b428ceccf9fccd358e61bb4213d54e29
PRIV_FALSA="ed25519:$(printf '3%.0s' {1..88})"

# --- dobles ---------------------------------------------------------------------------------------------------------
mkdir -p "$TMP/bin"
cat >"$TMP/bin/near" <<'NEAR'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$STUB/near.log"
case " $* " in *" --version "*) echo "near-cli-rs ${STUB_NEAR_VERSION:-0.30.1}"; exit 0 ;; esac
[ -f "$STUB/near.salida" ] && cat "$STUB/near.salida"
[ -n "${STUB_NEAR_SLEEP:-}" ] && sleep "$STUB_NEAR_SLEEP"
# Como near: lee el archivo de llave que le pasan (para ver que llega por el pipe y que es el correcto).
prev=""
for a in "$@"; do
  [ "$prev" = sign-with-access-key-file ] && [ -r "$a" ] && cat "$a" >"$STUB/firma.json"
  prev=$a
done
n=$(wc -l <"$STUB/near.log" | tr -d ' ')
echo "Transaction ID: STUBTX$n"
exit "${STUB_NEAR_RC:-0}"
NEAR
cat >"$TMP/bin/aws" <<'AWS'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$STUB/aws.log"
n=$(wc -l <"$STUB/aws.log" | tr -d ' ')
if [ -f "$STUB/aws.salida.$n" ]; then cat "$STUB/aws.salida.$n"; exit 0; fi
[ -f "$STUB/aws.salida" ] || exit 255
cat "$STUB/aws.salida"
AWS
cat >"$TMP/bin/curl" <<'CURL'
#!/usr/bin/env bash
exec python3 -I "$STUB_PY" "$@"
CURL
cat >"$TMP/rpc.py" <<'PY'
import json, os, sys
stub = os.environ["STUB"]
args = sys.argv[1:]
d = args[args.index("-d") + 1]
req = json.loads(d)
if req.get("method") == "status":
    f = os.path.join(stub, "rpc", "status")
    chain = json.load(open(f))["chain_id"] if os.path.exists(f) else os.environ.get("STUB_CHAIN", "mainnet")
    print(json.dumps({"jsonrpc": "2.0", "id": "arras", "result": {"chain_id": chain}}))
    sys.exit(0)
p = req["params"]
rt, acct, m = p["request_type"], p.get("account_id", ""), p.get("method_name", "")
with open(os.path.join(stub, "curl.log"), "a") as f:
    f.write(f"{rt} {acct} {m}\n")
base = f"{rt}.{acct}.{m}" if m else f"{rt}.{acct}"
cpath = os.path.join(stub, "count." + base)
n = int(open(cpath).read()) + 1 if os.path.exists(cpath) else 1
open(cpath, "w").write(str(n))
rpc = os.path.join(stub, "rpc")
for k in (f"{base}.{n}", base, f"{rt}.{m}" if m else None, rt):
    if k and os.path.exists(os.path.join(rpc, k)):
        body = open(os.path.join(rpc, k)).read()
        break
else:
    body = None
if body is not None and body.strip() == "UNKNOWN":
    body = None
if body is None:
    out = {"jsonrpc": "2.0", "id": "arras", "error": {"cause": {"name": "UNKNOWN_ACCOUNT"}, "name": "HANDLER_ERROR"}}
elif rt == "call_function":
    out = {"jsonrpc": "2.0", "id": "arras", "result": {"result": list(body.strip().encode())}}
else:
    out = {"jsonrpc": "2.0", "id": "arras", "result": json.loads(body)}
print(json.dumps(out))
PY
chmod +x "$TMP/bin/near" "$TMP/bin/curl" "$TMP/bin/aws"

# Cada caso: HOME, estado y dobles nuevos.
nuevo() {
  CASO=$1
  STUB=$TMP/c-$CASO
  rm -rf "$STUB"
  mkdir -p "$STUB/rpc" "$STUB/home"
  unset STUB_NEAR_VERSION STUB_NEAR_RC STUB_NEAR_SLEEP
  CORRE=$SCRIPT
}
correr() {
  local chain=mainnet a
  for a in "$@"; do [ "$a" = ARRAS_RED=testnet ] && chain=testnet; done
  env -i STUB_CHAIN=$chain ARRAS_ESPERA=0 PATH="$TMP/bin:/usr/bin:/bin:/opt/homebrew/bin:/usr/local/bin" HOME="$STUB/home" TMPDIR="$STUB" STUB="$STUB" \
    STUB_PY="$TMP/rpc.py" HTTPS_PROXY=http://127.0.0.1:9 HTTP_PROXY=http://127.0.0.1:9 NO_PROXY=127.0.0.1,localhost \
    AWS_SHARED_CREDENTIALS_FILE=/dev/null ARRAS_KMS_KEY=alias/arras-test ${STUB_NEAR_VERSION:+STUB_NEAR_VERSION=$STUB_NEAR_VERSION} \
    ${STUB_NEAR_RC:+STUB_NEAR_RC=$STUB_NEAR_RC} ${STUB_NEAR_SLEEP:+STUB_NEAR_SLEEP=$STUB_NEAR_SLEEP} "$@" "$SH" "$CORRE" ${ARGS[@]+"${ARGS[@]}"} >"$STUB/out" 2>&1
  RC=$?
  return "$RC"
}
ok() { PASAN=$((PASAN + 1)); }
mal() { FALLOS=$((FALLOS + 1)); printf 'FALLA %s: %s\n' "$CASO" "$*"; sed 's/^/    | /' "$STUB/out" | tail -n 12; }
sale() { if [ "$RC" -eq "$1" ]; then ok; else mal "exit $RC, se esperaba $1"; fi; }
sale_mal() { if [ "$RC" -ne 0 ]; then ok; else mal "exit 0, se esperaba error"; fi; }
dice() { if grep -qF -- "$1" "$STUB/out"; then ok; else mal "falta '$1' en la salida"; fi; }
no_dice() { if grep -qF -- "$1" "$STUB/out"; then mal "la salida trae '$1'"; else ok; fi; }
sin_near_tx() { if [ -f "$STUB/near.log" ] && grep -qv -- '--version' "$STUB/near.log"; then mal "near se llamo: $(grep -v -- '--version' "$STUB/near.log" | head -1)"; else ok; fi; }
sin_red() { if [ -f "$STUB/curl.log" ] || [ -f "$STUB/near.log" ] || [ -f "$STUB/aws.log" ]; then mal "hubo llamadas a near o al RPC en seco"; else ok; fi; }
near_hizo() { if [ -f "$STUB/near.log" ] && grep -qF -- "$1" "$STUB/near.log"; then ok; else mal "near no recibio '$1'"; fi; }
b58() { python3 -I -c '
import sys
a = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"
n = int(sys.argv[1], 16); s = ""
while n: n, r = divmod(n, 58); s = a[r] + s
print(s or "1")' "$1"; }
# v1 de testnet desplegado con la wasm del pin y la config por defecto (lo que sin-llaves exige antes de borrar).
v1_listo_testnet() {
  printf '{"amount":"1","storage_usage":1,"code_hash":"%s"}' "$(b58 7058319dff5f8d1e9da91a344ab3c699b709783e4153f3a7da6c2556cde0eada)" >"$STUB/rpc/view_account.v1.$P_TEST"
  jq -nc --arg o "owner.$P_TEST" --arg g "guardian.$P_TEST" --arg t "$USDC_TEST" '{version:"arras-v1",owner:$o,guardian:$g,token:$t,tope_por_pago:"10000000",tope_tvl_global:"1000000000"}' >"$STUB/rpc/call_function.v1.$P_TEST.get_config"
}
wasm_falsa() { printf 'wasm de prueba source_code_snapshot git+https://github.com/UltravioletaDAO/arras?rev=%s fin' "${2:-$REV}" >"$1"; }

# --- casos ----------------------------------------------------------------------------------------------------------
# T01 en seco por defecto: todo sale bien, no llama a near ni al RPC y muestra los comandos con los topes de P-6.
nuevo t01; ARGS=(todo); correr
sale 0; sin_red; dice "EN SECO"; dice "[en seco: no se ejecuta]"
dice "contract deploy v1.$P_MAIN use-file"; dice '"tope_por_pago":"10000000","tope_tvl_global":"1000000000"'
dice '"tope_tvl_operador":"1000000000","tope_tvl_por_payer":"20000000"'; dice "sign-as $FONDEO network-config mainnet sign-with-access-key-file"
dice "delete-keys v1.$P_MAIN public-keys"; dice "NEAR_ESCROW_CONTRACTS_MAINNET=v1.$P_MAIN,v2.$P_MAIN"
dice "grant-function-call-access --allowance 0.5 NEAR --contract-account-id v1.$P_MAIN --function-names release, refund, withdraw_for"

# T02 la bandera de testnet no vale en mainnet (y no toca nada).
nuevo t02; ARGS=(--testnet-ok todo); correr
sale_mal; sin_red; dice "no vale con ARRAS_RED=mainnet"
# T03 la bandera de mainnet no vale en testnet.
nuevo t03; ARGS=(--mainnet-ok todo); correr ARRAS_RED=testnet ARRAS_PADRE=$P_TEST
sale_mal; sin_red; dice "no vale con ARRAS_RED=testnet"
# T04 las dos banderas juntas.
nuevo t04; ARGS=(--mainnet-ok --testnet-ok plan); correr
sale_mal; sin_red; dice "juntas"
# T05 la red se normaliza a minusculas; con espacios no.
nuevo t05; ARGS=(plan); correr ARRAS_RED=MAINNET
sale 0; dice "red mainnet"
nuevo t05b; ARGS=(plan); correr "ARRAS_RED= testnet"
sale_mal; dice "ARRAS_RED tiene que ser mainnet o testnet"
# T06 el padre tiene que ser de la red, valido y en minusculas.
nuevo t06; ARGS=(plan); correr ARRAS_PADRE=arras-ensayo.testnet
sale_mal; dice "no es una cuenta valida de mainnet"
nuevo t06b; ARGS=(plan); correr ARRAS_PADRE=Arras.ultravioletadao.near
sale_mal
nuevo t06c; ARGS=(plan); correr "ARRAS_PADRE=$(printf 'arras.ultravioletadao.near\nx.near')"
sale_mal
nuevo t06d; ARGS=(plan); correr ARRAS_RED=testnet
sale_mal; dice "ARRAS_PADRE es obligatoria"
nuevo t06f; ARGS=(plan); correr ARRAS_PARENT=$P_MAIN
sale_mal; dice "se renombro a ARRAS_PADRE"
nuevo t06g; ARGS=(plan); correr ARRAS_RED=testnet ARRAS_PADRE=a.arras-ensayo.testnet
sale_mal; dice "en testnet el padre es"
nuevo t06e; ARGS=(plan); correr ARRAS_PADRE=near
sale_mal
# T07 el capturer: hija directa del padre y no otra cuenta del despliegue.
nuevo t07; ARGS=(plan); correr ARRAS_CAPTURER=a.em-capturer.$P_MAIN
sale_mal; dice "hija directa"
nuevo t07b; ARGS=(plan); correr ARRAS_CAPTURER=owner.$P_MAIN
sale_mal
nuevo t07c; ARGS=(plan); correr ARRAS_CAPTURER=em-capturer.otro.near
sale_mal
# T08 el fee_recipient: nunca un contrato del despliegue; una cuenta externa o implicita no se crea.
nuevo t08; ARGS=(plan); correr ARRAS_FEE_RECIPIENT=v1.$P_MAIN
sale_mal; dice "ARRAS_E46"
nuevo t08b; ARGS=(plan); correr ARRAS_FEE_RECIPIENT=v2.$P_MAIN
sale_mal
nuevo t08c; ARGS=(plan); correr ARRAS_FEE_RECIPIENT="$(printf 'a%.0s' {1..64})"
sale 0; dice "ya existe, no se crea"
nuevo t08d; ARGS=(plan); correr ARRAS_FEE_RECIPIENT=em-fees.testnet
sale_mal
# T09 topes coherentes y numeros sanos.
nuevo t09; ARGS=(plan); correr ARRAS_TOPE_TVL_GLOBAL=5000000
sale_mal; dice "no puede ser menor que el tope por pago"
nuevo t09b; ARGS=(plan); correr ARRAS_TOPE_TVL_OPERADOR=1000000001
sale_mal; dice "no puede pasar el global"
nuevo t09c; ARGS=(plan); correr ARRAS_TOPE_TVL_OPERADOR=300000000 ARRAS_TOPE_TVL_POR_PAYER=300000001
sale_mal; dice "no puede pasar el del operador"
nuevo t09d; ARGS=(plan); correr ARRAS_MIN_AMOUNT=10000001
sale_mal; dice "ningun pago entraria"
nuevo t09e; ARGS=(plan); correr ARRAS_FEE_BPS=1801
sale_mal; dice "ARRAS_E43"
nuevo t09f; ARGS=(plan); correr ARRAS_TOPE_POR_PAGO=10e6
sale_mal
nuevo t09g; ARGS=(plan); correr ARRAS_TOPE_TVL_GLOBAL=1234567890123456
sale_mal
nuevo t09h; ARGS=(plan); correr "ARRAS_POOL=0.5"
sale_mal
nuevo t09i; ARGS=(plan); correr ARRAS_TOPE_POR_PAGO=0 ARRAS_MIN_AMOUNT=0
sale_mal
nuevo t09j; ARGS=(plan); correr ARRAS_OPERATOR_ID=EM
sale_mal; dice "ARRAS_E45"
# T10 en real, una wasm que no es la del pin aborta antes de cualquier tx.
nuevo t10; wasm_falsa "$STUB/w.wasm"; ARGS=(--mainnet-ok deploy); correr ARRAS_WASM="$STUB/w.wasm"
sale_mal; sin_near_tx; dice "los pines son 7058319d"
# T10b ...y una de la rev equivocada tambien, aunque el sha256 coincida con el pin.
nuevo t10b; wasm_falsa "$STUB/w.wasm" "$(printf 'a%.0s' {1..40})"
ARGS=(--mainnet-ok deploy); correr ARRAS_WASM="$STUB/w.wasm" ARRAS_WASM_SHA256="$(shasum -a 256 "$STUB/w.wasm" | cut -c1-64)"
sale_mal; sin_near_tx; dice "rev aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa; los pines son"; no_dice "(= pines)"
# T10c sin wasm, en real aborta; en seco solo avisa.
nuevo t10c; ARGS=(--mainnet-ok deploy); correr ARRAS_WASM="$STUB/no-esta.wasm"
sale_mal; sin_near_tx; dice "corre el paso wasm"
nuevo t10d; ARGS=(deploy); correr ARRAS_WASM="$STUB/no-esta.wasm"
sale 0; sin_red; dice "AVISO: falta la wasm"
# T11 el pin en mayusculas vale igual (se normaliza) y un contrato ya desplegado con esa wasm no se pisa.
nuevo t11; wasm_falsa "$STUB/w.wasm"; h=$(shasum -a 256 "$STUB/w.wasm" | cut -c1-64)
printf '{"amount":"1","storage_usage":1,"code_hash":"%s"}' "$(b58 "$h")" >"$STUB/rpc/view_account.v1.$P_MAIN"
config_falsa "owner.$P_MAIN" "guardian.$P_MAIN" 1000000000 >"$STUB/rpc/call_function.v1.$P_MAIN.get_config"
ARGS=(--mainnet-ok deploy); correr ARRAS_WASM="$STUB/w.wasm" ARRAS_WASM_SHA256="$(printf '%s' "$h" | tr 'a-f' 'A-F')"
sale 0; sin_near_tx; dice "ya desplegado"; dice "OK: get_config"
# T11b un get_config con otro tope global aborta.
nuevo t11b; wasm_falsa "$STUB/w.wasm"; h=$(shasum -a 256 "$STUB/w.wasm" | cut -c1-64)
printf '{"amount":"1","storage_usage":1,"code_hash":"%s"}' "$(b58 "$h")" >"$STUB/rpc/view_account.v1.$P_MAIN"
config_falsa "owner.$P_MAIN" "guardian.$P_MAIN" 200000000 >"$STUB/rpc/call_function.v1.$P_MAIN.get_config"
ARGS=(--mainnet-ok deploy); correr ARRAS_WASM="$STUB/w.wasm" ARRAS_WASM_SHA256="$h"
sale_mal; dice "no es el esperado"
# T11d ...y uno con otro owner tambien.
nuevo t11d; wasm_falsa "$STUB/w.wasm"; h=$(shasum -a 256 "$STUB/w.wasm" | cut -c1-64)
printf '{"amount":"1","storage_usage":1,"code_hash":"%s"}' "$(b58 "$h")" >"$STUB/rpc/view_account.v1.$P_MAIN"
config_falsa "intruso.near" "guardian.$P_MAIN" 1000000000 >"$STUB/rpc/call_function.v1.$P_MAIN.get_config"
ARGS=(--mainnet-ok deploy); correr ARRAS_WASM="$STUB/w.wasm" ARRAS_WASM_SHA256="$h"
sale_mal; dice "no es el esperado"
# T11c una cuenta con OTRO codigo no se pisa.
nuevo t11c; wasm_falsa "$STUB/w.wasm"; h=$(shasum -a 256 "$STUB/w.wasm" | cut -c1-64)
printf '{"amount":"1","storage_usage":1,"code_hash":"%s"}' "$(b58 "$(printf 'b%.0s' {1..64})")" >"$STUB/rpc/view_account.v1.$P_MAIN"
ARGS=(--mainnet-ok deploy); correr ARRAS_WASM="$STUB/w.wasm" ARRAS_WASM_SHA256="$h"
sale_mal; sin_near_tx; dice "ya tiene otro codigo"
# T12 en real, otra version de near-cli aborta.
nuevo t12; STUB_NEAR_VERSION=0.30.0; ARGS=(--mainnet-ok plan); correr
sale_mal; dice "0.30.1"
# T13 sin-llaves: borra todas las llaves en una tx y exige la lista vacia.
nuevo t13
v1_listo_testnet
printf '[{"public_key":"ed25519:AAA","access_key":{"nonce":1,"permission":"FullAccess"}},{"public_key":"ed25519:BBB","access_key":{"nonce":1,"permission":"FullAccess"}}]' | jq -c '{keys: .}' >"$STUB/rpc/view_access_key_list.v1.$P_TEST.1"
echo '{"keys":[]}' >"$STUB/rpc/view_access_key_list.v1.$P_TEST"
ARGS=(--testnet-ok sin-llaves); correr ARRAS_RED=testnet ARRAS_PADRE=$P_TEST
sale 0; near_hizo "account delete-keys v1.$P_TEST public-keys ed25519:AAA,ed25519:BBB"; dice "view_access_key_list de v1.$P_TEST vacio"
# T13b si despues del borrado queda una llave, aborta.
nuevo t13b
v1_listo_testnet
echo '{"keys":[{"public_key":"ed25519:AAA","access_key":{"nonce":1,"permission":"FullAccess"}}]}' >"$STUB/rpc/view_access_key_list.v1.$P_TEST"
ARGS=(--testnet-ok sin-llaves); correr ARRAS_RED=testnet ARRAS_PADRE=$P_TEST
sale_mal; dice "le quedan 1 llaves"
# T13c si near falla, aborta (no sigue como si nada).
nuevo t13c; STUB_NEAR_RC=1
v1_listo_testnet
echo '{"keys":[{"public_key":"ed25519:AAA","access_key":{"nonce":1,"permission":"FullAccess"}}]}' >"$STUB/rpc/view_access_key_list.v1.$P_TEST"
ARGS=(--testnet-ok sin-llaves); correr ARRAS_RED=testnet ARRAS_PADRE=$P_TEST
sale_mal; dice "near fallo en 'sin-llaves"
# T14 la salida de near pasa por el filtro: nada que parezca una llave privada o una semilla llega a la pantalla, y el
# Transaction ID queda anotado.
nuevo t14
printf 'linea normal\n"private_key": "%s"\nmaster_seed_phrase: uno dos tres\notra %s\n' "$PRIV_FALSA" "$PRIV_FALSA" >"$STUB/near.salida"
echo null >"$STUB/rpc/call_function.storage_balance_of"
ARGS=(--testnet-ok usdc); correr ARRAS_RED=testnet ARRAS_PADRE=$P_TEST
sale 0; dice "linea normal"; no_dice "$PRIV_FALSA"; no_dice "uno dos tres"
if grep -q "STUBTX" "$STUB/home/.arras-despliegue/testnet/$P_TEST/txs.tsv" 2>/dev/null; then ok; else mal "el Transaction ID no quedo en txs.tsv"; fi
# T14b si el RPC falla en una lectura, aborta: no da la cuenta por registrada ni sigue con un valor vacio.
nuevo t14b; ARGS=(--testnet-ok usdc); correr ARRAS_RED=testnet ARRAS_PADRE=$P_TEST
sale_mal; sin_near_tx; dice "la vista storage_balance_of"; no_dice "ya registrada en USDC"
# T15 secretos en seco: solo que llaves hay y su nombre; NINGUNA linea ejecutable (ni subir ni borrar) y nunca el
# contenido de la llave.
nuevo t15; d="$STUB/home/.near-credentials/mainnet/owner.$P_MAIN"; mkdir -p "$d"
printf '{"public_key":"ed25519:PUBOWNER","private_key":"%s"}' "$PRIV_FALSA" >"$d/ed25519_PUBOWNER.json"
ARGS=(secretos); correr
sale 0; sin_red; no_dice "$PRIV_FALSA"; dice "owner (owner.$P_MAIN): ed25519:PUBOWNER en $d/ed25519_PUBOWNER.json; nombre: /arras/mainnet/owner"
dice "scripts/mainnet.sh --mainnet-ok secretos"; no_dice "put-parameter"; no_dice "rm '"; no_dice "rm -P"
# T15e en real, una llave viva es UNA linea: subir && comparar && borrar (el archivo y su copia <cuenta>.json).
nuevo t15e; d="$STUB/home/.near-credentials/mainnet/owner.$P_MAIN"; mkdir -p "$d"
printf '{"public_key":"ed25519:PUBOWNER","private_key":"%s"}' "$PRIV_FALSA" >"$d/ed25519_PUBOWNER.json"
cp "$d/ed25519_PUBOWNER.json" "$STUB/home/.near-credentials/mainnet/owner.$P_MAIN.json"
echo '{"amount":"1","storage_usage":182,"code_hash":"11111111111111111111111111111111"}' >"$STUB/rpc/view_account.owner.$P_MAIN"
echo '{"keys":[{"public_key":"ed25519:PUBOWNER","access_key":{"nonce":1,"permission":"FullAccess"}}]}' >"$STUB/rpc/view_access_key_list.owner.$P_MAIN"
ARGS=(--mainnet-ok secretos); correr
sale 0; no_dice "$PRIV_FALSA"; dice "--name '/arras/mainnet/owner' --type SecureString --key-id 'alias/arras-test' --value 'file://$d/ed25519_PUBOWNER.json'"
dice "&& rm '$d/ed25519_PUBOWNER.json' '$STUB/home/.near-credentials/mainnet/owner.$P_MAIN.json' && echo 'OK owner en /arras/mainnet/owner'"
dice "# guardian: guardian.$P_MAIN no existe en cadena: sus archivos no se tocan"
nuevo t15b; d="$STUB/home/.near-credentials/mainnet/owner.$P_MAIN"; mkdir -p "$d"; echo '{"public_key":"ed25519:PUBOWNER"}' >"$d/ed25519_PUBOWNER.json"
echo '{"amount":"1","storage_usage":182,"code_hash":"11111111111111111111111111111111"}' >"$STUB/rpc/view_account.owner.$P_MAIN"
echo '{"keys":[{"public_key":"ed25519:PUBOWNER","access_key":{"nonce":1,"permission":"FullAccess"}}]}' >"$STUB/rpc/view_access_key_list.owner.$P_MAIN"
ARGS=(--mainnet-ok secretos); correr ARRAS_SECRETOS=secretsmanager
sale 0; dice "aws secretsmanager create-secret"; no_dice "aws ssm put-parameter"
nuevo t15c; ARGS=(secretos); correr ARRAS_SECRETOS=vault
sale_mal
# T15d en real, secretos sube solo las llaves VIVAS en cadena: la FullAccess de creacion del capturer (ya borrada) solo
# se borra del disco, nunca va al mismo nombre que la FunctionCall key.
nuevo t15d; d="$STUB/home/.near-credentials/testnet/em-capturer.$P_TEST"; mkdir -p "$d"
echo '{"public_key":"ed25519:FC"}' >"$d/ed25519_FC.json"; echo '{"public_key":"ed25519:FULL"}' >"$d/ed25519_FULL.json"
echo '{"amount":"1","storage_usage":1,"code_hash":"11111111111111111111111111111111"}' >"$STUB/rpc/view_account.em-capturer.$P_TEST"
echo '{"keys":[{"public_key":"ed25519:FC","access_key":{"nonce":1,"permission":{"FunctionCall":{"allowance":null,"receiver_id":"v1.arras-ensayo.testnet","method_names":["release","refund","withdraw_for"]}}}}]}' >"$STUB/rpc/view_access_key_list.em-capturer.$P_TEST"
ARGS=(--testnet-ok secretos); correr ARRAS_RED=testnet ARRAS_PADRE=$P_TEST
sale 0; dice "llave publica ed25519:FC (viva)"; dice "llave publica ed25519:FULL (muerta)"
dice "--value 'file://$d/ed25519_FC.json'"; no_dice "--value 'file://$d/ed25519_FULL.json'"; dice "rm '$d/ed25519_FULL.json'   # ya no esta"
# T16 los pasos de pago son solo del ensayo en testnet, aun en seco.
for paso in authorize release withdraw ensayo; do
  nuevo "t16-$paso"; ARGS=("$paso"); correr
  sale_mal; sin_red; dice "solo del ensayo en testnet"
done
# T16b en testnet, en seco, el ensayo muestra el authorize, el release y los dos withdraw_for firmados por el capturer.
nuevo t16b; ARGS=(ensayo); correr ARRAS_RED=testnet ARRAS_PADRE=$P_TEST
sale 0; sin_red; dice "ft_transfer_call"; dice "release json-args"; dice "withdraw_for json-args {\"account\":\"worker-demo.arras-dev.testnet\"}"
dice "sign-as em-capturer.$P_TEST network-config testnet sign-with-access-key-file"
dice "sponsor-by-faucet-service $P_TEST"
# T16c el worker no puede ser el payer.
nuevo t16c; ARGS=(authorize); correr ARRAS_RED=testnet ARRAS_PADRE=$P_TEST ARRAS_WORKER=payer-demo.arras-dev.testnet
sale_mal; dice "ARRAS_E15"
# T17 argumentos raros: bandera desconocida, dos pasos, sin paso, paso desconocido.
nuevo t17; ARGS=(--mainnet-okk plan); correr
sale 1; sin_red; dice "bandera desconocida"
nuevo t17b; ARGS=(plan todo); correr
sale 1; dice "un paso por corrida"
nuevo t17c; ARGS=(); correr
sale 1; dice "EN SECO POR DEFECTO"
nuevo t17d; ARGS=(desplegar); correr
sale 1; dice "paso desconocido"
# T18 cuentas en real: si el padre no tiene el presupuesto, no crea nada.
nuevo t18
echo '{"amount":"1000000000000000000000000","storage_usage":200,"code_hash":"11111111111111111111111111111111"}' >"$STUB/rpc/view_account.$P_TEST"
ARGS=(--testnet-ok cuentas); correr ARRAS_RED=testnet ARRAS_PADRE=$P_TEST
sale_mal; sin_near_tx; dice "libres y el despliegue necesita"
# T18b con presupuesto, crea las seis hijas con la llave en el llavero legacy, firmadas por el padre.
nuevo t18b
echo '{"amount":"10000000000000000000000000","storage_usage":200,"code_hash":"11111111111111111111111111111111"}' >"$STUB/rpc/view_account.$P_TEST"
ARGS=(--testnet-ok cuentas); correr ARRAS_RED=testnet ARRAS_PADRE=$P_TEST
sale 0
for h in v1 v2 owner guardian em-capturer em-fees; do
  near_hizo "account create-account fund-myself $h.$P_TEST"
done
near_hizo "autogenerate-new-keypair save-to-legacy-keychain sign-as $P_TEST network-config testnet sign-with-legacy-keychain send"
# T18c el borde del presupuesto: libre exactamente 4,5025 NEAR (el plan) pasa; un yocto menos no.
nuevo t18c
echo '{"amount":"4504500000000000000000000","storage_usage":200,"code_hash":"11111111111111111111111111111111"}' >"$STUB/rpc/view_account.$P_TEST"
ARGS=(--testnet-ok cuentas); correr ARRAS_RED=testnet ARRAS_PADRE=$P_TEST
sale 0; near_hizo "account create-account fund-myself v1.$P_TEST 3.0 NEAR"
nuevo t18d
echo '{"amount":"4504499999999999999999999","storage_usage":200,"code_hash":"11111111111111111111111111111111"}' >"$STUB/rpc/view_account.$P_TEST"
ARGS=(--testnet-ok cuentas); correr ARRAS_RED=testnet ARRAS_PADRE=$P_TEST
sale_mal; sin_near_tx
# T19 capturer en real: agrega la FunctionCall key acotada, borra la FullAccess y exige que no quede ninguna.
nuevo t19; d="$STUB/home/.near-credentials/testnet/em-capturer.$P_TEST"; mkdir -p "$d"; echo '{}' >"$d/ed25519_FC.json"
echo '{"keys":[{"public_key":"ed25519:FULL","access_key":{"nonce":1,"permission":"FullAccess"}}]}' >"$STUB/rpc/view_access_key_list.em-capturer.$P_TEST.1"
echo '{"keys":[{"public_key":"ed25519:FULL","access_key":{"nonce":1,"permission":"FullAccess"}},{"public_key":"ed25519:FC","access_key":{"nonce":1,"permission":{"FunctionCall":{"allowance":"500000000000000000000000","receiver_id":"v1.arras-ensayo.testnet","method_names":["release","refund","withdraw_for"]}}}}]}' >"$STUB/rpc/view_access_key_list.em-capturer.$P_TEST"
for i in 6 7 8 9; do
  echo '{"keys":[{"public_key":"ed25519:FC","access_key":{"nonce":1,"permission":{"FunctionCall":{"allowance":"500000000000000000000000","receiver_id":"v1.arras-ensayo.testnet","method_names":["release","refund","withdraw_for"]}}}}]}' >"$STUB/rpc/view_access_key_list.em-capturer.$P_TEST.$i"
done
ARGS=(--testnet-ok capturer); correr ARRAS_RED=testnet ARRAS_PADRE=$P_TEST
sale 0; near_hizo "grant-function-call-access --allowance 0.5 NEAR --contract-account-id v1.$P_TEST"
near_hizo "account delete-keys em-capturer.$P_TEST public-keys ed25519:FULL"; dice "solo tiene la FunctionCall key ed25519:FC"
# T19b una FunctionCall key con un metodo de mas aborta.
nuevo t19b; d="$STUB/home/.near-credentials/testnet/em-capturer.$P_TEST"; mkdir -p "$d"; echo '{}' >"$d/ed25519_FC.json"
echo '{"keys":[{"public_key":"ed25519:FC","access_key":{"nonce":1,"permission":{"FunctionCall":{"allowance":null,"receiver_id":"v1.arras-ensayo.testnet","method_names":["release","refund","withdraw_for","reclaim"]}}}}]}' >"$STUB/rpc/view_access_key_list.em-capturer.$P_TEST"
ARGS=(--testnet-ok capturer); correr ARRAS_RED=testnet ARRAS_PADRE=$P_TEST
sale_mal; dice "no tiene exactamente release, refund y withdraw_for"
# T19c si despues de borrarla la FullAccess sigue en cadena, aborta.
nuevo t19c; d="$STUB/home/.near-credentials/testnet/em-capturer.$P_TEST"; mkdir -p "$d"; echo '{}' >"$d/ed25519_FC.json"
echo '{"keys":[{"public_key":"ed25519:FULL","access_key":{"nonce":1,"permission":"FullAccess"}},{"public_key":"ed25519:FC","access_key":{"nonce":1,"permission":{"FunctionCall":{"allowance":"500000000000000000000000","receiver_id":"v1.arras-ensayo.testnet","method_names":["release","refund","withdraw_for"]}}}}]}' >"$STUB/rpc/view_access_key_list.em-capturer.$P_TEST"
ARGS=(--testnet-ok capturer); correr ARRAS_RED=testnet ARRAS_PADRE=$P_TEST
sale_mal; dice "todavia tiene una FullAccess"
# T20 el padre de mainnet: en seco imprime el comando del dueno y no genera nada; en real, si el padre no existe, para.
nuevo t20; ARGS=(padre); correr ARRAS_PADRE=$P_DUENO
sale 0; sin_red; dice "use-manually-provided-public-key <llave publica nueva del padre> sign-as ultravioletadao.near network-config mainnet"
dice "fund-myself $P_DUENO '4.52 NEAR'"
if [ -d "$STUB/home/.near-credentials" ]; then mal "en seco creo el llavero"; else ok; fi
nuevo t20b; d="$STUB/home/.near-credentials/mainnet/$P_DUENO"; mkdir -p "$d"
printf '{"public_key":"ed25519:PUBPADRE","private_key":"%s"}' "$PRIV_FALSA" >"$d/ed25519_PUBPADRE.json"
ARGS=(--mainnet-ok padre); correr ARRAS_PADRE=$P_DUENO
sale_mal; sin_near_tx; dice "use-manually-provided-public-key ed25519:PUBPADRE"; dice "todavia no existe"; no_dice "$PRIV_FALSA"

# T21 si el archivo del script cambia EN EL LUGAR mientras corre (un git pull en medio de un deploy), la corrida sigue
# con lo que ya leyo: el script reescrito con 300 lineas de mas adelante no le corre lineas desde un offset viejo.
nuevo t21; cp "$SCRIPT" "$STUB/m.sh"; CORRE=$STUB/m.sh; STUB_NEAR_SLEEP=2
v1_listo_testnet
echo '{"keys":[{"public_key":"ed25519:AAA","access_key":{"nonce":1,"permission":"FullAccess"}}]}' >"$STUB/rpc/view_access_key_list.v1.$P_TEST.1"
echo '{"keys":[]}' >"$STUB/rpc/view_access_key_list.v1.$P_TEST"
ARGS=(--testnet-ok sin-llaves); correr ARRAS_RED=testnet ARRAS_PADRE=$P_TEST &
pid=$!
sleep 1
{ printf 'echo REESCRITO_%s\n' $(seq 1 300); cat "$SCRIPT"; } >"$STUB/m.nuevo"
cat "$STUB/m.nuevo" >"$STUB/m.sh"
wait "$pid"; RC=$?
sale 0; dice "view_access_key_list de v1.$P_TEST vacio"; no_dice "REESCRITO_"; no_dice "command not found"; no_dice "syntax error"
if [ "$(grep -c '^== Arras' "$STUB/out")" = 1 ]; then ok; else mal "el paso corrio $(grep -c '^== Arras' "$STUB/out") veces"; fi

# T22 ARRAS_KMS_KEY es obligatoria en el modo real (en seco no).
nuevo t22; ARGS=(--mainnet-ok plan); correr ARRAS_KMS_KEY=
sale_mal; sin_near_tx; dice "ARRAS_KMS_KEY es obligatoria"
nuevo t22b; ARGS=(plan); correr ARRAS_KMS_KEY=
sale 0
# T23 caso (a): la de fondeo tiene que ser implicita (64 hex en minusculas) y no puede venir vacia.
nuevo t23; ARGS=(plan); correr ARRAS_FONDEO=fondeo.near
sale_mal; dice "tiene que ser una cuenta implicita"
nuevo t23b; ARGS=(plan); correr ARRAS_FONDEO="$(printf '%s' "$FONDEO" | tr 'a-f' 'A-F')"
sale_mal
nuevo t23c; ARGS=(plan); correr ARRAS_FONDEO=
sale_mal; dice "vino vacia"
# T24 ARRAS_FONDEO_ARCHIVO es solo del ensayo en testnet.
nuevo t24; ARGS=(plan); correr ARRAS_FONDEO_ARCHIVO="$STUB/llave.json"
sale_mal; dice "solo del ensayo en testnet"
# T25 caso (a) en seco: el comando de near.create_account con la llave por un pipe, sin leer SSM.
nuevo t25; ARGS=(padre); correr
sale 0; sin_red; dice "create-account fund-myself $P_MAIN 4.52 NEAR autogenerate-new-keypair save-to-legacy-keychain sign-as $FONDEO"
dice "leida de SSM /arras/mainnet/fondeo (us-east-2) por un pipe"; dice "4.52 NEAR + 0.02 NEAR de gas"; dice "el dueno la fondea con 4.55 NEAR"
# T26 caso (a) en real: la llave sale de SSM por un pipe (/dev/fd), es la de la implicita, nunca aparece en pantalla ni
# en la linea de comandos, y el padre queda con la FullAccess del llavero.
llave_de() { python3 -I -c '
import sys
A = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"
def enc(b):
    v, s = int.from_bytes(b, "big"), ""
    while v:
        v, r = divmod(v, 58); s = A[r] + s
    return "1" * (len(b) - len(b.lstrip(b"\0"))) + s
pub = bytes.fromhex(sys.argv[1])
print("ed25519:" + enc(pub), "ed25519:" + enc(bytes(range(7, 39)) + pub))' "$1"; }
nuevo t26; read -r PUBF PRIVF < <(llave_de "$FONDEO")
jq -nc --arg v "$(jq -nc --arg p "$PUBF" --arg k "$PRIVF" '{public_key:$p,private_key:$k}')" '{Parameter:{Value:$v}}' >"$STUB/aws.salida"
echo UNKNOWN >"$STUB/rpc/view_account.$P_MAIN.1"
echo '{"amount":"1","storage_usage":182,"code_hash":"11111111111111111111111111111111"}' >"$STUB/rpc/view_account.$P_MAIN"
echo '{"amount":"5000000000000000000000000","storage_usage":0,"code_hash":"11111111111111111111111111111111"}' >"$STUB/rpc/view_account.$FONDEO"
d="$STUB/home/.near-credentials/mainnet/$P_MAIN"; mkdir -p "$d"; echo '{"public_key":"ed25519:PUBP"}' >"$d/ed25519_PUBP.json"
echo '{"keys":[{"public_key":"ed25519:PUBP","access_key":{"nonce":1,"permission":"FullAccess"}}]}' >"$STUB/rpc/view_access_key_list.$P_MAIN"
ARGS=(--mainnet-ok padre); correr
sale 0; near_hizo "sign-as $FONDEO network-config mainnet sign-with-access-key-file /dev/fd/"; dice "OK: $P_MAIN existe"
no_dice "$PRIVF"; if grep -qF -- "$PRIVF" "$STUB/near.log"; then mal "la llave privada paso por la linea de comandos"; else ok; fi
if jq -e --arg a "$FONDEO" --arg p "$PUBF" '.account_id == $a and .public_key == $p' "$STUB/firma.json" >/dev/null 2>&1; then ok; else mal "near no recibio la llave de la implicita por el pipe"; fi
if grep -qF -- "ssm get-parameter --region us-east-2 --name /arras/mainnet/fondeo --with-decryption" "$STUB/aws.log"; then ok; else mal "no leyo /arras/mainnet/fondeo de SSM"; fi
# T26b una llave en SSM (aca cruda, sin JSON) que no es la de la implicita: aborta antes de llamar a near.
nuevo t26b; read -r PUBX PRIVX < <(llave_de "$(printf 'b%.0s' {1..64})")
jq -nc --arg v "$PRIVX" '{Parameter:{Value:$v}}' >"$STUB/aws.salida"
echo UNKNOWN >"$STUB/rpc/view_account.$P_MAIN"
echo '{"amount":"5000000000000000000000000","storage_usage":0,"code_hash":"11111111111111111111111111111111"}' >"$STUB/rpc/view_account.$FONDEO"
ARGS=(--mainnet-ok padre); correr
sale_mal; sin_near_tx; dice "no es la de $FONDEO"; no_dice "$PRIVX"; no_dice "$PUBX"
# T26c el borde del saldo de la de fondeo (llave valida: solo el saldo puede frenar): 4,54 libres pasa, un yocto menos no.
nuevo t26c
jq -nc --arg v "$(jq -nc --arg p "$PUBF" --arg k "$PRIVF" '{public_key:$p,private_key:$k}')" '{Parameter:{Value:$v}}' >"$STUB/aws.salida"
echo UNKNOWN >"$STUB/rpc/view_account.$P_MAIN"
echo '{"amount":"4541819999999999999999999","storage_usage":182,"code_hash":"11111111111111111111111111111111"}' >"$STUB/rpc/view_account.$FONDEO"
ARGS=(--mainnet-ok padre); correr
sale_mal; sin_near_tx; dice "crear $P_MAIN necesita 4.52 NEAR + 0.02 NEAR"
nuevo t26f
jq -nc --arg v "$(jq -nc --arg p "$PUBF" --arg k "$PRIVF" '{public_key:$p,private_key:$k}')" '{Parameter:{Value:$v}}' >"$STUB/aws.salida"
echo UNKNOWN >"$STUB/rpc/view_account.$P_MAIN.1"
echo '{"amount":"1","storage_usage":182,"code_hash":"11111111111111111111111111111111"}' >"$STUB/rpc/view_account.$P_MAIN"
echo '{"amount":"4541820000000000000000000","storage_usage":182,"code_hash":"11111111111111111111111111111111"}' >"$STUB/rpc/view_account.$FONDEO"
d="$STUB/home/.near-credentials/mainnet/$P_MAIN"; mkdir -p "$d"; echo '{"public_key":"ed25519:PUBP"}' >"$d/ed25519_PUBP.json"
echo '{"keys":[{"public_key":"ed25519:PUBP","access_key":{"nonce":1,"permission":"FullAccess"}}]}' >"$STUB/rpc/view_access_key_list.$P_MAIN"
ARGS=(--mainnet-ok padre); correr
sale 0; near_hizo "create-account fund-myself $P_MAIN 4.52 NEAR"
# T26d la de fondeo que no existe todavia (sin fondos): para con el aviso.
nuevo t26d; echo UNKNOWN >"$STUB/rpc/view_account.$P_MAIN"
ARGS=(--mainnet-ok padre); correr
sale_mal; sin_near_tx; dice "la fondea el dueno"

# T26e un padre que ya existe pero sin ninguna FullAccess de este llavero: no se usa (seria el de otro).
nuevo t26e
echo '{"amount":"1","storage_usage":182,"code_hash":"11111111111111111111111111111111"}' >"$STUB/rpc/view_account.$P_MAIN"
d="$STUB/home/.near-credentials/mainnet/$P_MAIN"; mkdir -p "$d"; echo '{"public_key":"ed25519:MIA"}' >"$d/ed25519_MIA.json"
echo '{"keys":[{"public_key":"ed25519:OTRA","access_key":{"nonce":1,"permission":"FullAccess"}}]}' >"$STUB/rpc/view_access_key_list.$P_MAIN"
ARGS=(--mainnet-ok padre); correr
sale_mal; sin_near_tx; dice "ninguna llave de"

# --- lo que pidio el refutador (P1, P2 y P3) ---------------------------------------------------------------------------
# T27 el fee_recipient no puede ser el capturer (quedaria sin FullAccess y la tarifa atrapada), en los dos sentidos.
nuevo t27; ARGS=(plan); correr ARRAS_FEE_RECIPIENT=em-capturer.$P_MAIN
sale_mal; dice "no puede ser el capturer"
nuevo t27b; ARGS=(plan); correr ARRAS_CAPTURER=em-fees.$P_MAIN
sale_mal; dice "no puede ser el capturer"
# T28 una allowance de 0 deja al capturer sin poder pagar gas.
nuevo t28; ARGS=(plan); correr "ARRAS_ALLOWANCE=0 NEAR"
sale_mal; dice "ARRAS_ALLOWANCE"
nuevo t28b; ARGS=(plan); correr "ARRAS_ALLOWANCE=0.000 NEAR"
sale_mal
nuevo t28c; ARGS=(plan); correr "ARRAS_ALLOWANCE=0.25 NEAR"
sale 0
# T29 capturer: una FunctionCall key con otra allowance en cadena no deja borrar la FullAccess.
fc_lista() { jq -nc --arg a "$1" --arg c "v1.$P_TEST" '{keys:[{public_key:"ed25519:FULL",access_key:{nonce:1,permission:"FullAccess"}},{public_key:"ed25519:FC",access_key:{nonce:1,permission:{FunctionCall:{allowance:$a,receiver_id:$c,method_names:["release","refund","withdraw_for"]}}}}]}'; }
nuevo t29; d="$STUB/home/.near-credentials/testnet/em-capturer.$P_TEST"; mkdir -p "$d"; echo '{}' >"$d/ed25519_FC.json"
fc_lista 1000000000000000000000000 >"$STUB/rpc/view_access_key_list.em-capturer.$P_TEST"
ARGS=(--testnet-ok capturer); correr ARRAS_RED=testnet ARRAS_PADRE=$P_TEST
sale_mal; sin_near_tx; dice "no tiene allowance 0.5 NEAR"
# T30 ...ni una que no esta en el llavero legacy (la FullAccess es la unica forma de arreglarlo).
nuevo t30
fc_lista 500000000000000000000000 >"$STUB/rpc/view_access_key_list.em-capturer.$P_TEST"
ARGS=(--testnet-ok capturer); correr ARRAS_RED=testnet ARRAS_PADRE=$P_TEST
sale_mal; sin_near_tx; dice "no esta en el llavero legacy"
# T31 ...y si despues del add-key la FunctionCall key no aparece, para.
nuevo t31
echo '{"keys":[{"public_key":"ed25519:FULL","access_key":{"nonce":1,"permission":"FullAccess"}}]}' >"$STUB/rpc/view_access_key_list.em-capturer.$P_TEST"
ARGS=(--testnet-ok capturer); correr ARRAS_RED=testnet ARRAS_PADRE=$P_TEST
sale_mal; dice "no quedo con una FunctionCall key"; if grep -q "delete-keys" "$STUB/near.log"; then mal "borro la FullAccess sin FunctionCall key"; else ok; fi
# T32 (P1) sin-llaves solo: sin el codigo del pin, o con otra config, NO borra las llaves de v1.
nuevo t32
echo '{"keys":[{"public_key":"ed25519:AAA","access_key":{"nonce":1,"permission":"FullAccess"}}]}' >"$STUB/rpc/view_access_key_list.v1.$P_TEST"
echo '{"amount":"1","storage_usage":1,"code_hash":"11111111111111111111111111111111"}' >"$STUB/rpc/view_account.v1.$P_TEST"
ARGS=(--testnet-ok sin-llaves); correr ARRAS_RED=testnet ARRAS_PADRE=$P_TEST
sale_mal; sin_near_tx; dice "no se borran sus llaves"
nuevo t32b; v1_listo_testnet
echo '{"keys":[{"public_key":"ed25519:AAA","access_key":{"nonce":1,"permission":"FullAccess"}}]}' >"$STUB/rpc/view_access_key_list.v1.$P_TEST"
jq -nc --arg t "$USDC_TEST" '{version:"arras-v1",owner:"otro.testnet",guardian:"guardian.arras-ensayo.testnet",token:$t,tope_por_pago:"10000000",tope_tvl_global:"1000000000"}' >"$STUB/rpc/call_function.v1.$P_TEST.get_config"
ARGS=(--testnet-ok sin-llaves); correr ARRAS_RED=testnet ARRAS_PADRE=$P_TEST
sale_mal; sin_near_tx; dice "no es el esperado"
# T33 operador: alta nueva (register + fund_operator_storage) y la verificacion de lo registrado.
op_json() { jq -nc --arg c "$1" --arg r "em-fees.$P_TEST" --arg pool "$2" '{owner:"o",capturers:[$c],fee_bps:1300,fee_recipient:$r,min_amount:"10000",tope_tvl_operador:"1000000000",tvl_operador:"0",tope_tvl_por_payer:"20000000",sponsor_pool:$pool,sponsor_max_por_payer:"20000000000000000000000"}'; }
nuevo t33
echo null >"$STUB/rpc/call_function.v1.$P_TEST.get_operator.1"
op_json "em-capturer.$P_TEST" 0 >"$STUB/rpc/call_function.v1.$P_TEST.get_operator.2"
op_json "em-capturer.$P_TEST" 500000000000000000000000 >"$STUB/rpc/call_function.v1.$P_TEST.get_operator"
ARGS=(--testnet-ok operador); correr ARRAS_RED=testnet ARRAS_PADRE=$P_TEST
sale 0; near_hizo "register_operator json-args"; near_hizo "fund_operator_storage json-args {\"operator_id\":\"em\"} prepaid-gas 30.0 Tgas attached-deposit 0.5 NEAR sign-as $P_TEST"
near_hizo '"tope_tvl_operador":"1000000000","tope_tvl_por_payer":"20000000"'; dice "OK: operador em"
nuevo t33b
echo null >"$STUB/rpc/call_function.v1.$P_TEST.get_operator.1"
op_json "otro.testnet" 0 >"$STUB/rpc/call_function.v1.$P_TEST.get_operator"
ARGS=(--testnet-ok operador); correr ARRAS_RED=testnet ARRAS_PADRE=$P_TEST
sale_mal; dice "no es el esperado"; if grep -q fund_operator_storage "$STUB/near.log"; then mal "fondeo un operador equivocado"; else ok; fi
nuevo t33c
op_json "em-capturer.$P_TEST" 500000000000000000000000 | jq -c '.sponsor_max_por_payer = "1"' >"$STUB/rpc/call_function.v1.$P_TEST.get_operator"
ARGS=(--testnet-ok operador); correr ARRAS_RED=testnet ARRAS_PADRE=$P_TEST
sale_mal; dice "no es el esperado"
# T34 un fee_recipient externo tiene que existir.
nuevo t34
echo '{"amount":"10000000000000000000000000","storage_usage":200,"code_hash":"11111111111111111111111111111111"}' >"$STUB/rpc/view_account.$P_TEST"
ARGS=(--testnet-ok cuentas); correr ARRAS_RED=testnet ARRAS_PADRE=$P_TEST ARRAS_FEE_RECIPIENT=fees-externa.testnet
sale_mal; dice "el fee_recipient fees-externa.testnet no existe"
# T35 get_config con otro guardian, o con otro token, aborta.
for campo in guardian token; do
  nuevo "t35-$campo"; wasm_falsa "$STUB/w.wasm"; h=$(shasum -a 256 "$STUB/w.wasm" | cut -c1-64)
  printf '{"amount":"1","storage_usage":1,"code_hash":"%s"}' "$(b58 "$h")" >"$STUB/rpc/view_account.v1.$P_MAIN"
  config_falsa "owner.$P_MAIN" "guardian.$P_MAIN" 1000000000 | jq -c --arg k "$campo" '.[$k] = "otro.near"' >"$STUB/rpc/call_function.v1.$P_MAIN.get_config"
  ARGS=(--mainnet-ok deploy); correr ARRAS_WASM="$STUB/w.wasm" ARRAS_WASM_SHA256="$h"
  sale_mal; dice "no es el esperado"
done
# T36 caso (a): si el padre no aparece despues de create_account, para (no sigue como si existiera).
nuevo t36; jq -nc --arg v "$(jq -nc --arg p "$PUBF" --arg k "$PRIVF" '{public_key:$p,private_key:$k}')" '{Parameter:{Value:$v}}' >"$STUB/aws.salida"
echo UNKNOWN >"$STUB/rpc/view_account.$P_MAIN"
echo '{"amount":"5000000000000000000000000","storage_usage":182,"code_hash":"11111111111111111111111111111111"}' >"$STUB/rpc/view_account.$FONDEO"
ARGS=(--mainnet-ok padre); correr
sale_mal; near_hizo "create-account fund-myself $P_MAIN"; dice "no aparece despues de crearlo"
# T37 deploy: si despues del deploy el codigo en cadena no es el del pin, aborta.
nuevo t37; wasm_falsa "$STUB/w.wasm"; h=$(shasum -a 256 "$STUB/w.wasm" | cut -c1-64)
echo '{"amount":"1","storage_usage":1,"code_hash":"11111111111111111111111111111111"}' >"$STUB/rpc/view_account.v1.$P_MAIN.1"
printf '{"amount":"1","storage_usage":1,"code_hash":"%s"}' "$(b58 "$(printf 'c%.0s' {1..64})")" >"$STUB/rpc/view_account.v1.$P_MAIN"
config_falsa "owner.$P_MAIN" "guardian.$P_MAIN" 1000000000 >"$STUB/rpc/call_function.v1.$P_MAIN.get_config"
ARGS=(--mainnet-ok deploy); correr ARRAS_WASM="$STUB/w.wasm" ARRAS_WASM_SHA256="$h"
sale_mal; near_hizo "contract deploy v1.$P_MAIN"; dice "el codigo de v1.$P_MAIN tiene sha256 cccc"
# T38 una cuenta derivada de mas de 64 caracteres.
nuevo t38; ARGS=(plan); correr ARRAS_PADRE="$(printf 'a%.0s' {1..55}).near"
sale_mal; dice "64 caracteres como maximo): acorta ARRAS_PADRE"
# T39 un ARRAS_RPC de otra red aborta en real antes de nada.
nuevo t39; echo '{"chain_id":"testnet"}' >"$STUB/rpc/status"
ARGS=(--mainnet-ok plan); correr
sale_mal; sin_near_tx; dice "es de la red 'testnet', no de mainnet"
# T40 die dentro de <(llave_fondeo) corta el script principal tambien en bash 3.2 (ahi BASH_SUBSHELL vale 0): la
# primera lectura (la validacion) sale bien y la segunda (la del pipe de near) no.
nuevo t40
jq -nc --arg v "$(jq -nc --arg p "$PUBF" --arg k "$PRIVF" '{public_key:$p,private_key:$k}')" '{Parameter:{Value:$v}}' >"$STUB/aws.salida.1"
echo UNKNOWN >"$STUB/rpc/view_account.$P_MAIN.1"
echo '{"amount":"1","storage_usage":182,"code_hash":"11111111111111111111111111111111"}' >"$STUB/rpc/view_account.$P_MAIN"
echo '{"amount":"5000000000000000000000000","storage_usage":182,"code_hash":"11111111111111111111111111111111"}' >"$STUB/rpc/view_account.$FONDEO"
d="$STUB/home/.near-credentials/mainnet/$P_MAIN"; mkdir -p "$d"; echo '{"public_key":"ed25519:PUBP"}' >"$d/ed25519_PUBP.json"
echo '{"keys":[{"public_key":"ed25519:PUBP","access_key":{"nonce":1,"permission":"FullAccess"}}]}' >"$STUB/rpc/view_access_key_list.$P_MAIN"
ARGS=(--mainnet-ok padre); correr
sale_mal; dice "no se pudo leer o no es la de"; no_dice "OK: $P_MAIN existe"
# T41 el llavero es el credentials_home_dir de la config de near-cli.
nuevo t41; c="$STUB/home/Library/Application Support/near-cli"; mkdir -p "$c" "$STUB/llavero/mainnet/owner.$P_MAIN"
printf 'version = "5"\ncredentials_home_dir = "%s"\n' "$STUB/llavero" >"$c/config.toml"
echo '{"public_key":"ed25519:OWNERX"}' >"$STUB/llavero/mainnet/owner.$P_MAIN/ed25519_OWNERX.json"
ARGS=(secretos); correr
sale 0; dice "ed25519:OWNERX en $STUB/llavero/mainnet/owner.$P_MAIN/ed25519_OWNERX.json"

# --- RONDA1 de c0der (RARRAS-5) ---------------------------------------------------------------------------------------
# T42 (P2-1 a, c) secretos y todo en seco con llaves de v1 y del capturer (FULL y FC) en el llavero: ningun rm, ningun
# put-parameter, y nada de "se borraron en cadena".
nuevo t42; d1="$STUB/home/.near-credentials/mainnet/v1.$P_MAIN"; dc="$STUB/home/.near-credentials/mainnet/em-capturer.$P_MAIN"
mkdir -p "$d1" "$dc"; echo '{"public_key":"ed25519:V1VIVA"}' >"$d1/ed25519_V1VIVA.json"; cp "$d1/ed25519_V1VIVA.json" "$STUB/home/.near-credentials/mainnet/v1.$P_MAIN.json"
echo '{"public_key":"ed25519:FULL"}' >"$dc/ed25519_FULL.json"; echo '{"public_key":"ed25519:FC"}' >"$dc/ed25519_FC.json"
ARGS=(secretos); correr
sale 0; sin_red; no_dice "rm '"; no_dice "put-parameter"; no_dice "se borraron en cadena"; dice "v1 (v1.$P_MAIN): ed25519:V1VIVA"
cp -R "$STUB/home" "$STUB/home-secretos"
nuevo t42b; cp -R "$TMP/c-t42/home-secretos/." "$STUB/home/"; ARGS=(todo); correr
sale 0; sin_red; no_dice "rm '"; no_dice "put-parameter"; no_dice "se borraron en cadena"
# T42c (P2-1 b) en real, v1 con una llave todavia en cadena: OJO y ningun rm de v1.
nuevo t42c; d1="$STUB/home/.near-credentials/mainnet/v1.$P_MAIN"; mkdir -p "$d1"; echo '{"public_key":"ed25519:V1VIVA"}' >"$d1/ed25519_V1VIVA.json"
echo '{"amount":"1","storage_usage":1,"code_hash":"11111111111111111111111111111111"}' >"$STUB/rpc/view_account.v1.$P_MAIN"
echo '{"keys":[{"public_key":"ed25519:V1VIVA","access_key":{"nonce":1,"permission":"FullAccess"}}]}' >"$STUB/rpc/view_access_key_list.v1.$P_MAIN"
ARGS=(--mainnet-ok secretos); correr
sale 0; dice "OJO: v1.$P_MAIN todavia tiene llaves en cadena"; no_dice "rm '$d1"; no_dice "se borraron en cadena"
# T42d ...y si v1 no existe en cadena, tampoco.
nuevo t42d; d1="$STUB/home/.near-credentials/mainnet/v1.$P_MAIN"; mkdir -p "$d1"; echo '{"public_key":"ed25519:V1VIVA"}' >"$d1/ed25519_V1VIVA.json"
ARGS=(--mainnet-ok secretos); correr
sale 0; dice "v1.$P_MAIN no existe en cadena: sus archivos no se tocan"; no_dice "rm '$d1"
# T42e ...y con v1 sin llaves en cadena, si: sus archivos se borran.
nuevo t42e; d1="$STUB/home/.near-credentials/mainnet/v1.$P_MAIN"; mkdir -p "$d1"; echo '{"public_key":"ed25519:V1MUERTA"}' >"$d1/ed25519_V1MUERTA.json"
echo '{"amount":"1","storage_usage":1,"code_hash":"11111111111111111111111111111111"}' >"$STUB/rpc/view_account.v1.$P_MAIN"
echo '{"keys":[]}' >"$STUB/rpc/view_access_key_list.v1.$P_MAIN"
ARGS=(--mainnet-ok secretos); correr
sale 0; dice "sus llaves se borraron en cadena"; dice "rm '$d1/ed25519_V1MUERTA.json'"
# T43 (P2-2) get_config distinto en UN solo campo (el resto como config_falsa): tope_por_pago y version.
for caso in "tope_por_pago 20000000" "version arras-v2"; do
  set -- $caso
  nuevo "t43-$1"; wasm_falsa "$STUB/w.wasm"; h=$(shasum -a 256 "$STUB/w.wasm" | cut -c1-64)
  printf '{"amount":"1","storage_usage":1,"code_hash":"%s"}' "$(b58 "$h")" >"$STUB/rpc/view_account.v1.$P_MAIN"
  config_falsa "owner.$P_MAIN" "guardian.$P_MAIN" 1000000000 | jq -c --arg k "$1" --arg v "$2" '.[$k] = $v' >"$STUB/rpc/call_function.v1.$P_MAIN.get_config"
  ARGS=(--mainnet-ok deploy); correr ARRAS_WASM="$STUB/w.wasm" ARRAS_WASM_SHA256="$h"
  sale_mal; dice "no es el esperado"
done
# T44 (P2-2) get_operator distinto en UN solo campo (el resto como op_json), despues del register: aborta antes del
# fund_operator_storage.
for caso in "tope_tvl_operador s 200000000" "tope_tvl_por_payer s 30000000" "fee_recipient s otro-fees.testnet" "fee_bps n 1800" "min_amount s 1"; do
  set -- $caso
  nuevo "t44-$1"
  echo null >"$STUB/rpc/call_function.v1.$P_TEST.get_operator.1"
  if [ "$2" = n ]; then
    op_json "em-capturer.$P_TEST" 0 | jq -c --arg k "$1" --argjson v "$3" '.[$k] = $v' >"$STUB/rpc/call_function.v1.$P_TEST.get_operator"
  else
    op_json "em-capturer.$P_TEST" 0 | jq -c --arg k "$1" --arg v "$3" '.[$k] = $v' >"$STUB/rpc/call_function.v1.$P_TEST.get_operator"
  fi
  ARGS=(--testnet-ok operador); correr ARRAS_RED=testnet ARRAS_PADRE=$P_TEST
  sale_mal; dice "no es el esperado"; near_hizo "register_operator"
  if grep -q fund_operator_storage "$STUB/near.log"; then mal "fondeo un operador con $1 distinto"; else ok; fi
done
# T45 (MR4) una FunctionCall key de OTRO receiver no es la del capturer: igual agrega la de v1 (y como no aparece, para
# sin borrar la FullAccess).
nuevo t45
jq -nc '{keys:[{public_key:"ed25519:FULL",access_key:{nonce:1,permission:"FullAccess"}},{public_key:"ed25519:OTRA",access_key:{nonce:1,permission:{FunctionCall:{allowance:"500000000000000000000000",receiver_id:"v1.otro.testnet",method_names:["release","refund","withdraw_for"]}}}}]}' >"$STUB/rpc/view_access_key_list.em-capturer.$P_TEST"
d="$STUB/home/.near-credentials/testnet/em-capturer.$P_TEST"; mkdir -p "$d"; echo '{}' >"$d/ed25519_OTRA.json"
ARGS=(--testnet-ok capturer); correr ARRAS_RED=testnet ARRAS_PADRE=$P_TEST
sale_mal; near_hizo "grant-function-call-access --allowance 0.5 NEAR --contract-account-id v1.$P_TEST"; no_dice "ya tiene FunctionCall key"
if grep -q "delete-keys" "$STUB/near.log"; then mal "borro la FullAccess con una FunctionCall de otro receiver"; else ok; fi
# T46 (P3-3) despues de borrar la FullAccess, el capturer tiene que quedar con UNA sola llave: con otra FunctionCall de
# mas, aborta.
nuevo t46; d="$STUB/home/.near-credentials/testnet/em-capturer.$P_TEST"; mkdir -p "$d"; echo '{}' >"$d/ed25519_FC.json"
fc_v1='{public_key:"ed25519:FC",access_key:{nonce:1,permission:{FunctionCall:{allowance:"500000000000000000000000",receiver_id:"v1.arras-ensayo.testnet",method_names:["release","refund","withdraw_for"]}}}}'
fc_otra='{public_key:"ed25519:OTRA",access_key:{nonce:1,permission:{FunctionCall:{allowance:"500000000000000000000000",receiver_id:"v1.otro.testnet",method_names:["release"]}}}}'
jq -nc "{keys:[{public_key:\"ed25519:FULL\",access_key:{nonce:1,permission:\"FullAccess\"}},$fc_v1,$fc_otra]}" >"$STUB/rpc/view_access_key_list.em-capturer.$P_TEST"
for i in 6 7 8 9; do jq -nc "{keys:[$fc_v1,$fc_otra]}" >"$STUB/rpc/view_access_key_list.em-capturer.$P_TEST.$i"; done
ARGS=(--testnet-ok capturer); correr ARRAS_RED=testnet ARRAS_PADRE=$P_TEST
sale_mal; near_hizo "account delete-keys em-capturer.$P_TEST public-keys ed25519:FULL"; dice "tiene que quedar con UNA sola llave"
# T47 (P3-3) verificar: el capturer con una llave de mas aborta; con solo la FunctionCall de v1, pasa.
verificar_stubs() {
  wasm_falsa "$STUB/w.wasm"; h=$(shasum -a 256 "$STUB/w.wasm" | cut -c1-64)
  printf '{"amount":"1","storage_usage":1,"code_hash":"%s"}' "$(b58 "$h")" >"$STUB/rpc/view_account.v1.$P_TEST"
  echo '{"keys":[]}' >"$STUB/rpc/view_access_key_list.v1.$P_TEST"
  jq -nc --arg o "owner.$P_TEST" --arg g "guardian.$P_TEST" --arg t "$USDC_TEST" '{version:"arras-v1",owner:$o,guardian:$g,token:$t,tope_por_pago:"10000000",tope_tvl_global:"1000000000"}' >"$STUB/rpc/call_function.v1.$P_TEST.get_config"
  op_json "em-capturer.$P_TEST" 500000000000000000000000 >"$STUB/rpc/call_function.v1.$P_TEST.get_operator"
  echo '{"ok":true}' >"$STUB/rpc/call_function.v1.$P_TEST.get_storage_health"
}
nuevo t47; verificar_stubs
jq -nc "{keys:[$fc_v1,$fc_otra]}" >"$STUB/rpc/view_access_key_list.em-capturer.$P_TEST"
ARGS=(--testnet-ok verificar); correr ARRAS_RED=testnet ARRAS_PADRE=$P_TEST ARRAS_WASM="$STUB/w.wasm" ARRAS_WASM_SHA256="$h"
sale_mal; dice "tiene que quedar con UNA sola llave"
nuevo t47b; verificar_stubs
jq -nc "{keys:[$fc_v1]}" >"$STUB/rpc/view_access_key_list.em-capturer.$P_TEST"
ARGS=(--testnet-ok verificar); correr ARRAS_RED=testnet ARRAS_PADRE=$P_TEST ARRAS_WASM="$STUB/w.wasm" ARRAS_WASM_SHA256="$h"
sale 0; dice "una sola llave, la FunctionCall de v1.$P_TEST"; dice "llaves de v1.$P_TEST: 0"

printf '\nscripts/test_mainnet.sh: %s asserts pasan, %s fallan\n' "$PASAN" "$FALLOS"
[ "$FALLOS" -eq 0 ]
