#!/usr/bin/env python3
"""Genera testdata/vectores_256.rs: vectores de la aritmetica de 256 bits de summary.rs calculados con los enteros sin
tope de Python (otra implementacion, no la de Rust). Deterministico (semilla fija). Uso, desde
contracts/erc8004-common:
    python3 -I testdata/vectores_256.py > testdata/vectores_256.rs

MUL: (a, b, [w0, w1, w2, w3]) con a * b en palabras de 64 bits little-endian. Incluye a proposito productos en los
que, al multiplicar por mitades de 64 bits (a = a_lo + a_hi * 2^64), la suma de la parte alta de a_lo * b y la parte
baja de a_hi * b pasa de 2^64: el acarreo de la palabra 1 a la 2 (refutador, O4). Con b = 10^k eso pasa en ~5 % de
los a al azar, asi que se buscan.

RESUMEN: (lista de (value, valueDecimals), count, summaryValue, summaryValueDecimals) con la cuenta de getSummary de
ReputationRegistryUpgradeable 2.0.0: cada value se lleva a 18 decimales, se suman, se divide por la cantidad
truncando hacia cero, y el promedio se lleva a la moda de los decimales (empate: el menor) truncando hacia cero. Sin
el cast final a int128 (desviacion R-4: aca el resultado es el exacto)."""
import random

MAX_ABS = 10**38
M64 = (1 << 64) - 1


def trunc_div(a, b):
    q = abs(a) // abs(b)
    return q if (a >= 0) == (b > 0) else -q


def resumen(items):
    total = sum(v * 10 ** (18 - d) for v, d in items)
    counts = [0] * 19
    for _, d in items:
        counts[d] += 1
    moda = max(range(19), key=lambda d: (counts[d], -d))
    avg = trunc_div(total, len(items))
    return len(items), trunc_div(avg, 10 ** (18 - moda)), moda


def words(x):
    return [(x >> (64 * i)) & M64 for i in range(4)]


def con_acarreo(a, b):
    """Si a * b, hecho por mitades de 64 bits, acarrea de la palabra 1 a la 2."""
    return (((a & M64) * b) >> 64) + (((a >> 64) * b) & M64) > M64


r = random.Random(8004)
mul = []
for a in [2**120, 2**127, 2**128 - 1, MAX_ABS, MAX_ABS - 1, 2**64, 2**64 - 1, 0, 1]:
    for b in [10**18, M64, 1]:
        mul.append((a, b))
while len(mul) < 48:
    a = r.randrange(2**120, 2**128)
    b = 10 ** r.randrange(0, 19) if r.random() < 0.7 else r.randrange(1, 2**64)
    mul.append((a, b))
while len(mul) < 80:
    a = r.randrange(2**64, MAX_ABS + 1)
    b = 10 ** r.randrange(1, 19)
    if con_acarreo(a, b):
        mul.append((a, b))


def item_con_acarreo():
    while True:
        a, d = r.randrange(2**64, MAX_ABS + 1), r.randrange(0, 18)
        if con_acarreo(a, 10 ** (18 - d)):
            return (a if r.random() < 0.5 else -a, d)

res = [
    [(MAX_ABS, 0)],
    [(-MAX_ABS, 0)],
    [(MAX_ABS, 0), (1, 1), (1, 1)],
    [(MAX_ABS, 0), (-MAX_ABS, 0), (1, 0)],
    [(MAX_ABS, 0)] * 7 + [(-1, 18)],
    [(-MAX_ABS, 18), (MAX_ABS, 0)],
]
while len(res) < 120:
    n = r.randrange(1, 9)
    items = []
    for _ in range(n):
        mag = r.choice([MAX_ABS, r.randrange(0, MAX_ABS + 1), r.randrange(0, 10**r.randrange(1, 39))])
        v = mag if r.random() < 0.5 else -mag
        items.append((v, r.randrange(0, 19)))
    res.append(items)
# Uno solo con acarreo (el resumen tiene que devolver el mismo value) y mezclas con al menos uno.
while len(res) < 150:
    res.append([item_con_acarreo()])
while len(res) < 200:
    items = [item_con_acarreo() for _ in range(r.randrange(1, 4))]
    items += [(r.randrange(-MAX_ABS, MAX_ABS + 1), r.randrange(0, 19)) for _ in range(r.randrange(0, 5))]
    r.shuffle(items)
    res.append(items)

print("// Generado por testdata/vectores_256.py (enteros de Python). No editar a mano.")
print("pub const MUL: &[(u128, u64, [u64; 4])] = &[")
for a, b in mul:
    w = ", ".join(str(x) for x in words(a * b))
    print(f"    ({a}, {b}, [{w}]),")
print("];")
print("/// (calificaciones, count, summaryValue, summaryValueDecimals).")
print("pub type Caso = (&'static [(i128, u8)], u64, &'static str, u8);")
print("pub const RESUMEN: &[Caso] = &[")
for items in res:
    n, v, d = resumen(items)
    lst = ", ".join(f"({x}, {y})" for x, y in items)
    print(f'    (&[{lst}], {n}, "{v}", {d}),')
print("];")
