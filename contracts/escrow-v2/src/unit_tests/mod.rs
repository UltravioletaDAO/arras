//! Tests unitarios de v2. Los de v1 (`authorize`, `ciclo`, `retiro`, `estado`, `admin`) corren otra vez contra v2,
//! adaptados donde v2 cambia algo a proposito (version, reserva de la disputa, operador con arbitro); los nuevos son
//! `presupuesto`, `disputa` y `ticket`, y `invariante` (U-16) suma presupuestos y disputas. Toda prueba de rechazo
//! afirma ademas que el estado no cambio ([`common::Snapshot`]).

mod admin;
mod authorize;
mod ciclo;
mod common;
mod disputa;
mod estado;
mod invariante;
mod presupuesto;
mod refutacion;
mod retiro;
mod ticket;
mod vectores;
