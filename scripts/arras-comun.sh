#!/usr/bin/env bash
# Lo comun de scripts/mainnet.sh (Arras v1) y scripts/mainnet-v2.sh (Arras v2), movido tal cual desde mainnet.sh para
# no duplicarlo: argumentos y banderas, la red, el modo (en seco o real), las reglas de cuentas, los topes, las
# transacciones de near, las lecturas por RPC, la wasm pineada, las llaves por un pipe, la linea de cada secreto y la
# preparacion del modo real. Se carga con `.` desde los dos scripts; solo define funciones y el trap de `die`.
# near-cli-rs 0.30.1; corre con el /bin/bash 3.2 de macOS (sin arreglos asociativos ni ${x,,}).
#
# Los topes viven aca y solo aca (`comun_topes`): v2 sale con los mismos que v1 por construccion.

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
# Deja BANDERA (mainnet | testnet | vacia) y PASO.
comun_argumentos() {
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
}

# --- red y modo -----------------------------------------------------------------------------------------------------
# Deja RED, SUF, USDC, RPC, RPC_NOMBRE, NB, P, FONDEO, NET y MODO. RPC_NOMBRE es lo que se imprime en vez del RPC: si
# viene de ARRAS_RPC (el privado de c0der, con su API key en la URL) no se imprime nunca; el defecto publico si.
comun_red() {
  RED=$(printf '%s' "${ARRAS_RED:-mainnet}" | tr '[:upper:]' '[:lower:]')
  case "$RED" in
    mainnet)
      [ -z "${ARRAS_USDC_TESTNET:-}" ] || die "ARRAS_USDC_TESTNET es solo del ensayo en testnet: en mainnet el token es el USDC nativo"
      SUF=near
      USDC=17208628f84f5d6ad33f0da3bbbeb27ffcb398eac501a31bd6ad2011e36133a1
      RPC=${ARRAS_RPC:-https://rpc.mainnet.fastnear.com}
      NB=https://nearblocks.io
      P=${ARRAS_PADRE:-arras-escrow.near}
      # La implicita de fondeo del caso (a): la crea y la carga c0der; su llave vive en SSM /arras/mainnet/fondeo.
      FONDEO=${ARRAS_FONDEO-fe0a574258f8277099875e46d4ca0043cdb6274ff2501b375a0d3d64bc211079} ;;
    testnet)
      SUF=testnet
      # El USDC de Circle en testnet; ARRAS_USDC_TESTNET (solo testnet) cambia el token del ensayo por un NEP-141 de
      # prueba (el faucet de Circle pide reCAPTCHA). En mainnet el token es siempre el USDC nativo.
      USDC=${ARRAS_USDC_TESTNET:-3e2210e1184b45b64c8a434c0a7e7b23cc04ea7eb7a6c3c32520d03d4afcb8af}
      cuenta_valida "$USDC" || die "ARRAS_USDC_TESTNET='$USDC' no es una cuenta valida"
      RPC=${ARRAS_RPC:-https://rpc.testnet.fastnear.com}
      NB=https://testnet.nearblocks.io
      P=${ARRAS_PADRE:-}
      FONDEO=${ARRAS_FONDEO:-} ;;
    *) die "ARRAS_RED tiene que ser mainnet o testnet (vino '${ARRAS_RED:-}')" ;;
  esac
  RPC_NOMBRE=$RPC
  [ -z "${ARRAS_RPC:-}" ] || RPC_NOMBRE='ARRAS_RPC (no se imprime)'
  NET=$RED

  MODO=seco
  if [ -n "$BANDERA" ]; then
    [ "$BANDERA" = "$RED" ] || die "--$BANDERA-ok no vale con ARRAS_RED=$RED: el modo real de $RED es --$RED-ok"
    MODO=real
  fi
}
real() { [ "$MODO" = real ]; }

