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
# OpenSSL 3 (el ticket ed25519 de v2: genpkey, pkeyutl -rawin). Sin el, los casos de v2 con ticket corren sin ticket y
# la cadena de mentira no verifica firmas; se avisa al final.
OSSL=""
for o in "$(type -P openssl 2>/dev/null || true)" /opt/homebrew/opt/openssl@3/bin/openssl /usr/local/opt/openssl@3/bin/openssl; do
  [ -n "$o" ] && [ -x "$o" ] || continue
  case "$("$o" version 2>/dev/null)" in "OpenSSL 3"*) OSSL=$o; break ;; esac
done

# --- dobles ---------------------------------------------------------------------------------------------------------
mkdir -p "$TMP/bin"
cat >"$TMP/bin/near" <<'NEAR'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$STUB/near.log"
case " $* " in *" --version "*) echo "near-cli-rs ${STUB_NEAR_VERSION:-0.30.1}"; exit 0 ;; esac
# Con cadena de mentira (los tests de v2, scripts/test_cadena.py): la tx se aplica al estado si la firma una llave viva.
if [ -f "$STUB/cadena.json" ]; then
  rm -f "$STUB/firma.json"
  prev=""
  for a in "$@"; do
    [ "$prev" = sign-with-access-key-file ] && [ -r "$a" ] && cat "$a" >"$STUB/firma.json"
    prev=$a
  done
  n=$(wc -l <"$STUB/near.log" | tr -d ' ')
  python3 -I "$STUB_CADENA" aplicar "$@" || exit 1
  echo "Transaction ID: STUBTX$n"
  exit 0
fi
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
# Con $STUB/ssm (los tests de v2): un parametro por nombre ($STUB/ssm/<nombre con _>, o <...>.N en la lectura N).
if [ -d "$STUB/ssm" ]; then
  prev="" nombre=""
  for a in "$@"; do [ "$prev" = --name ] && nombre=$a; prev=$a; done
  f="$STUB/ssm/$(printf '%s' "$nombre" | tr / _)"
  c="$STUB/ssm-lecturas$(printf '%s' "$nombre" | tr / _)"
  k=1; [ -f "$c" ] && k=$(( $(cat "$c") + 1 )); echo "$k" >"$c"
  [ -f "$f.$k" ] && f="$f.$k"
  [ -f "$f" ] || { echo "An error occurred (ParameterNotFound)" >&2; exit 255; }
  jq -nc --rawfile v "$f" '{Parameter:{Value:$v}}'
  exit 0
fi
n=$(wc -l <"$STUB/aws.log" | tr -d ' ')
if [ -f "$STUB/aws.salida.$n" ]; then cat "$STUB/aws.salida.$n"; exit 0; fi
[ -f "$STUB/aws.salida" ] || exit 255
cat "$STUB/aws.salida"
AWS
cat >"$TMP/bin/curl" <<'CURL'
#!/usr/bin/env bash
# La linea de comandos de cada curl (lo que `ps` veria), para probar que el RPC privado no va en ella.
printf '%s\n' "$*" >>"$STUB/curl-args.log"
exec python3 -I "$STUB_PY" "$@"
CURL
cat >"$TMP/rpc.py" <<'PY'
import json, os, subprocess, sys
stub = os.environ["STUB"]
args = sys.argv[1:]
d = args[args.index("-d") + 1]
req = json.loads(d)
# Con cadena de mentira (los tests de v2): responde el estado de scripts/test_cadena.py; con $STUB/falla.<n>, la
# consulta n falla como un curl -f que no recibe nada (para los reintentos de rpc_query).
if os.path.exists(os.path.join(stub, "cadena.json")):
    cpath = os.path.join(stub, "consultas")
    n = int(open(cpath).read()) + 1 if os.path.exists(cpath) else 1
    open(cpath, "w").write(str(n))
    if os.path.exists(os.path.join(stub, f"falla.{n}")):
        sys.exit(22)
    p = req.get("params") or {}
    if isinstance(p, dict):
        with open(os.path.join(stub, "curl.log"), "a") as f:
            f.write(f"{p.get('request_type', '')} {p.get('account_id', '')} {p.get('method_name', '')}\n")
    sys.stdout.write(subprocess.run([sys.executable, "-I", os.environ["STUB_CADENA"], "rpc", d], capture_output=True, text=True, check=True).stdout)
    sys.exit(0)
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
    AWS_SHARED_CREDENTIALS_FILE=/dev/null ARRAS_KMS_KEY=alias/arras-test STUB_CADENA="$RAIZ/scripts/test_cadena.py" STUB_OPENSSL="$OSSL" \
    ${STUB_NEAR_VERSION:+STUB_NEAR_VERSION=$STUB_NEAR_VERSION} \
    ${STUB_NEAR_RC:+STUB_NEAR_RC=$STUB_NEAR_RC} ${STUB_NEAR_SLEEP:+STUB_NEAR_SLEEP=$STUB_NEAR_SLEEP} "$@" "$SH" "$CORRE" ${ARGS[@]+"${ARGS[@]}"} >"$STUB/out" 2>&1
  RC=$?
  return "$RC"
}
ok() { PASAN=$((PASAN + 1)); }
# ARRAS_TEST_PRIMER_FALLA=1 corta en el primer FALLA (las mutaciones: alcanza con un test con nombre que caiga).
mal() {
  FALLOS=$((FALLOS + 1)); printf 'FALLA %s: %s\n' "$CASO" "$*"; sed 's/^/    | /' "$STUB/out" | tail -n 12
  [ -z "${ARRAS_TEST_PRIMER_FALLA:-}" ] || exit 1
}
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
# ARRAS_TEST_SOLO=v2 corre solo los de scripts/mainnet-v2.sh (las mutaciones de ese script); por defecto, todos.
if [ "${ARRAS_TEST_SOLO:-}" != v2 ]; then
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
nuevo t21; cp "$SCRIPT" "$STUB/m.sh"; cp "$RAIZ/scripts/arras-comun.sh" "$STUB/"; CORRE=$STUB/m.sh; STUB_NEAR_SLEEP=2
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
fi # ARRAS_TEST_SOLO

