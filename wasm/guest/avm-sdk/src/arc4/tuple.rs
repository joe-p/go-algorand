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

//! Tuples `(T1,T2,…)`, up to 16 elements.

use super::desc::{read_tuple_elem, Desc};
use super::Decode;

impl Decode<'_> for () {
    const DESC: Desc = Desc::tuple(&[]);

    #[inline]
    unsafe fn read(_bytes: &[u8]) -> Self {}
}

macro_rules! decode_tuple {
    ($($name:ident $index:tt),+) => {
        impl<'a, $($name: Decode<'a>),+> Decode<'a> for ($($name,)+) {
            const DESC: Desc = Desc::tuple(&[$($name::DESC),+]);

            unsafe fn read(bytes: &'a [u8]) -> Self {
                let elems = Self::DESC.elems();
                // SAFETY: the caller validated `bytes` as this tuple.
                ($(unsafe { read_tuple_elem::<$name>(bytes, elems, $index) },)+)
            }
        }
        impl<$($name: super::Encode),+> super::Encode for ($($name,)+) {
            const STATIC_LEN: Option<usize> = super::encode::static_size(&[$(($name::STATIC_LEN, $name::IS_BOOL)),+]);
            fn encode(&self, out: &mut [u8]) -> Result<usize, super::EncodeError> {
                super::encode::sequence(&[$(&self.$index as &dyn super::encode::Value),+], out)
            }
        }
    };
}

decode_tuple!(A 0);
decode_tuple!(A 0, B 1);
decode_tuple!(A 0, B 1, C 2);
decode_tuple!(A 0, B 1, C 2, D 3);
decode_tuple!(A 0, B 1, C 2, D 3, E 4);
decode_tuple!(A 0, B 1, C 2, D 3, E 4, F 5);
decode_tuple!(A 0, B 1, C 2, D 3, E 4, F 5, G 6);
decode_tuple!(A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7);
decode_tuple!(A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8);
decode_tuple!(A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8, J 9);
decode_tuple!(A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8, J 9, K 10);
decode_tuple!(A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8, J 9, K 10, L 11);
decode_tuple!(A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8, J 9, K 10, L 11, M 12);
decode_tuple!(A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8, J 9, K 10, L 11, M 12, N 13);
decode_tuple!(A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8, J 9, K 10, L 11, M 12, N 13, O 14);
decode_tuple!(A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8, J 9, K 10, L 11, M 12, N 13, O 14, P 15);
