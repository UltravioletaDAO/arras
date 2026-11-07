#!/usr/bin/env bash
# Arras v2 en mainnet (H-11): la v2 revisada (presupuesto de agente, disputas, ticket del operador) en v2.<padre>, la
# cuenta que dejo reservada scripts/mainnet.sh el 2026-11-03, con el operador em, su capturer y su arbitro acotados a
# FunctionCall keys y la cuenta del contrato sin llaves; y el humo de la pista de seguridad como paso aparte. El mismo
# script corre el ensayo en testnet (ARRAS_RED=testnet) sobre lo que deja `scripts/mainnet.sh --testnet-ok todo`.
# near-cli-rs 0.30.1; OpenSSL 3 para el ticket del operador.
#
# EN SECO POR DEFECTO: sin bandera imprime cada comando y no ejecuta nada (ni una consulta de red). El modo real
# exige la bandera de LA MISMA red: --mainnet-ok con ARRAS_RED=mainnet (lo corre c0der) o --testnet-ok con
# ARRAS_RED=testnet; la bandera de la otra red aborta. Este script nunca imprime una llave ni el RPC de ARRAS_RPC.
#
# Uso: scripts/mainnet-v2.sh [--mainnet-ok | --testnet-ok] <paso>
#
#   wasm         build reproducible de ARRAS_V2_REV (ddc5121, el merge de v2 revisado) en un clon limpio y su sha256
#                (local: corre tambien en seco, no toca ninguna red de NEAR)
#   plan         cuentas, topes, wasm y el presupuesto en NEAR por paso y total; en real, contra el saldo del hub, de v2
#                y del padre (y cuanto falta, si falta)
#   1 fondeo     el hub manda a v2 lo que le falta para el codigo y la reserva libre, y al padre lo de las cuentas
#   2 cuentas    em-capturer-v2 y em-arbiter-v2, hijas del padre
#   3 usdc       storage_deposit de v2 en el USDC nativo (y de em-fees si le faltara)
#   4 deploy     sha256 y rev de la wasm contra los pines, deploy + `new` privado en una tx y codigo en cadena
#   5 operador   llave del ticket, register_operator em (capturer y arbitro de v2) y fund_operator_storage
#   6 capturer   FunctionCall key del capturer para v2 (release, refund, withdraw_for) y borrado de su FullAccess
#   7 arbitro    FunctionCall key del arbitro para v2 (resolve_dispute) y borrado de su FullAccess
#   8 sin-llaves borra TODAS las llaves de v2 en una tx y exige view_access_key_list vacio
#   9 verificar  codigo, NEP-330 (rev), llaves, get_config, get_operator, capturer, arbitro y get_storage_health
#     todo       plan, fondeo, cuentas, usdc, deploy, operador, capturer, arbitro, sin-llaves, verificar, secretos, enlaces
#     humo       el humo de la pista de seguridad, con los topes ARRAS_HUMO_*: humo-cuentas, humo-presupuesto,
#                humo-pago, humo-disputa, humo-cierre, secretos y enlaces (lo corre c0der aparte, despues de todo)
#     humo-cuentas      agent-v2 (queda con UNA FunctionCall key: authorize_from_budget) y worker-v2 (USDC registrado)
#     humo-presupuesto  el hub (el humano) registra su storage en v2 y deposita 2 x ARRAS_HUMO_MONTO de USDC para el
#                       agente (ft_transfer_call, msg budget con el ticket del operador)
#     humo-pago         el agente autoriza ARRAS_HUMO_MONTO al worker (authorize_from_budget con su FunctionCall key y
#                       el ticket), el capturer libera y retira por el worker y por em-fees
#     humo-disputa      el agente autoriza ARRAS_HUMO_MONTO; el hub abre la disputa; el arbitro le devuelve todo al
#                       payer (el hub) y el capturer retira por el hub
#     humo-cierre       revoke_agent, storage_withdraw del hub y el estado final del contrato
#     medir      saldo y storage de cada cuenta y get_storage_health (solo lectura)
#     secretos   imprime (no corre) los comandos de AWS que suben cada llave nueva por NOMBRE y la borran del disco
#     enlaces    enlaces de NearBlocks de las cuentas y de cada tx anotada, y NEAR_ESCROW_CONTRACTS_<RED>
#
# Quien firma: el hub (ARRAS_FONDEO, la implicita de fondeo) manda el NEAR, paga el storage de USDC y el pool, y es el
# humano del humo; el padre crea las hijas; v2 firma su deploy y el borrado de sus llaves; el owner registra el operador;
# capturer, arbitro y agente firman con sus FunctionCall keys. Las llaves que ya existen (hub, padre, owner, v2) salen del
# llavero legacy de near-cli si estan ahi y vivas en cadena; si no, de SSM (ARRAS_SECRETOS_PREFIJO/<rol>), siempre por un
# pipe (`sign-with-access-key-file /dev/fd/N`) y validadas contra view_access_key_list antes de firmar. En testnet no se
# lee SSM: el hub sale de ARRAS_FONDEO_ARCHIVO y el resto del llavero.
#
# Variables (todas con defecto; docs/MAINNET.md, seccion 8, tiene la tabla): ARRAS_RED, ARRAS_PADRE (mainnet:
# arras-escrow.near; testnet: el padre del ensayo de v1), ARRAS_FONDEO (el hub; mainnet fe0a5742...1079),
# ARRAS_FONDEO_ARCHIVO (solo testnet), ARRAS_RPC (OBLIGATORIA en mainnet real: el RPC privado de c0der, nunca se
# imprime; el defecto publico de fastnear solo vale para testnet), los topes de scripts/mainnet.sh (ARRAS_TOPE_POR_PAGO,
# ARRAS_TOPE_TVL_GLOBAL, ARRAS_FEE_BPS, ARRAS_MIN_AMOUNT, ARRAS_TOPE_TVL_OPERADOR, ARRAS_TOPE_TVL_POR_PAYER,
# ARRAS_SPONSOR_MAX_POR_PAYER, ARRAS_OPERATOR_ID: los mismos valores que v1, de arras-comun.sh), ARRAS_FEE_RECIPIENT,
# los de v2 (ARRAS_V2_REV, ARRAS_V2_WASM_SHA256, ARRAS_V2_WASM, ARRAS_V2_CAPTURER, ARRAS_V2_ARBITRO,
# ARRAS_V2_VENTANA_ARBITRO en segundos, ARRAS_V2_TICKET si|no, ARRAS_V2_RESERVA, ARRAS_V2_POOL, ARRAS_V2_NEAR_CAPTURER,
# ARRAS_V2_ALLOWANCE_CAPTURER, ARRAS_V2_NEAR_ARBITRO, ARRAS_V2_ALLOWANCE_ARBITRO), los del humo (ARRAS_HUMO_MONTO,
# ARRAS_HUMO_TOPE_POR_PAGO, ARRAS_HUMO_TOPE_TOTAL, ARRAS_HUMO_AGENTE, ARRAS_HUMO_WORKER, ARRAS_HUMO_NEAR_AGENTE,
# ARRAS_HUMO_ALLOWANCE_AGENTE, ARRAS_HUMO_NEAR_WORKER, ARRAS_HUMO_NEP145, ARRAS_HUMO_LIBRE_HUB), ARRAS_ESTADO
# (~/.arras-despliegue), ARRAS_REPO, ARRAS_ESPERA, y los secretos ARRAS_SECRETOS (ssm | secretsmanager),
# ARRAS_SECRETOS_PREFIJO (/arras/<red>), ARRAS_KMS_KEY (OBLIGATORIA en real) y ARRAS_AWS_REGION (us-east-2).
set -euo pipefail
umask 077

COMUN="$(cd "$(dirname "$0")" && pwd -P)/arras-comun.sh"
[ -f "$COMUN" ] || { printf 'ABORTO: falta %s (va junto a este script)\n' "$COMUN" >&2; exit 1; }
# shellcheck source=arras-comun.sh
. "$COMUN"

comun_argumentos "$@"
comun_red

# --- cuentas --------------------------------------------------------------------------------------------------------
[ -z "${ARRAS_PARENT:-}" ] || die "ARRAS_PARENT se renombro a ARRAS_PADRE: no se ignora en silencio"
[ -n "$P" ] || die "en testnet ARRAS_PADRE es obligatoria: el padre del ensayo de v1 (scripts/mainnet.sh --testnet-ok todo)"
de_la_red "$P" || die "ARRAS_PADRE='$P' no es una cuenta valida de $RED (*.$SUF)"
[ -n "$FONDEO" ] || die "ARRAS_FONDEO vino vacia: el hub (la implicita de fondeo) es quien paga v2"
implicita "$FONDEO" || die "ARRAS_FONDEO='$FONDEO' tiene que ser una cuenta implicita: 64 hex en minusculas"
REGION=${ARRAS_AWS_REGION:-us-east-2}
PREFIJO=${ARRAS_SECRETOS_PREFIJO:-/arras/$RED}
FONDEO_SSM=${ARRAS_FONDEO_SSM:-$PREFIJO/fondeo}
FONDEO_ARCHIVO=${ARRAS_FONDEO_ARCHIVO:-}
[ -z "$FONDEO_ARCHIVO" ] || [ "$RED" = testnet ] ||
  die "ARRAS_FONDEO_ARCHIVO es solo del ensayo en testnet: en mainnet la llave del hub se lee de SSM ($FONDEO_SSM)"
C1=v1.$P
C2=v2.$P
OWNER=owner.$P
GUARD=guardian.$P
FEES=${ARRAS_FEE_RECIPIENT:-em-fees.$P}
# El capturer de v2 es una cuenta NUEVA: em-capturer (v1) quedo con una sola FunctionCall key, que no puede agregar
# llaves (AddKey exige FullAccess) y cuyo receiver es v1 (una FunctionCall key sirve a un solo contrato).
CAPT=${ARRAS_V2_CAPTURER:-em-capturer-v2.$P}
ARB=${ARRAS_V2_ARBITRO:-em-arbiter-v2.$P}
AGENTE=${ARRAS_HUMO_AGENTE:-agent-v2.$P}
WORKER=${ARRAS_HUMO_WORKER:-worker-v2.$P}
for a in "$C1" "$C2" "$OWNER" "$GUARD"; do
  cuenta_valida "$a" || die "$a no es una cuenta valida (64 caracteres como maximo): acorta ARRAS_PADRE"
done
# Las que el script crea (su FullAccess firma el add-key y despues se borra): hijas directas del padre.
hija "$CAPT" || die "ARRAS_V2_CAPTURER='$CAPT' tiene que ser hija directa de $P: el script la crea y su FullAccess firma el add-key"
hija "$ARB" || die "ARRAS_V2_ARBITRO='$ARB' tiene que ser hija directa de $P: el script la crea y su FullAccess firma el add-key"
hija "$AGENTE" || die "ARRAS_HUMO_AGENTE='$AGENTE' tiene que ser hija directa de $P: el script la crea y su FullAccess firma el add-key"
if hija "$WORKER"; then
  WORKER_NUEVA=1
elif de_la_red "$WORKER" || implicita "$WORKER"; then
  WORKER_NUEVA=0
else
  die "ARRAS_HUMO_WORKER='$WORKER' no es una cuenta valida de $RED"
fi
de_la_red "$FEES" || implicita "$FEES" || die "ARRAS_FEE_RECIPIENT='$FEES' no es una cuenta valida de $RED"
# Una cuenta, un rol: el receiver no puede ser el contrato, el owner del presupuesto (el hub) ni el agente (E15); el
# agente no puede ser el contrato (E55); el fee_recipient no puede ser el contrato (E46) ni una cuenta que queda solo con
# una FunctionCall key (su USDC quedaria atrapado).
repetida=$(printf '%s\n' "$FONDEO" "$P" "$C1" "$C2" "$OWNER" "$GUARD" "$FEES" "$CAPT" "$ARB" "$AGENTE" "$WORKER" | sort | uniq -d | head -1)
[ -z "$repetida" ] || die "$repetida aparece en dos roles (hub, padre, v1, v2, owner, guardian, fee_recipient, capturer, arbitro, agente, worker): una cuenta por rol"