# === v2: scripts/mainnet-v2.sh ========================================================================================
SCRIPT_V2=$RAIZ/scripts/mainnet-v2.sh
REV2=ddc51211ed93973ec9eb537a85bac39367f6e763
RPC_PRIV=https://rpc.privado.example/CLAVE-SECRETA-123
P2T=arras-ensayo.testnet
HUBT=$(printf 'c%.0s' {1..64})
TK=si
[ -n "$OSSL" ] || TK=no
nuevo2() { nuevo "$1"; CORRE=$SCRIPT_V2; }
# La cadena de mentira (scripts/test_cadena.py) como quedo mainnet el 2026-11-03: las llaves de antes en SSM; o testnet,
# con el llavero legacy y el hub en $STUB/hub.json. $2 = yocto del hub (10 NEAR por defecto).
cadena() {
  if [ "$1" = mainnet ]; then
    HOME="$STUB/home" STUB="$STUB" python3 -I "$RAIZ/scripts/test_cadena.py" inicial mainnet "$P_MAIN" "$FONDEO" "${2:-10000000000000000000000000}" ssm
  else
    HOME="$STUB/home" STUB="$STUB" python3 -I "$RAIZ/scripts/test_cadena.py" inicial testnet "$P2T" "$HUBT" "${2:-10000000000000000000000000}" llavero
  fi
}
wasm2() { wasm_falsa "$STUB/w2.wasm" "${1:-$REV2}"; W2SHA=$(shasum -a 256 "$STUB/w2.wasm" | cut -c1-64); }
real_main() {
  correr ARRAS_RPC="$RPC_PRIV" ARRAS_V2_WASM="$STUB/w2.wasm" ARRAS_V2_WASM_SHA256="$W2SHA" ARRAS_V2_TICKET="$TK" ${OSSL:+ARRAS_OPENSSL=$OSSL} "$@"
}
real_test() {
  correr ARRAS_RED=testnet ARRAS_PADRE="$P2T" ARRAS_FONDEO="$HUBT" ARRAS_FONDEO_ARCHIVO="$STUB/hub.json" ARRAS_V2_WASM="$STUB/w2.wasm" \
    ARRAS_V2_WASM_SHA256="$W2SHA" ARRAS_V2_TICKET="$TK" ${OSSL:+ARRAS_OPENSSL=$OSSL} "$@"
}
cad() { jq -r "$1" "$STUB/cadena.json"; }
es() { if [ "$(cad "$1")" = "$2" ]; then ok; else mal "en la cadena $1 es '$(cad "$1")', se esperaba '$2'"; fi; }
n_tx() { if [ -f "$STUB/near.log" ]; then grep -vc -- '--version' "$STUB/near.log" || true; else echo 0; fi; }
# Ninguna llave privada (las de antes, las nuevas del llavero ni la del ticket) en la salida ni en la linea de comandos.
sin_privadas() {
  local k
  { cat "$STUB/privadas" 2>/dev/null
    find "$STUB/home/.near-credentials" -name '*.json' -exec jq -r '.private_key // empty' {} \; 2>/dev/null
    find "$STUB/home" -name '*.pem' -exec grep -v -- '-----' {} \; 2>/dev/null; } >"$STUB/todas-privadas"
  while read -r k; do
    [ -n "$k" ] || continue
    if grep -qF -- "$k" "$STUB/out" "$STUB/near.log" 2>/dev/null; then mal "una llave privada aparecio en la salida o en la linea de comandos de near"; return; fi
  done <"$STUB/todas-privadas"
  ok
}
foto() { rm -rf "$TMP/foto-$1"; cp -R "$STUB" "$TMP/foto-$1"; }
desde_foto() {
  nuevo2 "$2"
  rm -rf "$STUB"; cp -R "$TMP/foto-$1" "$STUB"
  rm -f "$STUB/near.log" "$STUB/curl.log" "$STUB/aws.log" "$STUB/out" "$STUB/consultas" "$STUB"/ssm-lecturas* "$STUB"/falla.*
}
TICKET_PEM_M="home/.arras-despliegue/mainnet/$P_MAIN/v2/em-ticket-v2.pem"

# --- en seco ------------------------------------------------------------------------------------------------------------
# V01 todo en seco: sin red; v2 en v2.<padre> con los topes de v1, el operador em con capturer y arbitro nuevos, el plazo
# de 7 dias y el ticket; las FunctionCall keys acotadas a v2; cada firma por un pipe (nada del llavero en linea).
nuevo2 v01; ARGS=(todo); correr
sale 0; sin_red; dice "EN SECO"; dice "contract deploy v2.$P_MAIN use-file"; no_dice "contract deploy v1."
dice '"tope_por_pago":"10000000","tope_tvl_global":"1000000000"'
dice '"capturers":["em-capturer-v2.arras-escrow.near"]'; dice '"arbiter":"em-arbiter-v2.arras-escrow.near","arbiter_window_ns":"604800000000000"'
dice '"tope_tvl_operador":"1000000000","tope_tvl_por_payer":"20000000","sponsor_max_por_payer":"20000000000000000000000"'
dice '"ticket_pubkey":"<ticket_pubkey'; dice '"fee_bps":1300,"fee_recipient":"em-fees.arras-escrow.near","min_amount":"10000"'
dice "grant-function-call-access --allowance 0.15 NEAR --contract-account-id v2.$P_MAIN --function-names refund, release, withdraw_for"
dice "grant-function-call-access --allowance 0.1 NEAR --contract-account-id v2.$P_MAIN --function-names resolve_dispute"
dice "account delete-keys v2.$P_MAIN public-keys"; dice "tokens $FONDEO send-near v2.$P_MAIN"
dice "SSM /arras/mainnet/v2-reserva (us-east-2), por un pipe"; dice "SSM /arras/mainnet/owner (us-east-2), por un pipe"
dice "NEAR_ESCROW_CONTRACTS_MAINNET=v1.$P_MAIN,v2.$P_MAIN"; no_dice "sign-with-legacy-keychain"; no_dice "put-parameter"
# V01b el `new` de v2 es el de v1 byte a byte (los topes salen de arras-comun.sh): mismos owner, guardian, token y topes.
nuevo v01b; ARGS=(todo); correr
nuevo_v1=$(sed -n 's/.*with-init-call new json-args \({[^}]*}\).*/\1/p' "$STUB/out" | head -1)
nuevo2 v01c; ARGS=(todo); correr
nuevo_v2=$(sed -n 's/.*with-init-call new json-args \({[^}]*}\).*/\1/p' "$STUB/out" | head -1)
if [ -n "$nuevo_v1" ] && [ "$nuevo_v1" = "$nuevo_v2" ]; then ok; else mal "el new de v2 ($nuevo_v2) no es el de v1 ($nuevo_v1)"; fi
# V02 el humo en seco: presupuesto de 2 x 0,05 con ticket, el agente paga con su FunctionCall key, el capturer libera y
# retira, la disputa vuelve entera al payer y el cierre revoca al agente.
nuevo2 v02; ARGS=(humo); correr
sale 0; sin_red; dice "ft_transfer_call json-args {\"receiver_id\":\"v2.$P_MAIN\",\"amount\":\"100000\""
dice '\"action\":\"budget\",\"operator_id\":\"em\",\"agent\":\"agent-v2.arras-escrow.near\",\"tope_por_pago\":\"50000\",\"tope_diario\":\"100000\",\"operator_ticket\"'
dice "authorize_from_budget json-args {\"owner\":\"$FONDEO\",\"operator_id\":\"em\",\"receiver\":\"worker-v2.$P_MAIN\",\"amount\":\"50000\""
dice "sign-as agent-v2.$P_MAIN network-config mainnet sign-with-access-key-file <la llave de agent-v2.$P_MAIN (authorize_from_budget)"
dice '"to_receiver":"0","to_payer":"50000"'; dice "sign-as em-arbiter-v2.$P_MAIN"; dice "open_dispute json-args"
dice "withdraw_for json-args {\"account\":\"worker-v2.$P_MAIN\"}"; dice "withdraw_for json-args {\"account\":\"$FONDEO\"}"
dice "revoke_agent"; dice "storage_withdraw"
# V02b dispute_deadline = release_deadline (E67) en los dos pagos del agente.
if grep -o '"release_deadline":"[0-9]*"' "$STUB/out" | sed 's/release_/dispute_/' | while read -r d; do grep -qF -- "$d" "$STUB/out" || exit 1; done; then ok; else mal "dispute_deadline distinto de release_deadline"; fi
# V03 banderas: la de la otra red aborta, las dos juntas tambien; sin tocar nada.
nuevo2 v03; ARGS=(--testnet-ok todo); correr
sale_mal; sin_red; dice "no vale con ARRAS_RED=mainnet"
nuevo2 v03b; ARGS=(--mainnet-ok plan); correr ARRAS_RED=testnet ARRAS_PADRE=$P2T ARRAS_FONDEO=$HUBT
sale_mal; sin_red; dice "no vale con ARRAS_RED=testnet"
nuevo2 v03c; ARGS=(--mainnet-ok --testnet-ok plan); correr
sale_mal; sin_red; dice "juntas"
nuevo2 v03d; ARGS=(desplegar); correr
sale 1; dice "paso desconocido"
# V04 el padre y el hub: testnet exige ARRAS_PADRE y ARRAS_FONDEO; el hub es una implicita en minusculas; la llave de
# archivo es solo del ensayo.
nuevo2 v04; ARGS=(plan); correr ARRAS_RED=testnet ARRAS_FONDEO=$HUBT
sale_mal; dice "ARRAS_PADRE es obligatoria"
nuevo2 v04b; ARGS=(plan); correr ARRAS_RED=testnet ARRAS_PADRE=$P2T
sale_mal; dice "ARRAS_FONDEO vino vacia"
nuevo2 v04c; ARGS=(plan); correr ARRAS_FONDEO=
sale_mal; dice "ARRAS_FONDEO vino vacia"
nuevo2 v04d; ARGS=(plan); correr ARRAS_FONDEO="$(printf '%s' "$FONDEO" | tr 'a-f' 'A-F')"
sale_mal; dice "tiene que ser una cuenta implicita"
nuevo2 v04e; ARGS=(plan); correr ARRAS_FONDEO_ARCHIVO="$STUB/hub.json"
sale_mal; dice "solo del ensayo en testnet"
nuevo2 v04f; ARGS=(plan); correr ARRAS_PARENT=$P_MAIN
sale_mal; dice "se renombro a ARRAS_PADRE"
# V05 una cuenta por rol, y las que el script crea son hijas directas del padre.
for caso in "ARRAS_V2_CAPTURER=a.em-capturer-v2.$P_MAIN hija" "ARRAS_V2_ARBITRO=em-arbiter.otro.near hija" \
  "ARRAS_HUMO_AGENTE=x.agent-v2.$P_MAIN hija" "ARRAS_V2_CAPTURER=em-fees.$P_MAIN roles" "ARRAS_HUMO_WORKER=$FONDEO roles" \
  "ARRAS_HUMO_WORKER=v2.$P_MAIN roles" "ARRAS_FEE_RECIPIENT=v2.$P_MAIN roles" "ARRAS_V2_ARBITRO=em-capturer-v2.$P_MAIN roles" \
  "ARRAS_HUMO_AGENTE=owner.$P_MAIN roles" "ARRAS_HUMO_WORKER=agent-v2.$P_MAIN roles" "ARRAS_HUMO_WORKER=Worker.near valida" \
  "ARRAS_FEE_RECIPIENT=em-fees.testnet valida"; do
  set -- $caso
  nuevo2 "v05-${1%%=*}-$2"; ARGS=(plan); correr "$1"
  sale_mal; sin_red
  case "$2" in hija) dice "hija directa" ;; roles) dice "aparece en dos roles" ;; valida) dice "no es una cuenta valida" ;; esac
