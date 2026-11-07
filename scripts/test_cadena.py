"""Una cadena de NEAR de mentira para los tests offline de scripts/mainnet-v2.sh (la usa scripts/test_mainnet.sh).

Sin red y sin llaves reales: el estado vive en un JSON ($STUB/cadena.json). El doble de `near` llama a
`aplicar` con los argumentos de near-cli; el doble del RPC llama a `rpc` con el cuerpo JSON-RPC. Lo que importa para los
tests es lo que el script tiene que respetar:

- cada tx la firma una llave VIVA de la cuenta (la del archivo de `sign-with-access-key-file`), con el permiso que hace
  falta: FullAccess para todo salvo una llamada sin deposito a un metodo y un receiver de una FunctionCall key;
- el deploy exige saldo para el storage del codigo; el contrato v2 (cualquier cuenta con codigo) aplica lo minimo del
  ciclo de vida (operador, presupuesto, pagos, disputa, retiros) y verifica el ticket del operador con OpenSSL 3;
- el token USDC (la cuenta de la red) registra cuentas y mueve saldos.

No es el contrato: los numeros finos (gas, storage exacto) se miden en el ensayo de testnet.
"""

import base64
import hashlib
import json
import os
import re
import secrets
import subprocess
import sys
import tempfile

A = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"
Y = 10**24
USDC = {
    "mainnet": "17208628f84f5d6ad33f0da3bbbeb27ffcb398eac501a31bd6ad2011e36133a1",
    "testnet": "3e2210e1184b45b64c8a434c0a7e7b23cc04ea7eb7a6c3c32520d03d4afcb8af",
}
ESTADO_CONTRATO = 1300  # bytes de estado que suma el deploy (aprox.)
GAS = 10**21  # 0,001 NEAR por tx


def b58(b):
    v, s = int.from_bytes(b, "big"), ""
    while v:
        v, r = divmod(v, 58)
        s = A[r] + s
    return "1" * (len(b) - len(b.lstrip(b"\0"))) + s


def de_b58(s, n):
    v = 0
    for c in s:
        v = v * 58 + A.index(c)
    return v.to_bytes(n, "big")


def ruta():
    return os.path.join(os.environ["STUB"], "cadena.json")


def leer():
    with open(ruta()) as f:
        return json.load(f)


def guardar(c):
    with open(ruta(), "w") as f:
        json.dump(c, f, indent=1, sort_keys=True)


def yocto(s):
    s = s.strip()
    if s.endswith("yoctoNEAR"):
        return int(s.split()[0])
    num = s.split()[0]
    ent, _, dec = num.partition(".")
    return int(ent) * Y + int((dec + "0" * 24)[:24] or 0)


def falla(msg):
    print(f"Error: {msg}")
    sys.exit(1)


def nueva_llave(cuenta, red):
    """Llave publica nueva y su archivo en el llavero legacy (la privada termina en la publica, como en near)."""
    pub = secrets.token_bytes(32)
    pk = "ed25519:" + b58(pub)
    sk = "ed25519:" + b58(secrets.token_bytes(32) + pub)
    d = os.path.join(os.environ["HOME"], ".near-credentials", red, cuenta)
    os.makedirs(d, exist_ok=True)
    with open(os.path.join(d, pk.replace(":", "_") + ".json"), "w") as f:
        json.dump({"account_id": cuenta, "public_key": pk, "private_key": sk}, f)
    return pk


def fc(allow, receiver, metodos):
    return {"FunctionCall": {"allowance": None if allow is None else str(allow), "receiver_id": receiver, "method_names": metodos}}


def cuenta(c, a):
    if a not in c["cuentas"]:
        falla(f"AccountDoesNotExist {a}")
    return c["cuentas"][a]