# --- topes y montos -------------------------------------------------------------------------------------------------
# Los topes del contrato y del operador em son los de v1 por construccion (comun_topes, arras-comun.sh).
comun_topes
# El plazo del arbitro para los pagos nuevos (congelado en cada pago), en [1 minuto, 30 dias] (E47). 7 dias: lo que EM
# le da a la revision (REVIEW_WINDOW_SEC); el owner lo cambia con update_operator.
VENTANA=${ARRAS_V2_VENTANA_ARBITRO:-604800}
entero ARRAS_V2_VENTANA_ARBITRO "$VENTANA"
[ "$VENTANA" -ge 60 ] && [ "$VENTANA" -le 2592000 ] || die "ARRAS_V2_VENTANA_ARBITRO=$VENTANA fuera de [60, 2592000] segundos (ARRAS_E47)"
VENTANA_NS=$((VENTANA * 1000000000))
# Con ticket, todo pago y todo deposito de presupuesto en em necesita la firma ed25519 del operador (F-5): mientras EM no
# firme tickets, nadie mas llena el TVL ni el pool de em en v2. El owner lo quita con update_operator.
TICKET=$(printf '%s' "${ARRAS_V2_TICKET:-si}" | tr '[:upper:]' '[:lower:]')
case "$TICKET" in si | no) ;; *) die "ARRAS_V2_TICKET tiene que ser si o no" ;; esac

# NEAR. Lo que entra a v2 (codigo, reserva y pool) queda ahi para siempre (la cuenta no tiene llaves), y lo de las
# cuentas que quedan solo con una FunctionCall key (capturer, arbitro, agente) solo sirve para su gas: nada de mas.
RESERVA=${ARRAS_V2_RESERVA:-0.25 NEAR}          # reserva libre de v2 al terminar; get_storage_health alarma bajo 0,2
POOL=${ARRAS_V2_POOL:-0.1 NEAR}                 # pool de patrocinio de em en v2 (0 = sin pool; lo fondea cualquiera)
NEAR_CAPT=${ARRAS_V2_NEAR_CAPTURER:-0.15 NEAR}
ALLOW_CAPT=${ARRAS_V2_ALLOWANCE_CAPTURER:-$NEAR_CAPT}
NEAR_ARB=${ARRAS_V2_NEAR_ARBITRO:-0.1 NEAR}
ALLOW_ARB=${ARRAS_V2_ALLOWANCE_ARBITRO:-$NEAR_ARB}
# Medido en los ensayos (docs/MAINNET.md, 8.3): una llamada reserva por adelantado ~0,00103 NEAR por TGas al precio
# pesimista (0,0309 para 30 TGas; EM midio 0,151 para 150, docs/EVIDENCE.md 5), y casi todo vuelve con el reembolso, que
# tambien repone la allowance (el agente gasto 0,0009 en dos authorize_from_budget). El agente solo firma el humo y su
# NEAR queda varado en una cuenta solo-FunctionCall: 0,06 es dos veces lo que reserva una llamada de 30 TGas.
NEAR_AGENTE=${ARRAS_HUMO_NEAR_AGENTE:-0.06 NEAR}
ALLOW_AGENTE=${ARRAS_HUMO_ALLOWANCE_AGENTE:-$NEAR_AGENTE}
NEAR_WORKER=${ARRAS_HUMO_NEAR_WORKER:-0.01 NEAR}
NEP145_HUB=${ARRAS_HUMO_NEP145:-0.03 NEAR}      # storage NEP-145 del hub en v2: 0,0243 en el pico del ensayo
LIBRE_HUB=${ARRAS_HUMO_LIBRE_HUB:-0.15 NEAR}    # el ft_transfer_call de 100 TGas reserva ~0,103; vuelve al hub
for v in RESERVA POOL NEAR_CAPT ALLOW_CAPT NEAR_ARB ALLOW_ARB NEAR_AGENTE ALLOW_AGENTE NEAR_WORKER NEP145_HUB LIBRE_HUB; do
  near_valido "$v" "${!v}"
done
python3 -I -c 'import sys, decimal; sys.exit(0 if decimal.Decimal(sys.argv[1].split()[0]) >= decimal.Decimal("0.2") else 1)' "$RESERVA" ||
  die "ARRAS_V2_RESERVA='$RESERVA' bajo 0,2 NEAR: get_storage_health de v2 nace en alarma (MIN_FREE_RESERVE)"
for v in ALLOW_CAPT ALLOW_ARB ALLOW_AGENTE; do
  [[ ${!v%% *} =~ [1-9] ]] || die "$v='${!v}': con 0 la FunctionCall key no puede pagar gas"
done
comun_topes_coherentes
# Medidos (docs/MAINNET.md, secciones 3 y 8).
USDC_STORAGE='0.00125 NEAR'
CREACION='0.01 NEAR'         # una sub-cuenta: 0,00711 medido (gas y lo que quema); el resto queda en el padre
GAS_TX='0.001 NEAR'          # una tx simple: 0,0002 a 0,0007
GAS_DEPLOY='0.005 NEAR'      # deploy + new de v2, lo paga v2: 0,00284 medido
ESTADO_V2=1000               # bytes de v2 sin la wasm despues de new y register_operator, sin llaves: 906 medidos
V2_SALDO_RESERVA='0.1 NEAR'  # lo que le dio scripts/mainnet.sh (cuentas) a v2 el 2026-11-03: el supuesto del seco

# El humo: topes explicitos en USDC (unidades de 6 decimales). Techo del encargo: una tarea chica de 0,01 a 0,10 USDC
# por pago, y 0,20 en total; el humo mueve 2 x ARRAS_HUMO_MONTO del hub a v2 (el presupuesto del agente).
HUMO_MONTO=${ARRAS_HUMO_MONTO:-50000}                   # 0,05 USDC por pago
HUMO_TOPE_POR_PAGO=${ARRAS_HUMO_TOPE_POR_PAGO:-100000}  # 0,10 USDC
HUMO_TOPE_TOTAL=${ARRAS_HUMO_TOPE_TOTAL:-200000}        # 0,20 USDC
for v in HUMO_MONTO HUMO_TOPE_POR_PAGO HUMO_TOPE_TOTAL; do entero "$v" "${!v}"; done
[ "$HUMO_TOPE_POR_PAGO" -le 100000 ] || die "ARRAS_HUMO_TOPE_POR_PAGO=$HUMO_TOPE_POR_PAGO pasa 0,10 USDC (100000): el humo es una tarea chica"
[ "$HUMO_TOPE_TOTAL" -le 200000 ] || die "ARRAS_HUMO_TOPE_TOTAL=$HUMO_TOPE_TOTAL pasa 0,20 USDC (200000)"
[ "$HUMO_MONTO" -ge 10000 ] && [ "$HUMO_MONTO" -ge "$MIN_AMOUNT" ] || die "ARRAS_HUMO_MONTO=$HUMO_MONTO debajo de 0,01 USDC o del min_amount ($MIN_AMOUNT)"
[ "$HUMO_MONTO" -le "$HUMO_TOPE_POR_PAGO" ] || die "ARRAS_HUMO_MONTO=$HUMO_MONTO pasa ARRAS_HUMO_TOPE_POR_PAGO=$HUMO_TOPE_POR_PAGO"
HUMO_PRESUPUESTO=$((2 * HUMO_MONTO))
[ "$HUMO_PRESUPUESTO" -le "$HUMO_TOPE_TOTAL" ] || die "el humo mueve 2 x $HUMO_MONTO = $HUMO_PRESUPUESTO y ARRAS_HUMO_TOPE_TOTAL es $HUMO_TOPE_TOTAL"

# --- wasm -----------------------------------------------------------------------------------------------------------
# Los pines: el build reproducible de ddc5121 (el merge de arras#6, la v2 que paso RARRAS-6; contracts/escrow-v2 y
# escrow-core son iguales en main), construido el 2026-11-07 dos veces desde dos clones limpios con el mismo sha256 (y el
# mismo entorno reproduce byte a byte la wasm de v1 que se construyo en la Mac, 7058319d...).
REV=$(printf '%s' "${ARRAS_V2_REV:-ddc51211ed93973ec9eb537a85bac39367f6e763}" | tr '[:upper:]' '[:lower:]')
WASM_SHA=$(printf '%s' "${ARRAS_V2_WASM_SHA256:-1541525c4bf3494ae5025b00df6955698975a6f8c376002f0296ff93b4e4fa77}" | tr '[:upper:]' '[:lower:]')
WASM_BYTES=366339            # tamano de la wasm del pin (para el plan en seco sin la wasm local)
[[ $REV =~ ^[0-9a-f]{40}$ ]] || die "ARRAS_V2_REV tiene que ser un sha de git de 40 caracteres hex"
[[ $WASM_SHA =~ ^[0-9a-f]{64}$ ]] || die "ARRAS_V2_WASM_SHA256 tiene que ser un sha256 de 64 caracteres hex"
BASE=${ARRAS_ESTADO:-$HOME/.arras-despliegue}
WASM=${ARRAS_V2_WASM:-$BASE/arras_escrow_v2-$REV.wasm}
# Junto al estado de v1 del mismo padre, en su propia carpeta: txs, saldos, el humo y la llave del ticket (modo 600).
EST=$BASE/$RED/$P/v2
TXS=$EST/txs.tsv
SALDOS=$EST/saldos.tsv
HUMO=$EST/humo
TICKET_PEM=$EST/em-ticket-v2.pem
POOL_MARCA=$EST/pool-$OP-fondeado
comun_llavero

METODOS_CAPT='refund,release,withdraw_for'
METODOS_ARB='resolve_dispute'
METODOS_AGENTE='authorize_from_budget'

# --- llaves -----------------------------------------------------------------------------------------------------------
# Nombre en SSM de cada rol: los de v1 (2026-11-03: fondeo, padre, owner, v2-reserva) y los nuevos de v2.
nombre_ssm() { if [ "$1" = fondeo ]; then echo "$FONDEO_SSM"; else echo "$PREFIJO/$1"; fi; }
# De donde sale la llave de un rol (para el texto del seco).
fuente_de() {
  if [ "$1" = fondeo ] && [ -n "$FONDEO_ARCHIVO" ]; then
    echo "ARRAS_FONDEO_ARCHIVO"
  elif [ "$RED" = mainnet ]; then
    echo "el llavero legacy o SSM $(nombre_ssm "$1") ($REGION)"
  else
    echo "el llavero legacy"
  fi
}

# Las llaves publicas vivas de $1 con el permiso $2: `full`, o los metodos ordenados separados por coma (una
# FunctionCall key con receiver v2 y exactamente esos metodos).
vivas() {
  llaves_json "$1" | jq -r --arg p "$2" --arg c "$C2" '.[] | select(
    if $p == "full" then .access_key.permission == "FullAccess"
    else ((.access_key.permission | objects | .FunctionCall) as $f | $f != null and $f.receiver_id == $c and (($f.method_names | sort | join(",")) == $p))
    end) | .public_key'
}

# La llave del rol $1 de la cuenta $2 con el permiso $3, como archivo de llave de near-cli por stdout (para
# `sign-with-access-key-file <(llave ...)`): del llavero legacy si hay un archivo con una llave viva en cadena con ese
# permiso; si no, del hub en el ensayo, ARRAS_FONDEO_ARCHIVO; si no, en mainnet, SSM. La publica se valida contra las
# vivas antes de usarla (normalizar_llave). Nunca toca el disco ni la pantalla.
llave() {
  local rol=$1 cuenta=$2 permiso=$3 v f
  v=$(vivas "$cuenta" "$permiso" | tr '\n' ' ')
  [ -n "${v// /}" ] || die "$cuenta no tiene en cadena ninguna llave viva con permiso $permiso"
  for f in "$CRED/$NET/$cuenta"/ed25519_*.json "$CRED/$NET/$cuenta.json"; do
    [ -f "$f" ] || continue
    if normalizar_llave "$cuenta" "$v" <"$f"; then return 0; fi
  done
  if [ "$rol" = fondeo ] && [ -n "$FONDEO_ARCHIVO" ]; then
    [ -f "$FONDEO_ARCHIVO" ] || die "no existe ARRAS_FONDEO_ARCHIVO=$FONDEO_ARCHIVO"
    normalizar_llave "$cuenta" "$v" <"$FONDEO_ARCHIVO" || die "ARRAS_FONDEO_ARCHIVO no es una llave viva de $cuenta"
    return 0
  fi
  [ "$RED" = mainnet ] || die "la llave de $cuenta ($rol) no esta en el llavero legacy ($CRED/$NET/$cuenta/): en testnet no se lee SSM"
  command -v aws >/dev/null 2>&1 || die "falta aws: la llave de $cuenta ($rol) se lee de SSM $(nombre_ssm "$rol")"
  leer_ssm "$(nombre_ssm "$rol")" | normalizar_llave "$cuenta" "$v" ||
    die "la llave de $cuenta en SSM $(nombre_ssm "$rol") no se pudo leer o no es una llave viva suya con permiso $permiso"
}

