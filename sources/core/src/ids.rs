//! Ids made from what places something (a line's, a node's, a ghost's, …): the same parts, the
//! same id, so main and a branch, and a project read again from its file, name the same thing alike.

use sha2::{Digest, Sha256};

/// An id made from the parts that place something: the same parts, the same id.
pub fn made_id(parts: &[&str]) -> String {
    let mut hash = Sha256::new();
    for part in parts {
        hash.update(part.as_bytes());
        hash.update([1u8]);
    }
    let digest = hash.finalize();
    digest[..12].iter().map(|b| format!("{b:02x}")).collect()
}