def firmante_ok(c, signer, accion):
    """La llave que near recibio por sign-with-access-key-file tiene que ser viva y servir para la accion."""
    f = os.path.join(os.environ["STUB"], "firma.json")
    if not os.path.exists(f):
        falla("sin llave (sign-with-access-key-file)")
    with open(f) as fh:
        k = json.load(fh)
    pk = k.get("public_key")
    if k.get("account_id") != signer:
        falla(f"InvalidAccessKey: la llave es de {k.get('account_id')} y firma {signer}")
    for llave in cuenta(c, signer)["keys"]:
        if llave["public_key"] != pk:
            continue
        perm = llave["access_key"]["permission"]
        if perm == "FullAccess":
            return
        f = perm["FunctionCall"]
        if accion[0] == "call" and accion[3] == 0 and accion[1] == f["receiver_id"] and accion[2] in f["method_names"]:
            return
        falla(f"InvalidAccessKey: {pk} no puede {accion}")
    falla(f"InvalidAccessKey: {pk} no esta viva en {signer}")


def contrato(c):
    for a, v in c["cuentas"].items():
        if v.get("contrato"):
            return a, v["contrato"]
    return None, None


def ticket_bytes(contrato_id, sujeto, expiry):
    return f"arras:v2:operator_ticket|{contrato_id}|{sujeto}|{expiry}".encode()


def verifica_ticket(op, contrato_id, sujeto, ticket):
    """True si no hace falta o si la firma ed25519 es de ticket_pubkey (con OpenSSL 3)."""
    tpk = op.get("ticket_pubkey")
    if not tpk:
        return ticket is None
    if not ticket or not ticket.get("sig"):
        return False
    openssl = os.environ.get("STUB_OPENSSL")
    if not openssl:
        return True
    pub = de_b58(tpk.split(":")[1], 32)
    der = bytes.fromhex("302a300506032b6570032100") + pub
    pem = "-----BEGIN PUBLIC KEY-----\n" + base64.b64encode(der).decode() + "\n-----END PUBLIC KEY-----\n"
    try:
        sig = de_b58(ticket["sig"], 64)
    except Exception:
        return False
    with tempfile.TemporaryDirectory() as d:
        open(os.path.join(d, "k.pem"), "w").write(pem)
        open(os.path.join(d, "m"), "wb").write(ticket_bytes(contrato_id, sujeto, ticket["expiry"]))
        open(os.path.join(d, "s"), "wb").write(sig)
        r = subprocess.run([openssl, "pkeyutl", "-verify", "-pubin", "-inkey", os.path.join(d, "k.pem"), "-rawin",
                            "-in", os.path.join(d, "m"), "-sigfile", os.path.join(d, "s")], capture_output=True)
        return r.returncode == 0


def id_pago(args):
    claves = ["owner", "operator_id", "receiver", "amount", "release_deadline", "reclaim_after", "fee_bps_max", "terms_hash", "salt"]
    return b58(hashlib.sha256(json.dumps([args[k] for k in claves]).encode()).digest())


def acreditar(k, a, n):
    k["claimable"][a] = str(int(k["claimable"].get(a, "0")) + n)


def usdc_mover(c, de, a, n):
    s = c["usdc"]["saldos"]
    if a not in c["usdc"]["registradas"]:
        return False
    if int(s.get(de, "0")) < n:
        falla(f"ft: {de} no tiene {n}")
    s[de] = str(int(s.get(de, "0")) - n)
    s[a] = str(int(s.get(a, "0")) + n)
    return True


