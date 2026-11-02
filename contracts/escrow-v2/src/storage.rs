//! NEP-145 (sec. 3.6), igual que v1: el payer paga el storage de sus pagos y el owner el de sus presupuestos.
//! `storage_balance_bounds.min` cubre el propio registro de una cuenta de 64 caracteres; ese minimo queda "usado" (no
//! es `available`). No hay `storage_unregister`. Aca viven tambien las cuentas de bytes que comparten el pago y el
//! presupuesto.

use crate::*;
use near_contract_standards::storage_management::{StorageBalance, StorageBalanceBounds};
use near_sdk::{Promise, assert_one_yocto};

/// Bytes del registro NEP-145 de una cuenta de largo maximo: clave `s` + borsh(AccountId), valor 2 x u128.
pub const STORAGE_RECORD_MAX_BYTES: u64 = record_bytes(1 + 4 + MAX_ACCOUNT_ID_LEN, 32);

impl StorageAccount {
    pub fn to_balance(self) -> StorageBalance {
        StorageBalance {
            total: NearToken::from_yoctonear(self.total),
            available: NearToken::from_yoctonear(self.available),
        }
    }
}

/// Bytes de lo que escribe un pago o un presupuesto, sin la entrada de `sponsored_used`.
pub(crate) struct Bytes {
    pub base: u64,
    /// La entrada de `sponsored_used` si el patrocinio paga y todavia no existe.
    pub sponsor_extra: u64,
}

/// `borsh(Budget)`: cuatro u128, un u64, un u128 y un bool.
pub const BUDGET_VALUE_BYTES: u64 = 16 * 4 + 8 + 16 + 1;

impl Arras {
    /// La clave `(operator_id, cuenta)` de `payer_tvl` y `sponsored_used`: prefijo + dos String borsh.
    pub(crate) fn pair_key_bytes(key: &(OperatorId, AccountId)) -> u64 {
        1 + 4 + key.0.len() as u64 + 4 + key.1.len() as u64
    }

    /// La entrada de `payer_tvl` si es nueva y las de `claimable` nuevas.
    pub(crate) fn pair_and_claimable_bytes(
        &self,
        tvl_key: &(OperatorId, AccountId),
        new_claimable: &[AccountId],
    ) -> u64 {
        let mut b = 0;
        if !self.payer_tvl.contains_key(tvl_key) {
            b += record_bytes(Self::pair_key_bytes(tvl_key), 16);
        }
        for a in new_claimable {
            b += record_bytes(1 + 4 + a.len() as u64, 16);
        }
        b
    }

    /// La entrada de `sponsored_used` si todavia no existe (solo cuenta si paga el patrocinio).
    pub(crate) fn sponsor_extra_bytes(&self, tvl_key: &(OperatorId, AccountId)) -> u64 {
        if self.sponsored_used.contains_key(tvl_key) {
            0
        } else {
            record_bytes(Self::pair_key_bytes(tvl_key), 16)
        }
    }

    /// El registro de un presupuesto: prefijo `b` + borsh(owner, agent, operator_id) -> `Budget`.
    pub(crate) fn budget_record_bytes(&self, key: &BudgetKey) -> u64 {
        record_bytes(
            1 + 4 + key.0.len() as u64 + 4 + key.1.len() as u64 + 4 + key.2.len() as u64,
            BUDGET_VALUE_BYTES,
        )
    }

    /// Devuelve `devuelto` yocto de storage a quien los pago: el saldo NEP-145 de `payer` o el `sponsor_pool` del
    /// operador (y baja su `sponsored_used`). Lo usan `compact` y `revoke_agent`.
    pub(crate) fn refund_storage(
        &mut self,
        operator_id: &str,
        payer: &AccountId,
        sponsored: bool,
        devuelto: u128,
    ) {
        if devuelto == 0 {
            return;
        }
        if sponsored {
            let op = self
                .operators
                .get_mut(operator_id)
                .unwrap_or_else(|| env::abort());
            op.sponsor_pool = U128(add(op.sponsor_pool.0, devuelto));
            let key = (operator_id.to_string(), payer.clone());
            if let Some(used) = self.sponsored_used.get_mut(&key) {
                *used = used.saturating_sub(devuelto);
            }
        } else if let Some(e) = self.storage.get_mut(payer) {
            e.available = add(e.available, devuelto);
            self.storage_available_total = add(self.storage_available_total, devuelto);
        }
    }
}

#[near]
impl Arras {
    /// NEP-145. Sin registro: exige `min` y lo descuenta; con `registration_only`, devuelve el resto. Ya registrado:
    /// con `registration_only` devuelve todo; si no, suma a `total` y a `available`.
    #[payable]
    pub fn storage_deposit(
        &mut self,
        account_id: Option<AccountId>,
        registration_only: Option<bool>,
    ) -> StorageBalance {
        let amount = env::attached_deposit().as_yoctonear();
        let account_id = account_id.unwrap_or_else(env::predecessor_account_id);
        let registration_only = registration_only.unwrap_or(false);
        let min = self.storage_balance_bounds().min.as_yoctonear();
        if let Some(entry) = self.storage.get(&account_id).copied() {
            if registration_only {
                if amount > 0 {
                    Promise::new(env::predecessor_account_id())
                        .transfer(NearToken::from_yoctonear(amount))
                        .detach();
                }
                return entry.to_balance();
            }
            let entry = StorageAccount {
                total: add(entry.total, amount),
                available: add(entry.available, amount),
            };
            self.storage.insert(account_id, entry);
            self.storage_available_total = add(self.storage_available_total, amount);
            return entry.to_balance();
        }
        if amount < min {
            env::panic_str("The attached deposit is less than the minimum storage balance");
        }
        let (total, refund) = if registration_only {
            (min, amount - min)
        } else {
            (amount, 0)
        };
        let entry = StorageAccount {
            total,
            available: total - min,
        };
        self.storage.insert(account_id, entry);
        self.storage_available_total = add(self.storage_available_total, entry.available);
        if refund > 0 {
            Promise::new(env::predecessor_account_id())
                .transfer(NearToken::from_yoctonear(refund))
                .detach();
        }
        entry.to_balance()
    }

    /// NEP-145: retira hasta `available` (todo si `amount` es `None`). 1 yocto.
    #[payable]
    pub fn storage_withdraw(&mut self, amount: Option<U128>) -> StorageBalance {
        assert_one_yocto();
        let account_id = env::predecessor_account_id();
        let Some(entry) = self.storage.get(&account_id).copied() else {
            env::panic_str(&format!("The account {account_id} is not registered"));
        };
        let amount = amount.map_or(entry.available, |a| a.0);
        if amount > entry.available {
            env::panic_str("The amount is greater than the available storage balance");
        }
        if amount == 0 {
            return entry.to_balance();
        }
        let entry = StorageAccount {
            total: entry.total - amount,
            available: entry.available - amount,
        };
        self.storage.insert(account_id.clone(), entry);
        self.storage_available_total = self
            .storage_available_total
            .checked_sub(amount)
            .unwrap_or_else(|| env::abort());
        Promise::new(account_id)
            .transfer(NearToken::from_yoctonear(amount))
            .detach();
        entry.to_balance()
    }

    pub fn storage_balance_of(&self, account_id: AccountId) -> Option<StorageBalance> {
        self.storage.get(&account_id).map(|e| e.to_balance())
    }

    pub fn storage_balance_bounds(&self) -> StorageBalanceBounds {
        StorageBalanceBounds {
            min: NearToken::from_yoctonear(bytes_cost(STORAGE_RECORD_MAX_BYTES)),
            max: None,
        }
    }
}
