// Copyright (C) 2019-2026 Algorand Foundation Ltd.
// This file is part of go-algorand
//
// go-algorand is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as
// published by the Free Software Foundation, either version 3 of the
// License, or (at your option) any later version.
//
// go-algorand is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with go-algorand.  If not, see <https://www.gnu.org/licenses/>.

//! Typed application IDs.
//!
//! [`Application`] is shared by ARC-4 app references and the context helpers
//! [`crate::txn::application_id`] and [`crate::globals::application_id`].

use crate::{txn, TxnField};

/// A typed application ID.
///
/// ARC-4 methods receive resolved IDs; [`Application::resolve`] maps a
/// zero-based index into this call's applications, where index 0 is the
/// current transaction's ApplicationID (0 during creation). It converts to and
/// from `u64`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Application {
    pub id: u64,
}

impl Application {
    /// Resolves a zero-based ARC-4 index into this call's applications.
    pub fn resolve(index: u8) -> Option<Self> {
        (u64::from(index) <= txn::uint(TxnField::NumApplications)).then(|| Self {
            id: txn::array_uint(TxnField::Applications, u32::from(index)),
        })
    }
}

impl From<u64> for Application {
    #[inline]
    fn from(id: u64) -> Self {
        Self { id }
    }
}

impl From<Application> for u64 {
    #[inline]
    fn from(application: Application) -> Self {
        application.id
    }
}