def llamar_contrato(c, cid, k, signer, m, j, dep):
    ops, pagos = k["operators"], k["payments"]
    if m == "register_operator":
        if signer != k["config"]["owner"] or dep != 1:
            falla("E40 / 1 yocto")
        if j["operator_id"] in ops:
            falla("E44")
        op = dict(j["input"])
        op.setdefault("ticket_pubkey", None)
        op.update(tvl_operador="0", sponsor_pool="0")
        ops[j["operator_id"]] = op
    elif m == "fund_operator_storage":
        ops[j["operator_id"]]["sponsor_pool"] = str(int(ops[j["operator_id"]]["sponsor_pool"]) + dep)
    elif m == "storage_deposit":
        a = j.get("account_id") or signer
        k["nep145"].setdefault(a, {"total": "0", "available": "0"})
        k["nep145"][a] = {"total": str(int(k["nep145"][a]["total"]) + dep), "available": str(int(k["nep145"][a]["available"]) + dep - 10**21)}
    elif m == "storage_withdraw":
        if dep != 1:
            falla("1 yocto")
        e = k["nep145"][signer]
        cuenta(c, signer)["amount"] = str(int(cuenta(c, signer)["amount"]) + int(e["available"]))
        cuenta(c, cid)["amount"] = str(int(cuenta(c, cid)["amount"]) - int(e["available"]))
        e["total"] = str(int(e["total"]) - int(e["available"]))
        e["available"] = "0"
    elif m == "authorize_from_budget":
        clave = f"{j['owner']}|{signer}|{j['operator_id']}"
        b = k["budgets"].get(clave)
        op = ops[j["operator_id"]]
        n = int(j["amount"])
        if b is None or int(b["saldo"]) < n:
            falla("E50")
        if int(j.get("dispute_deadline", j["reclaim_after"])) < int(j["release_deadline"]):
            falla("E67")
        pid = id_pago(j)
        if not verifica_ticket(op, cid, "pago:" + pid, j.get("operator_ticket")):
            falla("E53")
        if pid in pagos:
            falla("E13")
        b["saldo"] = str(int(b["saldo"]) - n)
        pagos[pid] = {"operator_id": j["operator_id"], "payer": j["owner"], "receiver": j["receiver"], "amount": str(n),
                      "released": "0", "refunded": "0", "fee_bps": op["fee_bps"], "fee_recipient": op["fee_recipient"],
                      "status": "Authorized", "dispute": None}
    elif m in ("release", "open_dispute", "resolve_dispute"):
        p = pagos[j["payment_id"]]
        op = ops[p["operator_id"]]
        n = int(p["amount"])
        if m == "release":
            if signer not in op["capturers"] or p["status"] != "Authorized":
                falla("E21 / E22")
            fee = n * p["fee_bps"] // 10000
            p.update(status="Released", released=str(n))
            acreditar(k, p["receiver"], n - fee)
            acreditar(k, p["fee_recipient"], fee)
        elif m == "open_dispute":
            if signer not in (p["payer"], p["receiver"]) or p["status"] != "Authorized":
                falla("E60 / E22")
            p.update(status="Disputed", dispute={"opened_by": "Payer" if signer == p["payer"] else "Receiver", "reason_hash": j["reason_hash"]})
        else:
            if signer != op["arbiter"] or p["status"] != "Disputed":
                falla("E62 / E22")
            r, py = int(j["to_receiver"]), int(j["to_payer"])
            if r + py != n:
                falla("E63")
            fee = r * p["fee_bps"] // 10000
            p.update(status="Resolved", released=str(r), refunded=str(py))
            acreditar(k, p["receiver"], r - fee)
            acreditar(k, p["fee_recipient"], fee)
            acreditar(k, p["payer"], py)
    elif m == "withdraw_for":
        a = j["account"]
        n = int(k["claimable"].get(a, "0"))
        if n and usdc_mover(c, cid, a, n):
            k["claimable"][a] = "0"
    elif m == "revoke_agent":
        clave = f"{signer}|{j['agent']}|{j['operator_id']}"
        b = k["budgets"].pop(clave, None)
        if b is None:
            falla("E50")
        acreditar(k, signer, int(b["saldo"]))
    else:
        falla(f"metodo desconocido {m}")