# Una tx firmada como $2 (rol $1, permiso $3) con la llave por un pipe. $4 = etiqueta; el resto, los argumentos de near
# hasta antes de network-config. En real, primero valida la llave (si no sirve, aborta antes de llamar a near).
tx_con_llave() {
  local rol=$1 cuenta=$2 permiso=$3 etiqueta=$4
  shift 4
  if ! real; then
    tx "$etiqueta" "$@" network-config "$NET" sign-with-access-key-file "<la llave de $cuenta ($permiso): $(fuente_de "$rol"), por un pipe>" send
    return 0
  fi
  llave "$rol" "$cuenta" "$permiso" >/dev/null
  reserva_ok "$cuenta" "$permiso" "$@"
  tx "$etiqueta" "$@" network-config "$NET" sign-with-access-key-file <(llave "$rol" "$cuenta" "$permiso") send
}

# Una llamada a un contrato reserva por adelantado su gas al precio pesimista (~0,00103 NEAR por TGas, medido) mas el
# deposito: si la cuenta (y su FunctionCall key, con la allowance) no lo tiene libre, near-cli la rechaza a mitad del
# paso. Se mira antes de firmar y se dice como recargarla (a una cuenta solo-FunctionCall le manda NEAR cualquiera).
RESERVA_POR_TGAS=1050000000000000000000      # 0,00105 NEAR
reserva_ok() {
  local cuenta=$1 permiso=$2 prev="" a gas="" dep=0 pedido libre allow
  shift 2
  for a in "$@"; do
    [ "$prev" = prepaid-gas ] && gas=${a%% *}
    [ "$prev" = attached-deposit ] && dep=$(yocto_de_near "$a")
    prev=$a
  done
  [ -n "$gas" ] || return 0
  pedido=$(python3 -I -c 'import sys, decimal; print(int(decimal.Decimal(sys.argv[1]) * int(sys.argv[2])) + int(sys.argv[3]))' "$gas" "$RESERVA_POR_TGAS" "$dep")
  libre=$(libre_de "$cuenta")
  mayor_o_igual "$libre" "$pedido" ||
    die "$cuenta tiene $(en_near "$libre") libres y esta llamada de $gas TGas reserva $(en_near "$pedido") por adelantado: recargala (near tokens <cuenta con NEAR> send-near $cuenta '<NEAR>' ...) y volve a correr el paso"
  [ "$permiso" = full ] && return 0
  allow=$(llaves_json "$cuenta" | jq -r --arg k "$(vivas "$cuenta" "$permiso" | head -1)" '.[] | select(.public_key == $k) | .access_key.permission.FunctionCall.allowance // "sin-tope"')
  [ "$allow" = sin-tope ] || mayor_o_igual "$allow" "$pedido" ||
    die "la FunctionCall key de $cuenta tiene $(en_near "$allow") de allowance y esta llamada reserva $(en_near "$pedido"): la llave ya no alcanza (una allowance no se sube; otra cuenta con update_operator)"
}
# Un monto de near-cli ('0.15 NEAR', '1 yoctoNEAR') en yocto.
yocto_de_near() {
  case "$1" in
    *yoctoNEAR) printf '%s' "${1%% *}" ;;
    *) yocto "$1" ;;
  esac
}

