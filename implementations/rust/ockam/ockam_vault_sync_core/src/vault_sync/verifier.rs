use crate::{VaultRequestMessage, VaultResponseMessage, VaultSync, VaultSyncCoreError};
use ockam_core::Result;
use ockam_vault_core::{PublicKey, Signature, Verifier};

impl Verifier for VaultSync {
    fn verify(
        &mut self,
        signature: &Signature,
        public_key: &PublicKey,
        data: &[u8],
    ) -> Result<bool> {
        let resp = self.call(VaultRequestMessage::Verify {
            signature: signature.clone(),
            public_key: public_key.clone(),
            data: data.into(),
        })?;

        if let VaultResponseMessage::Verify(s) = resp {
            Ok(s)
        } else {
            Err(VaultSyncCoreError::InvalidResponseType.into())
        }
    }
}