def aplicar(argv):
    c = leer()
    red = c["chain_id"]
    a = argv
    if a[:1] == ["tokens"]:
        signer, dest, monto = a[1], a[3], yocto(a[4])
        firmante_ok(c, signer, ("transfer",))
        s = cuenta(c, signer)
        if int(s["amount"]) - s["storage"] * 10**19 < monto + GAS:
            falla("NotEnoughBalance")
        s["amount"] = str(int(s["amount"]) - monto - GAS)
        d = c["cuentas"].setdefault(dest, {"amount": "0", "storage": 182, "code": "0" * 64, "keys": []})
        d["amount"] = str(int(d["amount"]) + monto)
    elif a[:2] == ["account", "create-account"]:
        nueva, monto, signer = a[3], yocto(a[4]), a[a.index("sign-as") + 1]
        firmante_ok(c, signer, ("create",))
        if not nueva.endswith("." + signer):
            falla("CreateAccountNotAllowed")
        if nueva in c["cuentas"]:
            falla("AccountAlreadyExists")
        s = cuenta(c, signer)
        if int(s["amount"]) - s["storage"] * 10**19 < monto + GAS:
            falla("NotEnoughBalance")
        s["amount"] = str(int(s["amount"]) - monto - 711 * 10**19)  # 0,00711: lo medido (MAINNET.md, 3)
        pk = nueva_llave(nueva, red)
        c["cuentas"][nueva] = {"amount": str(monto), "storage": 182, "code": "0" * 64,
                               "keys": [{"public_key": pk, "access_key": {"nonce": 0, "permission": "FullAccess"}}]}
    elif a[:2] == ["account", "add-key"]:
        signer = a[2]
        firmante_ok(c, signer, ("add-key",))
        allow = yocto(a[a.index("--allowance") + 1])
        rec = a[a.index("--contract-account-id") + 1]
        mets = [x.strip() for x in a[a.index("--function-names") + 1].split(",")]
        pk = nueva_llave(signer, red)
        cuenta(c, signer)["keys"].append({"public_key": pk, "access_key": {"nonce": 0, "permission": fc(allow, rec, mets)}})
    elif a[:2] == ["account", "delete-keys"]:
        signer = a[2]
        firmante_ok(c, signer, ("delete-keys",))
        borrar = a[a.index("public-keys") + 1].split(",")
        s = cuenta(c, signer)
        s["keys"] = [k for k in s["keys"] if k["public_key"] not in borrar]
    elif a[:2] == ["contract", "deploy"]:
        signer, wasm = a[2], a[a.index("use-file") + 1]
        firmante_ok(c, signer, ("deploy",))
        s = cuenta(c, signer)
        if s["code"] != "0" * 64:
            falla("ya tiene codigo")
        w = open(wasm, "rb").read()
        storage = s["storage"] + len(w) + ESTADO_CONTRATO
        if int(s["amount"]) - GAS < storage * 10**19:
            falla("LackBalanceForState")
        s.update(code=hashlib.sha256(w).hexdigest(), storage=storage, amount=str(int(s["amount"]) - GAS))
        nuevo = json.loads(a[a.index("json-args") + 1])
        rev = re.search(rb"rev=([0-9a-f]{40})", w)
        s["contrato"] = {
            "config": {"version": "arras-v2", "owner": nuevo["owner"], "guardian": nuevo["guardian"], "paused": False,
                       "token": nuevo["token"], "tope_por_pago": nuevo["tope_por_pago"],
                       "tope_tvl_global": nuevo["tope_tvl_global"], "tvl_global": "0", "in_flight": "0",
                       "budgets_total": "0", "payments": 0, "operators": 0},
            "rev": rev.group(1).decode() if rev else "",
            "operators": {}, "payments": {}, "budgets": {}, "claimable": {}, "nep145": {},
        }
    elif a[:3] == ["contract", "call-function", "as-transaction"]:
        dest, m, j = a[3], a[4], json.loads(a[a.index("json-args") + 1])
        dep = yocto(a[a.index("attached-deposit") + 1])
        signer = a[a.index("sign-as") + 1]
        firmante_ok(c, signer, ("call", dest, m, dep))
        s = cuenta(c, signer)
        s["amount"] = str(int(s["amount"]) - GAS - dep)
        if dest == USDC[red]:
            if m == "storage_deposit":
                if j["account_id"] not in c["usdc"]["registradas"]:
                    c["usdc"]["registradas"].append(j["account_id"])
            elif m == "ft_transfer_call":
                cid, k = contrato(c)
                n = int(j["amount"])
                if j["receiver_id"] not in c["usdc"]["registradas"]:
                    falla(f"ft: {j['receiver_id']} no esta registrada")
                msg = json.loads(j["msg"])
                ok = j["receiver_id"] == cid and k is not None and msg.get("action") == "budget"
                op = k["operators"].get(msg.get("operator_id")) if ok else None
                sujeto = f"presupuesto:{msg.get('operator_id')}|{signer}|{msg.get('agent')}"
                if op is not None and verifica_ticket(op, cid, sujeto, msg.get("operator_ticket")) and signer in k["nep145"]:
                    usdc_mover(c, signer, cid, n)
                    clave = f"{signer}|{msg['agent']}|{msg['operator_id']}"
                    b = k["budgets"].setdefault(clave, {"saldo": "0"})
                    b.update(saldo=str(int(b["saldo"]) + n), tope_por_pago=msg["tope_por_pago"], tope_diario=msg["tope_diario"])
                else:
                    print("Log: arras:rechazo (el token devuelve todo)")
            else:
                falla(f"usdc: metodo {m}")
        else:
            k = cuenta(c, dest).get("contrato")
            if k is None:
                falla(f"CodeDoesNotExist {dest}")
            # Lo adjunto entra al saldo del contrato (pool, NEP-145, 1 yocto).
            cuenta(c, dest)["amount"] = str(int(cuenta(c, dest)["amount"]) + dep)
            llamar_contrato(c, dest, k, signer, m, j, dep)
    else:
        falla(f"comando de near desconocido: {' '.join(a[:3])}")
    guardar(c)