# --- ticket del operador ------------------------------------------------------------------------------------------------
b58() {
  python3 -I -c '
import sys
A = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"
b = sys.stdin.buffer.read(); n = int.from_bytes(b, "big"); s = ""
while n:
    n, r = divmod(n, 58); s = A[r] + s
print("1" * (len(b) - len(b.lstrip(b"\0"))) + s)'
}
# El ticket ed25519 necesita OpenSSL 3 (genpkey ed25519, pkeyutl -rawin); el /usr/bin/openssl de macOS es LibreSSL. Se
# usa ARRAS_OPENSSL, o el primer OpenSSL 3 entre el del PATH y los de Homebrew.
OPENSSL=""
openssl_ok() {
  local o
  [ -z "$OPENSSL" ] || return 0
  if [ -n "${ARRAS_OPENSSL:-}" ]; then
    [ -x "$ARRAS_OPENSSL" ] || die "ARRAS_OPENSSL=$ARRAS_OPENSSL no es un ejecutable"
    case "$("$ARRAS_OPENSSL" version 2>/dev/null)" in
      "OpenSSL 3"*) OPENSSL=$ARRAS_OPENSSL; return 0 ;;
    esac
    die "ARRAS_OPENSSL=$ARRAS_OPENSSL es '$("$ARRAS_OPENSSL" version 2>/dev/null)', no OpenSSL 3"
  fi
  # type -P: el del PATH, no la funcion `openssl` de abajo.
  for o in "$(type -P openssl 2>/dev/null || true)" /opt/homebrew/opt/openssl@3/bin/openssl /usr/local/opt/openssl@3/bin/openssl; do
    [ -n "$o" ] && [ -x "$o" ] || continue
    case "$("$o" version 2>/dev/null)" in
      "OpenSSL 3"*) OPENSSL=$o; return 0 ;;
    esac
  done
  die "el ticket ed25519 necesita OpenSSL 3 y no hay ninguno (en macOS: brew install openssl@3, o ARRAS_OPENSSL=<ruta>)"
}
openssl() { openssl_ok; "$OPENSSL" "$@"; }
# La llave ed25519 del ticket (PEM) por stdout: el archivo local ($TICKET_PEM) o, en mainnet, SSM em-ticket-v2.
pem_ticket() {
  if [ -f "$TICKET_PEM" ]; then
    cat "$TICKET_PEM"
  elif [ "$RED" = mainnet ]; then
    command -v aws >/dev/null 2>&1 || die "falta aws: la llave del ticket se lee de SSM $(nombre_ssm em-ticket-v2)"
    leer_ssm "$(nombre_ssm em-ticket-v2)"
  else
    die "falta la llave del ticket ($TICKET_PEM)"
  fi
}
# La publica (`ed25519:<base58>`) de un PEM ed25519 que llega por stdin: los ultimos 32 bytes del SubjectPublicKeyInfo.
pub_de_pem() {
  local d
  openssl_ok
  d=$(openssl pkey -pubout -outform DER 2>/dev/null | tail -c 32 | b58) || true
  [ -n "$d" ] && [ "$d" != 1 ] || die "la llave del ticket no es un PEM ed25519 valido"
  printf 'ed25519:%s' "$d"
}
# La ticket_pubkey para registrar el operador: la de la llave que hay (local o SSM) o, si no hay ninguna, una nueva en
# $TICKET_PEM.
ticket_pubkey() {
  if [ ! -f "$TICKET_PEM" ] && [ "$RED" = mainnet ]; then
    command -v aws >/dev/null 2>&1 || die "falta aws: antes de generar una llave del ticket se mira SSM $(nombre_ssm em-ticket-v2)"
  fi
  if [ ! -f "$TICKET_PEM" ] && { [ "$RED" != mainnet ] || ! leer_ssm "$(nombre_ssm em-ticket-v2)" >/dev/null 2>&1; }; then
    mkdir -p "$EST"
    (umask 077 && openssl genpkey -algorithm ed25519 -out "$TICKET_PEM" 2>/dev/null) || die "openssl no genero la llave del ticket"
    echo "llave nueva del ticket: $TICKET_PEM (modo 600, no se imprime; el paso secretos la sube a $(nombre_ssm em-ticket-v2))" >&2
  fi
  pem_ticket | pub_de_pem
}
# El ticket {expiry, sig} para los bytes que devuelve la vista $1 de v2 con los args $2 (con su expiry). Que la llave sea
# la ticket_pubkey del operador lo exige humo_listo, que corre antes de cada parte del humo.
firmar_ticket() {
  local metodo=$1 args=$2 bytes sig
  mkdir -p "$HUMO"
  bytes=$(mktemp "$HUMO/ticket-msg.XXXXXX")
  # Los bytes que firma el operador son publicos: los da la vista del contrato (base64 en un string JSON).
  vista "$C2" "$metodo" "$args" | python3 -I -c 'import base64, json, sys; sys.stdout.buffer.write(base64.b64decode(json.loads(sys.stdin.read())))' >"$bytes"
  sig=$(openssl pkeyutl -sign -rawin -inkey <(pem_ticket) -in "$bytes" | b58)
  rm -f "$bytes"
  [ ${#sig} -ge 80 ] || die "openssl no firmo el ticket"
  jq -nc --arg e "$(jq -r .expiry <<<"$args")" --arg s "$sig" '{expiry:$e,sig:$s}'
}

# --- lecturas ---------------------------------------------------------------------------------------------------------
ahora_ns() { echo $(( $(date +%s) * 1000000000 )); }
registrada_usdc() { [ "$(vista "$USDC" storage_balance_of "$(jq -nc --arg a "$1" '{account_id:$a}')")" != null ]; }
usdc_de() { vista "$USDC" ft_balance_of "$(jq -nc --arg a "$1" '{account_id:$a}')" | tr -d '"'; }
claimable_de() { vista "$C2" get_claimable "$(jq -nc --arg a "$1" '{account:$a}')" | tr -d '"'; }
pago_de() { vista "$C2" get_payment "$(jq -nc --arg id "$1" '{payment_id:$id}')"; }
presupuesto_de() { vista "$C2" get_budget "$(jq -nc --arg o "$FONDEO" --arg a "$AGENTE" --arg op "$OP" '{owner:$o,agent:$a,operator_id:$op}')"; }
# El presupuesto del humo existe con saldo $1 ("" = que no exista).
presupuesto_con() {
  local p
  p=$(presupuesto_de)
  if [ -z "$1" ]; then [ "$p" = null ]; else jq -e --arg s "$1" '.saldo == $s' >/dev/null <<<"$p"; fi
}
saldo_de() { ver_cuenta "$1" | jq -r '.result.amount'; }
storage_de() { ver_cuenta "$1" | jq -r '.result.storage_usage'; }
# Redondea yocto hacia arriba al milesimo de NEAR, como argumento de near ("1.234 NEAR").
a_near_arriba() { python3 -I -c 'import sys; m = -(-int(sys.argv[1]) // 10**21); print(f"{m // 1000}.{m % 1000:03d} NEAR")' "$1"; }
sumar() { python3 -I -c 'import sys; print(sum(int(a) for a in sys.argv[1:]))' "$@"; }
restar_positivo() { python3 -I -c 'import sys; print(max(0, int(sys.argv[1]) - int(sys.argv[2])))' "$1" "$2"; }
bytes_yocto() { python3 -I -c 'import sys; print(int(sys.argv[1]) * 10**19)' "$1"; }

cuentas_todas() { printf '%s\n' "$FONDEO" "$P" "$C2" "$OWNER" "$CAPT" "$ARB" "$AGENTE" "$WORKER" "$FEES"; }
anotar_saldos() {
  real || return 0
  local a r
  for a in $(cuentas_todas); do
    r=$(ver_cuenta "$a")
    jq -e '.result.amount' >/dev/null 2>&1 <<<"$r" || continue
    printf '%s\t%s\t%s\t%s\t%s\n' "$(date -u +%FT%TZ)" "$1" "$a" "$(jq -r .result.amount <<<"$r")" "$(jq -r .result.storage_usage <<<"$r")" >>"$SALDOS"
  done
}
# Repite la condicion "$@" hasta 6 veces (ARRAS_ESPERA entre intentos): una lectura con finality final puede venir un
# bloque atrasada respecto de la tx que near-cli acaba de ver ejecutada.
hasta() {
  local _i
  for _i in 1 2 3 4 5 6; do
    "$@" && return 0
    sleep "$ESPERA"
  done
  return 1
}
# Espera (hasta 6 lecturas) a que la cuenta $1 tenga al menos $2 yocto.
esperar_saldo() {
  local _i
  for _i in 1 2 3 4 5 6; do
    mayor_o_igual "$(saldo_de "$1")" "$2" && return 0
    sleep "$ESPERA"
  done
  die "$1 no llego a $(en_near "$2") despues del envio: mira la tx"
}

ARGS_NEW=$(jq -nc --arg o "$OWNER" --arg g "$GUARD" --arg t "$USDC" --arg p "$TOPE_POR_PAGO" --arg v "$TOPE_TVL_GLOBAL" \
  '{owner:$o,guardian:$g,token:$t,tope_por_pago:$p,tope_tvl_global:$v}')

# --- presupuesto ------------------------------------------------------------------------------------------------------
# El estado que mira el presupuesto: en real, en vivo; en seco, los supuestos (v2 con la reserva de 0,1 y sin codigo, el
# padre sin NEAR libre, nada creado ni registrado salvo em-fees, que registro v1).
estado_plan() {
  if real; then
    wasm_ok >/dev/null
    E_WASM=$(wc -c <"$WASM" | tr -d ' ')
    E_V2_SALDO=$(saldo_de "$C2")
    E_V2_CODIGO=0; [ "$(codigo_hex "$C2")" = "$(printf '0%.0s' {1..64})" ] || E_V2_CODIGO=1
    E_PADRE_LIBRE=$(libre_de "$P")
    E_HUB_LIBRE=0; existe "$FONDEO" && E_HUB_LIBRE=$(libre_de "$FONDEO")
    E_HUB_USDC=$(usdc_de "$FONDEO")
    E_CAPT=0; existe "$CAPT" && E_CAPT=1
    E_ARB=0; existe "$ARB" && E_ARB=1
    E_AGENTE=0; existe "$AGENTE" && E_AGENTE=1
    E_WORKER=1; [ "$WORKER_NUEVA" = 1 ] && { E_WORKER=0; existe "$WORKER" && E_WORKER=1; }
    E_V2_USDC=0; registrada_usdc "$C2" && E_V2_USDC=1
    E_FEES_USDC=0; registrada_usdc "$FEES" && E_FEES_USDC=1
    E_WORKER_USDC=0; registrada_usdc "$WORKER" && E_WORKER_USDC=1
    E_POOL=0; E_HUB_NEP145=0
    [ ! -f "$POOL_MARCA" ] || E_POOL=1
    if [ "$E_V2_CODIGO" = 1 ]; then
      [ "$(vista "$C2" get_operator "$(jq -nc --arg id "$OP" '{operator_id:$id}')" | jq -r '.sponsor_pool // "0"')" = 0 ] || E_POOL=1
      [ "$(vista "$C2" storage_balance_of "$(jq -nc --arg a "$FONDEO" '{account_id:$a}')")" = null ] || E_HUB_NEP145=1
    fi
  else
    E_WASM=$WASM_BYTES; [ -f "$WASM" ] && E_WASM=$(wc -c <"$WASM" | tr -d ' ')
    E_V2_SALDO=$(yocto "$V2_SALDO_RESERVA"); E_V2_CODIGO=0; E_PADRE_LIBRE=0; E_HUB_LIBRE=""; E_HUB_USDC=""
    E_CAPT=0; E_ARB=0; E_AGENTE=0; E_WORKER=$((1 - WORKER_NUEVA)); E_V2_USDC=0; E_FEES_USDC=1; E_WORKER_USDC=0
    E_POOL=0; E_HUB_NEP145=0
  fi
}

# Las lineas del presupuesto (paso, que, yocto que salen del hub) y los totales; deja P_TODO y P_HUMO.
presupuesto_v2() {
  local v2_falta=0 padre_necesita=0 padre_manda=0 padre_resto humo_padre=0 humo_padre_manda=0 x
  echo "paso        que                                                                                   NEAR del hub"
  linea() { printf '%-11s %-85s %s\n' "$1" "$2" "$(en_near "$3")"; }
  if [ "$E_V2_CODIGO" = 0 ]; then
    v2_falta=$(restar_positivo "$(sumar "$(bytes_yocto $((E_WASM + ESTADO_V2)))" "$(yocto "$RESERVA")" "$(yocto "$GAS_DEPLOY")" "$(yocto "$GAS_TX")")" "$E_V2_SALDO")
    [ "$v2_falta" = 0 ] || v2_falta=$(yocto "$(a_near_arriba "$v2_falta")")
  fi
  linea fondeo "v2: wasm $E_WASM + estado $ESTADO_V2 bytes + reserva ${RESERVA%% *} + gas; tiene $(en_near "$E_V2_SALDO")" "$v2_falta"
  [ "$E_CAPT" = 1 ] || padre_necesita=$(sumar "$padre_necesita" "$(yocto "$NEAR_CAPT")" "$(yocto "$CREACION")")
  [ "$E_ARB" = 1 ] || padre_necesita=$(sumar "$padre_necesita" "$(yocto "$NEAR_ARB")" "$(yocto "$CREACION")")
  padre_manda=$(restar_positivo "$padre_necesita" "$E_PADRE_LIBRE")
  [ "$padre_manda" = 0 ] || padre_manda=$(yocto "$(a_near_arriba "$padre_manda")")
  linea fondeo "padre: crear capturer (${NEAR_CAPT%% *}) y arbitro (${NEAR_ARB%% *}), ${CREACION%% *} c/u; tiene $(en_near "$E_PADRE_LIBRE") libres" "$padre_manda"
  x=0; { [ "$v2_falta" = 0 ] && [ "$padre_manda" = 0 ]; } || x=$(yocto "$GAS_TX")
  linea fondeo "gas de los envios del hub" "$x"
  P_TODO=$(sumar "$v2_falta" "$padre_manda" "$x")
  x=0; [ "$E_V2_USDC" = 1 ] || x=$(sumar "$(yocto "$USDC_STORAGE")" "$(yocto "$GAS_TX")")
  linea usdc "storage_deposit de v2 en USDC" "$x"; P_TODO=$(sumar "$P_TODO" "$x")
  x=0; [ "$E_FEES_USDC" = 1 ] || x=$(sumar "$(yocto "$USDC_STORAGE")" "$(yocto "$GAS_TX")")
  linea usdc "storage_deposit de $FEES en USDC (v1 ya lo hizo)" "$x"; P_TODO=$(sumar "$P_TODO" "$x")
  x=0; { [ "$E_POOL" = 1 ] || [ "$(yocto "$POOL")" = 0 ]; } || x=$(sumar "$(yocto "$POOL")" "$(yocto "$GAS_TX")")
  linea operador "pool de patrocinio de $OP en v2 (fund_operator_storage)" "$x"; P_TODO=$(sumar "$P_TODO" "$x")
  linea "= todo" "lo que sale del hub en todo (deploy, operador, llaves)" "$P_TODO"
  # Lo que le queda libre al padre despues de todo (para las cuentas del humo).
  padre_resto=$(restar_positivo "$(sumar "$E_PADRE_LIBRE" "$padre_manda")" "$padre_necesita")
  [ "$E_AGENTE" = 1 ] || humo_padre=$(sumar "$humo_padre" "$(yocto "$NEAR_AGENTE")" "$(yocto "$CREACION")")
  [ "$E_WORKER" = 1 ] || humo_padre=$(sumar "$humo_padre" "$(yocto "$NEAR_WORKER")" "$(yocto "$CREACION")")
  humo_padre_manda=$(restar_positivo "$humo_padre" "$padre_resto")
  [ "$humo_padre_manda" = 0 ] || humo_padre_manda=$(sumar "$(yocto "$(a_near_arriba "$humo_padre_manda")")" "$(yocto "$GAS_TX")")
  linea humo "padre: crear el agente (${NEAR_AGENTE%% *}) y el worker (${NEAR_WORKER%% *}), mas el envio" "$humo_padre_manda"
  P_HUMO=$humo_padre_manda
  x=0; [ "$E_WORKER_USDC" = 1 ] || x=$(sumar "$(yocto "$USDC_STORAGE")" "$(yocto "$GAS_TX")")
  linea humo "storage_deposit del worker en USDC" "$x"; P_HUMO=$(sumar "$P_HUMO" "$x")
  x=0; [ "$E_HUB_NEP145" = 1 ] || x=$(sumar "$(yocto "$NEP145_HUB")" "$(yocto "$GAS_TX")")
  linea humo "NEP-145 del hub en v2 (presupuesto y dos pagos; lo libre vuelve en humo-cierre)" "$x"; P_HUMO=$(sumar "$P_HUMO" "$x")
  x=$(sumar "$(yocto "$GAS_TX")" "$(yocto "$GAS_TX")" "$(yocto "$GAS_TX")" "$(yocto "$GAS_TX")" "$(yocto "$GAS_TX")")
  linea humo "gas del hub: ft_transfer_call, open_dispute, revoke_agent, storage_withdraw, margen" "$x"; P_HUMO=$(sumar "$P_HUMO" "$x")
  linea humo "libre en el hub al firmar el ft_transfer_call (100 TGas a precio pesimista; vuelve)" "$(yocto "$LIBRE_HUB")"
  P_HUMO=$(sumar "$P_HUMO" "$(yocto "$LIBRE_HUB")")
  linea "= humo" "lo que el hub necesita para el humo" "$P_HUMO"
  linea "= total" "todo + humo" "$(sumar "$P_TODO" "$P_HUMO")"
}

# --- pasos ------------------------------------------------------------------------------------------------------------
paso_plan() {
  cat <<PLAN
red $RED ($MODO); RPC $RPC_NOMBRE; USDC $USDC
hub (ARRAS_FONDEO) $FONDEO (llave: $(fuente_de fondeo))
padre $P (llave: $(fuente_de padre)); v1 $C1 (no se toca)
v2 $C2 (llave: $(fuente_de v2-reserva)), queda sin llaves | owner $OWNER (llave: $(fuente_de owner)) | guardian $GUARD
capturer $CAPT (${NEAR_CAPT}, FunctionCall key $METODOS_CAPT, allowance $ALLOW_CAPT)
arbitro $ARB (${NEAR_ARB}, FunctionCall key $METODOS_ARB, allowance $ALLOW_ARB), plazo $VENTANA s
fee_recipient $FEES (ya existe, de v1)
new: tope_por_pago $TOPE_POR_PAGO, tope_tvl_global $TOPE_TVL_GLOBAL (unidades de 6 decimales; los mismos que v1)
operador $OP: fee_bps $FEE_BPS, min_amount $MIN_AMOUNT, tope_tvl_operador $TOPE_TVL_OPERADOR, tope_tvl_por_payer $TOPE_TVL_POR_PAYER, sponsor_max_por_payer $SPONSOR_MAX yocto, pool $POOL, ticket $TICKET
wasm $WASM (sha256 $WASM_SHA, rev $REV)
humo: agente $AGENTE (${NEAR_AGENTE}, FunctionCall key $METODOS_AGENTE), worker $WORKER$( [ "$WORKER_NUEVA" = 1 ] && echo " (nueva, $NEAR_WORKER)" || echo " (ya existe)"); $HUMO_MONTO por pago (tope $HUMO_TOPE_POR_PAGO), presupuesto $HUMO_PRESUPUESTO (tope total $HUMO_TOPE_TOTAL) de USDC del hub
PLAN
  estado_plan
  echo
  real || echo "EN SECO no se lee la red: v2 con $V2_SALDO_RESERVA y sin codigo, el padre sin NEAR libre, nada creado ni registrado salvo $FEES en USDC (el modo real lo mide)."
  presupuesto_v2
  if real; then
    echo
    echo "hub $FONDEO: $(en_near "$E_HUB_LIBRE") libres, $E_HUB_USDC de USDC (el humo usa $HUMO_PRESUPUESTO)"
    if mayor_o_igual "$E_HUB_LIBRE" "$(sumar "$P_TODO" "$P_HUMO")"; then
      echo "OK: alcanza; sobran $(en_near "$(restar_positivo "$E_HUB_LIBRE" "$(sumar "$P_TODO" "$P_HUMO")")")"
    else
      echo "FALTA: $(en_near "$(restar_positivo "$(sumar "$P_TODO" "$P_HUMO")" "$E_HUB_LIBRE")") en el hub para todo + humo (solo todo: $(en_near "$(restar_positivo "$P_TODO" "$E_HUB_LIBRE")"))"
    fi
    mayor_o_igual "${E_HUB_USDC:-0}" "$HUMO_PRESUPUESTO" || echo "FALTA USDC: el hub tiene $E_HUB_USDC y el humo usa $HUMO_PRESUPUESTO"
  fi
}

paso_wasm() { wasm_reproducible contracts/escrow-v2/Cargo.toml arras_escrow_v2; }

paso_fondeo() {
  if ! real; then
    tx_con_llave fondeo "$FONDEO" full "fondeo v2" tokens "$FONDEO" send-near "$C2" "<lo que le falta a $C2 (paso plan)>"
    tx_con_llave fondeo "$FONDEO" full "fondeo padre" tokens "$FONDEO" send-near "$P" "<lo que le falta a $P para crear $CAPT y $ARB (paso plan)>"
    return 0
  fi
  existe "$FONDEO" || die "el hub $FONDEO no existe en $RED (una implicita existe recien al recibir NEAR)"
  existe "$C2" || die "$C2 no existe: la crea scripts/mainnet.sh (paso cuentas) como reserva de v2"
  existe "$P" || die "$P no existe: v2 va bajo el padre de v1 (scripts/mainnet.sh)"
  estado_plan
  presupuesto_v2 >/dev/null
  # Nada se manda si el hub no tiene todo lo de este despliegue: un fondeo a medias no deja nada desplegable.
  mayor_o_igual "$E_HUB_LIBRE" "$P_TODO" || die "el hub tiene $(en_near "$E_HUB_LIBRE") libres y todo necesita $(en_near "$P_TODO") (paso plan): no se manda nada"
  local v2_falta padre_falta necesita antes
  v2_falta=0
  if [ "$E_V2_CODIGO" = 0 ]; then
    v2_falta=$(restar_positivo "$(sumar "$(bytes_yocto $((E_WASM + ESTADO_V2)))" "$(yocto "$RESERVA")" "$(yocto "$GAS_DEPLOY")" "$(yocto "$GAS_TX")")" "$E_V2_SALDO")
  fi
  if [ "$v2_falta" = 0 ]; then
    echo "v2 ya tiene lo suyo: $(en_near "$E_V2_SALDO")$( [ "$E_V2_CODIGO" = 1 ] && echo ", con codigo")"
  else
    tx_con_llave fondeo "$FONDEO" full "fondeo $C2" tokens "$FONDEO" send-near "$C2" "$(a_near_arriba "$v2_falta")"
    esperar_saldo "$C2" "$(sumar "$E_V2_SALDO" "$v2_falta")"
    echo "OK: $C2 tiene $(en_near "$(saldo_de "$C2")")"
  fi
  necesita=0
  existe "$CAPT" || necesita=$(sumar "$necesita" "$(yocto "$NEAR_CAPT")" "$(yocto "$CREACION")")
  existe "$ARB" || necesita=$(sumar "$necesita" "$(yocto "$NEAR_ARB")" "$(yocto "$CREACION")")
  padre_falta=$(restar_positivo "$necesita" "$(libre_de "$P")")
  if [ "$padre_falta" = 0 ]; then
    echo "el padre ya tiene lo de las cuentas: $(en_near "$(libre_de "$P")") libres"
  else
    antes=$(saldo_de "$P")
    tx_con_llave fondeo "$FONDEO" full "fondeo $P" tokens "$FONDEO" send-near "$P" "$(a_near_arriba "$padre_falta")"
    esperar_saldo "$P" "$(sumar "$antes" "$padre_falta")"
    echo "OK: $P tiene $(en_near "$(libre_de "$P")") libres"
  fi
  anotar_saldos fondeo
}

# Crea la hija $1 con $2 NEAR (firma el padre); su llave queda en el llavero legacy hasta `secretos`.
crear() {
  if real && existe "$1"; then
    echo "ya existe: $1"
    return 0
  fi
  if real; then
    mayor_o_igual "$(libre_de "$P")" "$(sumar "$(yocto "$2")" "$(yocto "$CREACION")")" ||
      die "$P tiene $(en_near "$(libre_de "$P")") libres y crear $1 necesita $2 + $CREACION: corre el paso fondeo (o humo-cuentas)"
  fi
  tx_con_llave padre "$P" full "cuenta $1" account create-account fund-myself "$1" "$2" autogenerate-new-keypair "$SAVE" sign-as "$P"
}

paso_cuentas() {
  crear "$CAPT" "$NEAR_CAPT"
  crear "$ARB" "$NEAR_ARB"
  anotar_saldos cuentas
}

# storage_deposit en USDC de $1, firmado por el hub (registration_only: lo de mas vuelve).
registrar_usdc() {
  if real && registrada_usdc "$1"; then
    echo "ya registrada en USDC: $1"
    return 0
  fi
  tx_con_llave fondeo "$FONDEO" full "usdc $1" contract call-function as-transaction "$USDC" storage_deposit json-args "$(jq -nc --arg a "$1" '{account_id:$a,registration_only:true}')" prepaid-gas '30.0 Tgas' attached-deposit "$USDC_STORAGE" sign-as "$FONDEO"
}

paso_usdc() {
  registrar_usdc "$C2"
  registrar_usdc "$FEES"
  anotar_saldos usdc
}

config_ok() {
  local c
  c=$(vista "$C2" get_config '{}')
  jq -e --arg o "$OWNER" --arg g "$GUARD" --arg t "$USDC" --arg p "$TOPE_POR_PAGO" --arg v "$TOPE_TVL_GLOBAL" \
    '.version == "arras-v2" and .owner == $o and .guardian == $g and .token == $t and .tope_por_pago == $p and .tope_tvl_global == $v and .paused == false' \
    >/dev/null <<<"$c" || die "get_config de $C2 no es el esperado: $c"
  echo "OK: get_config de $C2 = arras-v2, owner $OWNER, guardian $GUARD, token $USDC, topes $TOPE_POR_PAGO / $TOPE_TVL_GLOBAL, sin pausa"
}

paso_deploy() {
  wasm_ok
  local h _i cero
  cero=$(printf '0%.0s' {1..64})
  if real; then
    h=$(codigo_hex "$C2")
    if [ "$h" = "$WASM_SHA" ]; then
      echo "ya desplegado: el codigo de $C2 es la wasm $WASM_SHA"
      config_ok
      return 0
    fi
    [ "$h" = "$cero" ] || die "$C2 ya tiene otro codigo (sha256 $h): no se pisa"
    mayor_o_igual "$(libre_de "$C2")" "$(sumar "$(bytes_yocto $(($(wc -c <"$WASM" | tr -d ' ') + ESTADO_V2)))" "$(yocto "$GAS_DEPLOY")")" ||
      die "$C2 tiene $(en_near "$(libre_de "$C2")") libres y no le alcanza para el codigo: corre el paso fondeo"
  fi
  tx_con_llave v2-reserva "$C2" full "deploy + new $C2" contract deploy "$C2" use-file "$WASM" with-init-call new json-args "$ARGS_NEW" prepaid-gas '30.0 Tgas' attached-deposit '0 NEAR'
  if real; then
    for _i in 1 2 3 4 5 6; do
      h=$(codigo_hex "$C2")
      [ "$h" = "$WASM_SHA" ] && break
      sleep "$ESPERA"
    done
    [ "$h" = "$WASM_SHA" ] || die "el codigo de $C2 tiene sha256 $h y la wasm $WASM_SHA"
    echo "OK: el codigo de $C2 es la wasm $WASM_SHA (rev $REV)"
    config_ok
  fi
  anotar_saldos deploy
}

# El operador en cadena tiene que ser el decidido en cada campo que importa. $1 = la ticket_pubkey esperada: "" = sin
# ticket, "*" = una cualquiera (verificar, que no lee la llave privada), o la exacta.
operador_ok() {
  local o
  o=$(vista "$C2" get_operator "$(jq -nc --arg id "$OP" '{operator_id:$id}')")
  jq -e --arg ow "$OWNER" --arg c "$CAPT" --argjson f "$FEE_BPS" --arg r "$FEES" --arg m "$MIN_AMOUNT" --arg to "$TOPE_TVL_OPERADOR" \
    --arg tp "$TOPE_TVL_POR_PAYER" --arg s "$SPONSOR_MAX" --arg a "$ARB" --arg w "$VENTANA_NS" --arg k "$1" \
    '.owner == $ow and .capturers == [$c] and .fee_bps == $f and .fee_recipient == $r and .min_amount == $m and .tope_tvl_operador == $to and .tope_tvl_por_payer == $tp and .sponsor_max_por_payer == $s and .arbiter == $a and .arbiter_window_ns == $w and (if $k == "*" then ((.ticket_pubkey // "") | startswith("ed25519:")) else ((.ticket_pubkey // "") == $k) end)' \
    >/dev/null <<<"$o" || die "get_operator $OP de $C2 no es el esperado: $o"
  printf '%s' "$o"
}
# Una cuenta que puede mover lo que recibe: una implicita (su llave es la de su nombre) o una con alguna FullAccess.
controlable() {
  implicita "$1" && return 0
  existe "$1" || return 1
  [ -n "$(llaves_full "$1")" ]
}
# La ticket_pubkey de un operador ya registrado ("" si no tiene).
tpk_en_cadena() { vista "$C2" get_operator "$(jq -nc --arg id "$OP" '{operator_id:$id}')" | jq -r '.ticket_pubkey // empty'; }

paso_operador() {
  local o=null tpk="" input
  real && o=$(vista "$C2" get_operator "$(jq -nc --arg id "$OP" '{operator_id:$id}')")
  if real && [ "$o" != null ]; then
    # Ya registrado: lo esperado es lo que hay en cadena, coherente con ARRAS_V2_TICKET (y nunca se genera otra llave).
    tpk=$(jq -r '.ticket_pubkey // empty' <<<"$o")
    if [ "$TICKET" = si ] && [ -z "$tpk" ]; then die "el operador $OP de $C2 esta registrado sin ticket_pubkey y ARRAS_V2_TICKET=si"; fi
    if [ "$TICKET" = no ] && [ -n "$tpk" ]; then die "el operador $OP de $C2 tiene ticket_pubkey ($tpk) y ARRAS_V2_TICKET=no"; fi
  elif real && [ "$TICKET" = si ]; then
    openssl_ok
    tpk=$(ticket_pubkey)
  fi
  if [ "$o" = null ]; then
    # La tarifa congelada en cada pago va al fee_recipient: tiene que poder moverla (una FullAccess o una implicita).
    if real; then
      controlable "$FEES" || die "el fee_recipient $FEES no tiene ninguna FullAccess en cadena: la tarifa de v2 quedaria atrapada"
    fi
    real || tpk="<ticket_pubkey: la publica de una llave ed25519 nueva en $TICKET_PEM>"
    [ "$TICKET" = si ] || tpk=""
    input=$(jq -nc --arg o "$OWNER" --arg c "$CAPT" --argjson f "$FEE_BPS" --arg r "$FEES" --arg m "$MIN_AMOUNT" --arg to "$TOPE_TVL_OPERADOR" \
      --arg tp "$TOPE_TVL_POR_PAYER" --arg s "$SPONSOR_MAX" --arg a "$ARB" --arg w "$VENTANA_NS" --arg k "$tpk" \
      '{owner:$o,capturers:[$c],fee_bps:$f,fee_recipient:$r,min_amount:$m,tope_tvl_operador:$to,tope_tvl_por_payer:$tp,sponsor_max_por_payer:$s,arbiter:$a,arbiter_window_ns:$w} + (if $k == "" then {} else {ticket_pubkey:$k} end)')
    tx_con_llave owner "$OWNER" full "register_operator $OP" contract call-function as-transaction "$C2" register_operator json-args "$(jq -nc --arg id "$OP" --argjson i "$input" '{operator_id:$id,input:$i}')" prepaid-gas '30.0 Tgas' attached-deposit '1 yoctoNEAR' sign-as "$OWNER"
  else
    echo "ya registrado: operador $OP"
  fi
  real && o=$(operador_ok "$tpk")
  # El pool se fondea UNA vez (la marca $POOL_MARCA): si EM lo gasta, un segundo todo no le manda otro a una cuenta sin
  # llaves que no lo devuelve; recargarlo es fund_operator_storage a mano.
  if real && [ "$(jq -r .sponsor_pool <<<"$o")" != 0 ]; then
    echo "pool: sponsor_pool $(jq -r .sponsor_pool <<<"$o") yocto (ARRAS_V2_POOL $POOL)"
    [ -f "$POOL_MARCA" ] || date -u +%FT%TZ >"$POOL_MARCA"
  elif real && [ -f "$POOL_MARCA" ]; then
    echo "pool: ya se fondeo una vez ($(cat "$POOL_MARCA")) y hoy esta en 0: no se recarga solo"
  elif [ "$(yocto "$POOL")" != 0 ]; then
    # fund_operator_storage lo puede firmar cualquiera: lo firma el hub (el owner queda con poco NEAR a proposito).
    tx_con_llave fondeo "$FONDEO" full "fund_operator_storage $OP" contract call-function as-transaction "$C2" fund_operator_storage json-args "$(jq -nc --arg id "$OP" '{operator_id:$id}')" prepaid-gas '30.0 Tgas' attached-deposit "$POOL" sign-as "$FONDEO"
    real && date -u +%FT%TZ >"$POOL_MARCA"
  fi
  if real; then
    o=$(operador_ok "$tpk")
    echo "OK: operador $OP = $o"
  fi
  anotar_saldos operador
}

# La FunctionCall key de $1 para v2 con los metodos $2 (public_key), si esta.
llave_fc() { vivas "$1" "$2" | head -1; }
# $1 tiene que quedar con UNA sola llave: la FunctionCall de v2 con los metodos $2 (ni FullAccess ni otra de mas).
fc_ok() {
  local l
  l=$(llaves_json "$1")
  jq -e --arg c "$C2" --arg m "$2" 'length == 1 and ((.[0].access_key.permission | objects | .FunctionCall) as $f | $f != null and $f.receiver_id == $c and (($f.method_names | sort | join(",")) == $m))' >/dev/null <<<"$l" ||
    die "$1 tiene que quedar con UNA sola llave, la FunctionCall de $C2 ($2); tiene: $(jq -c '[.[] | {public_key, permission: (.access_key.permission | if type == "object" then {receiver: .FunctionCall.receiver_id, methods: .FunctionCall.method_names} else . end)}]' <<<"$l")"
}

# La FunctionCall key de $2 (rol $1) para v2 con los metodos $3 y allowance $4, y el borrado de su FullAccess (firma su
# propia FullAccess, del llavero legacy).
paso_fc() {
  local rol=$1 cuenta=$2 metodos=$3 allowance=$4 fc k full
  fc=""
  real && fc=$(llave_fc "$cuenta" "$metodos")
  if real && [ -n "$fc" ] && [ -z "$(llaves_full "$cuenta")" ]; then
    fc_ok "$cuenta" "$metodos"
    echo "ya listo: $cuenta solo tiene la FunctionCall key $fc ($metodos en $C2)"
    return 0
  fi
  if [ -z "$fc" ]; then
    tx_con_llave "$rol" "$cuenta" full "$rol: FunctionCall key" account add-key "$cuenta" grant-function-call-access --allowance "$allowance" --contract-account-id "$C2" --function-names "$(printf '%s' "$metodos" | sed 's/,/, /g')" autogenerate-new-keypair "$SAVE"
  else
    echo "ya tiene FunctionCall key para $C2: $fc"
  fi
  if ! real; then
    tx_con_llave "$rol" "$cuenta" full "$rol: borra FullAccess" account delete-keys "$cuenta" public-keys '<la FullAccess de creacion>'
    return 0
  fi
  fc=$(llave_fc "$cuenta" "$metodos")
  [ -n "$fc" ] || die "$cuenta no quedo con una FunctionCall key para $C2 con exactamente $metodos"
  llaves_json "$cuenta" | jq -e --arg k "$fc" --arg a "$(yocto "$allowance")" '.[] | select(.public_key == $k) | .access_key.permission.FunctionCall.allowance == $a' >/dev/null ||
    die "la FunctionCall key $fc de $cuenta no tiene allowance $allowance: no se borra la FullAccess"
  [ -f "$(archivo_llave "$cuenta" "$fc")" ] || die "la FunctionCall key $fc no esta en el llavero legacy ($(archivo_llave "$cuenta" "$fc"))"
  full=$(llaves_full "$cuenta")
  for k in $full; do
    tx_con_llave "$rol" "$cuenta" full "$rol: borra FullAccess $k" account delete-keys "$cuenta" public-keys "$k"
  done
  [ -z "$(llaves_full "$cuenta")" ] || die "$cuenta todavia tiene una FullAccess"
  fc_ok "$cuenta" "$metodos"
  echo "OK: $cuenta solo tiene la FunctionCall key $fc ($metodos en $C2, allowance $allowance)"
}

paso_capturer() { paso_fc em-capturer-v2 "$CAPT" "$METODOS_CAPT" "$ALLOW_CAPT"; anotar_saldos capturer; }
paso_arbitro() { paso_fc em-arbiter-v2 "$ARB" "$METODOS_ARB" "$ALLOW_ARB"; anotar_saldos arbitro; }

paso_sin_llaves() {
  local todas n h
  if real; then
    # Sin llaves ya no hay vuelta atras (con una llave, un deploy malo se arregla con delete-account): primero el codigo
    # tiene que ser el del pin y get_config el esperado.
    h=$(codigo_hex "$C2")
    [ "$h" = "$WASM_SHA" ] || die "$C2 tiene codigo sha256 $h y no la wasm $WASM_SHA: no se borran sus llaves (paso deploy)"
    config_ok
    fuente_ok
    todas=$(llaves_json "$C2" | jq -r '[.[].public_key] | join(",")')
    if [ -n "$todas" ]; then
      tx_con_llave v2-reserva "$C2" full "sin-llaves $C2" account delete-keys "$C2" public-keys "$todas"
    else
      echo "ya sin llaves: $C2"
    fi
    n=$(n_llaves "$C2")
    [ "$n" = 0 ] || die "a $C2 le quedan $n llaves: view_access_key_list no esta vacio"
    echo "OK: view_access_key_list de $C2 vacio (nadie puede redesplegar ni tocar la cuenta del contrato)"
  else
    tx_con_llave v2-reserva "$C2" full "sin-llaves $C2" account delete-keys "$C2" public-keys '<todas las de view_access_key_list, separadas por coma>'
  fi
  anotar_saldos sin-llaves
}

# NEP-330: contract_source_metadata tiene que apuntar al commit del pin.
fuente_ok() {
  local m
  m=$(vista "$C2" contract_source_metadata '{}')
  jq -e --arg r "$REV" '(.build_info.source_code_snapshot // "") | endswith("?rev=" + $r)' >/dev/null <<<"$m" ||
    die "contract_source_metadata de $C2 no apunta a rev $REV: $(jq -c '.build_info // .' <<<"$m")"
  echo "OK: NEP-330 de $C2: $(jq -r '.build_info.source_code_snapshot' <<<"$m")"
}

paso_verificar() {
  wasm_ok
  if ! real; then
    echo "[en seco] lee por RPC: code_hash y llaves de $C2, contract_source_metadata, get_config, get_operator $OP, las llaves de $CAPT y $ARB y get_storage_health"
    return 0
  fi
  local h n s tpk=""
  h=$(codigo_hex "$C2")
  n=$(n_llaves "$C2")
  echo "codigo de $C2: sha256 $h ($( [ "$h" = "$WASM_SHA" ] && echo "= pin" || echo "DISTINTO del pin $WASM_SHA"))"
  echo "llaves de $C2: $n"
  fuente_ok
  config_ok
  # Solo lectura: no se lee la llave privada del ticket de SSM; si esta el PEM local, tiene que ser la de cadena.
  if [ "$TICKET" = si ]; then
    tpk='*'
    if [ -f "$TICKET_PEM" ]; then
      openssl_ok
      tpk=$(pub_de_pem <"$TICKET_PEM")
    fi
  fi
  echo "get_operator $OP: $(operador_ok "$tpk")"
  fc_ok "$CAPT" "$METODOS_CAPT"
  echo "capturer $CAPT: una sola llave, la FunctionCall de $C2 ($METODOS_CAPT)"
  fc_ok "$ARB" "$METODOS_ARB"
  echo "arbitro $ARB: una sola llave, la FunctionCall de $C2 ($METODOS_ARB)"
  registrada_usdc "$C2" || die "$C2 no esta registrada en USDC: ningun ft_transfer_call le llegaria (paso usdc)"
  echo "USDC: $C2 registrada"
  s=$(vista "$C2" get_storage_health '{}')
  echo "get_storage_health: $s"
  [ "$h" = "$WASM_SHA" ] && [ "$n" = 0 ] || die "$C2 no esta listo (codigo o llaves)"
  jq -e '.ok == true' >/dev/null <<<"$s" || die "get_storage_health de $C2 en alarma (reserva libre bajo 0,2 NEAR): $s"
}

# --- humo -------------------------------------------------------------------------------------------------------------
# Antes de mover USDC: v2 desplegada y verificada (codigo, operador, capturer y arbitro).
humo_listo() {
  real || return 0
  [ "$(codigo_hex "$C2")" = "$WASM_SHA" ] || die "$C2 no tiene la wasm del pin: el humo va despues de todo"
  local tpk=""
  if [ "$TICKET" = si ]; then
    openssl_ok
    tpk=$(tpk_en_cadena)
    [ -n "$tpk" ] || die "el operador $OP de $C2 no tiene ticket_pubkey y ARRAS_V2_TICKET=si"
    [ "$(pem_ticket | pub_de_pem)" = "$tpk" ] || die "la llave del ticket (local o SSM) no es la ticket_pubkey del operador $OP ($tpk)"
  fi
  operador_ok "$tpk" >/dev/null
  fc_ok "$CAPT" "$METODOS_CAPT"
  fc_ok "$ARB" "$METODOS_ARB"
}

paso_humo_cuentas() {
  humo_listo
  local necesita=0 falta antes
  if real; then
    # El worker cobra USDC de verdad: si ya existe (o no es nuestro), tiene que poder moverlo; antes de gastar nada.
    if [ "$WORKER_NUEVA" = 0 ] || existe "$WORKER"; then
      controlable "$WORKER" || die "el worker $WORKER no tiene ninguna FullAccess en cadena: lo que cobre quedaria atrapado"
    fi
    existe "$AGENTE" || necesita=$(sumar "$necesita" "$(yocto "$NEAR_AGENTE")" "$(yocto "$CREACION")")
    if [ "$WORKER_NUEVA" = 1 ] && ! existe "$WORKER"; then necesita=$(sumar "$necesita" "$(yocto "$NEAR_WORKER")" "$(yocto "$CREACION")"); fi
    falta=$(restar_positivo "$necesita" "$(libre_de "$P")")
    if [ "$falta" != 0 ]; then
      mayor_o_igual "$(libre_de "$FONDEO")" "$(sumar "$falta" "$(yocto "$GAS_TX")")" || die "el hub no tiene $(en_near "$falta") para las cuentas del humo"
      antes=$(saldo_de "$P")
      tx_con_llave fondeo "$FONDEO" full "humo: fondeo $P" tokens "$FONDEO" send-near "$P" "$(a_near_arriba "$falta")"
      esperar_saldo "$P" "$(sumar "$antes" "$falta")"
    fi
  else
    tx_con_llave fondeo "$FONDEO" full "humo: fondeo $P" tokens "$FONDEO" send-near "$P" "<lo que le falta a $P para $AGENTE y $WORKER>"
  fi
  crear "$AGENTE" "$NEAR_AGENTE"
  if [ "$WORKER_NUEVA" = 1 ]; then crear "$WORKER" "$NEAR_WORKER"; fi
  paso_fc humo-agente-v2 "$AGENTE" "$METODOS_AGENTE" "$ALLOW_AGENTE"
  registrar_usdc "$WORKER"
  anotar_saldos humo-cuentas
}

paso_humo_presupuesto() {
  humo_listo
  local p msg ticket expiry hechos e necesita pedido nep145
  nep145=null
  if real; then
    # Decide la cadena, no los archivos: cuanto del presupuesto falta usar segun los pagos del humo que ya existen.
    hechos=0
    for e in pago disputa; do
      [ "$(estado_humo "$e")" = null ] || hechos=$((hechos + 1))
    done
    necesita=$((HUMO_MONTO * (2 - hechos)))
    p=$(presupuesto_de)
    if [ "$p" != null ]; then
      [ "$(jq -r .saldo <<<"$p")" = "$necesita" ] || die "ya hay un presupuesto de $FONDEO para $AGENTE con otro saldo (se esperaba $necesita): $p"
      echo "ya depositado: presupuesto $p"
      return 0
    fi
    # Despues de un compact los pagos ya no estan en cadena: la marca del deposito evita un segundo presupuesto.
    if [ "$hechos" != 0 ] || [ -f "$HUMO/presupuesto.hecho" ]; then
      echo "el presupuesto ya se uso y se revoco ($hechos pagos del humo en cadena; marca $HUMO/presupuesto.hecho): no se deposita otro"
      return 0
    fi
    mayor_o_igual "$(usdc_de "$FONDEO")" "$HUMO_PRESUPUESTO" || die "el hub tiene $(usdc_de "$FONDEO") de USDC y el humo necesita $HUMO_PRESUPUESTO"
    # Lo libre para el ft_transfer_call, mas el deposito NEP-145 si todavia hay que hacerlo.
    nep145=$(vista "$C2" storage_balance_of "$(jq -nc --arg a "$FONDEO" '{account_id:$a}')")
    pedido=$(yocto "$LIBRE_HUB")
    [ "$nep145" != null ] || pedido=$(sumar "$pedido" "$(yocto "$NEP145_HUB")" "$(yocto "$GAS_TX")")
    mayor_o_igual "$(libre_de "$FONDEO")" "$pedido" ||
      die "el hub tiene $(en_near "$(libre_de "$FONDEO")") libres y el humo necesita $(en_near "$pedido") (el ft_transfer_call reserva hasta $LIBRE_HUB)"
  fi
  # El storage NEP-145 del hub en v2 (paga el registro del presupuesto y los dos pagos; lo libre vuelve en humo-cierre).
  if real && [ "$nep145" != null ]; then
    echo "ya registrado en v2 (NEP-145): $FONDEO"
  else
    tx_con_llave fondeo "$FONDEO" full "humo: storage_deposit del hub en v2" contract call-function as-transaction "$C2" storage_deposit json-args "$(jq -nc --arg a "$FONDEO" '{account_id:$a}')" prepaid-gas '30.0 Tgas' attached-deposit "$NEP145_HUB" sign-as "$FONDEO"
  fi
  ticket=null
  if [ "$TICKET" = si ]; then
    if real; then
      expiry=$(( $(ahora_ns) + 600000000000 ))
      ticket=$(firmar_ticket ticket_message_budget "$(jq -nc --arg op "$OP" --arg o "$FONDEO" --arg a "$AGENTE" --arg e "$expiry" '{operator_id:$op,owner:$o,agent:$a,expiry:$e}')")
    else
      ticket='{"expiry":"<ahora + 10 min>","sig":"<firma ed25519 de ticket_message_budget>"}'
    fi
  fi
  msg=$(jq -nc --arg op "$OP" --arg a "$AGENTE" --arg t "$HUMO_MONTO" --arg d "$HUMO_PRESUPUESTO" --argjson k "$ticket" \
    '{v:1,action:"budget",operator_id:$op,agent:$a,tope_por_pago:$t,tope_diario:$d} + (if $k then {operator_ticket:$k} else {} end)')
  tx_con_llave fondeo "$FONDEO" full "humo: presupuesto $HUMO_PRESUPUESTO para $AGENTE" contract call-function as-transaction "$USDC" ft_transfer_call json-args "$(jq -nc --arg r "$C2" --arg a "$HUMO_PRESUPUESTO" --arg m "$msg" '{receiver_id:$r,amount:$a,msg:$m}')" prepaid-gas '100.0 Tgas' attached-deposit '1 yoctoNEAR' sign-as "$FONDEO"
  if real; then
    hasta presupuesto_con "$HUMO_PRESUPUESTO" || die "el deposito no creo el presupuesto (rechazo: mira el log arras:rechazo de la tx): $(presupuesto_de)"
    mkdir -p "$HUMO"
    date -u +%FT%TZ >"$HUMO/presupuesto.hecho"
    echo "OK: presupuesto de $FONDEO para $AGENTE: $(presupuesto_de)"
  fi
  anotar_saldos humo-presupuesto
}

# Los args de authorize_from_budget de un pago del humo ($1 = pago | disputa), guardados para que una segunda corrida
# siga con el mismo pago: owner = el hub, receiver = el worker, release en 1 dia, reclaim en 2 (E57: <= 60).
args_humo() {
  local f=$HUMO/$1.json now salt
  if real && [ -f "$f" ]; then
    jq -c .args "$f"
    return 0
  fi
  now=$(ahora_ns)
  salt=$(python3 -I -c 'import secrets; print(secrets.token_hex(16))')
  jq -nc --arg o "$FONDEO" --arg op "$OP" --arg r "$WORKER" --arg a "$HUMO_MONTO" --arg rel "$((now + 86400000000000))" \
    --arg rec "$((now + 172800000000000))" --argjson f "$FEE_BPS" --arg t "$(printf 'arras v2 humo %s %s' "$1" "$salt" | shasum -a 256 | awk '{ print $1 }')" --arg s "$salt" \
    '{owner:$o,operator_id:$op,receiver:$r,amount:$a,release_deadline:$rel,reclaim_after:$rec,fee_bps_max:$f,terms_hash:$t,salt:$s}'
}
# Un payment_id: base58 de 32 bytes (de 32 a 44 caracteres).
id_valido() { [[ $1 =~ ^[1-9A-HJ-NP-Za-km-z]{32,44}$ ]]; }
# El payment_id de un pago del humo (lo guarda la primera vez). Corre dentro de un $(...): si la vista falla, `die` corta
# el script principal pero esta subshell sigue, asi que no se escribe nada sin un id valido (y se escribe de una vez).
id_humo() {
  local f=$HUMO/$1.json args id
  if [ -f "$f" ]; then
    id=$(jq -r '.payment_id // empty' "$f")
    id_valido "$id" || die "$f tiene un payment_id invalido ('$id'): borralo si ese pago no existe en cadena"
    printf '%s' "$id"
    return 0
  fi
  args=$(args_humo "$1")
  id=$(vista "$C2" compute_budget_payment_id "$args" | tr -d '"')
  id_valido "$id" || die "compute_budget_payment_id no devolvio un payment_id ('$id')"
  mkdir -p "$HUMO"
  { jq -nc --argjson a "$args" --arg id "$id" '{args:$a,payment_id:$id}' >"$f.tmp" && mv "$f.tmp" "$f"; } ||
    die "no se pudo guardar $f (el pago tiene que quedar anotado antes de firmarlo)"
  printf '%s' "$id"
}
# El agente autoriza el pago del humo $1 desde el presupuesto, con su FunctionCall key y el ticket del pago.
autorizar_humo() {
  local args id ticket=null afb
  if real; then
    # Primero el id (guarda los args la primera vez), despues los args guardados: una sola sal por pago.
    id=$(id_humo "$1")
    args=$(args_humo "$1")
    # Un pago guardado que nunca entro y cuyo release_deadline ya (casi) paso daria E11 en cada corrida: se aparta y se
    # arma otro (el que se aparta no esta en cadena: autorizar solo corre con el pago en null).
    if [ "$(jq -r .release_deadline <<<"$args")" -lt $(( $(ahora_ns) + 3600000000000 )) ]; then
      mv "$HUMO/$1.json" "$HUMO/$1.json.vencido-$(date -u +%Y%m%dT%H%M%SZ)"
      echo "el pago guardado de $1 vencio sin entrar: se arma otro"
      id=$(id_humo "$1")
      args=$(args_humo "$1")
    fi
    if [ "$TICKET" = si ]; then
      ticket=$(firmar_ticket ticket_message_payment "$(jq -nc --arg p "$id" --arg e "$(( $(ahora_ns) + 600000000000 ))" '{payment_id:$p,expiry:$e}')")
    fi
  else
    args=$(args_humo "$1")
    [ "$TICKET" = si ] && ticket='{"expiry":"<ahora + 10 min>","sig":"<firma ed25519 de ticket_message_payment>"}'
  fi
  # dispute_deadline = release_deadline (E67: no antes).
  afb=$(jq -c --argjson k "$ticket" '. + {dispute_deadline:.release_deadline} + (if $k then {operator_ticket:$k} else {} end)' <<<"$args")
  tx_con_llave humo-agente-v2 "$AGENTE" "$METODOS_AGENTE" "humo $1: authorize_from_budget" contract call-function as-transaction "$C2" authorize_from_budget json-args "$afb" prepaid-gas '30.0 Tgas' attached-deposit '0 NEAR' sign-as "$AGENTE"
}
# Una llamada del capturer a v2 con su FunctionCall key.
capturer_llama() {
  tx_con_llave em-capturer-v2 "$CAPT" "$METODOS_CAPT" "$1" contract call-function as-transaction "$C2" "$2" json-args "$3" prepaid-gas "$4" attached-deposit '0 NEAR' sign-as "$CAPT"
}
# withdraw_for de $1 (lo firma el capturer) si tiene algo para cobrar; despues, claimable en 0 y el USDC en su cuenta (un
# ft_transfer que falla devuelve el saldo al claimable en el callback).
retirar_por() {
  local c antes
  if real; then
    c=$(claimable_de "$1")
    if [ "$c" = 0 ]; then
      echo "nada para cobrar: $1"
      return 0
    fi
    antes=$(usdc_de "$1")
  fi
  capturer_llama "humo: withdraw_for $1" withdraw_for "$(jq -nc --arg a "$1" '{account:$a}')" '50.0 Tgas'
  if real; then
    hasta claimable_en_cero "$1" || die "a $1 le quedo claimable $(claimable_de "$1") despues del withdraw_for (el ft_transfer fallo?)"
    hasta usdc_al_menos "$1" "$(sumar "$antes" "$c")" || die "$1 tenia $antes de USDC y no recibio los $c del withdraw_for"
    echo "OK: $1 cobro $c; USDC $(usdc_de "$1")"
  fi
}
claimable_en_cero() { [ "$(claimable_de "$1")" = 0 ]; }
usdc_al_menos() { mayor_o_igual "$(usdc_de "$1")" "$2"; }
# El pago del humo $1 en el estado $2.
estado_es() { [ "$(estado_humo "$1")" = "$2" ]; }
# El estado de un pago del humo, o null (y en seco, "seco").
estado_humo() {
  real || { echo seco; return 0; }
  [ -f "$HUMO/$1.json" ] || { echo null; return 0; }
  pago_de "$(id_humo "$1")" | jq -r 'if . == null then "null" else .status end'
}

paso_humo_pago() {
  humo_listo
  local e id
  e=$(estado_humo pago)
  if [ "$e" = null ] || [ "$e" = seco ]; then
    autorizar_humo pago
    real && { hasta estado_es pago Authorized || die "el authorize_from_budget no dejo el pago Authorized: $(estado_humo pago)"; e=Authorized; }
  fi
  id='<payment_id de humo-pago>'
  real && id=$(id_humo pago)
  if [ "$e" = Authorized ] || [ "$e" = seco ]; then
    capturer_llama "humo pago: release" release "$(jq -nc --arg id "$id" '{payment_id:$id}')" '30.0 Tgas'
    real && { hasta estado_es pago Released || die "el pago $id no quedo Released: $(estado_humo pago)"; e=Released; }
  fi
  [ "$e" = Released ] || [ "$e" = seco ] || die "el pago del humo $id esta $e (se esperaba Released)"
  real && echo "OK: pago $id Released; claimable worker $(claimable_de "$WORKER"), fee_recipient $(claimable_de "$FEES")"
  retirar_por "$WORKER"
  retirar_por "$FEES"
  anotar_saldos humo-pago
}

paso_humo_disputa() {
  humo_listo
  local e id
  e=$(estado_humo disputa)
  if [ "$e" = null ] || [ "$e" = seco ]; then
    autorizar_humo disputa
    real && { hasta estado_es disputa Authorized || die "el authorize_from_budget no dejo el pago Authorized: $(estado_humo disputa)"; e=Authorized; }
  fi
  id='<payment_id de humo-disputa>'
  real && id=$(id_humo disputa)
  if [ "$e" = Authorized ] || [ "$e" = seco ]; then
    tx_con_llave fondeo "$FONDEO" full "humo disputa: open_dispute" contract call-function as-transaction "$C2" open_dispute json-args "$(jq -nc --arg id "$id" --arg h "$(printf 'arras v2 humo: disputa de prueba' | shasum -a 256 | awk '{ print $1 }')" '{payment_id:$id,reason_hash:$h}')" prepaid-gas '30.0 Tgas' attached-deposit '0 NEAR' sign-as "$FONDEO"
    real && { hasta estado_es disputa Disputed || die "el pago $id no quedo Disputed: $(estado_humo disputa)"; e=Disputed; }
  fi
  if [ "$e" = Disputed ] || [ "$e" = seco ]; then
    tx_con_llave em-arbiter-v2 "$ARB" "$METODOS_ARB" "humo disputa: resolve_dispute (todo al payer)" contract call-function as-transaction "$C2" resolve_dispute json-args "$(jq -nc --arg id "$id" --arg a "$HUMO_MONTO" '{payment_id:$id,to_receiver:"0",to_payer:$a}')" prepaid-gas '30.0 Tgas' attached-deposit '0 NEAR' sign-as "$ARB"
    real && { hasta estado_es disputa Resolved || die "el pago $id no quedo Resolved: $(estado_humo disputa)"; e=Resolved; }
  fi
  [ "$e" = Resolved ] || [ "$e" = seco ] || die "el pago de la disputa $id esta $e (se esperaba Resolved)"
  if real; then
    pago_de "$id" | jq -e --arg a "$HUMO_MONTO" '.refunded == $a and .released == "0"' >/dev/null || die "la disputa no le devolvio todo al payer: $(pago_de "$id")"
    echo "OK: disputa $id Resolved, todo al payer; claimable del hub $(claimable_de "$FONDEO")"
  fi
  retirar_por "$FONDEO"
  anotar_saldos humo-disputa
}

paso_humo_cierre() {
  humo_listo
  local p s
  p=null
  real && p=$(presupuesto_de)
  if [ "$p" != null ] || ! real; then
    tx_con_llave fondeo "$FONDEO" full "humo: revoke_agent" contract call-function as-transaction "$C2" revoke_agent json-args "$(jq -nc --arg a "$AGENTE" --arg op "$OP" '{agent:$a,operator_id:$op}')" prepaid-gas '30.0 Tgas' attached-deposit '0 NEAR' sign-as "$FONDEO"
    # Lo que quedaba en el presupuesto pasa al claimable del hub: se retira abajo, despues de verlo revocado.
    if real; then hasta presupuesto_con "" || die "el presupuesto de $AGENTE sigue: revoke_agent no corrio"; fi
  else
    echo "ya revocado: $AGENTE"
  fi
  retirar_por "$FONDEO"
  s=null
  real && s=$(vista "$C2" storage_balance_of "$(jq -nc --arg a "$FONDEO" '{account_id:$a}')")
  if ! real || { [ "$s" != null ] && [ "$(jq -r .available <<<"$s")" != 0 ]; }; then
    tx_con_llave fondeo "$FONDEO" full "humo: storage_withdraw del hub" contract call-function as-transaction "$C2" storage_withdraw json-args '{}' prepaid-gas '30.0 Tgas' attached-deposit '1 yoctoNEAR' sign-as "$FONDEO"
  fi
  if real; then
    echo "get_config: $(vista "$C2" get_config '{}' | jq -c '{tvl_global,in_flight,budgets_total,payments}')"
    echo "USDC: v2 $(usdc_de "$C2"), hub $(usdc_de "$FONDEO"), worker $(usdc_de "$WORKER"), $FEES $(usdc_de "$FEES")"
    echo "NEP-145 del hub en v2: $(vista "$C2" storage_balance_of "$(jq -nc --arg a "$FONDEO" '{account_id:$a}')")"
    echo "get_storage_health: $(vista "$C2" get_storage_health '{}' | jq -c '{free_reserve,ok}')"
    echo "(los 0,01 a 0,02 NEAR del NEP-145 que guardan los dos pagos vuelven al hub con compact, 7 dias despues de su reclaim_after)"
  fi
  anotar_saldos humo-cierre
}

# --- medir, secretos, enlaces -----------------------------------------------------------------------------------------
paso_medir() {
  if ! real; then
    echo "[en seco] lee por RPC el saldo y el storage de $(cuentas_todas | tr '\n' ' ')y $SALDOS"
    return 0
  fi
  local a r
  printf '%-66s %18s %10s %18s\n' cuenta saldo storage 'storage en NEAR'
  for a in $(cuentas_todas); do
    r=$(ver_cuenta "$a")
    jq -e '.result.amount' >/dev/null 2>&1 <<<"$r" || { printf '%-66s %18s\n' "$a" '(no existe)'; continue; }
    printf '%-66s %18s %10s %18s\n' "$a" "$(en_near "$(jq -r .result.amount <<<"$r")")" "$(jq -r .result.storage_usage <<<"$r")" \
      "$(en_near "$(bytes_yocto "$(jq -r .result.storage_usage <<<"$r")")")"
  done
  existe "$C2" && [ "$(codigo_hex "$C2")" = "$WASM_SHA" ] && echo "get_storage_health de $C2: $(vista "$C2" get_storage_health '{}')"
  if [ -f "$SALDOS" ]; then
    echo
    echo "saldo del hub, del padre y de v2 despues de cada paso ($SALDOS):"
    awk -F'\t' -v h="$FONDEO" -v p="$P" -v c="$C2" '$3 == h || $3 == p || $3 == c { print "  " $2 "\t" $3 "\t" $4 }' "$SALDOS"
  fi
}

# Las llaves nuevas de v2 (rol cuenta permiso): las vivas van a SSM con su nombre; las ya borradas en cadena (las
# FullAccess de creacion) solo se borran del disco.
ROLES_V2="em-capturer-v2 $CAPT
em-arbiter-v2 $ARB
humo-agente-v2 $AGENTE"
if [ "$WORKER_NUEVA" = 1 ]; then
  ROLES_V2="$ROLES_V2
humo-worker-v2 $WORKER"
fi

paso_secretos() {
  local modo=${ARRAS_SECRETOS:-ssm} rol cuenta f pk vivas_c estado copia pk_copia copia_hecha borrar tpk
  case "$modo" in ssm | secretsmanager) ;; *) die "ARRAS_SECRETOS tiene que ser ssm o secretsmanager" ;; esac
  SEC_MODO=$modo SEC_REGION=$REGION SEC_KMS=${ARRAS_KMS_KEY:-<ARRAS_KMS_KEY: la llave KMS propia de Arras (decision 230)>}
  echo "Las de antes de v2 (hub, padre, owner, v2-reserva) ya viven en SSM desde el 2026-11-03: este paso no las toca."
  if ! real; then
    # En seco no sale NINGUNA linea ejecutable: sin la cadena no se sabe que llave sigue viva.
    echo "EN SECO: solo las llaves que hay en $CRED/$NET/ y en $EST, y el nombre que llevarian en $modo; ningun comando."
    echo "Los comandos (subir, comparar, borrar) los da el modo real, que mira cada llave en cadena: scripts/mainnet-v2.sh --$RED-ok secretos"
    while read -r rol cuenta; do
      [ -n "$rol" ] || continue
      for f in "$CRED/$NET/$cuenta"/ed25519_*.json "$CRED/$NET/$cuenta.json"; do
        [ -f "$f" ] || continue
        echo "  $rol ($cuenta): $(jq -r '.public_key // empty' "$f") en $f; nombre: $(nombre_ssm "$rol")"
      done
    done <<<"$ROLES_V2"
    if [ -f "$TICKET_PEM" ]; then echo "  em-ticket-v2: la llave del ticket en $TICKET_PEM; nombre: $(nombre_ssm em-ticket-v2)"; fi
    for f in "$CRED/$NET/$C2"/ed25519_*.json "$CRED/$NET/$C2.json"; do
      if [ -f "$f" ]; then echo "  v2 ($C2): $(jq -r '.public_key // empty' "$f") en $f; nombre: (ninguno: se borran en cadena en sin-llaves)"; fi
    done
    return 0
  fi
  echo "Comandos para c0der (este script NO los corre y nunca imprime una llave). Cada llave VIVA es UNA linea:"
  echo "sube a $modo por NOMBRE && compara sha256 sin mostrar nada && recien entonces borra el archivo (y la copia"
  echo "<cuenta>.json de near-cli si es la misma llave). Si la subida o la comparacion fallan, no se borra nada."
  while read -r rol cuenta; do
    [ -n "$rol" ] || continue
    if ! existe "$cuenta"; then
      echo
      echo "# $rol: $cuenta no existe en cadena: sus archivos no se tocan"
      continue
    fi
    vivas_c=$(llaves_json "$cuenta" | jq -r '.[].public_key')
    copia=$CRED/$NET/$cuenta.json
    pk_copia=""
    [ -f "$copia" ] && pk_copia=$(jq -r '.public_key // empty' "$copia")
    copia_hecha=0
    for f in "$CRED/$NET/$cuenta"/ed25519_*.json; do
      [ -f "$f" ] || continue
      pk=$(jq -r '.public_key // empty' "$f")
      if printf '%s\n' "$vivas_c" | grep -qxF -- "$pk"; then estado=viva; else estado=muerta; fi
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
      linea_secreto "$(nombre_ssm "$rol")" "$f" "Arras $RED $rol $cuenta $pk" "$borrar" "$rol"
    done
    if [ -n "$pk_copia" ] && [ "$copia_hecha" = 0 ]; then
      if ! printf '%s\n' "$vivas_c" | grep -qxF -- "$pk_copia"; then
        echo "rm '$copia'   # copia de near-cli con la llave $pk_copia, que ya no esta en cadena"
      else
        echo "# $copia tiene la llave $pk_copia, viva y sin archivo en $CRED/$NET/$cuenta/: revisarla a mano antes de borrarla"
      fi
    fi
  done <<<"$ROLES_V2"
  echo
  if [ -f "$TICKET_PEM" ]; then
    tpk=""
    existe "$C2" && [ "$(codigo_hex "$C2")" = "$WASM_SHA" ] &&
      tpk=$(vista "$C2" get_operator "$(jq -nc --arg id "$OP" '{operator_id:$id}')" | jq -r '.ticket_pubkey // empty')
    if [ -n "$tpk" ] && [ "$(pub_de_pem <"$TICKET_PEM")" = "$tpk" ]; then
      echo "# em-ticket-v2: la llave del ticket del operador $OP (ticket_pubkey $tpk)"
      linea_secreto "$(nombre_ssm em-ticket-v2)" "$TICKET_PEM" "Arras $RED em-ticket-v2 operador $OP de $C2 $tpk" "'$TICKET_PEM'" em-ticket-v2
    else
      echo "# OJO: $TICKET_PEM no es la ticket_pubkey del operador $OP en cadena (${tpk:-sin operador o sin ticket}): no se toca"
    fi
  fi
  echo
  # v2: sus llaves se borran en cadena en sin-llaves; los archivos (si hay) solo se borran cuando la lista esta vacia.
  if existe "$C2" && [ "$(n_llaves "$C2")" != 0 ]; then
    echo "# OJO: $C2 todavia tiene llaves en cadena: corre el paso sin-llaves; sus archivos no se tocan."
  elif existe "$C2"; then
    echo "# v2 ($C2): sus llaves se borraron en cadena (view_access_key_list vacio)."
    for f in "$CRED/$NET/$C2"/ed25519_*.json "$CRED/$NET/$C2.json"; do
      if [ -f "$f" ]; then echo "rm '$f'"; fi
    done
    if [ "$RED" = mainnet ]; then
      echo "# El parametro $(nombre_ssm v2-reserva) guarda una llave que ya no esta en cadena; borrarlo es opcional: aws ssm delete-parameter --region $REGION --name '$(nombre_ssm v2-reserva)'"
    fi
  fi
  echo "# (rm comun: en APFS sobrescribir no garantiza nada; el disco lo protege FileVault.)"
}