# --- cuentas --------------------------------------------------------------------------------------------------------
RE_CUENTA='^(([a-z0-9]+[-_])*[a-z0-9]+\.)*([a-z0-9]+[-_])*[a-z0-9]+$'
cuenta_valida() { [[ ${#1} -ge 2 && ${#1} -le 64 && $1 =~ $RE_CUENTA ]]; }
de_la_red() { cuenta_valida "$1" && [[ $1 == *."$SUF" ]]; }
implicita() { [[ $1 =~ ^[0-9a-f]{64}$ ]]; }
# Hija DIRECTA del padre: solo el padre la puede crear.
hija() { cuenta_valida "$1" && [[ $1 == *."$P" && ${1%."$P"} != *.* ]]; }

# --- topes y montos -------------------------------------------------------------------------------------------------
entero() { [[ $2 =~ ^[0-9]{1,15}$ ]] || die "$1='$2' tiene que ser un entero de hasta 15 cifras (unidades atomicas)"; }
yocto_valido() { [[ $2 =~ ^[0-9]{1,30}$ ]] || die "$1='$2' tiene que ser un entero de yoctoNEAR"; }
near_valido() { [[ $2 =~ ^[0-9]+(\.[0-9]+)?\ NEAR$ ]] || die "$1='$2' tiene que ser '<numero> NEAR' (p. ej. '0.5 NEAR')"; }

# Los topes del contrato y del operador `em`, los mismos para v1 y v2. Deja OP, TOPE_POR_PAGO, TOPE_TVL_GLOBAL, FEE_BPS,
# MIN_AMOUNT, TOPE_TVL_OPERADOR, TOPE_TVL_POR_PAYER y SPONSOR_MAX.
comun_topes() {
  OP=${ARRAS_OPERATOR_ID:-em}
  [[ $OP =~ ^[a-z0-9_-]{1,32}$ ]] || die "ARRAS_OPERATOR_ID='$OP' fuera de [a-z0-9_-]{1,32} (ARRAS_E45)"
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
  for v in TOPE_POR_PAGO TOPE_TVL_GLOBAL FEE_BPS MIN_AMOUNT TOPE_TVL_OPERADOR TOPE_TVL_POR_PAYER; do entero "$v" "${!v}"; done
  yocto_valido ARRAS_SPONSOR_MAX_POR_PAYER "$SPONSOR_MAX"
}
# Las relaciones entre los topes (despues de validar los montos de NEAR de cada script, como antes en mainnet.sh).
comun_topes_coherentes() {
  [ "$TOPE_POR_PAGO" -gt 0 ] || die "ARRAS_TOPE_POR_PAGO tiene que ser mayor que 0"
  [ "$TOPE_TVL_GLOBAL" -ge "$TOPE_POR_PAGO" ] || die "el TVL global ($TOPE_TVL_GLOBAL) no puede ser menor que el tope por pago ($TOPE_POR_PAGO)"
  [ "$TOPE_TVL_OPERADOR" -le "$TOPE_TVL_GLOBAL" ] || die "el TVL del operador ($TOPE_TVL_OPERADOR) no puede pasar el global ($TOPE_TVL_GLOBAL)"
  [ "$TOPE_TVL_POR_PAYER" -le "$TOPE_TVL_OPERADOR" ] || die "el TVL por payer ($TOPE_TVL_POR_PAYER) no puede pasar el del operador ($TOPE_TVL_OPERADOR)"
  [ "$MIN_AMOUNT" -le "$TOPE_POR_PAGO" ] || die "min_amount ($MIN_AMOUNT) por encima del tope por pago ($TOPE_POR_PAGO): ningun pago entraria"
  [ "$FEE_BPS" -le 1800 ] || die "ARRAS_FEE_BPS=$FEE_BPS pasa 1800 (MAX_FEE_BPS, ARRAS_E43)"
}

# --- llavero y wasm -------------------------------------------------------------------------------------------------
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
# Deja CRED, ESPERA, SIGN y SAVE.
comun_llavero() {
  CRED=$(cred_de_near)
  CRED=${CRED:-$HOME/.near-credentials}
  ESPERA=${ARRAS_ESPERA:-5}
  [[ $ESPERA =~ ^[0-9]{1,2}$ ]] || die "ARRAS_ESPERA tiene que ser segundos (0-99)"
  SIGN=sign-with-legacy-keychain
  SAVE=save-to-legacy-keychain
}

sha256_de() { shasum -a 256 "$1" | awk '{ print $1 }'; }
rev_de_wasm() { LC_ALL=C grep -ao 'rev=[0-9a-f]\{40\}' "$1" | head -1 | cut -d= -f2; }

# La wasm local ($WASM) tiene que ser la de los pines ($WASM_SHA, $REV). En seco solo avisa; en real aborta antes de
# cualquier tx.
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

# Build reproducible de $REV en un clon limpio fuera del repo: $1 = el Cargo.toml del crate, $2 = el nombre del
# artefacto (target/near/<nombre>/<nombre>.wasm). El clon sale de ARRAS_REPO (por defecto, el repo de este script).
wasm_reproducible() {
  if [ -f "$WASM" ] && [ "$(sha256_de "$WASM")" = "$WASM_SHA" ]; then
    wasm_ok
    return 0
  fi
  local dir repo
  dir=$BASE/build-$REV
  repo=${ARRAS_REPO:-$(git -C "$(dirname "$0")" rev-parse --show-toplevel)}
  mkdir -p "$BASE"
  [ -d "$dir/.git" ] || git clone -q "$repo" "$dir"
  git -C "$dir" checkout -q "$REV"
  [ -z "$(git -C "$dir" status --porcelain)" ] || die "$dir tiene cambios: el build reproducible exige git limpio"
  # Sin CARGO_TARGET_DIR: con el seteado, cargo-near 0.22.0 borra el artefacto antes de copiarlo (os error 2).
  (cd "$dir" && env -u CARGO_TARGET_DIR cargo near build reproducible-wasm --manifest-path "$1")
  cp "$dir/target/near/$2/$2.wasm" "$WASM"
  MODO=real wasm_ok
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
  local r _i
  r=""
  # Una lectura es idempotente: tres intentos (el RPC publico de testnet corta por cuota en medio de un ensayo).
  for _i in 1 2 3; do
    r=$(curl -sf -m 30 -X POST "$RPC" -H 'Content-Type: application/json' \
      -d "$(jq -nc --argjson p "$1" '{jsonrpc:"2.0",id:"arras",method:"query",params:$p}')") && break
    r=""
    sleep "${ESPERA:-5}"
  done
  [ -n "$r" ] || die "el RPC $RPC_NOMBRE no respondio"
  printf '%s' "$r"
}
ver_cuenta() { rpc_query "$(jq -nc --arg a "$1" '{request_type:"view_account",finality:"final",account_id:$a}')"; }
existe() {
  local r
  r=$(ver_cuenta "$1")
  jq -e '.result.amount | strings' >/dev/null 2>&1 <<<"$r" && return 0
  jq -e '[.error | .. | strings] | any(test("UNKNOWN_ACCOUNT|does not exist"))' >/dev/null 2>&1 <<<"$r" && return 1
  die "view_account de $1 en $RPC_NOMBRE no trajo ni la cuenta ni UNKNOWN_ACCOUNT: $(head -c 300 <<<"$r")"
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
# Yocto libres de una cuenta: saldo menos su storage (10^19 por byte).
libre_de() {
  local r
  r=$(ver_cuenta "$1")
  python3 -I -c 'import sys; print(int(sys.argv[1]) - int(sys.argv[2]) * 10**19)' "$(jq -r .result.amount <<<"$r")" "$(jq -r .result.storage_usage <<<"$r")"
}
# 1 si el entero $1 >= el entero $2 (yocto: pasan los 64 bits de bash).
mayor_o_igual() { [ "$(python3 -I -c 'import sys; print(int(int(sys.argv[1]) >= int(sys.argv[2])))' "$1" "$2")" = 1 ]; }

# Archivo del llavero legacy de una llave publica (near-cli: <cred>/<red>/<cuenta>/ed25519_<base58>.json).
archivo_llave() { printf '%s/%s/%s/%s.json' "$CRED" "$NET" "$1" "${2/:/_}"; }

# --- llaves por un pipe -----------------------------------------------------------------------------------------------
# Un parametro de SSM por stdout (SecureString descifrado). Nunca toca el disco ni la pantalla.
leer_ssm() {
  aws ssm get-parameter --region "$REGION" --name "$1" --with-decryption --output json | jq -j '.Parameter.Value'
}

# Normaliza una llave de near que llega por stdin (el JSON de near-cli o la llave cruda `ed25519:...`) al archivo de
# llave de near-cli (JSON con account_id, public_key y private_key), por stdout, para `sign-with-access-key-file <(...)`.
# Su llave publica (los ultimos 32 bytes de la privada) tiene que ser una de las de $2 (base58 `ed25519:...`, separadas
# por espacios: las vivas en cadena con el permiso pedido) o, si $2 viene vacio, la de la implicita $1 (una implicita es
# el hex de su llave publica). Si no, sale con error sin escribir nada.
normalizar_llave() {
  python3 -I -c '
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
    pk = "ed25519:" + enc(b[32:])
    vivas = sys.argv[2].split()
    if vivas:
        if pk not in vivas:
            sys.exit(3)
    elif b[32:].hex() != sys.argv[1]:
        sys.exit(3)
    sys.stdout.write(json.dumps({"account_id": sys.argv[1], "public_key": pk, "private_key": sk}))
except SystemExit:
    raise
except Exception:
    sys.exit(4)
' "$1" "${2:-}" 2>/dev/null
}

# --- secretos -------------------------------------------------------------------------------------------------------
# La linea de UNA llave viva para c0der: sube a $SEC_MODO por NOMBRE && compara sha256 sin mostrar nada && recien
# entonces borra. $1 = nombre, $2 = archivo, $3 = descripcion, $4 = lo que se borra (rutas ya entre comillas), $5 = rol.
# Usa SEC_MODO (ssm | secretsmanager), SEC_REGION y SEC_KMS.
linea_secreto() {
  local subir verificar
  if [ "$SEC_MODO" = ssm ]; then
    subir="aws ssm put-parameter --region $SEC_REGION --name '$1' --type SecureString --key-id '$SEC_KMS' --value 'file://$2' --description '$3'"
    verificar="[ \"\$(aws ssm get-parameter --region $SEC_REGION --name '$1' --with-decryption --output json | jq -j .Parameter.Value | shasum -a 256)\" = \"\$(shasum -a 256 < '$2')\" ]"
  else
    subir="aws secretsmanager create-secret --region $SEC_REGION --name '${1#/}' --kms-key-id '$SEC_KMS' --secret-string 'file://$2' --description '$3'"
    verificar="[ \"\$(aws secretsmanager get-secret-value --region $SEC_REGION --secret-id '${1#/}' --output json | jq -j .SecretString | shasum -a 256)\" = \"\$(shasum -a 256 < '$2')\" ]"
  fi
  echo "$subir && $verificar && rm $4 && echo 'OK $5 en $1'"
}

# --- preparacion del modo real ----------------------------------------------------------------------------------------
# near-cli 0.30.1, jq, shasum, python3; en real tambien el RPC de la red (status.chain_id), ARRAS_KMS_KEY y $EST.
comun_preparar() {
  command -v near >/dev/null 2>&1 || PATH=$HOME/.cargo/bin:$PATH
  for herramienta in jq shasum python3; do
    command -v "$herramienta" >/dev/null 2>&1 || die "falta $herramienta"
  done
  if real; then
    command -v near >/dev/null 2>&1 || die "falta near (near-cli-rs 0.30.1)"
    v=$(NO_COLOR=1 near --version 2>/dev/null | awk '/near-cli-rs/ { print $2 }' | tail -1)
    [ "$v" = 0.30.1 ] || die "near-cli-rs es '${v:-?}' y la sintaxis de este script es la de 0.30.1"
    cadena=$(curl -sf -m 30 -X POST "$RPC" -H 'Content-Type: application/json' -d '{"jsonrpc":"2.0","id":"arras","method":"status","params":[]}' | jq -r '.result.chain_id // empty') ||
      die "el RPC $RPC_NOMBRE no respondio a status"
    [ "$cadena" = "$RED" ] || die "el RPC $RPC_NOMBRE es de la red '${cadena:-?}', no de $RED"
    [ -n "${ARRAS_KMS_KEY:-}" ] || die "ARRAS_KMS_KEY es obligatoria en el modo real: la llave KMS propia de Arras (decision 230) con la que van las llaves a SSM"
    mkdir -p "$EST"
  fi
}