def vista(c, cid, m, j):
    red = c["chain_id"]
    if cid == USDC[red]:
        if m == "storage_balance_of":
            return {"total": "1250000000000000000000", "available": "0"} if j["account_id"] in c["usdc"]["registradas"] else None
        if m == "ft_balance_of":
            return c["usdc"]["saldos"].get(j["account_id"], "0")
    k = c["cuentas"].get(cid, {}).get("contrato")
    if k is None:
        raise KeyError("CodeDoesNotExist")
    if m == "get_config":
        conf = dict(k["config"])
        conf.update(payments=len(k["payments"]), operators=len(k["operators"]))
        return conf
    if m == "get_operator":
        return k["operators"].get(j["operator_id"])
    if m == "get_payment":
        return k["payments"].get(j["payment_id"])
    if m == "get_claimable":
        return k["claimable"].get(j["account"], "0")
    if m == "get_budget":
        b = k["budgets"].get(f"{j['owner']}|{j['agent']}|{j['operator_id']}")
        return None if b is None else dict(b)
    if m == "compute_budget_payment_id":
        return id_pago(j)
    if m == "ticket_message_payment":
        return base64.b64encode(ticket_bytes(cid, "pago:" + j["payment_id"], j["expiry"])).decode()
    if m == "ticket_message_budget":
        return base64.b64encode(ticket_bytes(cid, f"presupuesto:{j['operator_id']}|{j['owner']}|{j['agent']}", j["expiry"])).decode()
    if m == "storage_balance_of":
        return k["nep145"].get(j["account_id"])
    if m == "get_storage_health":
        s = c["cuentas"][cid]
        libre = int(s["amount"]) - s["storage"] * 10**19 - sum(int(o["sponsor_pool"]) for o in k["operators"].values())
        return {"free_reserve": str(libre), "ok": libre >= 2 * 10**23}
    if m == "contract_source_metadata":
        return {"version": "0.1.0", "build_info": {"source_code_snapshot": "git+https://github.com/UltravioletaDAO/arras?rev=" + k["rev"]}}
    raise KeyError(f"MethodNotFound {m}")


def rpc(cuerpo):
    c = leer()
    req = json.loads(cuerpo)
    if req.get("method") == "status":
        return {"jsonrpc": "2.0", "id": "arras", "result": {"chain_id": c["chain_id"]}}
    p = req["params"]
    rt, acct = p["request_type"], p.get("account_id", "")
    error = {"jsonrpc": "2.0", "id": "arras", "error": {"cause": {"name": "UNKNOWN_ACCOUNT"}, "name": "HANDLER_ERROR"}}
    if rt == "call_function":
        try:
            r = vista(c, acct, p["method_name"], json.loads(base64.b64decode(p["args_base64"]) or b"{}"))
        except KeyError as e:
            return {"jsonrpc": "2.0", "id": "arras", "error": {"cause": {"name": str(e)}, "name": "HANDLER_ERROR"}}
        return {"jsonrpc": "2.0", "id": "arras", "result": {"result": list(json.dumps(r).encode())}}
    s = c["cuentas"].get(acct)
    if s is None:
        return error
    if rt == "view_account":
        return {"jsonrpc": "2.0", "id": "arras", "result": {"amount": s["amount"], "locked": "0", "storage_usage": s["storage"],
                                                           "code_hash": b58(bytes.fromhex(s["code"]))}}
    if rt == "view_access_key_list":
        return {"jsonrpc": "2.0", "id": "arras", "result": {"keys": s["keys"]}}
    return error