paso_enlaces() {
  local a
  echo "Cuentas:"
  for a in $(cuentas_todas); do
    echo "  $NB/address/$a"
  done
  echo "Codigo verificado del contrato: $NB/address/$C2?tab=contract (sha256 $WASM_SHA, rev $REV)"
  if [ -f "$TXS" ]; then
    echo "Transacciones ($TXS):"
    awk -F'\t' -v nb="$NB" '{ printf "  %-58s %s/txns/%s\n", $2, nb, $3 }' "$TXS"
  fi
  echo "Cuando EM vaya a usar v2 (el humo no lo necesita): el facilitador con NEAR_ESCROW_CONTRACTS_$(printf '%s' "$RED" | tr '[:lower:]' '[:upper:]')=$C1,$C2;"
  echo "EM libera con $CAPT (llave en $(nombre_ssm em-capturer-v2)) y, mientras $OP tenga ticket_pubkey, firma un ticket por"
  echo "cada pago y deposito ($(nombre_ssm em-ticket-v2)) o el owner lo quita con update_operator (docs/MAINNET.md, 8.1)."
}

# --- principal --------------------------------------------------------------------------------------------------------
# Todo lo que tarda corre dentro de `principal`, que bash ya leyo entera antes de empezar; el `exit` va en la misma linea.
principal() {
  if real && [ "$RED" = mainnet ] && [ -z "${ARRAS_RPC:-}" ]; then
    die "en mainnet ARRAS_RPC es obligatoria (el RPC privado de c0der; no se imprime): el defecto publico solo vale para testnet"
  fi
  comun_preparar
  # El ticket necesita OpenSSL 3: se mira al arrancar, antes de mandar un solo NEAR, en los pasos que lo usan.
  if real && [ "$TICKET" = si ]; then
    case "$PASO" in todo | 5 | operador | humo | humo-*) openssl_ok ;; esac
  fi
  echo "== Arras v2 $RED, modo $MODO, padre $P, contrato $C2, paso $PASO"
  [ "$MODO" = seco ] && echo "== EN SECO: nada se ejecuta. Modo real: --$RED-ok"

  case "$PASO" in
    plan) paso_plan ;;
    wasm) paso_wasm ;;
    1 | fondeo) paso_fondeo ;;
    2 | cuentas) paso_cuentas ;;
    3 | usdc) paso_usdc ;;
    4 | deploy) paso_deploy ;;
    5 | operador) paso_operador ;;
    6 | capturer) paso_capturer ;;
    7 | arbitro) paso_arbitro ;;
    8 | sin-llaves) paso_sin_llaves ;;
    9 | verificar) paso_verificar ;;
    humo-cuentas) paso_humo_cuentas ;;
    humo-presupuesto) paso_humo_presupuesto ;;
    humo-pago) paso_humo_pago ;;
    humo-disputa) paso_humo_disputa ;;
    humo-cierre) paso_humo_cierre ;;
    medir) paso_medir ;;
    secretos) paso_secretos ;;
    enlaces) paso_enlaces ;;
    todo)
      paso_plan; paso_fondeo; paso_cuentas; paso_usdc; paso_deploy; paso_operador; paso_capturer; paso_arbitro
      paso_sin_llaves; paso_verificar; paso_secretos; paso_enlaces ;;
    humo)
      # Con secretos al final: el agente y el worker nacen en el humo y sus llaves tambien van a SSM.
      paso_humo_cuentas; paso_humo_presupuesto; paso_humo_pago; paso_humo_disputa; paso_humo_cierre; paso_secretos
      paso_enlaces ;;
    *) echo "paso desconocido: $PASO" >&2; uso ;;
  esac
}
principal; exit $?