done
nuevo2 v05b; ARGS=(plan); correr ARRAS_HUMO_WORKER=worker.testnet ARRAS_RED=testnet ARRAS_PADRE=$P2T ARRAS_FONDEO=$HUBT
sale 0; dice "worker worker.testnet (ya existe)"
# V06 los topes son los de v1 (comun_topes): las mismas guardas.
nuevo2 v06; ARGS=(plan); correr ARRAS_TOPE_TVL_GLOBAL=5000000
sale_mal; dice "no puede ser menor que el tope por pago"
nuevo2 v06b; ARGS=(plan); correr ARRAS_FEE_BPS=1801
sale_mal; dice "ARRAS_E43"
nuevo2 v06c; ARGS=(plan); correr ARRAS_OPERATOR_ID=EM
sale_mal; dice "ARRAS_E45"
nuevo2 v06d; ARGS=(plan); correr ARRAS_TOPE_TVL_POR_PAYER=1000000001
sale_mal; dice "no puede pasar el del operador"
# V07 el plazo del arbitro en [60 s, 30 dias] (E47), en segundos enteros.
for v in 59 2592001 7d; do
  nuevo2 "v07-$v"; ARGS=(plan); correr ARRAS_V2_VENTANA_ARBITRO=$v
  sale_mal
done
nuevo2 v07b; ARGS=(todo); correr ARRAS_V2_VENTANA_ARBITRO=60
sale 0; dice '"arbiter_window_ns":"60000000000"'
nuevo2 v07c; ARGS=(todo); correr ARRAS_V2_VENTANA_ARBITRO=2592000
sale 0; dice '"arbiter_window_ns":"2592000000000000"'
# V08 el ticket: si o no (en mayusculas tambien); sin ticket, ni el operador ni el humo llevan ticket.
nuevo2 v08; ARGS=(plan); correr ARRAS_V2_TICKET=quizas
sale_mal; dice "ARRAS_V2_TICKET tiene que ser si o no"
nuevo2 v08b; ARGS=(todo); correr ARRAS_V2_TICKET=NO
sale 0; no_dice '"ticket_pubkey"'
nuevo2 v08c; ARGS=(humo); correr ARRAS_V2_TICKET=no
sale 0; no_dice "operator_ticket"
# V09 NEAR: la reserva libre no baja de 0,2 (la alarma de get_storage_health); las allowances no son 0; '<n> NEAR'.
nuevo2 v09; ARGS=(plan); correr "ARRAS_V2_RESERVA=0.19 NEAR"
sale_mal; dice "bajo 0,2 NEAR"
nuevo2 v09b; ARGS=(plan); correr "ARRAS_V2_RESERVA=0.2 NEAR"
sale 0
for v in ARRAS_V2_ALLOWANCE_CAPTURER ARRAS_V2_ALLOWANCE_ARBITRO ARRAS_HUMO_ALLOWANCE_AGENTE; do
  nuevo2 "v09-$v"; ARGS=(plan); correr "$v=0 NEAR"
  sale_mal; dice "no puede pagar gas"
done
nuevo2 v09c; ARGS=(plan); correr ARRAS_V2_POOL=0.5
sale_mal; dice "tiene que ser '<numero> NEAR'"
nuevo2 v09d; ARGS=(todo); correr "ARRAS_V2_POOL=0 NEAR"
sale 0; no_dice "fund_operator_storage json-args"
# V10 los topes del humo: 0,01 a 0,10 USDC por pago, 0,20 en total (2 x el monto).
for caso in "ARRAS_HUMO_MONTO=9999" "ARRAS_HUMO_MONTO=100001" "ARRAS_HUMO_TOPE_POR_PAGO=100001" "ARRAS_HUMO_TOPE_TOTAL=200001" \
  "ARRAS_HUMO_MONTO=1e5" "ARRAS_HUMO_MONTO=60000 ARRAS_HUMO_TOPE_POR_PAGO=50000"; do
  nuevo2 "v10-$(printf '%s' "$caso" | tr ' =' '__')"; ARGS=(plan); correr $caso
  sale_mal; sin_red