def inicial(red, padre, hub, hub_yocto, modo):
    """La cadena como la dejo scripts/mainnet.sh (v1 desplegado y sin llaves, v2 reservada con 0,1 NEAR y su FullAccess)
    mas el hub con hub_yocto y 3,957303 USDC. Las llaves que ya existian van a SSM ($STUB/ssm/<nombre con _>, modo ssm)
    o al llavero legacy (modo llavero; la del hub a $STUB/hub.json, la de ARRAS_FONDEO_ARCHIVO). Cada privada se anota en
    $STUB/privadas para buscarla en la salida."""
    stub = os.environ["STUB"]
    c = {"chain_id": red, "cuentas": {}, "usdc": {"registradas": [], "saldos": {}}}
    privadas = []

    def con_llave(a, monto, rol):
        es_implicita = len(a) == 64 and all(ch in "0123456789abcdef" for ch in a)
        pub = bytes.fromhex(a) if es_implicita else secrets.token_bytes(32)
        pk, sk = "ed25519:" + b58(pub), "ed25519:" + b58(secrets.token_bytes(32) + pub)
        privadas.append(sk)
        c["cuentas"][a] = {"amount": str(monto), "storage": 182, "code": "0" * 64,
                           "keys": [{"public_key": pk, "access_key": {"nonce": 0, "permission": "FullAccess"}}]}
        clave = json.dumps({"account_id": a, "public_key": pk, "private_key": sk})
        if modo == "ssm":
            d = os.path.join(stub, "ssm")
            os.makedirs(d, exist_ok=True)
            with open(os.path.join(d, f"/arras/{red}/{rol}".replace("/", "_")), "w") as f:
                f.write(clave)
        elif rol == "fondeo":
            with open(os.path.join(stub, "hub.json"), "w") as f:
                f.write(clave)
        else:
            d = os.path.join(os.environ["HOME"], ".near-credentials", red, a)
            os.makedirs(d, exist_ok=True)
            with open(os.path.join(d, pk.replace(":", "_") + ".json"), "w") as f:
                f.write(clave)

    con_llave(hub, int(hub_yocto), "fondeo")
    con_llave(padre, Y // 10, "padre")
    con_llave("v2." + padre, Y // 10, "v2-reserva")
    con_llave("owner." + padre, Y // 10, "owner")
    con_llave("guardian." + padre, Y // 10, "guardian")
    con_llave("em-fees." + padre, Y // 10, "em-fees")
    c["cuentas"]["v1." + padre] = {"amount": str(3 * Y), "storage": 269634, "code": "70" * 32, "keys": []}
    c["cuentas"]["em-capturer." + padre] = {"amount": str(Y // 2), "storage": 200, "code": "0" * 64, "keys": [
        {"public_key": "ed25519:" + b58(secrets.token_bytes(32)),
         "access_key": {"nonce": 0, "permission": fc(Y // 2, "v1." + padre, ["release", "refund", "withdraw_for"])}}]}
    c["usdc"]["registradas"] = ["v1." + padre, "em-fees." + padre, hub]
    c["usdc"]["saldos"] = {hub: "3957303"}
    with open(os.path.join(stub, "privadas"), "w") as f:
        f.write("\n".join(privadas) + "\n")
    guardar(c)


if __name__ == "__main__":
    if sys.argv[1] == "aplicar":
        aplicar(sys.argv[2:])
    elif sys.argv[1] == "rpc":
        print(json.dumps(rpc(sys.argv[2])))
    elif sys.argv[1] == "inicial":
        inicial(*sys.argv[2:7])
