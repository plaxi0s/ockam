use crate::VaultMutex;
use ockam_core::Result;
use ockam_vault_core::{KeyId, KeyIdVault, PublicKey, Secret};

impl<V: KeyIdVault> KeyIdVault for VaultMutex<V> {
    fn get_secret_by_key_id(&mut self, key_id: &str) -> Result<Secret> {
        #[cfg(feature = "std")]
        return self.0.lock().unwrap().get_secret_by_key_id(key_id);
        #[cfg(not(feature = "std"))]
        return ockam_node::interrupt::free(|cs| {
            self.0
                .borrow(cs)
                .borrow_mut()
                .as_mut()
                .unwrap()
                .get_secret_by_key_id(key_id)
        });
    }

    fn compute_key_id_for_public_key(&mut self, public_key: &PublicKey) -> Result<KeyId> {
        #[cfg(feature = "std")]
        return self
            .0
            .lock()
            .unwrap()
            .compute_key_id_for_public_key(public_key);
        #[cfg(not(feature = "std"))]
        return ockam_node::interrupt::free(|cs| {
            self.0
                .borrow(cs)
                .borrow_mut()
                .as_mut()
                .unwrap()
                .compute_key_id_for_public_key(public_key)
        });
    }
}

#[cfg(test)]
mod tests {
    use crate::VaultMutex;
    use ockam_vault::SoftwareVault;
    use ockam_vault_test_attribute::*;

    fn new_vault() -> VaultMutex<SoftwareVault> {
        VaultMutex::create(SoftwareVault::default())
    }

    #[vault_test]
    fn compute_key_id_for_public_key() {}

    #[vault_test]
    fn get_secret_by_key_id() {}
}