done
nuevo2 v10b; ARGS=(plan); correr ARRAS_HUMO_MONTO=100000 ARRAS_HUMO_TOPE_TOTAL=150000
sale_mal; dice "el humo mueve 2 x 100000 = 200000"
nuevo2 v10c; ARGS=(plan); correr ARRAS_MIN_AMOUNT=20000 ARRAS_HUMO_MONTO=15000
sale_mal; dice "del min_amount (20000)"
nuevo2 v10d; ARGS=(humo); correr ARRAS_HUMO_MONTO=100000
sale 0; dice '"amount":"200000"'; dice '"to_payer":"100000"'
nuevo2 v10e; ARGS=(humo); correr ARRAS_HUMO_MONTO=10000
sale 0; dice '"amount":"20000"'
# V11 mainnet en real exige ARRAS_RPC (el defecto publico solo vale para testnet), antes de cualquier lectura.
nuevo2 v11; ARGS=(--mainnet-ok plan); correr
sale_mal; sin_red; dice "ARRAS_RPC es obligatoria"
# V12 el RPC privado nunca se imprime: ni en el plan en seco, ni cuando es de otra red, ni en v1.
nuevo2 v12; ARGS=(plan); correr ARRAS_RPC="$RPC_PRIV"
sale 0; no_dice "CLAVE-SECRETA"; dice "RPC ARRAS_RPC (no se imprime)"
nuevo2 v12b; echo '{"chain_id":"testnet"}' >"$STUB/rpc/status"; ARGS=(--mainnet-ok plan); correr ARRAS_RPC="$RPC_PRIV"
sale_mal; dice "es de la red 'testnet', no de mainnet"; no_dice "CLAVE-SECRETA"
nuevo v12c; ARGS=(plan); correr ARRAS_RPC="$RPC_PRIV"
sale 0; no_dice "CLAVE-SECRETA"
nuevo v12d; echo '{"chain_id":"testnet"}' >"$STUB/rpc/status"; ARGS=(--mainnet-ok plan); correr ARRAS_RPC="$RPC_PRIV"
sale_mal; no_dice "CLAVE-SECRETA"
# V13 OpenSSL: un ARRAS_OPENSSL que no es OpenSSL 3 (el LibreSSL de macOS) corta todo al arrancar, antes de mandar nada;
# uno que no es ejecutable tambien. Sin ticket no hace falta.
nuevo2 v13; cadena mainnet; wasm2
printf '#!/bin/sh\necho "LibreSSL 3.3.6"\n' >"$STUB/openssl-falso"; chmod +x "$STUB/openssl-falso"
ARGS=(--mainnet-ok todo); correr ARRAS_RPC="$RPC_PRIV" ARRAS_V2_WASM="$STUB/w2.wasm" ARRAS_V2_WASM_SHA256="$W2SHA" ARRAS_OPENSSL="$STUB/openssl-falso"
sale_mal; dice "no OpenSSL 3"; sin_near_tx
nuevo2 v13b; cadena mainnet; wasm2
ARGS=(--mainnet-ok humo); correr ARRAS_RPC="$RPC_PRIV" ARRAS_V2_WASM="$STUB/w2.wasm" ARRAS_V2_WASM_SHA256="$W2SHA" ARRAS_OPENSSL="$STUB/no-esta"
sale_mal; dice "no es un ejecutable"; sin_near_tx
nuevo2 v13c; cadena mainnet; wasm2
ARGS=(--mainnet-ok fondeo); correr ARRAS_RPC="$RPC_PRIV" ARRAS_V2_WASM="$STUB/w2.wasm" ARRAS_V2_WASM_SHA256="$W2SHA" ARRAS_OPENSSL="$STUB/no-esta" ARRAS_V2_TICKET=no
sale 0; near_hizo "send-near v2.$P_MAIN"
# V13d enteros con ceros a la izquierda (bash los leeria en octal): se rechazan, en v1 y en v2.
nuevo2 v13d; ARGS=(plan); correr ARRAS_HUMO_MONTO=050000
sale_mal; dice "sin ceros a la izquierda"
nuevo2 v13e; ARGS=(plan); correr ARRAS_V2_VENTANA_ARBITRO=0604800
sale_mal; dice "sin ceros a la izquierda"
nuevo v13f; ARGS=(plan); correr ARRAS_TOPE_POR_PAGO=010000000
sale_mal; dice "sin ceros a la izquierda"
nuevo2 v13g; ARGS=(plan); correr ARRAS_HUMO_TOPE_TOTAL=0 ARRAS_HUMO_MONTO=10000
sale_mal; dice "el humo mueve 2 x 10000"
# V14 secretos en seco: solo el inventario, ninguna linea ejecutable.
nuevo2 v14; d="$STUB/home/.near-credentials/mainnet/em-capturer-v2.$P_MAIN"; mkdir -p "$d"
printf '{"public_key":"ed25519:PUBCAPT","private_key":"%s"}' "$PRIV_FALSA" >"$d/ed25519_PUBCAPT.json"
ARGS=(secretos); correr
sale 0; sin_red; no_dice "$PRIV_FALSA"; no_dice "put-parameter"; no_dice "rm '"
dice "em-capturer-v2 (em-capturer-v2.$P_MAIN): ed25519:PUBCAPT en $d/ed25519_PUBCAPT.json; nombre: /arras/mainnet/em-capturer-v2"

# --- en real, contra la cadena de mentira -------------------------------------------------------------------------------
# VR01 todo en mainnet: las llaves de antes salen de SSM por un pipe (la cadena rechaza una tx que no firme una llave viva
# con el permiso que hace falta), v2 queda con la wasm del pin y sin llaves, el operador como se decidio y el capturer y
# el arbitro con UNA FunctionCall key cada uno. Ninguna llave privada en la salida ni en la linea de comandos.
nuevo2 vr01; cadena mainnet; wasm2; ARGS=(--mainnet-ok todo); real_main
sale 0; sin_privadas; dice "OK: view_access_key_list de v2.$P_MAIN vacio"; dice "OK: NEP-330 de v2.$P_MAIN: git+https://github.com/UltravioletaDAO/arras?rev=$REV2"
es ".cuentas[\"v2.$P_MAIN\"].code" "$W2SHA"; es ".cuentas[\"v2.$P_MAIN\"].keys | length" 0
es ".cuentas[\"v2.$P_MAIN\"].contrato.operators.em.capturers | join(\",\")" "em-capturer-v2.$P_MAIN"
es ".cuentas[\"v2.$P_MAIN\"].contrato.operators.em.arbiter" "em-arbiter-v2.$P_MAIN"
es ".cuentas[\"v2.$P_MAIN\"].contrato.operators.em.arbiter_window_ns" 604800000000000
es ".cuentas[\"v2.$P_MAIN\"].contrato.operators.em.sponsor_pool" 100000000000000000000000
es ".cuentas[\"em-capturer-v2.$P_MAIN\"].keys | map(.access_key.permission.FunctionCall | .receiver_id + \":\" + (.method_names | sort | join(\",\"))) | join(\" \")" "v2.$P_MAIN:refund,release,withdraw_for"
es ".cuentas[\"em-arbiter-v2.$P_MAIN\"].keys | map(.access_key.permission.FunctionCall | .receiver_id + \":\" + (.method_names | join(\",\"))) | join(\" \")" "v2.$P_MAIN:resolve_dispute"
es ".usdc.registradas | index(\"v2.$P_MAIN\") != null" true
es ".cuentas[\"v1.$P_MAIN\"].code" "$(printf '70%.0s' {1..32})"
near_hizo "sign-with-access-key-file /dev/fd/"; no_dice "sign-with-legacy-keychain"
for rol in fondeo padre v2-reserva owner; do
  if grep -qF -- "--name /arras/mainnet/$rol --with-decryption" "$STUB/aws.log"; then ok; else mal "no leyo /arras/mainnet/$rol de SSM"; fi
done
if [ "$TK" = si ]; then
  es ".cuentas[\"v2.$P_MAIN\"].contrato.operators.em.ticket_pubkey" "ed25519:$("$OSSL" pkey -in "$STUB/$TICKET_PEM_M" -pubout -outform DER | tail -c 32 | python3 -I -c '
import sys
a = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"
b = sys.stdin.buffer.read(); n = int.from_bytes(b, "big"); s = ""
while n: n, r = divmod(n, 58); s = a[r] + s
print(s)')"
  if [ -n "$(find "$STUB/$TICKET_PEM_M" -perm 600 2>/dev/null)" ]; then ok; else mal "la llave del ticket no quedo en modo 600"; fi
