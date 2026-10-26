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

use super::layout::{decode_at, layout, split_tuple, static_len, Layout, Span};
use super::Decode;
use super::DecodeError;

impl Decode<'_> for () {
    const STATIC_LEN: Option<usize> = Some(0);

    #[inline]
    fn decode(bytes: &[u8]) -> Result<Self, DecodeError> {
        if bytes.is_empty() {
            Ok(())
        } else {
            Err(DecodeError)
        }
    }
}

macro_rules! decode_tuple {
    ($($name:ident $index:tt),+) => {
        impl<'a, $($name: Decode<'a>),+> Decode<'a> for ($($name,)+) {
            const STATIC_LEN: Option<usize> = static_len(Self::ELEMS);
            const ELEMS: &'static [Layout] = &[$(layout::<$name>()),+];

            fn decode(bytes: &'a [u8]) -> Result<Self, DecodeError> {
                let mut spans = [$({ stringify!($name); Span::EMPTY }),+];
                split_tuple(bytes, Self::ELEMS, &mut spans)?;
                Ok(($(decode_at::<$name>(bytes, spans[$index])?,)+))
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