fi
foto todo-main
# VR02 un segundo todo no manda ninguna tx (idempotente).
CASO=vr02; antes=$(n_tx); ARGS=(--mainnet-ok todo); real_main
sale 0; if [ "$(n_tx)" = "$antes" ]; then ok; else mal "el segundo todo mando $(( $(n_tx) - antes )) tx"; fi
# VR03 el humo: el agente paga con su FunctionCall key y el ticket, el capturer libera y retira, la disputa vuelve entera
# al hub; al cerrar, el presupuesto no existe y v2 no guarda USDC.
CASO=vr03; ARGS=(--mainnet-ok humo); real_main
sale 0; sin_privadas
es ".cuentas[\"v2.$P_MAIN\"].contrato.payments | map(.status) | sort | join(\",\")" "Released,Resolved"
es ".cuentas[\"v2.$P_MAIN\"].contrato.payments | map(select(.status == \"Resolved\") | .refunded) | join(\"\")" 50000
es ".cuentas[\"v2.$P_MAIN\"].contrato.budgets | length" 0
es ".usdc.saldos[\"worker-v2.$P_MAIN\"]" 43500; es ".usdc.saldos[\"em-fees.$P_MAIN\"]" 6500
es ".usdc.saldos[\"$FONDEO\"]" 3907303; es ".usdc.saldos[\"v2.$P_MAIN\"]" 0
es ".cuentas[\"agent-v2.$P_MAIN\"].keys | map(.access_key.permission.FunctionCall.method_names | join(\",\")) | join(\" \")" authorize_from_budget
es "[.cuentas[\"v2.$P_MAIN\"].contrato.claimable[] | select(. != \"0\")] | length" 0
dice "--name '/arras/mainnet/humo-agente-v2' --type SecureString"; dice "--name '/arras/mainnet/humo-worker-v2' --type SecureString"
foto humo-main
# VR04 un segundo humo no manda ninguna tx: sigue cada pago por su estado.
CASO=vr04; antes=$(n_tx); ARGS=(--mainnet-ok humo); real_main
sale 0; if [ "$(n_tx)" = "$antes" ]; then ok; else mal "el segundo humo mando $(( $(n_tx) - antes )) tx"; fi
# VR05 secretos en real: una linea por llave viva nueva (FunctionCall keys y la del ticket), solo rm para las FullAccess
# ya borradas, y lo de v2 (sin llaves) como comentario.
desde_foto humo-main vr05; ARGS=(--mainnet-ok secretos); real_main
sale 0; sin_privadas
dice "--name '/arras/mainnet/em-capturer-v2' --type SecureString --key-id 'alias/arras-test'"
dice "--name '/arras/mainnet/em-arbiter-v2' --type SecureString"; dice "--name '/arras/mainnet/humo-agente-v2' --type SecureString"
dice "--name '/arras/mainnet/humo-worker-v2' --type SecureString"; dice "(muerta)"; dice "# ya no esta en view_access_key_list"
dice "# El parametro /arras/mainnet/v2-reserva guarda una llave que ya no esta en cadena"
if [ "$TK" = si ]; then dice "--name '/arras/mainnet/em-ticket-v2' --type SecureString"; fi
if [ "$(grep -c 'put-parameter' "$STUB/out")" = "$(( 4 + $( [ "$TK" = si ] && echo 1 || echo 0) ))" ]; then ok; else mal "secretos subio $(grep -c 'put-parameter' "$STUB/out") llaves"; fi
# VR06 verificar despues del humo: solo lectura (no lee SSM), todo en verde.
desde_foto humo-main vr06; ARGS=(--mainnet-ok verificar); real_main
sale 0; sin_near_tx; dice "una sola llave, la FunctionCall de v2.$P_MAIN (refund,release,withdraw_for)"
if [ -f "$STUB/aws.log" ]; then mal "verificar leyo SSM"; else ok; fi
# VR07 el ensayo de testnet: el mismo todo con las llaves del llavero y el hub de ARRAS_FONDEO_ARCHIVO; no se lee SSM.
nuevo2 vr07; cadena testnet; wasm2; ARGS=(--testnet-ok todo); real_test
sale 0; sin_privadas; es ".cuentas[\"v2.$P2T\"].keys | length" 0; es ".cuentas[\"v2.$P2T\"].code" "$W2SHA"
if [ -f "$STUB/aws.log" ]; then mal "el ensayo de testnet leyo SSM"; else ok; fi
CASO=vr07b; ARGS=(--testnet-ok humo); real_test
sale 0; es ".usdc.saldos[\"$HUBT\"]" 3907303

# --- en real: las guardas que cortan antes de firmar ----------------------------------------------------------------
# VN01 fondeo: si el hub no tiene TODO lo del despliegue, no manda nada; con lo justo, si (el borde exacto).
p_todo() {
  python3 -I -c '
import sys
w = int(sys.argv[1]); E = 10**24
v2 = (w + 1000) * 10**19 + E // 4 + 5 * 10**21 + 10**21 - E // 10
v2 = -(-v2 // 10**21) * 10**21
print(v2 + 10**21 + 225 * 10**19 + 101 * 10**21 + int(sys.argv[2]))' "$(wc -c <"$STUB/w2.wasm" | tr -d ' ')" "$1"
}
# (182 bytes del hub = 1820000000000000000000 yocto: como string, que en bash 10^21 desborda los 64 bits)
nuevo2 vn01; wasm2; cadena mainnet "$(p_todo 1819999999999999999999)"
jq '.cuentas["arras-escrow.near"].amount = "10000000000000000000000000"' "$STUB/cadena.json" >"$STUB/c.json" && mv "$STUB/c.json" "$STUB/cadena.json"
ARGS=(--mainnet-ok fondeo); real_main
sale_mal; sin_near_tx; dice "no se manda nada"
nuevo2 vn01b; wasm2; cadena mainnet "$(p_todo 1820000000000000000000)"
jq '.cuentas["arras-escrow.near"].amount = "10000000000000000000000000"' "$STUB/cadena.json" >"$STUB/c.json" && mv "$STUB/c.json" "$STUB/cadena.json"
ARGS=(--mainnet-ok fondeo); real_main
sale 0; near_hizo "tokens $FONDEO send-near v2.$P_MAIN"; no_dice "send-near $P_MAIN"; sin_privadas
# VN02 una llave de SSM que no esta viva en cadena: no se firma nada. (El padre con NEAR: la guarda es la de la llave.)
padre_con_near() { jq --arg p "$1" '.cuentas[$p].amount = "10000000000000000000000000"' "$STUB/cadena.json" >"$STUB/c.json" && mv "$STUB/c.json" "$STUB/cadena.json"; }
nuevo2 vn02; cadena mainnet; wasm2; padre_con_near "$P_MAIN"
python3 -I -c '
import json, secrets, sys
A = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"
def b58(b):
    v, s = int.from_bytes(b, "big"), ""
    while v: v, r = divmod(v, 58); s = A[r] + s
    return s
pub = secrets.token_bytes(32)
print(json.dumps({"account_id": sys.argv[1], "public_key": "ed25519:" + b58(pub), "private_key": "ed25519:" + b58(secrets.token_bytes(32) + pub)}))' "$P_MAIN" >"$STUB/ssm/_arras_mainnet_padre"
jq -r .private_key "$STUB/ssm/_arras_mainnet_padre" >>"$STUB/privadas"
ARGS=(--mainnet-ok cuentas); real_main
sale_mal; sin_near_tx; dice "no se pudo leer o no es una llave viva suya"; sin_privadas
# VN03 en testnet, una llave que no esta en el llavero: aborta sin leer SSM.
nuevo2 vn03; cadena testnet; wasm2; padre_con_near "$P2T"; rm -rf "$STUB/home/.near-credentials/testnet/$P2T"
ARGS=(--testnet-ok cuentas); real_test
sale_mal; sin_near_tx; dice "en testnet no se lee SSM"; if [ -f "$STUB/aws.log" ]; then mal "leyo SSM en testnet"; else ok; fi
# VN04 die dentro de <(llave ...) corta el script: la primera lectura (la validacion) sale bien y la del pipe no.
nuevo2 vn04; cadena mainnet; wasm2; padre_con_near "$P_MAIN"; echo 'basura' >"$STUB/ssm/_arras_mainnet_padre.2"
ARGS=(--mainnet-ok cuentas); real_main
sale_mal; dice "no se pudo leer"; if grep -q "em-arbiter-v2" "$STUB/near.log"; then mal "siguio despues del die en el pipe"; else ok; fi
# VN05 deploy: otra wasm, otro codigo en v2, o v2 sin fondos: no se despliega.
desde_foto todo-main vn05; jq --arg h "$(printf '0%.0s' {1..64})" '.cuentas["v2.arras-escrow.near"].code = $h | del(.cuentas["v2.arras-escrow.near"].contrato)' "$STUB/cadena.json" >"$STUB/c.json" && mv "$STUB/c.json" "$STUB/cadena.json"
wasm_falsa "$STUB/w2.wasm" "$(printf 'a%.0s' {1..40})"
ARGS=(--mainnet-ok deploy); real_main
sale_mal; sin_near_tx; dice "los pines son"
nuevo2 vn05b; cadena mainnet; wasm2
jq '.cuentas["v2.arras-escrow.near"].code = "'"$(printf 'b%.0s' {1..64})"'"' "$STUB/cadena.json" >"$STUB/c.json" && mv "$STUB/c.json" "$STUB/cadena.json"
ARGS=(--mainnet-ok deploy); real_main
sale_mal; sin_near_tx; dice "ya tiene otro codigo"
nuevo2 vn05c; cadena mainnet; wasm2
jq '.cuentas["v2.arras-escrow.near"].amount = "10000000000000000000000"' "$STUB/cadena.json" >"$STUB/c.json" && mv "$STUB/c.json" "$STUB/cadena.json"
ARGS=(--mainnet-ok deploy); real_main
sale_mal; sin_near_tx; dice "corre el paso fondeo"
# VN06 sin-llaves: v2 con un codigo que no es el del pin (o un get_config distinto) conserva sus llaves.
nuevo2 vn06; cadena mainnet; wasm2
jq '.cuentas["v2.arras-escrow.near"].code = "'"$(printf 'b%.0s' {1..64})"'"' "$STUB/cadena.json" >"$STUB/c.json" && mv "$STUB/c.json" "$STUB/cadena.json"
ARGS=(--mainnet-ok sin-llaves); real_main
sale_mal; sin_near_tx; dice "no se borran sus llaves"
for campo in "owner \"otro.near\"" "paused true" "version \"arras-v1\"" "tope_tvl_global \"200000000\""; do
  set -- $campo
  nuevo2 "vn06-$1"; cp -R "$TMP/foto-todo-main/." "$STUB/"; rm -f "$STUB/near.log" "$STUB/aws.log" "$STUB/curl.log"
  jq --arg k "$1" --argjson v "$2" '.cuentas["v2.arras-escrow.near"].keys = [{"public_key":"ed25519:AAA","access_key":{"nonce":0,"permission":"FullAccess"}}] | .cuentas["v2.arras-escrow.near"].contrato.config[$k] = $v' "$STUB/cadena.json" >"$STUB/c.json" && mv "$STUB/c.json" "$STUB/cadena.json"
  ARGS=(--mainnet-ok sin-llaves); real_main
  sale_mal; sin_near_tx; dice "no es el esperado"
done
# VN07 operador: registrado sin ticket con ARRAS_V2_TICKET=si (o al reves), o con otro arbitro, plazo, capturer o
# fee_recipient: aborta antes del pool.
desde_foto todo-main vn07
jq '.cuentas["v2.arras-escrow.near"].contrato.operators.em.ticket_pubkey = null | .cuentas["v2.arras-escrow.near"].contrato.operators.em.sponsor_pool = "0"' "$STUB/cadena.json" >"$STUB/c.json" && mv "$STUB/c.json" "$STUB/cadena.json"
ARGS=(--mainnet-ok operador); correr ARRAS_RPC="$RPC_PRIV" ARRAS_V2_WASM="$STUB/w2.wasm" ARRAS_V2_WASM_SHA256="$W2SHA" ARRAS_V2_TICKET=si
sale_mal; sin_near_tx; dice "registrado sin ticket_pubkey y ARRAS_V2_TICKET=si"
desde_foto todo-main vn07b
jq '.cuentas["v2.arras-escrow.near"].contrato.operators.em.ticket_pubkey = "ed25519:AAA"' "$STUB/cadena.json" >"$STUB/c.json" && mv "$STUB/c.json" "$STUB/cadena.json"
ARGS=(--mainnet-ok operador); correr ARRAS_RPC="$RPC_PRIV" ARRAS_V2_WASM="$STUB/w2.wasm" ARRAS_V2_WASM_SHA256="$W2SHA" ARRAS_V2_TICKET=no
sale_mal; sin_near_tx; dice "tiene ticket_pubkey (ed25519:AAA) y ARRAS_V2_TICKET=no"
for campo in "arbiter otro.near" "arbiter_window_ns 60000000000" "fee_recipient otro.near" "min_amount 1" "tope_tvl_por_payer 30000000"; do
  set -- $campo
  desde_foto todo-main "vn07-$1"
  jq --arg k "$1" --arg v "$2" '.cuentas["v2.arras-escrow.near"].contrato.operators.em[$k] = $v | .cuentas["v2.arras-escrow.near"].contrato.operators.em.sponsor_pool = "0"' "$STUB/cadena.json" >"$STUB/c.json" && mv "$STUB/c.json" "$STUB/cadena.json"
  ARGS=(--mainnet-ok operador); real_main
  sale_mal; sin_near_tx; dice "no es el esperado"
done
desde_foto todo-main vn07c
jq '.cuentas["v2.arras-escrow.near"].contrato.operators.em.capturers = ["em-capturer.arras-escrow.near"]' "$STUB/cadena.json" >"$STUB/c.json" && mv "$STUB/c.json" "$STUB/cadena.json"
ARGS=(--mainnet-ok verificar); real_main
sale_mal; dice "no es el esperado"
# VN08 capturer: con una FunctionCall key de mas (otro metodo) no queda con UNA sola llave: aborta.
nuevo2 vn08; cadena testnet; wasm2
ARGS=(--testnet-ok fondeo); real_test; ARGS=(--testnet-ok cuentas); real_test
jq --arg c "v2.$P2T" '.cuentas["em-capturer-v2.arras-ensayo.testnet"].keys += [{"public_key":"ed25519:OTRA","access_key":{"nonce":0,"permission":{"FunctionCall":{"allowance":"1","receiver_id":$c,"method_names":["release","refund","withdraw_for","reclaim"]}}}}]' "$STUB/cadena.json" >"$STUB/c.json" && mv "$STUB/c.json" "$STUB/cadena.json"
ARGS=(--testnet-ok capturer); real_test
sale_mal; dice "tiene que quedar con UNA sola llave"
# VN08b una FunctionCall key con otra allowance no deja borrar la FullAccess.
nuevo2 vn08b; cadena testnet; wasm2; ARGS=(--testnet-ok fondeo); real_test; ARGS=(--testnet-ok cuentas); real_test
d="$STUB/home/.near-credentials/testnet/em-capturer-v2.$P2T"; echo '{"public_key":"ed25519:FCX"}' >"$d/ed25519_FCX.json"
jq --arg c "v2.$P2T" '.cuentas["em-capturer-v2.arras-ensayo.testnet"].keys += [{"public_key":"ed25519:FCX","access_key":{"nonce":0,"permission":{"FunctionCall":{"allowance":"1","receiver_id":$c,"method_names":["refund","release","withdraw_for"]}}}}]' "$STUB/cadena.json" >"$STUB/c.json" && mv "$STUB/c.json" "$STUB/cadena.json"
antes=$(n_tx); ARGS=(--testnet-ok capturer); real_test
sale_mal; dice "no tiene allowance 0.15 NEAR"; if [ "$(n_tx)" = "$antes" ]; then ok; else mal "mando una tx"; fi
# VN09 verificar: NEP-330 con otro rev, o get_storage_health en alarma, aborta.
desde_foto todo-main vn09; jq '.cuentas["v2.arras-escrow.near"].contrato.rev = "'"$(printf 'e%.0s' {1..40})"'"' "$STUB/cadena.json" >"$STUB/c.json" && mv "$STUB/c.json" "$STUB/cadena.json"
ARGS=(--mainnet-ok verificar); real_main
sale_mal; dice "no apunta a rev $REV2"
desde_foto todo-main vn09b; jq '.cuentas["v2.arras-escrow.near"].amount = "1"' "$STUB/cadena.json" >"$STUB/c.json" && mv "$STUB/c.json" "$STUB/cadena.json"
ARGS=(--mainnet-ok verificar); real_main
sale_mal; dice "en alarma"
# VN10 humo: sin el USDC del presupuesto, o con un presupuesto distinto ya depositado, no mueve nada.
desde_foto todo-main vn10; ARGS=(--mainnet-ok humo-cuentas); real_main
jq --arg h "$FONDEO" '.usdc.saldos[$h] = "99999"' "$STUB/cadena.json" >"$STUB/c.json" && mv "$STUB/c.json" "$STUB/cadena.json"
antes=$(n_tx); ARGS=(--mainnet-ok humo-presupuesto); real_main
sale_mal; dice "el humo necesita 100000"; if [ "$(n_tx)" = "$antes" ]; then ok; else mal "mando una tx"; fi
desde_foto todo-main vn10b; ARGS=(--mainnet-ok humo-cuentas); real_main
jq --arg k "$FONDEO|agent-v2.$P_MAIN|em" '.cuentas["v2.arras-escrow.near"].contrato.budgets[$k] = {"saldo":"7"}' "$STUB/cadena.json" >"$STUB/c.json" && mv "$STUB/c.json" "$STUB/cadena.json"
antes=$(n_tx); ARGS=(--mainnet-ok humo-presupuesto); real_main
sale_mal; dice "con otro saldo"; if [ "$(n_tx)" = "$antes" ]; then ok; else mal "mando una tx"; fi
# VN11 humo antes de todo: no mueve USDC.
nuevo2 vn11; cadena mainnet; wasm2; ARGS=(--mainnet-ok humo); real_main
sale_mal; sin_near_tx; dice "el humo va despues de todo"
# VN12 humo con la llave del ticket equivocada: aborta antes de firmar el deposito.
if [ "$TK" = si ]; then
  desde_foto todo-main vn12; "$OSSL" genpkey -algorithm ed25519 -out "$STUB/$TICKET_PEM_M" 2>/dev/null
  ARGS=(--mainnet-ok humo); real_main
  sale_mal; dice "no es la ticket_pubkey del operador em"; if grep -q ft_transfer_call "$STUB/near.log"; then mal "deposito con otra llave"; else ok; fi
fi
# VN13 la disputa: si el arbitro no le devolvio todo al payer, el humo no sigue como si nada.
desde_foto humo-main vn13
jq '(.cuentas["v2.arras-escrow.near"].contrato.payments[] | select(.status == "Resolved")).refunded = "1"' "$STUB/cadena.json" >"$STUB/c.json" && mv "$STUB/c.json" "$STUB/cadena.json"
ARGS=(--mainnet-ok humo-disputa); real_main
sale_mal; dice "no le devolvio todo al payer"
# VN14 el token del ensayo (ARRAS_USDC_TESTNET) es solo de testnet: en mainnet aborta, en v1 y en v2; en testnet va al new.
nuevo2 vn14; ARGS=(plan); correr ARRAS_USDC_TESTNET=usdc-ensayo.testnet
sale_mal; sin_red; dice "ARRAS_USDC_TESTNET es solo del ensayo en testnet"
nuevo vn14b; ARGS=(plan); correr ARRAS_USDC_TESTNET=usdc-ensayo.testnet
sale_mal; sin_red; dice "ARRAS_USDC_TESTNET es solo del ensayo en testnet"
nuevo2 vn14c; ARGS=(todo); correr ARRAS_RED=testnet ARRAS_PADRE=$P2T ARRAS_FONDEO=$HUBT ARRAS_USDC_TESTNET=usdc-ensayo.testnet
sale 0; dice '"token":"usdc-ensayo.testnet"'; dice "call-function as-transaction usdc-ensayo.testnet storage_deposit"
nuevo2 vn14d; ARGS=(plan); correr ARRAS_RED=testnet ARRAS_PADRE=$P2T ARRAS_FONDEO=$HUBT ARRAS_USDC_TESTNET=Usdc.testnet
sale_mal; dice "no es una cuenta valida"
# VN15 una lectura del RPC que falla una vez se reintenta (rpc_query): verificar termina igual.
desde_foto todo-main vn15; touch "$STUB/falla.3"; ARGS=(--mainnet-ok verificar); real_main
sale 0; dice "llaves de v2.$P_MAIN: 0"
desde_foto todo-main vn15b; touch "$STUB/falla.3" "$STUB/falla.4" "$STUB/falla.5"; ARGS=(--mainnet-ok verificar); real_main
sale_mal; dice "no respondio"; no_dice "CLAVE-SECRETA"
# VN16 sin-llaves: si despues del borrado a v2 le queda una llave, aborta (la cadena no borra: $STUB/no-borrar).
nuevo2 vn16; cadena mainnet; wasm2
for paso in fondeo cuentas usdc deploy; do ARGS=(--mainnet-ok "$paso"); real_main; done
touch "$STUB/no-borrar"; ARGS=(--mainnet-ok sin-llaves); real_main
sale_mal; near_hizo "account delete-keys v2.$P_MAIN"; dice "le quedan 1 llaves"
# VN17 capturer: una FunctionCall key en cadena que no esta en el llavero no deja borrar la FullAccess (seria la unica
# forma de arreglarlo).
nuevo2 vn17; cadena testnet; wasm2; ARGS=(--testnet-ok fondeo); real_test; ARGS=(--testnet-ok cuentas); real_test
jq --arg c "v2.$P2T" '.cuentas["em-capturer-v2.arras-ensayo.testnet"].keys += [{"public_key":"ed25519:FCY","access_key":{"nonce":0,"permission":{"FunctionCall":{"allowance":"150000000000000000000000","receiver_id":$c,"method_names":["release","refund","withdraw_for"]}}}}]' "$STUB/cadena.json" >"$STUB/c.json" && mv "$STUB/c.json" "$STUB/cadena.json"
antes=$(n_tx); ARGS=(--testnet-ok capturer); real_test
sale_mal; dice "no esta en el llavero legacy"; if [ "$(n_tx)" = "$antes" ]; then ok; else mal "mando una tx"; fi
if [ "$TK" = si ]; then
  # VN18 verificar con el PEM local de otra llave: no es la ticket_pubkey del operador, aborta.
  desde_foto todo-main vn18; "$OSSL" genpkey -algorithm ed25519 -out "$STUB/$TICKET_PEM_M" 2>/dev/null
  ARGS=(--mainnet-ok verificar); real_main
  sale_mal; dice "no es el esperado"
  # VN19 secretos con el PEM de otra llave: no se sube (OJO) y las demas lineas siguen.
  desde_foto humo-main vn19; "$OSSL" genpkey -algorithm ed25519 -out "$STUB/$TICKET_PEM_M" 2>/dev/null
  ARGS=(--mainnet-ok secretos); real_main
  sale 0; dice "no es la ticket_pubkey del operador em en cadena"; no_dice "--name '/arras/mainnet/em-ticket-v2'"
  dice "--name '/arras/mainnet/em-capturer-v2'"
fi
# --- la ronda del refutador (P2 y P3) y la mutacion que sobrevivio (M21) ---------------------------------------------
# VN20 (P2-1) si la vista del payment_id falla, el humo no deja un pago guardado con el id vacio; con la vista de vuelta,
# sigue normal. Un archivo con un id invalido aborta y no se usa.
desde_foto todo-main vn20; ARGS=(--mainnet-ok humo-cuentas); real_main; ARGS=(--mainnet-ok humo-presupuesto); real_main
touch "$STUB/falla-compute_budget_payment_id"; ARGS=(--mainnet-ok humo-pago); real_main
sale_mal; if [ -e "$STUB/$(dirname "$TICKET_PEM_M")/humo/pago.json" ]; then mal "quedo un pago.json con la vista caida"; else ok; fi
rm -f "$STUB/falla-compute_budget_payment_id"; CASO=vn20b; ARGS=(--mainnet-ok humo-pago); real_main
sale 0; es ".cuentas[\"v2.$P_MAIN\"].contrato.payments | map(.status) | join(\",\")" Released
desde_foto todo-main vn20c; mkdir -p "$STUB/$(dirname "$TICKET_PEM_M")/humo"
echo '{"args":{},"payment_id":""}' >"$STUB/$(dirname "$TICKET_PEM_M")/humo/pago.json"
ARGS=(--mainnet-ok humo-pago); real_main
sale_mal; dice "tiene un payment_id invalido"
# VN20d (P2-1) el presupuesto lo decide la cadena: con un pago del humo ya hecho y el presupuesto revocado, no se
# deposita otro.
desde_foto humo-main vn20d; antes=$(n_tx); ARGS=(--mainnet-ok humo-presupuesto); real_main
sale 0; dice "ya se uso y se revoco"; if [ "$(n_tx)" = "$antes" ]; then ok; else mal "deposito otro presupuesto"; fi
# VN21 (P3-4) un fee_recipient sin FullAccess (el capturer de v1) no se registra: la tarifa quedaria atrapada.
nuevo2 vn21; cadena mainnet; wasm2
for paso in fondeo cuentas usdc deploy; do ARGS=(--mainnet-ok "$paso"); real_main ARRAS_FEE_RECIPIENT="em-capturer.$P_MAIN"; done
ARGS=(--mainnet-ok operador); real_main ARRAS_FEE_RECIPIENT="em-capturer.$P_MAIN"
sale_mal; dice "no tiene ninguna FullAccess"; if grep -q register_operator "$STUB/near.log"; then mal "registro el operador"; else ok; fi
# VN21b ...y un worker que ya existe sin llaves (otra hija del padre): el humo no le paga.
desde_foto todo-main vn21b
jq '.cuentas["registry.arras-escrow.near"] = {"amount":"1000000000000000000000000","storage":500,"code":"'"$(printf '7%.0s' {1..64})"'","keys":[]}' "$STUB/cadena.json" >"$STUB/c.json" && mv "$STUB/c.json" "$STUB/cadena.json"
antes=$(n_tx); ARGS=(--mainnet-ok humo-cuentas); real_main ARRAS_HUMO_WORKER="registry.$P_MAIN"
sale_mal; dice "no tiene ninguna FullAccess"; if [ "$(n_tx)" = "$antes" ]; then ok; else mal "mando una tx"; fi
# VN22 (P3-2) el pool se fondea una vez: si EM lo gasto, un segundo operador no manda otro.
desde_foto todo-main vn22
jq '.cuentas["v2.arras-escrow.near"].contrato.operators.em.sponsor_pool = "0"' "$STUB/cadena.json" >"$STUB/c.json" && mv "$STUB/c.json" "$STUB/cadena.json"
ARGS=(--mainnet-ok operador); real_main
sale 0; dice "ya se fondeo una vez"; if grep -q fund_operator_storage "$STUB/near.log"; then mal "volvio a fondear el pool"; else ok; fi
# VN23 (P3-1) el libre del hub para el humo cuenta el deposito NEP-145 que falta: 0,12 libres no alcanzan (0,1 + 0,03).
desde_foto todo-main vn23; ARGS=(--mainnet-ok humo-cuentas); real_main
jq --arg h "$FONDEO" '.cuentas[$h].amount = "121820000000000000000000"' "$STUB/cadena.json" >"$STUB/c.json" && mv "$STUB/c.json" "$STUB/cadena.json"
antes=$(n_tx); ARGS=(--mainnet-ok humo-presupuesto); real_main
sale_mal; dice "el humo necesita 0.13100 NEAR"; if [ "$(n_tx)" = "$antes" ]; then ok; else mal "mando una tx"; fi
# VN24 (M21) verificar: una FunctionCall key de mas DESPUES de la buena tambien es una llave de mas.
desde_foto todo-main vn24
jq --arg c "v2.$P_MAIN" '.cuentas["em-capturer-v2.arras-escrow.near"].keys += [{"public_key":"ed25519:OTRA","access_key":{"nonce":0,"permission":{"FunctionCall":{"allowance":"1","receiver_id":$c,"method_names":["release"]}}}}]' "$STUB/cadena.json" >"$STUB/c.json" && mv "$STUB/c.json" "$STUB/cadena.json"
ARGS=(--mainnet-ok verificar); real_main
sale_mal; dice "tiene que quedar con UNA sola llave"
# VN25 (P3-9) verificar exige v2 registrada en USDC.
desde_foto todo-main vn25
jq '.usdc.registradas -= ["v2.arras-escrow.near"]' "$STUB/cadena.json" >"$STUB/c.json" && mv "$STUB/c.json" "$STUB/cadena.json"
ARGS=(--mainnet-ok verificar); real_main
sale_mal; dice "no esta registrada en USDC"
# VN26 (P3-7) un withdraw_for que no entrega el USDC (claimable en 0 pero el saldo no sube) no se da por bueno.
desde_foto todo-main vn26; ARGS=(--mainnet-ok humo-cuentas); real_main; ARGS=(--mainnet-ok humo-presupuesto); real_main
jq '.usdc.registradas -= ["worker-v2.arras-escrow.near"]' "$STUB/cadena.json" >"$STUB/c.json" && mv "$STUB/c.json" "$STUB/cadena.json"
ARGS=(--mainnet-ok humo-pago); real_main
sale_mal; dice "despues del withdraw_for"
desde_foto todo-main vn26b; ARGS=(--mainnet-ok humo-cuentas); real_main; ARGS=(--mainnet-ok humo-presupuesto); real_main
touch "$STUB/retiro-roto"; ARGS=(--mainnet-ok humo-pago); real_main
sale_mal; dice "no recibio los 43500 del withdraw_for"
# VN27 (P3-9) sin-llaves exige tambien el NEP-330 del pin: con otro commit en la metadata, v2 conserva su llave.
nuevo2 vn27; cadena mainnet; wasm2
for paso in fondeo cuentas usdc deploy; do ARGS=(--mainnet-ok "$paso"); real_main; done
jq '.cuentas["v2.arras-escrow.near"].contrato.rev = "'"$(printf 'e%.0s' {1..40})"'"' "$STUB/cadena.json" >"$STUB/c.json" && mv "$STUB/c.json" "$STUB/cadena.json"
antes=$(n_tx); ARGS=(--mainnet-ok sin-llaves); real_main
sale_mal; dice "no apunta a rev $REV2"; if [ "$(n_tx)" = "$antes" ]; then ok; else mal "borro las llaves de v2"; fi
# VN28 (P3-6) la URL de ARRAS_RPC no va en la linea de comandos de curl (ps la mostraria con su API key), en v2 y en v1.
desde_foto todo-main vn28; ARGS=(--mainnet-ok verificar); real_main
sale 0; if grep -q "CLAVE-SECRETA" "$STUB/curl-args.log"; then mal "el RPC privado quedo en los argumentos de curl"; else ok; fi
nuevo vn28b; ARGS=(--mainnet-ok verificar); correr ARRAS_RPC="$RPC_PRIV"
if grep -q "CLAVE-SECRETA" "$STUB/curl-args.log" 2>/dev/null; then mal "v1: el RPC privado quedo en los argumentos de curl"; else ok; fi
if [ -s "$STUB/curl-args.log" ]; then ok; else mal "v1 no consulto el RPC (el caso no prueba nada)"; fi

[ "$TK" = si ] || printf '\nAVISO: sin OpenSSL 3; los casos de v2 corrieron sin ticket y la cadena no verifico firmas\n'

printf '\nscripts/test_mainnet.sh: %s asserts pasan, %s fallan\n' "$PASAN" "$FALLOS"
[ "$FALLOS" -eq 0 ]
